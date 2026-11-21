#include "common/sim_harness.hpp"
#include <queue>
#include <random>

#ifdef TEST_STREAM
#include "VStreamSyncFifo.h"
using Dut = VStreamSyncFifo;
#else
#include "VSyncFifo.h"
using Dut = VSyncFifo;
#endif

#ifndef TEST_DEPTH
#define TEST_DEPTH 8
#endif

int main() {
    constexpr unsigned depth = TEST_DEPTH;
    Dut dut;
    dut.clk = 0;
    dut.rst = 0;
    std::queue<std::uint8_t> expected;
    auto check = [&] {
        dut.eval();
#ifdef TEST_STREAM
        SIM_CHECK_EQ(dut.up_ready, expected.size() < depth && !dut.rst, "input ready");
        SIM_CHECK_EQ(dut.down_valid, !expected.empty() && !dut.rst, "output valid");
        SIM_CHECK_EQ(dut.down_payload, expected.empty() ? 0 : expected.front(), "FWFT word");
#else
        SIM_CHECK_EQ(dut.full, expected.size() == depth, "full flag");
        SIM_CHECK_EQ(dut.empty, expected.empty(), "empty flag");
        SIM_CHECK_EQ(dut.count, expected.size(), "occupancy");
        SIM_CHECK_EQ(dut.rdata, expected.empty() ? 0 : expected.front(), "FWFT word");
#endif
    };
    auto inputs = [&](bool push, bool pop, std::uint8_t data) {
#ifdef TEST_STREAM
        dut.up_valid = push;
        dut.down_ready = pop;
        dut.up_payload = data;
#else
        dut.push = push;
        dut.pop = pop;
        dut.wdata = data;
#endif
    };
    inputs(false, false, 0);
    sim::apply_sync_reset(dut);
    check();
    unsigned writes = 0, reads = 0, both = 0, full_writes = 0, empty_reads = 0;
    auto step = [&](bool push, bool pop, std::uint8_t data) {
        inputs(push, pop, data);
        check();
        // Decide both acceptances before mutating the reference queue.
        bool accept_push = push && expected.size() < depth;
        bool accept_pop = pop && !expected.empty();
        if (push && !accept_push) ++full_writes;
        if (pop && !accept_pop) ++empty_reads;
        if (accept_push && accept_pop) ++both;
        if (accept_pop) {
            expected.pop();
            ++reads;
        }
        if (accept_push) {
            expected.push(data);
            ++writes;
        }
        sim::tick(dut, dut.clk);
        check();
        return accept_push;
    };

    for (unsigned i = 0; i < 3; ++i) step(false, true, 0); // underflow
    for (unsigned i = 0; i < depth; ++i) step(true, false, i * 17 + 5);
    for (unsigned i = 0; i < 3; ++i) step(true, false, 0xee); // overflow
    step(true, true, 0xa5); // full: only pop succeeds
    while (!expected.empty()) step(false, true, 0);
    step(true, true, 0x63); // empty: only push succeeds, no bypass
    step(false, true, 0);
    for (unsigned i = 0; i < depth / 2; ++i) step(true, false, i + 0x20);
    for (unsigned i = 0; i < 64; ++i) step(true, true, i + 0x40);
    while (!expected.empty()) step(false, true, 0);

    std::mt19937 random(0xf1f03b);
    bool stalled = false;
    std::uint8_t held_data = 0;
    for (unsigned i = 0; i < 512; ++i) {
        // Bias alternating blocks toward full, empty, and concurrent traffic.
        unsigned phase = (i / 64) % 3;
        bool push = stalled || random() % 4 < (phase == 0 ? 3u : 2u);
        bool pop = random() % 4 < (phase == 1 ? 3u : 2u);
        if (!stalled) held_data = random();
        stalled = !step(push, pop, held_data) && push;
    }
    if (stalled) {
        while (!step(true, true, held_data)) {}
    }
    while (!expected.empty()) step(false, true, 0);
    step(false, true, 0);
    SIM_CHECK(writes > 8 * depth && reads == writes, "multiple complete pointer wraps");
    SIM_CHECK(both >= 64, "sustained simultaneous read/write coverage");
    SIM_CHECK(full_writes >= 4 && empty_reads >= 4, "both boundary protections exercised");

    // Reset a nonempty FIFO while both operations are requested.
    for (unsigned i = 0; i < depth / 2; ++i) step(true, false, 0x70 + i);
    inputs(true, true, 0xff);
    dut.rst = 1;
    sim::tick(dut, dut.clk);
    expected = {};
    check();
    inputs(false, false, 0);
    dut.rst = 0;
    check();
    for (unsigned i = 0; i < depth; ++i) step(true, false, 0x80 + i);
    while (!expected.empty()) step(false, true, 0);
    dut.final();
}
