#!/usr/bin/env bash
# Takes the pictures of the README and the guide again: headless screenshots
# from the snapshot tests (crates/omaspec-app/src/editor/tests.rs) and four
# bundled Omarchy themes as one picture. Needs ImageMagick; the themes need
# Omarchy's bundled themes.
#
# play.gif is a screen recording of Omaspec, not taken here. Cut a recording
# to the play and turn it into the GIF with:
#   ffmpeg -i recording.mp4 -vf "fps=20,scale=1200:-2:flags=lanczos,hqdn3d=0:0:4:4,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" docs/images/play.gif
set -euo pipefail
cd "$(dirname "$0")/../.."
out=docs/images
shots=$(mktemp -d)
trap 'rm -rf "$shots"' EXIT

OMASPEC_SNAPSHOT_DIR="$shots" cargo test --release -p omaspec-app --bin omaspec -- \
  snapshot_story snapshot_readme snapshot_themes snapshot_gallery >/dev/null

shot() { magick "$shots/$1-wgpu.png" -strip "$out/$2.png"; }
mkdir -p "$out/guide"
shot readme-hero hero
shot readme-specs specs
shot readme-trace trace
shot readme-compare compare
shot gallery-omarchy gallery
shot guide-machine guide/machine
shot guide-finding guide/finding
shot guide-play guide/play

# Two dark themes and two light ones.
magick montage "$shots"/theme-{tokyo-night,catppuccin-latte,gruvbox,rose-pine}-wgpu.png \
  -tile 2x2 -geometry 800x427+6+6 -background '#16161e' -depth 8 -strip "$out/themes.png"

# What GitHub shows for a shared link (Settings, Social preview; uploaded by
# hand): the mark, the name and the hero's diagram.
bold=$(fc-match -f '%{file}' 'monospace:bold')
regular=$(fc-match -f '%{file}' 'monospace')
magick -respect-parentheses -size 1280x640 xc:'#1a1b26' \
  \( -background none -density 384 packaging/omaspec.svg -resize 112x112 \) -geometry +72+72 -composite \
  -font "$bold" -pointsize 64 -fill '#c0caf5' -annotate +72+270 'Omaspec' \
  -font "$regular" -pointsize 26 -fill '#a9b1d6' \
  -annotate +72+336 $'PlantUML and Mermaid,\ndrawn as you type,\nunderstood as behavior.' \
  -pointsize 22 -fill '#565f89' -annotate +72+566 'A diagram editor for Omarchy' \
  \( "$out/hero.png" -crop 1110x900+1245+200 +repage -resize x520 \) \
  -gravity northeast -geometry +60+60 -composite -strip "$out/social.png"
