// M2: shapes, strokes, dashes, curves, rotation, page background.
#set page(paper: "a5", margin: 1.5cm, fill: rgb("fdf6e3"))

#place(top + left, rect(width: 3cm, height: 2cm, fill: blue.lighten(60%), stroke: 2pt + navy))

#place(top + right, circle(radius: 1cm, fill: yellow, stroke: 1pt + orange))

#place(dx: 0cm, dy: 3cm, line(length: 8cm, stroke: (paint: red, thickness: 1pt, dash: "dashed")))

#place(dx: 0cm, dy: 3.5cm, line(length: 8cm, angle: 8deg, stroke: 3pt + green))

#place(dx: 1cm, dy: 5cm, rotate(30deg, rect(width: 3cm, height: 1.5cm, fill: purple.lighten(40%))))

#place(dx: 5cm, dy: 5cm, scale(150%, origin: top + left, rect(width: 2cm, height: 1cm, fill: teal)))

#place(dx: 1cm, dy: 8cm, curve(
  fill: olive.lighten(50%),
  stroke: 1.5pt + olive,
  curve.move((0pt, 0pt)),
  curve.cubic((20pt, -40pt), (60pt, -40pt), (80pt, 0pt)),
  curve.line((40pt, 30pt)),
  curve.close(),
))

#place(dx: 0cm, dy: 11cm, block[
  Text over shapes: #box(rect(width: 8pt, height: 8pt, fill: red)) inline square.
])
