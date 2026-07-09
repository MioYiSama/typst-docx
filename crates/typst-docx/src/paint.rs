//! Resolving Typst paints into DrawingML fills.

use typst_library::foundations::Smart;
use typst_library::layout::Ratio;
use typst_library::visualize::{Color, Gradient, Paint, RelativeTo};

use crate::Exporter;
use crate::write::{self, Xml, solid_fill};

/// Reduce a paint to a solid color.
///
/// Gradients collapse to their first stop and tilings to black, both with a
/// warning, since WordprocessingML drawings in this exporter only use solid
/// fills.
pub fn solid(exporter: &mut Exporter, paint: &Paint, context: &str) -> Color {
    match paint {
        Paint::Solid(color) => color.clone(),
        Paint::Gradient(gradient) => {
            exporter.warn(format!(
                "{context}: gradients are not supported, using the first stop color"
            ));
            gradient
                .stops_ref()
                .first()
                .map(|(color, _)| color.clone())
                .unwrap_or(Color::BLACK)
        }
        Paint::Tiling(_) => {
            exporter.warn(format!(
                "{context}: tiling fills are not supported, using black"
            ));
            Color::BLACK
        }
    }
}

/// Write a DrawingML fill for shape interiors.
///
/// Shape fills can preserve the common DrawingML-native gradient forms. Other
/// paint users (text, strokes, and page backgrounds) intentionally stay on the
/// solid fallback path until their own OOXML support exists.
pub fn fill(xml: &mut Xml, exporter: &mut Exporter, paint: &Paint, context: &str) {
    match paint {
        Paint::Solid(color) => solid_fill(xml, color),
        Paint::Gradient(gradient) => gradient_fill(xml, exporter, gradient, context),
        Paint::Tiling(_) => {
            exporter.warn(format!(
                "{context}: tiling fills are not supported, using black"
            ));
            solid_fill(xml, &Color::BLACK);
        }
    }
}

fn gradient_fill(xml: &mut Xml, exporter: &mut Exporter, gradient: &Gradient, context: &str) {
    match gradient {
        Gradient::Linear(linear) => {
            if relative_to_parent(&linear.relative) {
                exporter.warn(format!(
                    "{context}: parent-relative linear gradients are approximated \
                     against the shape bounding box"
                ));
            }

            xml.begin("a:gradFill").attr("rotWithShape", 1);
            gradient_stops(xml, &linear.stops);
            let angle = (linear.angle.to_deg().rem_euclid(360.0) * 60000.0).round() as i64;
            xml.begin("a:lin")
                .attr("ang", angle)
                .attr("scaled", 1)
                .end();
            xml.end();
        }
        Gradient::Radial(radial) => {
            if relative_to_parent(&radial.relative) {
                exporter.warn(format!(
                    "{context}: parent-relative radial gradients are approximated \
                     against the shape bounding box"
                ));
            }
            if radial.focal_center != radial.center {
                exporter.warn(format!(
                    "{context}: radial gradient focal-center is not supported, \
                     using the center point"
                ));
            }
            if !radial.focal_radius.is_zero() {
                exporter.warn(format!(
                    "{context}: radial gradient focal-radius is not supported \
                     and was ignored"
                ));
            }
            if radial.radius != Ratio::new(0.5) {
                exporter.warn(format!(
                    "{context}: radial gradient radius is approximated by \
                     DrawingML fillToRect"
                ));
            }

            xml.begin("a:gradFill").attr("rotWithShape", 1);
            gradient_stops(xml, &radial.stops);
            xml.begin("a:path").attr("path", "circle");
            xml.begin("a:fillToRect")
                .attr("l", pct100k(radial.center.x))
                .attr("t", pct100k(radial.center.y))
                .attr("r", pct100k(Ratio::new(1.0 - radial.center.x.get())))
                .attr("b", pct100k(Ratio::new(1.0 - radial.center.y.get())))
                .end();
            xml.end();
            xml.end();
        }
        Gradient::Conic(conic) => {
            exporter.warn(format!(
                "{context}: conic gradients require raster fallback, using the \
                 first stop color"
            ));
            solid_fill(xml, &first_stop(&conic.stops));
        }
    }
}

fn gradient_stops(xml: &mut Xml, stops: &[(Color, Ratio)]) {
    xml.begin("a:gsLst");
    for (color, offset) in stops {
        xml.begin("a:gs").attr("pos", pct100k(*offset));
        write::srgb(xml, color);
        xml.end();
    }
    xml.end();
}

fn first_stop(stops: &[(Color, Ratio)]) -> Color {
    stops
        .first()
        .map(|(color, _)| color.clone())
        .unwrap_or(Color::BLACK)
}

fn pct100k(ratio: Ratio) -> i64 {
    (ratio.get() * 100000.0).round().clamp(0.0, 100000.0) as i64
}

fn relative_to_parent(relative: &Smart<RelativeTo>) -> bool {
    matches!(relative, Smart::Custom(RelativeTo::Parent))
}
