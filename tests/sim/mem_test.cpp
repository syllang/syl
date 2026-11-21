#include "common/sim_harness.hpp"
#include <array>
#include <random>

#if defined(TEST_SINGLE_PORT)
#include "VSinglePortRam.h"
using Dut = VSinglePortRam;
#elif defined(TEST_DISTRIBUTED)
#include "VDistributedRam.h"
using Dut = VDistributedRam;
#else
#include "VSimpleDualPortRam.h"
using Dut = VSimpleDualPortRam;
#endif

int main() {
    Dut dut;
    dut.clk = 0;
    std::array<std::uint8_t, 16> words{};
    std::array<bool, 16> initialized{};
    std::uint8_t read_value = 0;
    bool read_valid = false;
    auto step = [&](bool enabled, bool write, bool read, unsigned wa, unsigned ra,
                    std::uint8_t data) {
        bool we = enabled && write;
        bool re = enabled && read;
#ifdef TEST_SINGLE_PORT
        dut.en = enabled;
        dut.we = write;
        dut.addr = write ? wa : ra;
        // Single-port reads are selected by en && !we.
        re = enabled && !write;
#else
        dut.we = we;
        dut.waddr = wa;
        dut.raddr = ra;
#ifndef TEST_DISTRIBUTED
        dut.re = re;
#endif
#endif
        dut.wdata = data;
        dut.eval();
#ifdef TEST_DISTRIBUTED
        if (initialized[ra]) {
            SIM_CHECK_EQ(dut.rdata, words[ra], "address changes read without a clock");
        }
#else
        if (read_valid) {
            SIM_CHECK_EQ(dut.rdata, read_value, "synchronous read holds before edge");
        }
        if (re && initialized[ra]) {
            read_value = words[ra];
            read_valid = true;
        }
#endif
        if (we) {
            words[wa] = data;
            initialized[wa] = true;
        }
        sim::tick(dut, dut.clk);
#ifdef TEST_DISTRIBUTED
        if (initialized[ra]) {
            SIM_CHECK_EQ(dut.rdata, words[ra], "combinational read after write edge");
        }
#else
        if (read_valid) {
            SIM_CHECK_EQ(dut.rdata, read_value, "one-edge read or disabled read hold");
        }
#endif
    };

    for (unsigned a = 0; a < 16; ++a) step(true, true, false, a, 0, a * 17 + 5);
    for (unsigned a = 0; a < 16; ++a) step(true, false, true, 0, a, 0);
    for (unsigned a = 0; a < 16; ++a) {
        step(false, true, true, a, 15 - a, 0xff);
        step(true, false, true, 0, a, 0);
    }
    // Every address is overwritten, then revisited in a different permutation.
    for (unsigned a = 0; a < 16; ++a) step(true, true, false, a, 0, a * 29 + 11);
    for (unsigned a = 0; a < 16; ++a) step(true, false, true, 0, (a * 7) % 16, 0);
#ifndef TEST_SINGLE_PORT
    for (unsigned a = 0; a < 16; ++a) {
        step(true, true, true, a, (a + 3) % 16, a * 13 + 7);
        step(true, true, false, a, (a + 5) % 16, a * 19 + 3);
    }
    // Same-address simulation semantics are read before write for sync RAM.
    step(true, true, true, 6, 6, 0x92);
    step(true, false, true, 0, 6, 0);
#endif
    std::mt19937 random(0x3b02);
    for (unsigned i = 0; i < 256; ++i) {
        bool enabled = random() % 4 != 0;
        bool write = random() & 1;
        bool read = random() & 1;
        unsigned wa = random() % 16;
        unsigned ra = random() % 16;
        std::uint8_t data = random();
        step(enabled, write, read, wa, ra, data);
    }
    for (unsigned a = 0; a < 16; ++a) step(true, false, true, 0, a, 0);
    dut.final();
}
