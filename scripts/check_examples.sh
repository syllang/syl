#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace"

sv_tmp="$(mktemp -d)"
trap 'rm -rf "$sv_tmp"' EXIT

cargo run --quiet -p sylc -- --std-root examples --out "$sv_tmp/minimal_features.sv" \
    examples/minimal_features.syl
cargo run --quiet -p sylc -- --std-root examples --out "$sv_tmp/mvp.sv" examples/mvp
cargo run --quiet -p sylc -- --std-root examples --out "$sv_tmp/pipeline_user.sv" \
    examples/pipeline_user.syl
cargo run --quiet -p sylc -- --std-root examples --out "$sv_tmp/std_user.sv" \
    examples/std_user

if ! command -v verilator >/dev/null; then
    printf '{"check":"verilator_smoke","status":"skip","reason":"verilator_not_found"}\n'
    exit 0
fi

verilator --lint-only --sv "$sv_tmp/minimal_features.sv"
for top in CombAlu32 CounterPair BufferedWordPipe LaneArray; do
    verilator --lint-only --sv --top-module "$top" "$sv_tmp/mvp.sv"
done
