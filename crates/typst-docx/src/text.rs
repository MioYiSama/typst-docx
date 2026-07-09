//! Anchored text boxes with exact baseline placement.
//!
//! Every merged line segment becomes one borderless, inset-free text box
//! anchored to the page. Inside the box, a single paragraph with
//! `w:lineRule="exact"` pins the baseline: Word places the baseline at
//! `box_top + line − descent`, where ascent/descent are the OS/2 Windows
//! metrics of the font (risk R1, verified by the M1 calibration fixture).

use ecow::EcoString;
use typst_library::layout::{Abs, Em, Transform};
use typst_library::text::{FontInstance, TextItem};
use typst_library::visualize::Paint;

use crate::Exporter;
use crate::frame::{self, Placed, PlacedItem, Placement};
use crate::write::{Anchor, Xml, drawing_run, emu, half_points, hex6, twips_ceil};

/// Extra text box width so `wrap="none"` never elides the last glyph.
fn width_margin() -> Abs {
    Abs::pt(2.0)
}

/// Render a maximal run of consecutive placed text items, merging them into
/// line segments where possible.
pub fn render_texts(exporter: &mut Exporter, items: &[Placed]) -> String {
    let mut out = String::new();
    let mut seg: Option<Segment> = None;

    for placed in items {
        let PlacedItem::Text(item) = &placed.item else { continue };

        if item.stroke.is_some() {
            exporter.warn("text stroke is not supported and was dropped");
        }

        match frame::classify(placed.transform) {
            Placement::Simple { x, y } => {
                for frag in fragments(item) {
                    let fx = x + frag.dx;
                    let fy = y + frag.dy;
                    // A fragment may join the open segment only if it starts
                    // exactly where Word's natural (unkerned) advances will
                    // place the next glyph; otherwise the merged run would
                    // render at a drifted position.
                    let matches = seg.as_ref().is_some_and(|s| {
                        !s.closed
                            && !frag.atomic
                            && s.font == item.font
                            && s.size == item.size
                            && s.fill == item.fill
                            && (fy - s.y).abs() <= Abs::pt(0.01)
                            && (fx - (s.x + s.width_word)).abs() <= Abs::pt(0.1)
                    });
                    if matches {
                        let s = seg.as_mut().unwrap();
                        s.text.push_str(&frag.text);
                        s.width_word += frag.width_word;
                    } else {
                        flush(&mut out, exporter, seg.take());
                        seg = Some(Segment::new(item, fx, fy, frag));
                    }
                }
            }
            Placement::Rotated { rot, scale } => {
                flush(&mut out, exporter, seg.take());
                out.push_str(&rotated_box(exporter, item, placed.transform, rot, scale));
            }
            Placement::Skewed => {
                exporter.warn(
                    "skewed or non-uniformly scaled text is not supported; \
                     only its translation is applied",
                );
                flush(&mut out, exporter, seg.take());
                let whole = whole_fragment(item);
                let s = Segment::new(item, placed.transform.tx, placed.transform.ty, whole);
                flush(&mut out, exporter, Some(s));
            }
        }
    }

    flush(&mut out, exporter, seg.take());
    out
}

/// An accumulated line segment.
struct Segment {
    font: FontInstance,
    size: Abs,
    fill: Paint,
    lang: EcoString,
    /// Page position of the segment's baseline start point.
    x: Abs,
    y: Abs,
    text: EcoString,
    /// Width by natural (unkerned) advances; matches Word's rendering and
    /// determines both the box size and where a continuation must start.
    width_word: Abs,
    /// No further fragments may be merged into this segment.
    closed: bool,
}

impl Segment {
    fn new(item: &TextItem, x: Abs, y: Abs, frag: Frag) -> Self {
        let mut lang = EcoString::from(item.lang.as_str());
        if let Some(region) = item.region {
            lang.push('-');
            lang.push_str(region.as_str());
        }
        Self {
            font: item.font.clone(),
            size: item.size,
            fill: item.fill.clone(),
            lang,
            x,
            y,
            text: frag.text,
            width_word: frag.width_word,
            closed: frag.atomic,
        }
    }
}

/// A slice of a text item that can be placed as one uninterrupted run.
struct Frag {
    /// Offset of the fragment's baseline start from the item origin.
    dx: Abs,
    /// Baseline shift (positive is down).
    dy: Abs,
    text: EcoString,
    width_word: Abs,
    /// Whether this fragment must stay alone in its box (offset glyphs).
    atomic: bool,
}

/// The whole item as a single fragment.
fn whole_fragment(item: &TextItem) -> Frag {
    let natural: Em = item
        .glyphs
        .iter()
        .map(|g| item.font.x_advance(g.id).unwrap_or(g.x_advance))
        .sum();
    Frag {
        dx: Abs::zero(),
        dy: Abs::zero(),
        text: item.text.clone(),
        width_word: natural.at(item.size),
        atomic: false,
    }
}

