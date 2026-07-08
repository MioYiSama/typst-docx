// M3: raster images, plain / sized / rotated.
#set page(paper: "a5", margin: 1.5cm)

#image("sample.png", width: 4cm)

#v(1cm)
#rotate(15deg, image("sample.png", width: 3cm))

#v(1cm)
Text after images.
