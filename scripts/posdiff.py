#!/usr/bin/env python3
"""Compare per-character positions between two PDFs (pt units, y-down)."""
import sys

import pypdfium2 as pdfium


def chars(path):
    doc = pdfium.PdfDocument(path)
    page = doc[0]
    h = page.get_size()[1]
    tp = page.get_textpage()
    out = []
    for i in range(tp.count_chars()):
        c = chr(tp.get_char(i)) if hasattr(tp, "get_char") else None
        # charbox: (left, bottom, right, top) in PDF points, y-up
        l, b, r, t = tp.get_charbox(i, loose=False)
        out.append((c, l, h - b, r))
    return out, tp


def text_of(tp):
    return tp.get_text_bounded()


ref_chars, ref_tp = chars(sys.argv[1])
got_chars, got_tp = chars(sys.argv[2])
ref_text = text_of(ref_tp)
got_text = text_of(got_tp)

# Align by matching character sequences (skip whitespace).
ri = [i for i, ch in enumerate(ref_text) if not ch.isspace()]
gi = [i for i, ch in enumerate(got_text) if not ch.isspace()]
rt = "".join(ref_text[i] for i in ri)
gt = "".join(got_text[i] for i in gi)

import difflib

sm = difflib.SequenceMatcher(None, rt, gt, autojunk=False)
rows = []
for a, b, n in sm.get_matching_blocks():
    for k in range(n):
        i, j = ri[a + k], gi[b + k]
        rc, gc = ref_chars[i], got_chars[j]
        dx = gc[1] - rc[1]
        dy = gc[2] - rc[2]
        rows.append((rt[a + k], rc[1], rc[2], dx, dy))

print(f"matched {len(rows)} chars (ref {len(rt)}, got {len(gt)})")
print(f"{'char':>4} {'x':>7} {'y':>7} {'dx':>7} {'dy':>7}")
# Group rows by baseline y (line) and print per-line summary + worst chars.
from collections import defaultdict

lines = defaultdict(list)
for row in rows:
    lines[round(row[2] / 3)].append(row)
for key in sorted(lines):
    group = lines[key]
    xs = [r[3] for r in group]
    ys = [r[4] for r in group]
    text = "".join(r[0] for r in group)[:28]
    print(
        f"y≈{group[0][2]:6.1f} n={len(group):3d} dx[avg {sum(xs)/len(xs):+6.2f} "
        f"max {max(xs, key=abs):+6.2f}] dy[avg {sum(ys)/len(ys):+6.2f} "
        f"max {max(ys, key=abs):+6.2f}]  {text}"
    )
