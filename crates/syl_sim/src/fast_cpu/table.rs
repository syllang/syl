//! A small two-input priority select can fit its entire truth table in one
//! machine immediate. Enumerate the original word program to preserve widths,
//! signed operations, and source priority exactly.
use super::{
    kernel::{self, FastOp},
    mask,
};
use std::{
    collections::BTreeMap,
    ops::{Index, IndexMut},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Table {
    pub a_bits: u32,
    pub input_mask: u64,
    pub stride: u32,
    pub values: u64,
    pub output_mask: u64,
}

impl Table {
    pub fn get(self, x: u64, y: u64) -> u64 {
        let index = ((x & mask(self.a_bits)) | (y << self.a_bits)) & self.input_mask;
        (self.values >> (index << self.stride)) & self.output_mask
    }
}

// Compilation visits only the slots used by this expression, independent of
// the size of the DUT's RAM.
#[derive(Default)]
struct Words(BTreeMap<usize, u64>);
impl Index<usize> for Words {
    type Output = u64;
    fn index(&self, i: usize) -> &u64 {
        self.0.get(&i).unwrap_or(&0)
    }
}
impl IndexMut<usize> for Words {
    fn index_mut(&mut self, i: usize) -> &mut u64 {
        self.0.entry(i).or_default()
    }
}

pub(super) fn compile(
    ops: &[FastOp],
    inputs: [(usize, u32); 2],
    result: usize,
    width: u32,
) -> Option<Table> {
    let [(a, a_bits), (b, b_bits)] = inputs;
    let stride = width.next_power_of_two().ilog2();
    if a_bits.checked_add(b_bits)?.checked_add(stride)? > 6
        || ops.iter().any(|op| matches!(op, FastOp::Gather { .. }))
    {
        return None;
    }
    let input_mask = mask(a_bits + b_bits);
    let output_mask = mask(width);
    let mut values = 0;
    for i in 0..=input_mask {
        let mut words = Words::default();
        words[a] = i & mask(a_bits);
        words[b] = i >> a_bits;
        kernel::scalar_words(ops, &mut words, false);
        values |= (words[result] & output_mask) << (i << stride);
    }
    Some(Table {
        a_bits,
        input_mask,
        stride,
        values,
        output_mask,
    })
}
