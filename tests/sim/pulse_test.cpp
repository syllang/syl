#include "common/sim_harness.hpp"

#ifdef TEST_STRETCHER
#include "VPulseStretcher.h"
int main() {
    VPulseStretcher dut;
    dut.trigger = 0;
    sim::apply_sync_reset(dut);
    SIM_CHECK_EQ(dut.pulse, 0, "reset");
    // The second pulse train retriggers two cycles into the active interval.
    for (int i = 0; i < 16; ++i) {
        dut.trigger = i == 0 || i == 7 || i == 9;
        sim::tick(dut, dut.clk);
        SIM_CHECK_EQ(dut.pulse, i < 4 || (i >= 7 && i < 13), "pulse width");
    }
    dut.final();
}
#else
#include "VEdgeDetector.h"
int main() {
    VEdgeDetector dut;
    dut.level = 0;
    sim::apply_sync_reset(dut);
    int previous = 0;
    for (int level : {0, 1, 1, 0, 0, 1, 0}) {
        dut.level = level;
        dut.eval();
        SIM_CHECK_EQ(dut.rising, level > previous, "rising");
        SIM_CHECK_EQ(dut.falling, level < previous, "falling");
        SIM_CHECK_EQ(dut.any_edge, level != previous, "either edge");
        sim::tick(dut, dut.clk);
        SIM_CHECK_EQ(dut.any_edge, 0, "sampled input");
        previous = level;
    }
    dut.final();
}
#endif
