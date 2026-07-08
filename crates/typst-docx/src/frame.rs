//! Flattening a page frame into a list of absolutely placed items.

use typst_library::layout::{Abs, Frame, FrameItem, Size, Transform};
use typst_library::text::TextItem;
use typst_library::visualize::{Image, Shape};

use crate::Exporter;

/// A frame item together with its accumulated page-space transform.
pub struct Placed<'a> {
    pub transform: Transform,
    pub item: PlacedItem<'a>,
}

/// The payload of a placed item.
pub enum PlacedItem<'a> {
    Text(&'a TextItem),
    Shape(&'a Shape),
    Image(&'a Image, Size),
}

/// Flatten a page frame into placed items in paint order.
pub fn collect<'a>(exporter: &mut Exporter, frame: &'a Frame) -> Vec<Placed<'a>> {
    let mut out = Vec::new();
    walk(exporter, frame, Transform::identity(), &mut out);
    out
}

fn walk<'a>(
    exporter: &mut Exporter,
    frame: &'a Frame,
    transform: Transform,
    out: &mut Vec<Placed<'a>>,
) {
    for (pos, item) in frame.items() {
        let transform = transform.pre_concat(Transform::translate(pos.x, pos.y));
        match item {
            FrameItem::Group(group) => {
                if group.clip.is_some() {
                    exporter.warn(
                        "clipping is not supported, contents are drawn unclipped",
                    );
                }
                walk(exporter, &group.frame, transform.pre_concat(group.transform), out);
            }
            FrameItem::Text(text) => {
                out.push(Placed { transform, item: PlacedItem::Text(text) });
            }
            FrameItem::Shape(shape, _) => {
                out.push(Placed { transform, item: PlacedItem::Shape(shape) });
            }
            FrameItem::Image(image, size, _) => {
                out.push(Placed { transform, item: PlacedItem::Image(image, *size) });
            }
            FrameItem::Link(..) => {
                exporter.warn("links are dropped in the DOCX output");
            }
            FrameItem::Tag(_) => {}
        }
    }
}

/// How to place an object on the page.
pub enum Placement {
    /// Pure translation: the object's local origin ends up at `(x, y)`.
    Simple { x: Abs, y: Abs },
    /// Rotation and/or uniform scaling around the object.
    ///
    /// `rot` is in clockwise degrees, `scale` is the uniform scale factor.
    /// Positioning must be done via the transformed center of the object's
    /// local bounding box, since Word rotates drawings around their center.
    Rotated { rot: f64, scale: f64 },
    /// A skew or non-uniform scale that WordprocessingML cannot express.
    Skewed,
}

/// Classify the linear part of a transform.
pub fn classify(transform: Transform) -> Placement {
    const EPS: f64 = 1e-6;
    let sx = transform.sx.get();
    let kx = transform.kx.get();
    let ky = transform.ky.get();
    let sy = transform.sy.get();

    if (sx - 1.0).abs() < EPS && (sy - 1.0).abs() < EPS && kx.abs() < EPS && ky.abs() < EPS
    {
        Placement::Simple { x: transform.tx, y: transform.ty }
    } else if (sx - sy).abs() < EPS && (kx + ky).abs() < EPS {
        // A similarity transform: rotation by `atan2(ky, sx)` (clockwise in
        // the y-down page coordinate system, matching Word's `rot` sense)
        // combined with a uniform scale.
        let scale = sx.hypot(ky);
        if scale < EPS {
            return Placement::Skewed;
        }
        let rot = ky.atan2(sx).to_degrees();
        Placement::Rotated { rot, scale }
    } else {
        Placement::Skewed
    }
}

/// Map a local point through a transform.
pub fn apply(transform: Transform, x: Abs, y: Abs) -> (Abs, Abs) {
    (
        transform.tx + x * transform.sx.get() + y * transform.kx.get(),
        transform.ty + x * transform.ky.get() + y * transform.sy.get(),
    )
}
