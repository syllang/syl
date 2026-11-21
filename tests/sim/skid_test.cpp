#include "common/sim_harness.hpp"
#include "VWordSkidBuffer.h"
#include <queue>
#include <random>

int main() {
    VWordSkidBuffer dut;
    dut.up_valid = 0;
    dut.down_ready = 0;
    sim::apply_sync_reset(dut);
    std::queue<std::uint8_t> expected;
    std::uint8_t data = 0;
    auto step = [&](bool valid, bool ready) {
        dut.up_valid = valid;
        dut.up_payload = data;
        dut.down_ready = ready;
        dut.eval();
        SIM_CHECK_EQ(dut.up_ready, expected.size() < 2, "registered capacity");
        SIM_CHECK_EQ(dut.down_valid, !expected.empty(), "registered output");
        if (!expected.empty()) {
            SIM_CHECK_EQ(dut.down_payload, expected.front(), "oldest accepted byte");
            if (ready) expected.pop();
        }
        bool accepted = valid && dut.up_ready;
        if (accepted) expected.push(data++);
        sim::tick(dut, dut.clk);
        return accepted;
    };

    for (int i = 0; i < 16; ++i) step(true, true);
    for (int i = 0; i < 5; ++i) step(true, false);
    step(true, true);
    step(true, true);
    step(false, true);
    step(false, true);

    std::mt19937 random(307);
    bool stalled = false;
    for (int i = 0; i < 200; ++i) {
        bool valid = stalled || (random() & 1);
        stalled = !step(valid, random() & 1) && valid;
    }
    while (stalled) stalled = !step(true, true);
    while (!expected.empty()) step(false, true);
    step(false, true);
    dut.final();
}
