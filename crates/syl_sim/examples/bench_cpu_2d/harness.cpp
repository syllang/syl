#include "Vdut.h"
#include <algorithm>
#include <barrier>
#include <chrono>
#include <cstdint>
#include <cstdio>
#include <memory>
#include <string>
#include <thread>
#include <vector>

using Clock = std::chrono::steady_clock;
static uint64_t mix(uint64_t x) {
    x += 0x9e3779b97f4a7c15ULL;
    x = (x ^ (x >> 30)) * 0xbf58476d1ce4e5b9ULL;
    x = (x ^ (x >> 27)) * 0x94d049bb133111ebULL;
    return x ^ (x >> 31);
}
static uint64_t random_word(size_t lane, uint64_t cycle, uint64_t port, uint64_t word) {
    return mix(lane * 0x517cc1b727220a95ULL ^ cycle * 0x5851f42d4c957f2dULL ^ (port << 32) ^ word);
}

int main(int argc, char **argv) {
    if (argc != 5)
        return 2;
    const auto cold = Clock::now();
    const int batch = std::stoi(argv[1]), cycles = std::stoi(argv[2]);
    const int workers = std::min(batch, std::stoi(argv[3])), internal = std::stoi(argv[4]);
    std::vector<uint64_t> sums(workers), failures(workers, UINT64_MAX);
    std::vector<std::thread> team;
    Clock::time_point hot, end;
    std::barrier start(workers, [&]() noexcept { hot = Clock::now(); });
    std::barrier finish(workers, [&]() noexcept { end = Clock::now(); });
    auto worker = [&](int id) {
        VerilatedContext context;
        context.threads(internal);
        context.fatalOnError(false);
        std::vector<std::unique_ptr<Vdut>> models;
        const int begin = batch * id / workers, limit = batch * (id + 1) / workers;
        for (int lane = begin; lane < limit; ++lane) {
            models.push_back(std::make_unique<Vdut>(&context));
            auto &d = *models.back();
            d.clk = 0;
            d.eval();
        }
        start.arrive_and_wait();
        for (int cycle = 1; cycle <= cycles && failures[id] == UINT64_MAX; ++cycle) {
            for (int lane = begin; lane < limit; ++lane) {
                auto &d = *models[lane - begin];
                @DRIVE@
                d.clk = 0;
                d.eval();
                d.clk = 1;
                d.eval();
                if constexpr (@FAULT@) {
                    if (context.gotError()) {
                        failures[id] = (uint64_t(cycle) << 32) + lane;
                        break;
                    }
                }
                d.clk = 0;
                d.eval();
            }
        }
        uint64_t sum = 0;
        for (int lane = begin; lane < limit; ++lane) {
            auto &d = *models[lane - begin];
            @DIGEST@
        }
        sums[id] = sum;
        finish.arrive_and_wait();
    };
    for (int i = 1; i < workers; ++i)
        team.emplace_back(worker, i);
    worker(0);
    for (auto &t : team)
        t.join();
    uint64_t sum = 0;
    for (auto s : sums)
        sum += s;
    std::printf("%.9f %.9f %llu %llu\n", std::chrono::duration<double>(hot - cold).count(),
                std::chrono::duration<double>(end - hot).count(), (unsigned long long)sum,
                (unsigned long long)*std::min_element(failures.begin(), failures.end()));
}
