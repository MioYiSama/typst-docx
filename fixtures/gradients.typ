#set page(paper: "a5", margin: 1.2cm)
#set rect(width: 3.4cm, height: 1.8cm, radius: 2pt, stroke: 0.5pt + rgb("334155"))

#grid(
  columns: (1fr, 1fr),
  gutter: 0.8cm,
  row-gutter: 0.8cm,
  rect(fill: gradient.linear(rgb("2563eb"), rgb("f97316"), angle: 0deg)),
  rect(fill: gradient.linear(rgb("16a34a"), rgb("facc15"), angle: 45deg)),
  rect(fill: gradient.linear(rgb("dc2626"), rgb("7c3aed"), angle: 90deg)),
  rect(fill: gradient.linear(
    (rgb("0f172a"), 0%),
    (rgb("0ea5e980"), 45%),
    (rgb("f8fafc"), 100%),
    angle: 0deg,
  )),
  circle(
    radius: 0.85cm,
    fill: gradient.radial(rgb("fef08a"), rgb("ea580c")),
  ),
  circle(
    radius: 0.85cm,
    fill: gradient.radial(rgb("bae6fd"), rgb("1d4ed8"), center: (30%, 70%)),
  ),
)
