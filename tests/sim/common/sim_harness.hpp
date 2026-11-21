#pragma once
#include <verilated.h>
#include <cstdint>
#include <cstdlib>
#include <iostream>

namespace sim {
inline std::uint64_t cycle = 0;

template<class A, class B>
void check_eq(A actual, B expected, const char* message, const char* file, int line) {
    if (actual == expected) return;
    std::cerr << file << ':' << line << " cycle " << cycle << ": " << message
              << " expected " << +expected << ", actual " << +actual << '\n';
    std::exit(EXIT_FAILURE);
}

template<class Dut, class Clock>
void tick(Dut& dut, Clock& clock) {
    clock = 0;
    dut.eval();
    clock = 1;
    dut.eval();
    clock = 0;
    dut.eval();
    ++cycle;
}

template<class Dut>
void apply_sync_reset(Dut& dut, unsigned cycles = 2) {
    dut.rst = 1;
    for (unsigned i = 0; i < cycles; ++i) tick(dut, dut.clk);
    dut.rst = 0;
    dut.eval();
}

template<class Dut>
void apply_async_low_reset(Dut& dut) {
    dut.dst_clk = 0;
    dut.dst_rst_n = 1;
    dut.eval();
    dut.dst_rst_n = 0;
    dut.eval();
    dut.dst_rst_n = 1;
    dut.eval();
}
}

#define SIM_CHECK_EQ(a, e, msg) sim::check_eq((a), (e), (msg), __FILE__, __LINE__)
#define SIM_CHECK(cond, msg) SIM_CHECK_EQ(bool(cond), true, msg)
