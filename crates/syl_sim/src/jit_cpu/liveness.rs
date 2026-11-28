//! Walk definitions backwards across a complete phase. Bit masks keep a
//! part-select merge from making unrelated parts of a word live.
use crate::fast_cpu::kernel::FastOp;

pub(super) fn trace<'a>(
    ops: impl DoubleEndedIterator<Item = &'a FastOp>,
    live: &mut [u64],
) -> Vec<FastOp> {
    let mut selected = Vec::new();
    for &op in ops.rev() {
        let mut op = op;
        let need = if let Some(dst) = op.dst() {
            let bits = std::mem::take(&mut live[dst]);
            if bits == 0 {
                continue;
            }
            bits
        } else {
            u64::MAX
        };
        match &mut op {
            FastOp::Copy(_, s) => live[*s] |= need,
            FastOp::Bits {
                dst,
                source,
                shift,
                mask,
                merge,
            } => {
                let copied = need & *mask;
                let retained = need & !*mask;
                *merge &= retained != 0;
                if *merge {
                    live[*dst] |= retained;
                }
                live[*source] |= if *shift >= 0 {
                    copied >> *shift
                } else {
                    copied << -*shift
                };
            }
            FastOp::Stop => continue,
            _ => {
                for slot in op.operands().into_iter().flatten() {
                    live[slot] = u64::MAX;
                }
            }
        }
        selected.push(op);
    }
    selected.reverse();
    selected
}
