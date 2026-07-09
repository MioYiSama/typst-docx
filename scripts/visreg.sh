#!/bin/zsh
# Visual regression: typst PDF (reference) vs LibreOffice-rendered DOCX.
#
# Usage: scripts/visreg.sh [fixture ...]     (default: all fixtures)
#
# Requirements:
#   - typst on PATH
#   - LibreOffice.app (macOS) or soffice on PATH
#   - Fonts visible to LibreOffice: run `scripts/visreg.sh --install-fonts`
#     once to copy the typst-embedded fonts into ~/Library/Fonts.
set -e
cd "$(dirname "$0")/.."

SOFFICE=${SOFFICE:-/Applications/LibreOffice.app/Contents/MacOS/soffice}
command -v "$SOFFICE" >/dev/null || SOFFICE=soffice
VENV=scripts/.venv

if [[ "$1" == "--install-fonts" ]]; then
    assets=$(ls -d ~/.cargo/registry/src/*/typst-assets-* | head -1)
    dest=~/Library/Fonts/typst-docx-visreg
    mkdir -p "$dest"
    cp "$assets"/files/fonts/LibertinusSerif-*.otf \
       "$assets"/files/fonts/NewCM*.otf \
       "$assets"/files/fonts/DejaVuSansMono*.ttf "$dest/"
    echo "fonts installed to $dest"
    exit 0
fi

if [[ ! -d $VENV ]]; then
    python3 -m venv $VENV
    $VENV/bin/pip -q install pypdfium2 pillow
fi

cargo build --release -p typst-docx-cli

fixtures=("$@")
[[ ${#fixtures} -eq 0 ]] && fixtures=(blank calibration shapes images)

out=target/visreg
mkdir -p $out
pkill -f soffice 2>/dev/null || true
sleep 1

fail=0
for f in $fixtures; do
    typst compile fixtures/$f.typ $out/$f-ref.pdf
    target/release/typst-docx fixtures/$f.typ -o $out/$f.docx 2>/dev/null
    rm -f $out/$f.pdf
    "$SOFFICE" --headless --convert-to pdf --outdir $out $out/$f.docx >/dev/null 2>&1
    echo "--- $f"
    $VENV/bin/python scripts/imgdiff.py $out/$f-ref.pdf $out/$f.pdf $out \
        | sed "s/^page /  page /" || fail=1
    for d in $out/diff-*.png; do
        [[ -e $d ]] && mv $d ${d:h}/$f-${d:t}
    done
done
exit $fail
