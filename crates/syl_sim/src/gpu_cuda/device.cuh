#include <algorithm>
#include <cerrno>
#include <climits>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <cuda_runtime.h>
#include <vector>
using Word = unsigned long long;
__device__ __forceinline__ Word mask(unsigned w) {
  return w == 64 ? ~0ull : ((1ull << w) - 1);
}
// Bit offset selects one or two consecutive 64-bit SoA slots.
struct Frame {
  Word *state, *mem, *failure;
  unsigned b, batch;
  __device__ Word &word(unsigned slot) const {
    return state[size_t(slot) * batch + b];
  }
  __device__ Word read(unsigned slot, unsigned low, unsigned w) const {
    slot += low / 64;
    low %= 64;
    Word v = word(slot) >> low;
    if (low + w > 64)
      v |= word(slot + 1) << (64 - low);
    return v & mask(w);
  }
  __device__ void put(unsigned slot, unsigned low, unsigned w, Word v) const {
    slot += low / 64;
    low %= 64;
    Word field = mask(w);
    word(slot) = (word(slot) & ~(field << low)) | ((v & field) << low);
    if (low + w > 64) {
      Word high = field >> (64 - low);
      word(slot + 1) = (word(slot + 1) & ~high) | ((v >> (64 - low)) & high);
    }
  }
  __device__ Word read_mem(size_t base, unsigned depth, Word addr) const {
    return addr < depth ? mem[(base + addr) * batch + b] : 0;
  }
};
__device__ Word stimulus(unsigned batch, unsigned cycle, unsigned slot) {
  Word z =
      ((Word(batch) << 32) | cycle) + 0x9e3779b97f4a7c15ull * (slot + 1ull);
  z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ull;
  z = (z ^ (z >> 27)) * 0x94d049bb133111ebull;
  return z ^ (z >> 31);
}
