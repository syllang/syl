//! Test transport for exercising the real JSON-RPC service without extra dependencies.
use crate::{SylLanguageServer, cancellation::CancellationRegistry, drc::BackgroundDrcScheduler};
use serde_json::{Value, json};
use std::{
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Instant,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    sync::mpsc,
    task::JoinHandle,
};
use tower_lsp::{LspService, Server};

pub(crate) struct TestClient {
    writer: mpsc::UnboundedSender<Vec<u8>>,
    messages: mpsc::UnboundedReceiver<Value>,
    server: JoinHandle<()>,
    reader: JoinHandle<()>,
    pub(crate) scheduler: Arc<BackgroundDrcScheduler>,
    pub(crate) registry: Arc<CancellationRegistry>,
}

impl TestClient {
    pub(crate) fn new() -> Self {
        let (writer, input) = mpsc::unbounded_channel();
        let server_input = ChannelReader {
            input,
            buffer: Vec::new(),
        };
        let (output, mut reader) = mpsc::unbounded_channel::<Vec<u8>>();
        let server_output = ChannelWriter { output };
        let (service, socket) = LspService::new(SylLanguageServer::new);
        let scheduler = Arc::clone(&service.inner().drc_scheduler);
        let registry = Arc::clone(&service.inner().cancellation_registry);
        let server = tokio::spawn(Server::new(server_input, server_output, socket).serve(service));
        let (tx, messages) = mpsc::unbounded_channel();
        let reader = tokio::spawn(async move {
            let mut pending = Vec::new();
            while let Some(chunk) = reader.recv().await {
                pending.extend(chunk);
                while let Some(end) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header = std::str::from_utf8(&pending[..end]).unwrap();
                    let length = header
                        .lines()
                        .next()
                        .unwrap()
                        .strip_prefix("Content-Length: ")
                        .unwrap()
                        .parse::<usize>()
                        .unwrap();
                    if pending.len() < end + 4 + length {
                        break;
                    }
                    let value =
                        serde_json::from_slice(&pending[end + 4..end + 4 + length]).unwrap();
                    pending.drain(..end + 4 + length);
                    if tx.send(value).is_err() {
                        return;
                    }
                }
            }
        });
        Self {
            writer,
            messages,
            server,
            reader,
            scheduler,
            registry,
        }
    }

    pub(crate) async fn send(&mut self, value: Value) {
        let body = value.to_string();
        let message = format!("Content-Length: {}\r\n\r\n{body}", body.len());
        self.writer.send(message.into_bytes()).unwrap();
    }

    pub(crate) async fn initialize(&mut self, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0", "id":1,"method":"initialize", "params":params}))
            .await;
        let response = self.recv().await;
        assert!(response.get("error").is_none(), "{response}");
        self.send(json!({"jsonrpc":"2.0", "method":"initialized", "params":{}}))
            .await;
        response
    }

    pub(crate) async fn recv(&mut self) -> Value {
        // Stay runnable while a blocking worker finishes: paused time must not
        // auto-advance across either diagnostic debounce boundary.
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match self.messages.try_recv() {
                Ok(value) => return value,
                Err(mpsc::error::TryRecvError::Empty) => {}
                Err(error) => panic!("test transport closed: {error}"),
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for server message"
            );
            tokio::task::yield_now().await;
        }
    }

    pub(crate) fn try_recv(&mut self) -> Option<Value> {
        self.messages.try_recv().ok()
    }

    pub(crate) async fn settle(&self) {
        for _ in 0..30 {
            tokio::task::yield_now().await;
        }
    }
}

impl Drop for TestClient {
    fn drop(&mut self) {
        self.scheduler.next_generation(&self.registry);
        self.server.abort();
        self.reader.abort();
    }
}

struct ChannelReader {
    input: mpsc::UnboundedReceiver<Vec<u8>>,
    buffer: Vec<u8>,
}
impl AsyncRead for ChannelReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        output: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        while self.buffer.is_empty() {
            match self.input.poll_recv(cx) {
                Poll::Ready(Some(chunk)) => self.buffer = chunk,
                Poll::Ready(None) => return Poll::Ready(Ok(())),
                Poll::Pending => return Poll::Pending,
            }
        }
        let count = output.remaining().min(self.buffer.len());
        output.put_slice(&self.buffer[..count]);
        self.buffer.drain(..count);
        Poll::Ready(Ok(()))
    }
}
struct ChannelWriter {
    output: mpsc::UnboundedSender<Vec<u8>>,
}
impl AsyncWrite for ChannelWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        Poll::Ready(
            self.output
                .send(bytes.to_vec())
                .map(|_| bytes.len())
                .map_err(|_| io::ErrorKind::BrokenPipe.into()),
        )
    }
    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
