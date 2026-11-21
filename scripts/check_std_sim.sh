#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
workspace=$PWD
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
compiler=(cargo run --quiet -p sylc --)
if [[ -n ${SYLC:-} ]]; then compiler=("$SYLC"); fi

for package in pulse skid arbiter; do
    "${compiler[@]}" --std-root examples/std --out "$tmp/$package.sv" "examples/std/$package.syl"
done

lint() {
    verilator --lint-only --sv --top-module "$2" "$tmp/$1.sv"
}

simulate() {
    local package=$1 top=$2 flags="-std=c++17 ${3:-}"
    lint "$package" "$top"
    verilator --cc --exe --build -j 2 --sv --top-module "$top" --Mdir "$tmp/$top" \
        -CFLAGS "$flags" "$tmp/$package.sv" "$workspace/tests/sim/tb_$package.cpp" -o sim
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
