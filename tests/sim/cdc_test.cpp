#include "common/sim_harness.hpp"

#ifdef TEST_PULSE_SYNC
#include "VPulseSynchronizer.h"
#include <algorithm>

// Independent clocks include coincident edges. The reference uses pre-edge
// source state so a simultaneous source transition cannot bypass stage one.
void check_clocks(unsigned src_half, unsigned dst_half) {
    VPulseSynchronizer dut;
    dut.src_clk = 0;
    dut.dst_clk = 0;
    dut.src_rst = 1;
    dut.dst_rst = 1;
    dut.src_pulse = 0;
    dut.eval();
    bool toggle = false, stage1 = false, stage2 = false, previous = false;
    bool last_pulse = false;
    unsigned sent = 0, received = 0, src_edges = 0;
    unsigned reset_until = 8 * std::max(src_half, dst_half);
    unsigned spacing = 6 * ((dst_half + src_half - 1) / src_half) + 4;
    unsigned end = reset_until + 2 * src_half * spacing * 10;
    for (unsigned t = 1; t <= end; ++t) {
        bool src_edge = t % src_half == 0 && !dut.src_clk;
        bool dst_edge = t % dst_half == 0 && !dut.dst_clk;
        bool reset = t <= reset_until;
        dut.src_rst = reset;
        dut.dst_rst = reset;
        if (src_edge) {
            dut.src_pulse = !reset && sent < 8 && src_edges % spacing == 0;
            if (!reset) ++src_edges;
            if (dut.src_pulse) ++sent;
        }
        dut.eval();
        bool old_toggle = toggle;
        if (src_edge) toggle = reset ? false : toggle != bool(dut.src_pulse);
        if (dst_edge) {
            previous = reset ? false : stage2;
            stage2 = reset ? false : stage1;
            stage1 = reset ? false : old_toggle;
        }
        if (t % src_half == 0) dut.src_clk ^= 1;
        if (t % dst_half == 0) dut.dst_clk ^= 1;
        dut.eval();
        ++sim::cycle;
        if (t > reset_until) {
            SIM_CHECK_EQ(dut.dst_pulse, stage2 != previous, "dual-clock toggle latency");
            if (dst_edge) {
                SIM_CHECK(!(last_pulse && dut.dst_pulse), "pulse is one destination cycle");
                if (dut.dst_pulse) ++received;
                last_pulse = dut.dst_pulse;
            }
        }
    }
    SIM_CHECK_EQ(sent, 8u, "source emitted eight events");
    SIM_CHECK_EQ(received, sent, "no lost or duplicated destination events");
    dut.final();
}

int main() {
    check_clocks(5, 7);
    check_clocks(11, 3);
}
#else
#ifdef TEST_ASYNC_LOW
#include "VSync2FFAsyncLow.h"
using Dut = VSync2FFAsyncLow;
#else
#include "VSync2FF.h"
using Dut = VSync2FF;
#endif

int main() {
    Dut dut;
    dut.dst_clk = 0;
    dut.async_in = 1;
#ifdef TEST_ASYNC_LOW
    dut.dst_rst_n = 1;
    dut.eval();
    dut.dst_rst_n = 0;
#else
    dut.dst_rst = 1;
#endif
    dut.eval();
    sim::tick(dut, dut.dst_clk);
    SIM_CHECK_EQ(dut.sync_out, 0, "reset output");
#ifdef TEST_ASYNC_LOW
    dut.dst_rst_n = 1;
#else
    dut.dst_rst = 0;
#endif
    auto transition = [&](bool level) {
        bool old = dut.sync_out;
        dut.async_in = level;
        dut.eval();
        SIM_CHECK_EQ(dut.sync_out, old, "no combinational crossing");
        sim::tick(dut, dut.dst_clk);
        SIM_CHECK_EQ(dut.sync_out, old, "stage one alone cannot change output");
        sim::tick(dut, dut.dst_clk);
        SIM_CHECK_EQ(dut.sync_out, level, "exactly two destination edges");
        sim::tick(dut, dut.dst_clk);
        SIM_CHECK_EQ(dut.sync_out, level, "stable input stays synchronized");
    };
    transition(true);
    transition(false);
    transition(true);
#ifdef TEST_ASYNC_LOW
    // The output is known to be high before assertion; no clock edge occurs.
    dut.dst_rst_n = 0;
    dut.eval();
    SIM_CHECK_EQ(dut.dst_clk, 0, "clock remains low during asynchronous reset");
    SIM_CHECK_EQ(dut.sync_out, 0, "low reset immediately clears both stages");
    sim::tick(dut, dut.dst_clk);
    SIM_CHECK_EQ(dut.sync_out, 0, "reset dominates active input");
    dut.dst_rst_n = 1;
    dut.eval();
    SIM_CHECK_EQ(dut.sync_out, 0, "reset release does not sample input");
    transition(true);
#else
    dut.dst_rst = 1;
    dut.eval();
    SIM_CHECK_EQ(dut.sync_out, 1, "synchronous reset waits for an edge");
    sim::tick(dut, dut.dst_clk);
    SIM_CHECK_EQ(dut.sync_out, 0, "synchronous reset on destination edge");
    dut.dst_rst = 0;
    transition(true);
#endif
    dut.final();
}
#endif
