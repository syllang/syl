#include "Vdut.h"
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
using Clock = std::chrono::steady_clock;
static uint64_t random_word(uint64_t cycle, uint64_t word) {
    uint64_t z = cycle + 0x9e3779b97f4a7c15ULL * (word + 1);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    return z ^ (z >> 31);
}
int main(int argc, char **argv) {
    if (argc != 2) return 2;
    const auto cycles = std::strtoull(argv[1], nullptr, 10);
    const auto cold = Clock::now();
    VerilatedContext context;
    context.threads(1);
    Vdut d(&context);
    d.clk = 0;
    d.eval();
    const auto hot = Clock::now();
    for (uint64_t cycle = 1; cycle <= cycles; ++cycle) {
        @DRIVE@
        d.clk = 0;
        d.eval();
        d.clk = 1;
        d.eval();
    }
    d.clk = 0;
    d.eval();
    uint64_t sum = 0;
    @DIGEST@
    const auto end = Clock::now();
    using Ms = std::chrono::duration<double, std::milli>;
    std::printf("{\"cold_ms\":%.9f,\"hot_ms\":%.9f,\"digest\":%llu}\n",
                Ms(hot-cold).count(), Ms(end-hot).count(), (unsigned long long)sum);
}
