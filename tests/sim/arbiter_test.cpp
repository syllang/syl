#include "common/sim_harness.hpp"

#ifdef TEST_FIXED
#include "VFixedPriorityArbiter4.h"
int main() {
    VFixedPriorityArbiter4 dut;
    for (int req = 0; req < 16; ++req) {
        dut.req = req;
        dut.eval();
        SIM_CHECK_EQ(dut.grant, req & -req, "lowest requested bit");
    }
    dut.final();
}
#else
#include "VRoundRobinArbiter4.h"
int main() {
    VRoundRobinArbiter4 dut;
    dut.req = 0;
    sim::apply_sync_reset(dut);
    auto step = [&](int req, int index) {
        dut.req = req;
        dut.eval();
        SIM_CHECK_EQ(dut.grant_valid, req != 0, "valid");
        SIM_CHECK_EQ(dut.grant, req ? 1 << index : 0, "one-hot grant");
        if (req) SIM_CHECK_EQ(dut.grant_idx, index, "winner index");
        sim::tick(dut, dut.clk);
    };
    for (int i = 0; i < 16; ++i) step(15, i % 4);
    for (int i = 0; i < 8; ++i) {
        step(10, 1);
        step(0, 0);
        step(10, 3);
    }
    dut.final();
}
#endif
