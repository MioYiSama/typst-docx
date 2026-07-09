//! Anchored drawing shapes for lines, rectangles, and curves.

use typst_library::layout::{Abs, Point, Size};
use typst_library::visualize::{
    Color, CurveItem, FillRule, FixedStroke, Geometry, LineCap, LineJoin, Shape,
};

use crate::Exporter;
use crate::frame::{self, Placed, Placement};
use crate::text::normalize_rot;
use crate::write::{Anchor, Xml, drawing_run, emu, solid_fill};

const WPS_URI: &str =
    "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";

/// Render a single placed shape as an anchored drawing.
pub fn render(exporter: &mut Exporter, placed: &Placed, shape: &Shape) -> String {
    let bbox = shape.bbox(false);
    let width = bbox.max.x - bbox.min.x;
    let height = bbox.max.y - bbox.min.y;

    // Drop truly degenerate shapes; a thin line keeps one non-zero dimension.
    if !width.to_pt().is_finite()
        || !height.to_pt().is_finite()
        || (width <= Abs::zero() && height <= Abs::zero())
    {
        exporter.warn("zero-size or non-finite shape was dropped");
        return String::new();
    }

    let (x, y, scale, rot) = match frame::classify(placed.transform) {
        Placement::Simple { x, y } => {
            (emu(x + bbox.min.x), emu(y + bbox.min.y), 1.0, None)
        }
        Placement::Rotated { rot, scale } => {
            // Word rotates drawings around their center, so the box is placed
            // by mapping the local center through the full transform.
            let center_x = (bbox.min.x + bbox.max.x) / 2.0;
            let center_y = (bbox.min.y + bbox.max.y) / 2.0;
            let (page_cx, page_cy) = frame::apply(placed.transform, center_x, center_y);
            let cx = emu(width * scale).max(1);
            let cy = emu(height * scale).max(1);
            (
                emu(page_cx) - cx / 2,
                emu(page_cy) - cy / 2,
                scale,
                normalize_rot(rot),
            )
        }
        Placement::Skewed => {
            exporter.warn(
                "skewed or non-uniformly scaled shapes are not supported; \
                 only their translation is applied",
            );
            (
                emu(placed.transform.tx + bbox.min.x),
                emu(placed.transform.ty + bbox.min.y),
                1.0,
                None,
            )
        }
    };

    let inner =
        shape_fragment(exporter, shape, width * scale, height * scale, bbox.min, scale, rot);
    let id = exporter.next_docpr();
    drawing_run(&Anchor {
        x,
        y,
        cx: emu(width * scale).max(1),
        cy: emu(height * scale).max(1),
        id,
        name: &format!("s{id}"),
        z: exporter.next_rel_height(),
        behind: false,
        uri: WPS_URI,
        inner: &inner,
    })
}

/// Render a full-page background rectangle behind everything else.
pub fn background(exporter: &mut Exporter, size: Size, color: Color) -> String {
    let cx = emu(size.x).max(1);
    let cy = emu(size.y).max(1);

    let mut xml = Xml::new();
    xml.begin("wps:wsp");
    xml.leaf("wps:cNvSpPr");
    xml.begin("wps:spPr");
    xml.begin("a:xfrm");
    xml.begin("a:off").attr("x", 0).attr("y", 0).end();
    xml.begin("a:ext").attr("cx", cx).attr("cy", cy).end();
    xml.end();
    xml.begin("a:prstGeom").attr("prst", "rect").leaf("a:avLst").end();
    solid_fill(&mut xml, &color);
    xml.begin("a:ln").leaf("a:noFill").end();
    xml.end();
    xml.leaf("wps:bodyPr");
    xml.end();
    let inner = xml.finish();

    let id = exporter.next_docpr();
    drawing_run(&Anchor {
        x: 0,
        y: 0,
        cx,
        cy,
        id,
        name: &format!("bg{id}"),
        z: exporter.next_rel_height(),
        behind: true,
        uri: WPS_URI,
        inner: &inner,
    })
}

/// Build the `wps:wsp` fragment for a shape.
///
/// `width`/`height` are the (already scaled) box dimensions, `min` is the
/// unscaled local bounding box origin subtracted from all geometry
/// coordinates, and `scale` is applied to every local length.
fn shape_fragment(
    exporter: &mut Exporter,
    shape: &Shape,
    width: Abs,
    height: Abs,
    min: Point,
    scale: f64,
    rot: Option<i64>,
) -> String {
    let mut xml = Xml::new();
    xml.begin("wps:wsp");
    xml.leaf("wps:cNvSpPr");
    xml.begin("wps:spPr");

    xml.begin("a:xfrm");
    if let Some(rot) = rot {
        xml.attr("rot", rot);
    }
    xml.begin("a:off").attr("x", 0).attr("y", 0).end();
    xml.begin("a:ext")
        .attr("cx", emu(width).max(1))
        .attr("cy", emu(height).max(1))
        .end();
    xml.end();

    geometry(&mut xml, &shape.geometry, width, height, min, scale);

    match &shape.fill {
        Some(paint) => {
            if shape.fill_rule == FillRule::EvenOdd
                && matches!(shape.geometry, Geometry::Curve(_))
            {
                exporter.warn(
                    "the even-odd fill rule is not supported; overlapping \
                     curve regions may fill differently",
                );
            }
            let color = crate::paint::solid(exporter, paint, "shape fill");
            solid_fill(&mut xml, &color);
        }
        None => {
            xml.leaf("a:noFill");
        }
    }

    line(&mut xml, exporter, shape.stroke.as_ref(), scale);

    xml.end();
    xml.leaf("wps:bodyPr");
    xml.end();
    xml.finish()
}

