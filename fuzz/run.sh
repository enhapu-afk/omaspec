#!/usr/bin/env bash
# Seeds each fuzz target with the repository's own inputs and runs it for a
# while: `fuzz/run.sh [SECONDS] [TARGET...]` (default 300 s, every target).
# A crash, a hang past the timeout or a leak lands in fuzz/artifacts/TARGET;
# `cargo +nightly fuzz run TARGET fuzz/artifacts/TARGET/crash-…` repeats it.
# The corpus grows in fuzz/corpus/TARGET and is kept between runs.
set -euo pipefail

cd "$(dirname "$0")"
root=..
seconds=${1:-300}
shift || true
targets=("$@")
if [ ${#targets[@]} -eq 0 ]; then
  targets=(plantuml mermaid layout hand text model document trace sanitize image_source)
fi

# Files of the repository, by what they feed (no build output, no packages).
seeds() {
  find "$root/examples" "$root/crates" "$root/docs" -type f \( "$@" \) \
    -not -path '*/target/*' -not -path '*/golden/*' 2>/dev/null
}

seed() {
  local target=$1
  shift
  mkdir -p "corpus/$target"
  seeds "$@" | while read -r file; do
    cp "$file" "corpus/$target/seed-$(basename "$file")" 2>/dev/null || true
  done
}

for target in "${targets[@]}"; do
  case $target in
    plantuml | hand) seed "$target" -name '*.puml' ;;
    mermaid) seed "$target" -name '*.mmd' ;;
    text | model) seed "$target" -name '*.puml' -o -name '*.mmd' ;;
    document) seed "$target" -name '*.md' ;;
    trace) seed "$target" -name '*.jsonl' ;;
    sanitize) seed "$target" -name '*.svg' ;;
    *) mkdir -p "corpus/$target" ;;
  esac
done

# Without the address sanitizer: Omaspec has no unsafe code, and it made
# every input many times slower. A diagram that renders slowly but right
# is no hang: an endless `!while` runs the preprocessor to its limit
# (0.4 s in a release build), mmdr takes as long on a large broken
# flowchart; 20 s leaves room for both on a busy machine.
status=0
for target in "${targets[@]}"; do
  echo "== $target for ${seconds}s"
  mkdir -p "artifacts/$target"
  if ! cargo +nightly fuzz run -O -s none "$target" "corpus/$target" -- \
    -dict=dict/diagrams.dict -max_total_time="$seconds" -timeout=20 \
    -rss_limit_mb=4096 -max_len=16384 -print_final_stats=1; then
    status=1
  fi
done
exit $status
