#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
workspace=$PWD
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
compiler=(cargo run --quiet -p sylc --)
if [[ -n ${SYLC:-} ]]; then compiler=("$SYLC"); fi

if ! command -v verilator >/dev/null; then
    echo 'error: verilator is required for std cycle simulation' >&2
    exit 1
fi

for package in pulse skid arbiter cdc mem fifo; do
    "${compiler[@]}" --std-root examples/std --out "$tmp/$package.sv" "examples/std/$package.syl"
done

lint() {
    verilator --lint-only --sv --top-module "$2" "$tmp/$1.sv"
}

simulate() {
    local package=$1 top=$2 flags="-std=c++17 ${3:-}"
    lint "$package" "$top"
    verilator --cc --exe --build -j 2 --sv --top-module "$top" --Mdir "$tmp/$top" \
        -CFLAGS "$flags" "$tmp/$package.sv" "$workspace/tests/sim/${package}_test.cpp" -o sim
    "$tmp/$top/sim"
    echo "PASS $top"
}

lint pulse RisingEdgeDetector
lint pulse FallingEdgeDetector
simulate pulse EdgeDetector
simulate pulse PulseStretcher -DTEST_STRETCHER
simulate skid WordSkidBuffer
simulate arbiter FixedPriorityArbiter4 -DTEST_FIXED
simulate arbiter RoundRobinArbiter4
simulate cdc Sync2FF
simulate cdc Sync2FFAsyncLow -DTEST_ASYNC_LOW
simulate cdc PulseSynchronizer -DTEST_PULSE_SYNC
simulate mem SinglePortRam -DTEST_SINGLE_PORT
simulate mem SimpleDualPortRam
simulate mem DistributedRam -DTEST_DISTRIBUTED
simulate fifo SyncFifo
simulate fifo StreamSyncFifo -DTEST_STREAM

if command -v yosys >/dev/null; then
    for top in SinglePortRam SimpleDualPortRam DistributedRam; do
        yosys -Q -q -p "read_verilog -sv $tmp/mem.sv; hierarchy -check -top $top; proc; memory; check -assert; stat"
        echo "PASS synthesis $top"
    done
    yosys -Q -q -p "read_verilog -sv $tmp/cdc.sv; hierarchy -check -top Sync2FFAsyncLow; proc; memory; check -assert; stat"
    echo 'PASS synthesis Sync2FFAsyncLow'
else
    echo 'SKIP std synthesis: yosys is missing'
fi
