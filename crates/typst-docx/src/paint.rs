//! Resolving Typst paints into solid colors.

use typst_library::visualize::{Color, Paint};

use crate::Exporter;

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
