/// Implementation strategy promised by a CDC trust boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryCdcKind {
    TwoFlopSync,
    Handshake,
    AsyncFifo,
    Custom,
}

/// One input-to-output channel synchronized to the named destination clock.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SummaryCdcCrossing {
    input_port: String,
    output_port: String,
    dst_clock: String,
    kind: SummaryCdcKind,
}
impl SummaryCdcCrossing {
    pub fn new(
        input_port: impl Into<String>,
        output_port: impl Into<String>,
        dst_clock: impl Into<String>,
    ) -> Self {
        Self {
            input_port: input_port.into(),
            output_port: output_port.into(),
            dst_clock: dst_clock.into(),
            kind: SummaryCdcKind::TwoFlopSync,
        }
    }
    pub fn with_kind(mut self, kind: SummaryCdcKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn input_port(&self) -> &str {
        &self.input_port
    }
    pub fn output_port(&self) -> &str {
        &self.output_port
    }
    pub fn dst_clock(&self) -> &str {
        &self.dst_clock
    }
    pub fn kind(&self) -> SummaryCdcKind {
        self.kind
    }
}

pub(super) fn source_crossings(attrs: &[syl_hir::HirAttribute]) -> Vec<SummaryCdcCrossing> {
    attrs
        .iter()
        .filter(|a| a.name == "cdc_sync")
        .filter_map(|a| {
            let names = a
                .args
                .iter()
                .map(|e| {
                    if let syl_hir::HirExprNode::Ident(n) = &e.node {
                        Some(n.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>()?;
            let [input, output, clock] = names.as_slice() else {
                return None;
            };
            Some(SummaryCdcCrossing::new(*input, *output, *clock))
        })
        .collect()
}
