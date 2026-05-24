use super::{AstNodeId, AstNodeKind, AstNodeRecord};
use syl_span::{SourceRange, Span};

const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
const FNV_PRIME: u64 = 1_099_511_628_211;

#[derive(Clone, Debug)]
pub(super) struct PendingNode {
    kind: AstNodeKind,
    span: Span,
    range: SourceRange,
    parent: Option<usize>,
    local_seed: u64,
}

impl PendingNode {
    pub(super) fn new(
        kind: AstNodeKind,
        span: Span,
        range: SourceRange,
        parent: Option<usize>,
        local_seed: u64,
    ) -> Self {
        Self {
            kind,
            span,
            range,
            parent,
            local_seed,
        }
    }
}

pub(super) fn finalize_nodes(
    root: usize,
    nodes: Vec<PendingNode>,
) -> (AstNodeId, Vec<AstNodeRecord>) {
    let mut children = vec![Vec::new(); nodes.len()];
    for (idx, node) in nodes.iter().enumerate() {
        if let Some(parent) = node.parent {
            children[parent].push(idx);
        }
    }

    let mut descriptors = vec![0; nodes.len()];
    descriptors[root] = descriptor_hash(nodes[root].kind, nodes[root].local_seed, None, None, 1);
    for siblings in &children {
        assign_sibling_descriptors(siblings, &nodes, &mut descriptors);
    }

    let mut path_ids = vec![AstNodeId::new(1); nodes.len()];
    for (idx, node) in nodes.iter().enumerate() {
        let parent = node.parent.map(|parent| path_ids[parent].get());
        path_ids[idx] = path_hash(parent, descriptors[idx]);
    }

    let root_id = path_ids[root];
    let records = nodes
        .into_iter()
        .enumerate()
        .map(|(idx, node)| {
            AstNodeRecord::new(
                path_ids[idx],
                node.kind,
                node.span,
                node.range,
                node.parent.map(|parent| path_ids[parent]),
            )
        })
        .collect();
    (root_id, records)
}

pub(super) fn local_seed_kind() -> u64 {
    StableHasher::new("kind").finish()
}

pub(super) fn local_seed_tag(tag: &str) -> u64 {
    let mut hasher = StableHasher::new("tag");
    hasher.write_str(tag);
    hasher.finish()
}

pub(super) fn local_seed_name(name: &str) -> u64 {
    let mut hasher = StableHasher::new("name");
    hasher.write_str(name);
    hasher.finish()
}

pub(super) fn local_seed_named_tag(tag: &str, name: &str) -> u64 {
    let mut hasher = StableHasher::new("named_tag");
    hasher.write_str(tag);
    hasher.write_str(name);
    hasher.finish()
}

pub(super) fn local_seed_path(path: &[String]) -> u64 {
    let mut hasher = StableHasher::new("path");
    for segment in path {
        hasher.write_str(segment);
    }
    hasher.finish()
}

pub(super) fn local_seed_int(value: u64) -> u64 {
    let mut hasher = StableHasher::new("int");
    hasher.write_u64(value);
    hasher.finish()
}

pub(super) fn local_seed_bool(value: bool) -> u64 {
    let mut hasher = StableHasher::new("bool");
    hasher.write_bool(value);
    hasher.finish()
}

pub(super) fn local_seed_text(text: &str) -> u64 {
    let mut hasher = StableHasher::new("text");
    hasher.write_str(text);
    hasher.finish()
}

fn assign_sibling_descriptors(siblings: &[usize], nodes: &[PendingNode], descriptors: &mut [u64]) {
    let mut runs = Vec::new();
    let mut start = 0;
    while start < siblings.len() {
        let node = &nodes[siblings[start]];
        let identity = identity_hash(node.kind, node.local_seed);
        let mut end = start.saturating_add(1);
        while end < siblings.len() {
            let next = &nodes[siblings[end]];
            let next_identity = identity_hash(next.kind, next.local_seed);
            if next_identity != identity {
                break;
            }
            end = end.saturating_add(1);
        }
        runs.push(SiblingRun {
            start,
            end,
            identity,
        });
        start = end;
    }

    for (run_idx, run) in runs.iter().enumerate() {
        let prev_identity = run_idx.checked_sub(1).map(|idx| runs[idx].identity);
        let next_identity = runs.get(run_idx.saturating_add(1)).map(|run| run.identity);
        let mut ordinal = 1usize;
        for pos in run.start..run.end {
            let node_idx = siblings[pos];
            let node = &nodes[node_idx];
            descriptors[node_idx] = descriptor_hash(
                node.kind,
                node.local_seed,
                prev_identity,
                next_identity,
                ordinal,
            );
            ordinal = ordinal.saturating_add(1);
        }
    }
}

fn identity_hash(kind: AstNodeKind, local_seed: u64) -> u64 {
    let mut hasher = StableHasher::new("identity");
    hasher.write_kind(kind);
    hasher.write_u64(local_seed);
    hasher.finish()
}

fn descriptor_hash(
    kind: AstNodeKind,
    local_seed: u64,
    prev_identity: Option<u64>,
    next_identity: Option<u64>,
    ordinal: usize,
) -> u64 {
    let mut hasher = StableHasher::new("descriptor");
    hasher.write_kind(kind);
    hasher.write_u64(local_seed);
    hasher.write_option_u64(prev_identity);
    hasher.write_option_u64(next_identity);
    let ordinal = u64::try_from(ordinal).unwrap_or(u64::MAX);
    hasher.write_u64(ordinal);
    hasher.finish()
}

fn path_hash(parent: Option<u64>, descriptor: u64) -> AstNodeId {
    let mut hasher = StableHasher::new("path");
    hasher.write_option_u64(parent);
    hasher.write_u64(descriptor);
    AstNodeId::new(hasher.finish())
}

#[derive(Clone, Copy, Debug)]
struct SiblingRun {
    start: usize,
    end: usize,
    identity: u64,
}

#[derive(Clone, Copy, Debug)]
struct StableHasher {
    hash: u64,
}

impl StableHasher {
    fn new(domain: &str) -> Self {
        let mut hasher = Self { hash: FNV_OFFSET };
        hasher.write_str(domain);
        hasher
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u8(if value { 1 } else { 0 });
    }

    fn write_kind(&mut self, kind: AstNodeKind) {
        self.write_str(<&'static str>::from(kind));
    }

    fn write_option_u64(&mut self, value: Option<u64>) {
        match value {
            Some(value) => {
                self.write_bool(true);
                self.write_u64(value);
            }
            None => {
                self.write_bool(false);
            }
        }
    }

    fn write_str(&mut self, value: &str) {
        for byte in value.bytes() {
            self.write_u8(byte);
        }
        self.write_u8(0xff);
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.write_u8(byte);
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.hash ^= u64::from(value);
        self.hash = self.hash.wrapping_mul(FNV_PRIME);
    }

    fn finish(self) -> u64 {
        self.hash
    }
}
