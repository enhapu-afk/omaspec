#!/usr/bin/env bash
# The landing page's film, docs/images/omaspec.mp4, and its poster
# omaspec.jpg: the editor recorded headless on the GPU, every frame of
# every motion at 60 a second (crates/omaspec-app/src/editor/tests/film.rs),
# a caption under each scene, the scenes fading into one another. Needs a
# GPU, ffmpeg and ImageMagick. With FRAMES=DIR the frames are kept there,
# or taken from there when they are, to cut the film again.
set -euo pipefail
cd "$(dirname "$0")/../.."
out=docs/images
work=${FRAMES:-$(mktemp -d)}
[[ -n ${FRAMES:-} ]] || trap 'rm -rf "$work"' EXIT
mkdir -p "$work"

if [[ ! -d $work/5-export ]]; then
  OMASPEC_FILM_DIR="$(realpath "$work")" cargo test --release -q -p omaspec-app --bin omaspec -- \
    --exact editor::tests::film::film >/dev/null
fi

regular=$(fc-match -f '%{file}' 'monospace')
bold=$(fc-match -f '%{file}' 'monospace:bold')
width=1920
band=112
calc() { awk "BEGIN { printf \"%.4f\", $1 }"; }

# A caption: the keys in a pill outlined in the accent, the words beside.
caption() {
  local name=$1 keys=$2 words=$3
  magick -background none -fill '#7aa2f7' -font "$bold" -pointsize 30 label:"$keys" "$work/keys.png"
  local w h
  w=$(magick identify -format %w "$work/keys.png")
  h=$(magick identify -format %h "$work/keys.png")
  local pw=$((w + 40)) ph=$((h + 18))
  magick -size "${pw}x${ph}" xc:none -fill '#24283b' -stroke '#7aa2f7' -strokewidth 2 \
    -draw "roundrectangle 1,1 $((pw - 2)),$((ph - 2)) $((ph / 2)),$((ph / 2))" \
    "$work/keys.png" -gravity center -composite "$work/pill.png"
  magick -background none -fill '#c0caf5' -font "$regular" -pointsize 30 label:"$words" "$work/words.png"
  magick "$work/pill.png" \( -size 24x1 xc:none \) "$work/words.png" -background none \
    -gravity center +append "$work/line.png"
  magick -size "${width}x${band}" xc:'#1a1b26' -fill '#2f3449' -draw "rectangle 0,0 $width,1" \
    "$work/line.png" -gravity center -composite "$work/$name.png"
}
caption 1-type 'Type' 'PlantUML or Mermaid, drawn as you type and checked as you go'
caption 2-hand 'Alt + drag' 'a line where you want it: the layout stays in the file'
caption 3-play 'F5' 'plays the machine: click what happens, a light runs each step'
caption 4-pills 'Scroll' 'a long scenario keeps its participants on top, as pills'
caption 5-export 'File › Export as' 'SVG, PNG, PDF, Markdown, ZIP, a page that plays it, an animation'

# Each scene over its caption, faded into the next.
scenes=(1-type 2-hand 3-play 4-pills 5-export)
fade=0.4
inputs=()
graph=""
offset=0
for i in "${!scenes[@]}"; do
  scene=${scenes[$i]}
  inputs+=(-framerate 60 -i "$work/$scene/%05d.png" -loop 1 -framerate 60 -i "$work/$scene.png")
  frames=$(find "$work/$scene" -name '*.png' | wc -l)
  seconds=$(calc "$frames / 60")
  graph+="[$((2 * i))][$((2 * i + 1))]vstack=shortest=1,format=yuv444p[s$i];"
  if ((i == 0)); then
    last=s0
    offset=$seconds
  else
    offset=$(calc "$offset - $fade")
    graph+="[$last][s$i]xfade=transition=fade:duration=$fade:offset=$offset[x$i];"
    last=x$i
    offset=$(calc "$offset + $seconds")
  fi
done
total=$offset
graph+="[$last]fade=t=in:d=0.35:color=0x13141c,fade=t=out:st=$(calc "$total - 0.45"):d=0.45:color=0x13141c,format=yuv420p[v]"
ffmpeg -v error -y "${inputs[@]}" -filter_complex "$graph" -map '[v]' -r 60 \
  -c:v libx264 -preset slow -tune animation -crf 24 -movflags +faststart "$out/omaspec.mp4"

# The poster: the play, as the page shows it before the film starts.
poster=$(calc "($(find "$work/1-type" -name '*.png' | wc -l) + $(find "$work/2-hand" -name '*.png' | wc -l)) / 60 + 3")
ffmpeg -v error -y -ss "$poster" -i "$out/omaspec.mp4" -frames:v 1 -q:v 3 "$out/omaspec.jpg"
echo "$out/omaspec.mp4: $(printf %.1f "$total") s, $(du -h "$out/omaspec.mp4" | cut -f1)"
