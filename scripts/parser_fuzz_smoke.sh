#!/usr/bin/env bash
set -euo pipefail

workspace="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$workspace"

cargo run --quiet -p syl_fuzz --bin parser_fuzz -- fuzz/corpus/parser
