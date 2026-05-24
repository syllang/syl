use super::{
    OpaqueItemSummary,
    collect::{collect_extern_summary, collect_source_cell_summary},
};
use crate::{
    facts::{CapabilityTable, ProtocolFacts, TypeTable},
    tir::TirDesign,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpaqueSummaryTable {
    values: BTreeMap<String, OpaqueItemSummary>,
}

impl OpaqueSummaryTable {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn register(&mut self, summary: OpaqueItemSummary) {
        let callable = summary.callable().to_string();
        if let Some(existing) = self.values.get(&callable) {
            self.values.insert(callable, existing.merged_with(&summary));
            return;
        }
        self.values.insert(callable, summary);
    }

    pub fn get(&self, callable: &str) -> Option<&OpaqueItemSummary> {
        self.values.get(callable)
    }

    pub fn summaries(&self) -> impl Iterator<Item = &OpaqueItemSummary> {
        self.values.values()
    }

    pub fn merged(&self, overlay: &Self) -> Self {
        let mut merged = self.clone();
        merged.merge_from(overlay);
        merged
    }

    pub fn merge_from(&mut self, other: &Self) {
        for summary in other.summaries().cloned() {
            self.register(summary);
        }
    }

    pub(crate) fn empty() -> Self {
        Self::new()
    }

    pub(crate) fn collect(
        tir: &TirDesign,
        types: &TypeTable,
        capabilities: &CapabilityTable,
        protocols: &ProtocolFacts,
    ) -> Self {
        let mut table = Self::new();
        for callable in tir.hir().callables.values() {
            match callable {
                crate::hir::HirCallable::Cell(item) => {
                    table.register(collect_source_cell_summary(
                        tir,
                        types,
                        capabilities,
                        protocols,
                        item,
                    ));
                }
                crate::hir::HirCallable::Extern(item) => {
                    table.register(collect_extern_summary(
                        tir,
                        types,
                        capabilities,
                        protocols,
                        item,
                    ));
                }
                crate::hir::HirCallable::Module(_) => {}
                _ => {}
            }
        }
        table
    }
}

impl Extend<OpaqueItemSummary> for OpaqueSummaryTable {
    fn extend<T: IntoIterator<Item = OpaqueItemSummary>>(&mut self, iter: T) {
        for summary in iter {
            self.register(summary);
        }
    }
}

impl FromIterator<OpaqueItemSummary> for OpaqueSummaryTable {
    fn from_iter<T: IntoIterator<Item = OpaqueItemSummary>>(iter: T) -> Self {
        let mut table = Self::new();
        table.extend(iter);
        table
    }
}
