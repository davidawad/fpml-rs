#!/usr/bin/env bash
# Reproduce docs/showcase/cds-summary.{txt,png} from the ISDA CDS examples.
# Needs cargo; the PNG step needs charm `freeze` (skipped if absent).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
cd "$root"
cargo build -q --example cds_summary
dir=schemas/confirmation/examples/credit-derivatives
{
  echo "\$ cargo run --example cds_summary -- $dir"
  target/debug/examples/cds_summary "$dir"
} > "$here/cds-summary.txt"
cat "$here/cds-summary.txt"
if command -v freeze > /dev/null; then
  freeze "$here/cds-summary.txt" --language text --window --padding 20 --font.size 14 \
    --output "$here/cds-summary.png"
fi