/// Split a text item into exactly positionable fragments.
///
/// Word renders each fragment with the font's natural advances, so within a
/// fragment the deviation between Typst's shaped advances (kerning,
/// justification, tracking) and the natural advances accumulates as visible
/// drift. A boundary is inserted as soon as the accumulated deviation
/// exceeds 0.1pt — the next fragment restarts at the exact cursor position —
/// and around glyphs with vertical or large horizontal offsets.
#[allow(unused_assignments, reason = "the closing macro always resets the widths")]
fn fragments(item: &TextItem) -> Vec<Frag> {
    let size = item.size;

    // Bail out to a single fragment for non-monotonic glyph-to-text mappings
    // (e.g. RTL scripts), where slicing per glyph would reorder the text.
    let mut last_end = 0;
    for glyph in &item.glyphs {
        let range = glyph.range();
        if range.start < last_end {
            return vec![whole_fragment(item)];
        }
        last_end = range.end;
    }

    let mut frags = Vec::new();
    let mut cursor = Abs::zero();
    // Current fragment state.
    let mut start_x = Abs::zero();
    let mut start_byte = None::<usize>;
    let mut end_byte = 0;
    let mut width_word = Abs::zero();
    // Accumulated shaped-minus-natural deviation within the fragment.
    let mut drift = Abs::zero();

    macro_rules! close {
        () => {
            if let Some(start) = start_byte.take() {
                frags.push(Frag {
                    dx: start_x,
                    dy: Abs::zero(),
                    text: item.text[start..end_byte].into(),
                    width_word,
                    atomic: false,
                });
                width_word = Abs::zero();
                drift = Abs::zero();
            }
        };
    }

    for glyph in &item.glyphs {
        let advance = glyph.x_advance.at(size);
        let natural = item.font.x_advance(glyph.id).unwrap_or(glyph.x_advance).at(size);
        let x_offset = glyph.x_offset.at(size);
        let y_offset = glyph.y_offset.at(size);
        let range = glyph.range();

        if y_offset != Abs::zero() || x_offset.abs() > Abs::pt(0.5) {
            // An offset glyph gets its own box at its exact position.
            close!();
            frags.push(Frag {
                dx: cursor + x_offset,
                // Typst's y-offset is y-up; page coordinates are y-down.
                dy: -y_offset,
                text: item.text[range].into(),
                width_word: natural,
                atomic: true,
            });
            cursor += advance;
            continue;
        }

        if start_byte.is_none() {
            start_x = cursor;
            start_byte = Some(range.start);
        }
        end_byte = range.end;
        width_word += natural;
        cursor += advance;
        drift += advance - natural;

        if drift.abs() > Abs::pt(0.1) {
            close!();
        }
    }

    close!();
    frags
}

/// The ascent/descent Word uses for baseline placement (both positive).
///
/// Word uses the OS/2 Windows metrics, unless the font sets the
/// USE_TYPO_METRICS bit in `fsSelection`, in which case the typographic
/// metrics apply. hhea is the last-resort fallback.
pub fn win_metrics(font: &FontInstance) -> (Em, Em) {
    let ttf = font.ttf();
    if let Some(os2) = ttf.tables().os2 {
        if os2.use_typographic_metrics() {
            let ascent = os2.typographic_ascender();
            let descent = -os2.typographic_descender();
            if ascent > 0 && descent >= 0 {
                return (font.to_em(ascent), font.to_em(descent));
            }
        }
        let ascent = os2.windows_ascender();
        // `windows_descender` is negated by ttf-parser; flip it back to the
        // spec's positive-down convention.
        let descent = -os2.windows_descender();
        if ascent > 0 && descent >= 0 {
            return (font.to_em(ascent), font.to_em(descent));
        }
    }
    (font.to_em(ttf.ascender()), font.to_em(-ttf.descender()))
}

/// Emit a finished segment as an anchored text box.
fn flush(out: &mut String, exporter: &mut Exporter, seg: Option<Segment>) {
    let Some(seg) = seg else { return };
    if seg.text.chars().all(char::is_whitespace) {
        return;
    }

    let (line, top) = baseline_box(&seg.font, seg.size, seg.y);
    let cx = emu(seg.width_word + width_margin());
    let cy = emu(line);

    let inner = textbox_shape(exporter, &seg, line, None);
    let id = exporter.next_docpr();
    let run = drawing_run(&Anchor {
        x: emu(seg.x),
        y: emu(top),
        cx,
        cy,
        id,
        name: &format!("t{id}"),
        z: exporter.next_rel_height(),
        behind: false,
        uri: "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
        inner: &inner,
    });
    out.push_str(&run);
}