/// Write the `a:prstGeom`/`a:custGeom` element.
fn geometry(
    xml: &mut Xml,
    geometry: &Geometry,
    width: Abs,
    height: Abs,
    min: Point,
    scale: f64,
) {
    match geometry {
        Geometry::Rect(_) => {
            xml.begin("a:prstGeom").attr("prst", "rect").leaf("a:avLst").end();
        }
        Geometry::Line(end) => {
            custom_geometry(xml, width, height, |xml| {
                let map = |p: Point| ((p.x - min.x) * scale, (p.y - min.y) * scale);
                move_to(xml, map(Point::zero()));
                line_to(xml, map(*end));
            });
        }
        Geometry::Curve(curve) => {
            custom_geometry(xml, width, height, |xml| {
                let map = |p: Point| ((p.x - min.x) * scale, (p.y - min.y) * scale);
                // DrawingML requires every subpath to start with a moveTo.
                let mut started = false;
                let mut current = Point::zero();
                for item in &curve.0 {
                    match item {
                        CurveItem::Move(p) => {
                            move_to(xml, map(*p));
                            started = true;
                            current = *p;
                        }
                        CurveItem::Line(p) => {
                            if !started {
                                move_to(xml, map(current));
                                started = true;
                            }
                            line_to(xml, map(*p));
                            current = *p;
                        }
                        CurveItem::Cubic(c1, c2, p) => {
                            if !started {
                                move_to(xml, map(current));
                                started = true;
                            }
                            xml.begin("a:cubicBezTo");
                            point(xml, map(*c1));
                            point(xml, map(*c2));
                            point(xml, map(*p));
                            xml.end();
                            current = *p;
                        }
                        CurveItem::Close => {
                            if started {
                                xml.leaf("a:close");
                            }
                            started = false;
                        }
                    }
                }
            });
        }
    }
}

/// Write an `a:custGeom` with a single path of the given size.
fn custom_geometry(
    xml: &mut Xml,
    width: Abs,
    height: Abs,
    path: impl FnOnce(&mut Xml),
) {
    xml.begin("a:custGeom");
    xml.leaf("a:avLst");
    xml.leaf("a:gdLst");
    xml.leaf("a:ahLst");
    xml.leaf("a:cxnLst");
    xml.begin("a:rect")
        .attr("l", 0)
        .attr("t", 0)
        .attr("r", 0)
        .attr("b", 0)
        .end();
    xml.begin("a:pathLst");
    xml.begin("a:path")
        .attr("w", emu(width).max(1))
        .attr("h", emu(height).max(1));
    path(xml);
    xml.end();
    xml.end();
    xml.end();
}

fn move_to(xml: &mut Xml, p: (Abs, Abs)) {
    xml.begin("a:moveTo");
    point(xml, p);
    xml.end();
}

fn line_to(xml: &mut Xml, p: (Abs, Abs)) {
    xml.begin("a:lnTo");
    point(xml, p);
    xml.end();
}

fn point(xml: &mut Xml, (x, y): (Abs, Abs)) {
    xml.begin("a:pt").attr("x", emu(x)).attr("y", emu(y)).end();
}

/// Write the `a:ln` stroke properties. An explicit `a:noFill` line is always
/// written so that Word does not fall back to its themed default outline.
fn line(xml: &mut Xml, exporter: &mut Exporter, stroke: Option<&FixedStroke>, scale: f64) {
    let Some(stroke) = stroke else {
        xml.begin("a:ln").leaf("a:noFill").end();
        return;
    };

    let thickness = stroke.thickness * scale;
    let cap = match stroke.cap {
        LineCap::Butt => "flat",
        LineCap::Round => "rnd",
        LineCap::Square => "sq",
    };

    xml.begin("a:ln").attr("w", emu(thickness).max(1)).attr("cap", cap);

    let color = crate::paint::solid(exporter, &stroke.paint, "stroke");
    solid_fill(xml, &color);

    if let Some(dash) = &stroke.dash
        && !dash.array.is_empty()
        && thickness > Abs::zero()
    {
        if dash.phase != Abs::zero() {
            exporter.warn("dash phase is not supported and was ignored");
        }
        // `a:ds` lengths are percentages of the line width in thousandths
        // of a percent. An odd array alternates roles on repetition, so it
        // is doubled first, matching SVG semantics.
        let mut array = dash.array.clone();
        if array.len() % 2 == 1 {
            array.extend_from_slice(&dash.array);
        }
        let permille = |len: Abs| {
            ((len / thickness) * 100000.0).round().max(1.0) as i64
        };
        xml.begin("a:custDash");
        for pair in array.chunks(2) {
            xml.begin("a:ds")
                .attr("d", permille(pair[0] * scale))
                .attr("sp", permille(pair[1] * scale))
                .end();
        }
        xml.end();
    }

    match stroke.join {
        LineJoin::Miter => {
            let lim = (stroke.miter_limit.get() * 100000.0).round() as i64;
            xml.begin("a:miter").attr("lim", lim.max(0)).end();
        }
        LineJoin::Round => {
            xml.leaf("a:round");
        }
        LineJoin::Bevel => {
            xml.leaf("a:bevel");
        }
    }

    xml.end();
}
