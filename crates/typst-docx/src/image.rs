//! Anchored raster images.

use std::collections::HashMap;

use typst_library::foundations::Bytes;
use typst_library::layout::Size;
use typst_library::visualize::{ExchangeFormat, Image, ImageKind, RasterFormat};

use crate::Exporter;
use crate::frame::{self, Placed, Placement};
use crate::text::normalize_rot;
use crate::write::{Anchor, Xml, drawing_run, emu};

/// Collects all image files referenced by the document.
pub struct MediaCollection {
    /// Cache of per-image registration results.
    map: HashMap<Image, Option<usize>>,
    /// Registered media files, in order (`media/image{i+1}.{ext}`).
    pub items: Vec<Media>,
}

/// A media file stored in the package.
pub struct Media {
    pub data: Bytes,
    pub ext: &'static str,
    pub content_type: &'static str,
}

impl MediaCollection {
    pub fn new() -> Self {
        Self { map: HashMap::new(), items: Vec::new() }
    }

    /// The distinct (extension, content type) pairs of all media files.
    pub fn content_types(&self) -> Vec<(&'static str, &'static str)> {
        let mut seen = Vec::new();
        for media in &self.items {
            if !seen.contains(&(media.ext, media.content_type)) {
                seen.push((media.ext, media.content_type));
            }
        }
        seen
    }
}

impl Exporter {
    /// Register an image in the media collection. Returns its index, or
    /// `None` (with a warning) if the image kind cannot be embedded.
    fn media_index(&mut self, image: &Image) -> Option<usize> {
        if let Some(&cached) = self.media.map.get(image) {
            return cached;
        }

        let result = match image.kind() {
            ImageKind::Raster(raster) => match raster.format() {
                RasterFormat::Exchange(format) => {
                    let (ext, content_type) = match format {
                        ExchangeFormat::Png => ("png", "image/png"),
                        ExchangeFormat::Jpg => ("jpg", "image/jpeg"),
                        ExchangeFormat::Gif => ("gif", "image/gif"),
                        ExchangeFormat::Webp => {
                            self.warn(
                                "WebP images are embedded as-is; some Word \
                                 versions cannot display them",
                            );
                            ("webp", "image/webp")
                        }
                    };
                    self.media.items.push(Media {
                        data: raster.data().clone(),
                        ext,
                        content_type,
                    });
                    Some(self.media.items.len() - 1)
                }
                RasterFormat::Pixel(_) => {
                    self.warn("raw pixel images are not supported and were skipped");
                    None
                }
            },
            ImageKind::Svg(_) => {
                self.warn("SVG images are not supported and were skipped");
                None
            }
            ImageKind::Pdf(_) => {
                self.warn("PDF images are not supported and were skipped");
                None
            }
        };

        self.media.map.insert(image.clone(), result);
        result
    }
}

/// Render a single placed image as an anchored drawing.
pub fn render(
    exporter: &mut Exporter,
    placed: &Placed,
    image: &Image,
    size: Size,
) -> String {
    let Some(index) = exporter.media_index(image) else {
        return String::new();
    };

    let (x, y, scale, rot) = match frame::classify(placed.transform) {
        Placement::Simple { x, y } => (emu(x), emu(y), 1.0, None),
        Placement::Rotated { rot, scale } => {
            let (page_cx, page_cy) =
                frame::apply(placed.transform, size.x / 2.0, size.y / 2.0);
            let cx = emu(size.x * scale).max(1);
            let cy = emu(size.y * scale).max(1);
            (
                emu(page_cx) - cx / 2,
                emu(page_cy) - cy / 2,
                scale,
                normalize_rot(rot),
            )
        }
        Placement::Skewed => {
            exporter.warn(
                "skewed or non-uniformly scaled images are not supported; \
                 only their translation is applied",
            );
            (emu(placed.transform.tx), emu(placed.transform.ty), 1.0, None)
        }
    };

    let cx = emu(size.x * scale).max(1);
    let cy = emu(size.y * scale).max(1);
    let id = exporter.next_docpr();

    let mut xml = Xml::new();
    xml.begin("pic:pic");
    xml.begin("pic:nvPicPr");
    xml.begin("pic:cNvPr").attr("id", id).attr("name", format!("i{id}")).end();
    xml.leaf("pic:cNvPicPr");
    xml.end();
    xml.begin("pic:blipFill");
    xml.begin("a:blip").attr("r:embed", format!("rIdImg{}", index + 1)).end();
    xml.begin("a:stretch").leaf("a:fillRect").end();
    xml.end();
    xml.begin("pic:spPr");
    xml.begin("a:xfrm");
    if let Some(rot) = rot {
        xml.attr("rot", rot);
    }
    xml.begin("a:off").attr("x", 0).attr("y", 0).end();
    xml.begin("a:ext").attr("cx", cx).attr("cy", cy).end();
    xml.end();
    xml.begin("a:prstGeom").attr("prst", "rect").leaf("a:avLst").end();
    xml.end();
    xml.end();
    let inner = xml.finish();

    drawing_run(&Anchor {
        x,
        y,
        cx,
        cy,
        id,
        name: &format!("i{id}"),
        z: exporter.next_rel_height(),
        behind: false,
        uri: "http://schemas.openxmlformats.org/drawingml/2006/picture",
        inner: &inner,
    })
}
