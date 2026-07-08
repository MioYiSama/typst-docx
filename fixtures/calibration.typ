// M1: baseline calibration. Each text sits on a hairline drawn by Typst at
// the exact baseline position. In Word, the glyph baselines must touch the
// lines. Multiple fonts and sizes exercise the OS/2 win-metric rule.
#set page(paper: "a4", margin: 2cm)

#let sample(font, size) = {
  set text(font: font, size: size)
  box(place(dx: 0pt, dy: 0pt, line(length: 12cm, stroke: 0.2pt + red)))
  [Agjpqy 0123 Word 基线校准 #font #repr(size)]
}

#for (font, size) in (
  ("Libertinus Serif", 8pt),
  ("Libertinus Serif", 11pt),
  ("Libertinus Serif", 16pt),
  ("Libertinus Serif", 24pt),
  ("New Computer Modern", 11pt),
  ("New Computer Modern", 18pt),
  ("DejaVu Sans Mono", 9pt),
  ("DejaVu Sans Mono", 14pt),
) {
  block(above: 18pt, sample(font, size))
}

// Justified text: spaces are stretched, so fragments must reposition.
#block(above: 24pt)[
  #set par(justify: true)
  #set text(size: 10pt)
  The quick brown fox jumps over the lazy dog again and again while the
  slow yellow turtle watches from a warm rock near the river bend, and
  the narrow column width forces heavy justification stretching.
]

// Bold, italic, colored text.
#block(above: 18pt)[
  Regular *bold* _italic_ #text(fill: blue)[blue] #text(fill: rgb(80%, 20%, 20%))[dark red]
]
