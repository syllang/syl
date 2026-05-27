use super::{CreateFact, DriveFact, DriverCellSummary, ReadFact};
use crate::eir::{EirExpansion, EirOrigin};
use std::collections::BTreeMap;
use syl_span::SourceId;

#[non_exhaustive]
pub(super) struct CellSummaryCollector<'a> {
    drives: &'a [DriveFact],
    reads: &'a [ReadFact],
    creates: &'a [CreateFact],
    summaries: BTreeMap<CellSummaryKey, DriverCellSummary>,
}

impl<'a> CellSummaryCollector<'a> {
    pub(super) fn new(
        drives: &'a [DriveFact],
        reads: &'a [ReadFact],
        creates: &'a [CreateFact],
    ) -> Self {
        Self {
            drives,
            reads,
            creates,
            summaries: BTreeMap::new(),
        }
    }

    pub(super) fn collect(mut self) -> Self {
        for drive in self.drives {
            if let Some(summary) = self.summary_for_origin(drive.origin()) {
                summary.add_drive(drive.target_place().clone());
            }
        }
        for read in self.reads {
            if let Some(summary) = self.summary_for_origin(read.origin()) {
                summary.add_read(read.source_place().clone());
            }
        }
        for create in self.creates {
            if let Some(summary) = self.summary_for_origin(create.origin()) {
                summary.add_create(create.name());
            }
        }
        self
    }

    pub(super) fn finish(self) -> Vec<DriverCellSummary> {
        self.summaries.into_values().collect()
    }

    fn summary_for_origin(&mut self, origin: &EirOrigin) -> Option<&mut DriverCellSummary> {
        let key = CellSummaryKey::from_origin(origin)?;
        let expansion = origin.expansion_stack().last()?;
        // Opaque/precompiled cells will eventually need loaded summaries here. Until then, only cells
        // with a concrete expansion stack are safe to attribute.
        Some(self.summaries.entry(key).or_insert_with(|| {
            DriverCellSummary::new(expansion.callable(), expansion.instance(), origin.clone())
        }))
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CellSummaryKey {
    stack: Vec<CellSummaryFrameKey>,
}

impl CellSummaryKey {
    fn from_origin(origin: &EirOrigin) -> Option<Self> {
        let stack = origin
            .expansion_stack()
            .iter()
            .map(CellSummaryFrameKey::from_expansion)
            .collect::<Vec<_>>();
        if stack.is_empty() {
            None
        } else {
            Some(Self { stack })
        }
    }
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct CellSummaryFrameKey {
    callable: String,
    instance: String,
    source: SourceId,
    start: usize,
    end: usize,
}

impl CellSummaryFrameKey {
    fn from_expansion(expansion: &EirExpansion) -> Self {
        let span = expansion.span();
        Self {
            callable: expansion.callable().to_string(),
            instance: expansion.instance().to_string(),
            source: span.source,
            start: span.start,
            end: span.end,
        }
    }
}