/// Emit a whole rotated/scaled text item as one box.
fn rotated_box(
    exporter: &mut Exporter,
    item: &TextItem,
    transform: Transform,
    rot: f64,
    scale: f64,
) -> String {
    // Compute everything at the scaled size; the remaining transform is then
    // a pure rotation plus translation.
    let size = item.size * scale;
    let frag = whole_fragment(item);
    let seg = Segment::new(item, Abs::zero(), Abs::zero(), frag);
    let seg = Segment { size, width_word: seg.width_word * scale, ..seg };
    if seg.text.chars().all(char::is_whitespace) {
        return String::new();
    }

    let (line, top) = baseline_box(&seg.font, size, Abs::zero());
    let w = seg.width_word + width_margin();
    let cx = emu(w);
    let cy = emu(line);

    // Center of the scaled local box, mapped to unscaled local coordinates
    // and then through the full transform.
    let center_x = w / 2.0 / scale;
    let center_y = (top + line / 2.0) / scale;
    let (page_cx, page_cy) = frame::apply(transform, center_x, center_y);

    let rot_attr = normalize_rot(rot);
    let inner = textbox_shape(exporter, &seg, line, rot_attr);
    let id = exporter.next_docpr();
    drawing_run(&Anchor {
        x: emu(page_cx) - cx / 2,
        y: emu(page_cy) - cy / 2,
        cx,
        cy,
        id,
        name: &format!("t{id}"),
        z: exporter.next_rel_height(),
        behind: false,
        uri: "http://schemas.microsoft.com/office/word/2010/wordprocessingShape",
        inner: &inner,
    })
}

/// Word's `rot` attribute: 60000ths of a degree, clockwise, in [0, 21600000).
pub fn normalize_rot(degrees: f64) -> Option<i64> {
    let val = ((degrees * 60000.0).round() as i64).rem_euclid(21600000);
    (val != 0).then_some(val)
}

/// The exact line height (rounded up to whole twips) and resulting box top
/// for a baseline at `y`.
fn baseline_box(font: &FontInstance, size: Abs, y: Abs) -> (Abs, Abs) {
    let (ascent, descent) = win_metrics(font);
    let ascent = ascent.at(size);
    let descent = descent.at(size);
    let line_twips = twips_ceil(ascent + descent).max(1);
    let line = Abs::pt(line_twips as f64 / 20.0);
    let top = y - (line - descent);
    (line, top)
}

/// Build the `wps:wsp` fragment holding the segment's single run.
fn textbox_shape(
    exporter: &mut Exporter,
    seg: &Segment,
    line: Abs,
    rot: Option<i64>,
) -> String {
    let color = crate::paint::solid(exporter, &seg.fill, "text fill");
    let font_ref = exporter.font_ref(&seg.font);
    let sz = half_points(seg.size);
    let line_twips = twips_ceil(line).max(1);
    let cx = emu(seg.width_word + width_margin()).max(1);
    let cy = emu(line).max(1);

    let mut xml = Xml::new();
    xml.begin("wps:wsp");
    xml.begin("wps:cNvSpPr").attr("txBox", 1).end();
    xml.begin("wps:spPr");
    {
        xml.begin("a:xfrm");
        if let Some(rot) = rot {
            xml.attr("rot", rot);
        }
        xml.begin("a:off").attr("x", 0).attr("y", 0).end();
        xml.begin("a:ext").attr("cx", cx).attr("cy", cy).end();
        xml.end();
        xml.begin("a:prstGeom").attr("prst", "rect").leaf("a:avLst").end();
        xml.leaf("a:noFill");
        xml.begin("a:ln").leaf("a:noFill").end();
    }
    xml.end();
    xml.begin("wps:txbx");
    xml.begin("w:txbxContent");
    {
        xml.begin("w:p");
        xml.begin("w:pPr");
        xml.begin("w:spacing")
            .attr("w:before", 0)
            .attr("w:after", 0)
            .attr("w:line", line_twips)
            .attr("w:lineRule", "exact")
            .end();
        xml.begin("w:rPr");
        xml.begin("w:sz").attr("w:val", sz).end();
        xml.end();
        xml.end();

        xml.begin("w:r");
        xml.begin("w:rPr");
        xml.begin("w:rFonts")
            .attr("w:ascii", &font_ref.name)
            .attr("w:hAnsi", &font_ref.name)
            .attr("w:eastAsia", &font_ref.name)
            .attr("w:cs", &font_ref.name)
            .end();
        if font_ref.bold {
            xml.leaf("w:b");
        }
        if font_ref.italic {
            xml.leaf("w:i");
        }
        xml.begin("w:color").attr("w:val", hex6(&color)).end();
        xml.begin("w:kern").attr("w:val", 0).end();
        xml.begin("w:sz").attr("w:val", sz).end();
        xml.begin("w:szCs").attr("w:val", sz).end();
        if !seg.lang.is_empty() {
            xml.begin("w:lang")
                .attr("w:val", &seg.lang)
                .attr("w:eastAsia", &seg.lang)
                .end();
        }
        xml.end();
        xml.begin("w:t").attr("xml:space", "preserve").text(&seg.text).end();
        xml.end();

        xml.end();
    }
    xml.end();
    xml.end();
    xml.begin("wps:bodyPr")
        .attr("rot", 0)
        .attr("wrap", "none")
        .attr("lIns", 0)
        .attr("tIns", 0)
        .attr("rIns", 0)
        .attr("bIns", 0)
        .attr("anchor", "t")
        .attr("anchorCtr", 0);
    xml.leaf("a:noAutofit");
    xml.end();
    xml.end();
    xml.finish()
}
