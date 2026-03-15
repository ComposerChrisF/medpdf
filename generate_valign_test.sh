#!/usr/bin/env bash
# generate_valign_test.sh
#
# Recreates valign_test.pdf using the pdf-maker CLI.
# Demonstrates --blank-page, --draw-rect, --draw-line, and --watermark with v_align options.
#
# Requirements:
#   - cargo build --release (or cargo build) has been run
#   - /Library/Fonts/CrimsonPro-Light.ttf is installed

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PDF_MERGER="${SCRIPT_DIR}/target/release/pdf-maker"
OUTPUT="${SCRIPT_DIR}/valign_test_cli.pdf"
FONT="/Library/Fonts/CrimsonPro-Light.ttf"

# Build if needed
if [ ! -f "$PDF_MERGER" ]; then
    echo "Building pdf-maker..."
    cargo build --release -p pdf-maker --manifest-path "${SCRIPT_DIR}/Cargo.toml"
fi

# Font shorthand
F="$FONT"

# Gray color for labels/headings
GRAY="#4D4D4D"

"$PDF_MERGER" -o "$OUTPUT" \
    --blank-page letter \
    \
    `# === Red reference lines (under content) ===` \
    --draw-rect "x=30,y=680,w=550,h=0.5,color=red,layer=under" \
    --draw-rect "x=30,y=590,w=550,h=0.5,color=red,layer=under" \
    --draw-rect "x=30,y=500,w=550,h=0.5,color=red,layer=under" \
    --draw-rect "x=30,y=410,w=550,h=0.5,color=red,layer=under" \
    --draw-rect "x=30,y=320,w=550,h=0.5,color=red,layer=under" \
    --draw-rect "x=30,y=230,w=550,h=0.5,color=red,layer=under" \
    \
    `# === Title ===` \
    --watermark "text=VAlign Test \u2014 red line = anchor y-coordinate,font=${F},size=14,x=50,y=750,units=pt,color=${GRAY}" \
    \
    `# === Row labels (small, gray) ===` \
    --watermark "text=v_align=Top,font=${F},size=10,x=35,y=700,units=pt,color=${GRAY}" \
    --watermark "text=v_align=CapTop,font=${F},size=10,x=35,y=610,units=pt,color=${GRAY}" \
    --watermark "text=v_align=Center,font=${F},size=10,x=35,y=520,units=pt,color=${GRAY}" \
    --watermark "text=v_align=Baseline,font=${F},size=10,x=35,y=430,units=pt,color=${GRAY}" \
    --watermark "text=v_align=DescentBottom,font=${F},size=10,x=35,y=340,units=pt,color=${GRAY}" \
    --watermark "text=v_align=Bottom,font=${F},size=10,x=35,y=250,units=pt,color=${GRAY}" \
    \
    `# === Sample text with each VAlign ===` \
    --watermark "text=Tepqjy Align,font=${F},size=36,x=150,y=680,units=pt,v_align=top" \
    --watermark "text=Tepqjy Align,font=${F},size=36,x=150,y=590,units=pt,v_align=cap_top" \
    --watermark "text=Tepqjy Align,font=${F},size=36,x=150,y=500,units=pt,v_align=center" \
    --watermark "text=Tepqjy Align,font=${F},size=36,x=150,y=410,units=pt,v_align=baseline" \
    --watermark "text=Tepqjy Align,font=${F},size=36,x=150,y=320,units=pt,v_align=descent_bottom" \
    --watermark "text=Tepqjy Align,font=${F},size=36,x=150,y=230,units=pt,v_align=bottom" \
    \
    `# === WinAnsi spot-check heading ===` \
    --watermark "text=\"WinAnsi 0x80\u20130x9F spot-check (should be evenly spaced, no overlaps):\",font=${F},size=11,x=50,y=150,units=pt,color=${GRAY}" \
    \
    `# === WinAnsi spot-check text ===` \
    --watermark "text=\u201Ccurly quotes\u201D \u2018single quotes\u2019 \u2013 en dash \u2013 \u2014 em dash \u2014 \u20AC100 \u2022 bullet \u2026 ellipsis \u2122 TM \u0152\u0153 \u0160\u0161,font=${F},size=13,x=50,y=120,units=pt"

echo ""
echo "Generated: ${OUTPUT}"
echo "Open with:  open ${OUTPUT}"
