//! The document body: one Word section per page.
//!
//! Each page becomes a single "carrier" paragraph that holds all of the
//! page's anchored drawings and, for all but the last page, its section
//! properties (page size, zero margins). The carrier line is 1 twip tall so
//! it contributes no visible content of its own. The last page's section
//! properties are the body-level `w:sectPr`.

use typst_layout::PagedDocument;
use typst_library::layout::{Abs, Size};

use crate::Exporter;
use crate::frame::{self, PlacedItem};
use crate::write::{Xml, hex6, twips};
use crate::{image, shape, text};

/// Build the contents of `w:body`.
pub fn body(exporter: &mut Exporter, document: &PagedDocument) -> String {
    let mut out = String::new();
    let pages = document.pages();

    if pages.is_empty() {
        // A document needs at least a final sectPr; default to A4.
        out.push_str(&sect_pr(Size::new(Abs::pt(595.28), Abs::pt(841.89))));
        return out;
    }

    for (index, page) in pages.iter().enumerate() {
        let size = page.frame.size();

        // Word rejects pages larger than 22 inches; write them anyway but flag
        // that Word may clamp or refuse the size.
        const MAX_TWIPS: i64 = 31680;
        if twips(size.x) > MAX_TWIPS || twips(size.y) > MAX_TWIPS {
            exporter.warn("page size exceeds Word's 22 inch limit; written as-is");
        }

        let mut runs = String::new();

        if let Some(paint) = page.fill_or_transparent() {
            let color = crate::paint::solid(exporter, &paint, "page background");
            // Word pages are white by default; only a non-white background
            // needs an explicit rectangle.
            if hex6(&color) != "ffffff" {
                runs.push_str(&shape::background(exporter, size, color));
            }
        }

        let placed = frame::collect(exporter, &page.frame);
        let mut i = 0;
        while i < placed.len() {
            match &placed[i].item {
                PlacedItem::Text(_) => {
                    // Batch consecutive text items so segments can merge.
                    let mut j = i;
                    while j < placed.len()
                        && matches!(placed[j].item, PlacedItem::Text(_))
                    {
                        j += 1;
                    }
                    runs.push_str(&text::render_texts(exporter, &placed[i..j]));
                    i = j;
                }
                PlacedItem::Shape(s) => {
                    runs.push_str(&shape::render(exporter, &placed[i], s));
                    i += 1;
                }
                PlacedItem::Image(img, size) => {
                    runs.push_str(&image::render(exporter, &placed[i], img, *size));
                    i += 1;
                }
            }
        }

        let last = index + 1 == pages.len();
        let sect = sect_pr(size);
        out.push_str(&carrier(&runs, (!last).then_some(sect.as_str())));
        if last {
            out.push_str(&sect);
        }
    }

    out
}

/// Build a `w:sectPr` for a page of the given size.
///
/// Child order is fixed by the schema: type, pgSz, pgMar.
fn sect_pr(size: Size) -> String {
    let mut xml = Xml::new();
    xml.begin("w:sectPr");
    xml.begin("w:type").attr("w:val", "nextPage").end();
    xml.begin("w:pgSz")
        .attr("w:w", twips(size.x).max(1))
        .attr("w:h", twips(size.y).max(1))
        .end();
    xml.begin("w:pgMar")
        .attr("w:top", 0)
        .attr("w:right", 0)
        .attr("w:bottom", 0)
        .attr("w:left", 0)
        .attr("w:header", 0)
        .attr("w:footer", 0)
        .attr("w:gutter", 0)
        .end();
    xml.end();
    xml.finish()
}

/// Build a page's carrier paragraph around its drawing runs.
fn carrier(runs: &str, sect: Option<&str>) -> String {
    let mut xml = Xml::new();
    xml.begin("w:p");
    xml.begin("w:pPr");
    xml.begin("w:spacing")
        .attr("w:before", 0)
        .attr("w:after", 0)
        .attr("w:line", 1)
        .attr("w:lineRule", "exact")
        .end();
    xml.begin("w:rPr");
    xml.begin("w:sz").attr("w:val", 2).end();
    xml.end();
    if let Some(sect) = sect {
        xml.raw(sect);
    }
    xml.end();
    xml.raw(runs);
    xml.end();
    xml.finish()
}
