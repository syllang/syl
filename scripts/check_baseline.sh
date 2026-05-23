#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace"

cargo fmt --all --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --no-fail-fast
git diff --check

debt_regex="$(printf '%s%s|%s%s|%s%s|%s%s|%s%s|%s%s' MUST _FIX SHOULD _FIX FIX ME TO DO HA CK X XX)"
debt_out="$(mktemp)"
if rg -n "$debt_regex" . --glob '!target/**' --glob '!.git/**' --glob '!.tmp/**' \
    | grep -v 'crates/sylc/tests/architecture_markers.rs' > "$debt_out"; then
    cat "$debt_out"
    rm -f "$debt_out"
    exit 1
fi
rm -f "$debt_out"

artifact_out="$(mktemp)"
if find . -path './target' -prune -o -path './.git' -prune -o -path './.tmp' -prune -o -type f \
    \( -name '*.pyc' -o -name '*.pyo' -o -name '.DS_Store' -o -name '*~' -o -name '*.swp' -o -name '*.tmp' \) \
    -print > "$artifact_out" && [[ -s "$artifact_out" ]]; then
    cat "$artifact_out"
    rm -f "$artifact_out"
    exit 1
fi
rm -f "$artifact_out"

sv_tmp="$(mktemp -d)"
trap 'rm -rf "$sv_tmp"' EXIT

cargo run -p sylc -- --out "$sv_tmp/minimal_features.sv" examples/minimal_features.syl
cargo run -p sylc -- --out "$sv_tmp/mvp.sv" examples/mvp

if [[ "${SYL_SKIP_VERILATOR:-0}" == "1" ]]; then
    echo "Skipping Verilator smoke because SYL_SKIP_VERILATOR=1"
else
    command -v verilator >/dev/null || {
        echo "verilator is required for baseline smoke; set SYL_SKIP_VERILATOR=1 to skip"
        exit 1
    }
    verilator --lint-only --sv "$sv_tmp/minimal_features.sv"
    for top in CombAlu32 CounterPair BufferedWordPipe LaneArray; do
        verilator --lint-only --sv --top-module "$top" "$sv_tmp/mvp.sv"
    done
fi
