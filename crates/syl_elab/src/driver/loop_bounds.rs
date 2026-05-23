use crate::{
    driver_place::{DriverBound, DriverExpr},
    eir_expr::EirBound,
    eir_guard::{EirGuard, EirGuardFrame},
};

#[non_exhaustive]
pub(super) struct DriverLoopGuard<'a> {
    guard: &'a EirGuard,
}

impl<'a> DriverLoopGuard<'a> {
    pub(super) fn new(guard: &'a EirGuard) -> Self {
        Self { guard }
    }

    pub(super) fn single_loop_bounds(&self) -> Option<DriverLoopBounds<'a>> {
        self.single_loop_context()
            .map(DriverLoopContext::into_bounds)
    }

    pub(super) fn single_loop_context(&self) -> Option<DriverLoopContext<'a>> {
        DriverLoopFrames::new(self.guard.frames()).single_loop_context()
    }
}

#[non_exhaustive]
struct DriverLoopFrames<'a> {
    frames: &'a [EirGuardFrame],
}

impl<'a> DriverLoopFrames<'a> {
    fn new(frames: &'a [EirGuardFrame]) -> Self {
        Self { frames }
    }

    fn single_loop_context(&self) -> Option<DriverLoopContext<'a>> {
        let mut loop_frame = None;
        let mut residual = Vec::new();
        for frame in self.frames {
            if matches!(frame, EirGuardFrame::Loop { .. }) {
                if loop_frame.replace(frame).is_some() {
                    return None;
                }
            } else {
                residual.push(frame.clone());
            }
        }
        let Some(EirGuardFrame::Loop {
            index, start, end, ..
        }) = loop_frame
        else {
            return None;
        };
        Some(DriverLoopContext::new(
            DriverLoopBounds::new(index, start, end),
            EirGuard::from_frames(&residual),
        ))
    }
}

#[non_exhaustive]
pub(super) struct DriverLoopContext<'a> {
    bounds: DriverLoopBounds<'a>,
    residual: EirGuard,
}

impl<'a> DriverLoopContext<'a> {
    fn new(bounds: DriverLoopBounds<'a>, residual: EirGuard) -> Self {
        Self { bounds, residual }
    }

    pub(super) fn bounds(&self) -> &DriverLoopBounds<'a> {
        &self.bounds
    }

    pub(super) fn into_bounds(self) -> DriverLoopBounds<'a> {
        self.bounds
    }

    pub(super) fn into_residual_guard(self) -> EirGuard {
        self.residual
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(super) struct DriverLoopBounds<'a> {
    loop_index: &'a str,
    start: DriverBound,
    end: DriverBound,
}

impl<'a> DriverLoopBounds<'a> {
    fn new(loop_index: &'a str, start: &EirBound, end: &EirBound) -> Self {
        Self {
            loop_index,
            start: DriverBound::from_eir_bound(start),
            end: DriverBound::from_eir_bound(end),
        }
    }

    pub(super) fn index_matches(&self, index: &DriverExpr) -> bool {
        self.starts_at_zero() && matches!(index, DriverExpr::Ident(name) if name == self.loop_index)
    }

    pub(super) fn covers_bit_array(&self, width: &DriverBound) -> bool {
        self.starts_at_zero() && width.has_same_formula(&self.end)
    }

    pub(super) fn covers_part_array(&self, width: &DriverBound, part_width: &DriverBound) -> bool {
        self.starts_at_zero() && width.has_product_formula(&self.end, part_width)
    }

    fn starts_at_zero(&self) -> bool {
        self.start.value() == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::DriverLoopGuard;
    use crate::{
        driver_place::{DriverBound, DriverExpr},
        eir_guard::{EirGuard, EirGuardFrame},
    };
    use syl_span::Span;

    #[test]
    fn single_loop_bounds_accept_symbolic_part_array() {
        let frame = EirGuardFrame::loop_frame("gen_i", "i", "0", "N", Span::default());
        let guard = EirGuard::from_frames(&[frame]);
        let bounds = DriverLoopGuard::new(&guard)
            .single_loop_bounds()
            .expect("single loop guard must expose symbolic bounds");

        assert!(bounds.index_matches(&DriverExpr::Ident("i".to_string())));
        assert!(bounds.covers_part_array(&DriverBound::new("N * 8"), &DriverBound::new("8")));
    }

    #[test]
    fn single_loop_context_keeps_branch_residual() {
        let frames = [
            EirGuardFrame::loop_frame("gen_i", "i", "0", "N", Span::default()),
            EirGuardFrame::if_then("branch", Span::default()),
        ];
        let guard = EirGuard::from_frames(&frames);
        let context = DriverLoopGuard::new(&guard)
            .single_loop_context()
            .expect("one loop plus branch context should expose symbolic bounds");

        assert!(
            context
                .bounds()
                .index_matches(&DriverExpr::Ident("i".to_string()))
        );
        assert_eq!(context.into_residual_guard().frames().len(), 1);
    }

    #[test]
    fn single_loop_bounds_reject_branch_context() {
        let frame = EirGuardFrame::if_then("branch", Span::default());
        let guard = EirGuard::from_frames(&[frame]);

        assert!(DriverLoopGuard::new(&guard).single_loop_bounds().is_none());
    }
}
