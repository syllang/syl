#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace"

run() {
    printf '==> %s\n' "$*"
    "$@"
}

run cargo fmt --all --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace --all-targets --no-fail-fast
run scripts/parser_fuzz_smoke.sh
run scripts/check_examples.sh
run scripts/check_docs_syl.py
run scripts/check_public_api.py --check
run cargo doc --workspace --no-deps
run git diff --check

baseline_ref="${QUALITY_GATE_BASELINE:-origin/feat/initial-syl-baseline}"
if git rev-parse --verify --quiet "$baseline_ref" >/dev/null; then
    run git diff --check "$baseline_ref"...HEAD
elif git rev-parse --verify --quiet HEAD^ >/dev/null; then
    printf '==> baseline %s not found; falling back to HEAD^..HEAD whitespace check\n' "$baseline_ref"
    run git diff --check HEAD^..HEAD
else
    printf 'error: no baseline ref %s and no HEAD^ fallback for committed whitespace check\n' \
        "$baseline_ref" >&2
    exit 1
fi

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

line_report="$(find crates -type f -name '*.rs' -print0 | xargs -0 wc -l | sort -nr | head)"
printf '==> largest Rust source files\n%s\n' "$line_report"
