#!/usr/bin/env bash
# Writes the live examples of the landing page (docs/index.html) again: real
# Omaspec pages, each one file that fetches nothing, made from the examples
# with the commands the page names. Needs no GPU; git for the history.
set -euo pipefail
cd "$(dirname "$0")/../.."
out=docs/live
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
omaspec() { cargo run --release -q -p omaspec-app -- "$@" 2>/dev/null; }
theme=(--theme built-in)

# A state machine and a scenario to step through.
omaspec render examples/gallery/coffee-machine.puml --look dark -o "$out/machine.html"
omaspec render examples/gallery/pizza.puml --look dark -o "$out/scenario.html"
# A scenario in lockstep with the machines of its participants.
omaspec play examples/home/keys.puml --lockstep "${theme[@]}" -o "$out/lockstep.html"
# Code becomes a picture.
omaspec play examples/gallery/parcel.puml --build "${theme[@]}" -o "$out/build.html"
# Recorded traces flowing through a machine.
omaspec play examples/orders/order.puml --traffic examples/orders/week.jsonl "${theme[@]}" -o "$out/traffic.html"
# A diagram's history, what changed glowing.
docs/images/history.sh "$work/repo"
omaspec play "$work/repo/parcel.puml" --history "${theme[@]}" -o "$out/history.html"
# A project as one page.
omaspec export examples/cafe --html --look dark -o "$out/project.html"

# Pictures: what only Omaspec draws, and what traces take of a machine.
omaspec render examples/gallery/online-course.puml --look dark -o "$out/storymap.svg"
omaspec render examples/gallery/city-library.puml --look dark -o "$out/capabilities.svg"
omaspec orbit examples/gallery/parcel.puml -o "$out/orbit.svg"
omaspec coverage examples/orders/order.puml --traces examples/orders/week.jsonl --river -o "$out/river.svg" || true
ls -la "$out"
