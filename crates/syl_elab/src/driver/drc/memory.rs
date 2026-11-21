use super::guard::DriverGuardSet;
use crate::{
    CompileError, DriverError,
    eir::{EirBound, EirDesign, EirGuard, EirGuardFrame, EirItem, EirModule, EirOrigin},
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct MemoryPortDrcChecker<'a> {
    eir: &'a EirDesign,
}
impl<'a> MemoryPortDrcChecker<'a> {
    pub(super) fn new(eir: &'a EirDesign) -> Self {
        Self { eir }
    }
    pub(super) fn collect_errors(&self) -> Vec<CompileError> {
        let mut errors = Vec::new();
        for module in self.eir.modules() {
            self.check_module(module, &mut errors);
        }
        errors
    }
    fn check_module(&self, module: &EirModule, errors: &mut Vec<CompileError>) {
        let mut items = Vec::new();
        guarded_items(module.items(), &mut Vec::new(), &mut items);
        let names = items
            .iter()
            .filter_map(|(i, _)| {
                if let EirItem::MemoryDecl { name, .. } = i {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        let mut writers = BTreeMap::<&str, Vec<(&EirOrigin, &EirGuard)>>::new();
        for (item, guard) in &items {
            match item {
                EirItem::MemoryWrite { memory, origin, .. }
                | EirItem::MemorySyncRead { memory, origin, .. } => {
                    if !names.contains(memory.as_str()) {
                        errors.push(CompileError::driver_error(
                            DriverError::UnknownHardwareObject {
                                module: module.name().into(),
                                name: memory.clone(),
                            },
                            origin.span(),
                        ));
                        continue;
                    }
                    if matches!(item, EirItem::MemoryWrite { .. }) {
                        let previous = writers.entry(memory).or_default();
                        if let Some((old_origin, _)) = previous.iter().find(|(_, old_guard)| {
                            !DriverGuardSet::new(old_guard, guard).is_mutually_exclusive()
                        }) {
                            errors.push(CompileError::driver_error_with_related(
                                DriverError::DuplicateMemoryWritePort {
                                    name: memory.clone(),
                                },
                                origin.span(),
                                vec![
                                    (old_origin.span(), "previous memory write port".into()),
                                    (origin.span(), "conflicting memory write port".into()),
                                ],
                            ));
                        }
                        previous.push((origin, guard));
                    }
                }
                _ => {}
            }
        }
    }
}

fn guarded_items<'a>(
    items: &'a [EirItem],
    frames: &mut Vec<EirGuardFrame>,
    out: &mut Vec<(&'a EirItem, EirGuard)>,
) {
    for item in items {
        match item {
            EirItem::CellExpansion(e) => guarded_items(e.items(), frames, out),
            EirItem::SymbolicStaticIf {
                label,
                origin,
                then_items,
                else_items,
                ..
            } => {
                frames.push(EirGuardFrame::if_then(label, origin.span()));
                guarded_items(then_items, frames, out);
                frames.pop();
                frames.push(EirGuardFrame::if_else(label, origin.span()));
                guarded_items(else_items, frames, out);
                frames.pop();
            }
            EirItem::SymbolicStaticFor {
                index,
                label,
                start,
                end,
                origin,
                items,
            } => {
                frames.push(EirGuardFrame::loop_frame(
                    label,
                    index,
                    EirBound::from_expr(start.clone()),
                    EirBound::from_expr(end.clone()),
                    origin.span(),
                ));
                guarded_items(items, frames, out);
                frames.pop();
            }
            _ => out.push((item, EirGuard::from_frames(frames))),
        }
    }
}

#[cfg(test)]
mod tests;
