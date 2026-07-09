//! XML string building and unit conversions.

use std::fmt::Write as _;

use typst_library::layout::Abs;
use typst_library::visualize::Color;

/// XML namespaces used in `word/document.xml`.
pub const NS_W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const NS_R: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const NS_WP: &str =
    "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
pub const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const NS_PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
pub const NS_WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
pub const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
pub const NS_W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";

/// Convert a length to English Metric Units (1 pt = 12700 EMU).
///
/// A non-finite length (from a NaN or infinite transform) collapses to 0 so it
/// cannot produce a garbage coordinate that trips Word's repair dialog.
pub fn emu(len: Abs) -> i64 {
    let pt = len.to_pt();
    if !pt.is_finite() {
        return 0;
    }
    (pt * 12700.0).round() as i64
}

/// Convert a length to twentieths of a point.
pub fn twips(len: Abs) -> i64 {
    let pt = len.to_pt();
    if !pt.is_finite() {
        return 0;
    }
    (pt * 20.0).round() as i64
}

/// Convert a length to twips, rounding up.
pub fn twips_ceil(len: Abs) -> i64 {
    let pt = len.to_pt();
    if !pt.is_finite() {
        return 0;
    }
    (pt * 20.0).ceil() as i64
}

/// Convert a length to half-points (`w:sz` etc.), minimum 1.
pub fn half_points(len: Abs) -> i64 {
    ((len.to_pt() * 2.0).round() as i64).max(1)
}

/// The six-digit RRGGBB hex representation of a color, without `#`.
/// Alpha is dropped.
pub fn hex6(color: &Color) -> String {
    let hex = color.to_hex();
    hex[1..7].to_string()
}

/// A minimal streaming XML builder.
///
/// Elements are opened with [`begin`](Self::begin) and closed with
/// [`end`](Self::end); attributes must be written directly after `begin`.
/// Prebuilt fragments can be injected verbatim with [`raw`](Self::raw).
pub struct Xml {
    buf: String,
    stack: Vec<&'static str>,
    /// Whether the most recent start tag is still missing its closing `>`.
    open: bool,
}

impl Xml {
    pub fn new() -> Self {
        Self { buf: String::new(), stack: Vec::new(), open: false }
    }

    fn seal(&mut self) {
        if self.open {
            self.buf.push('>');
            self.open = false;
        }
    }

    /// Open an element. Must eventually be matched by [`end`](Self::end).
    pub fn begin(&mut self, tag: &'static str) -> &mut Self {
        self.seal();
        self.buf.push('<');
        self.buf.push_str(tag);
        self.stack.push(tag);
        self.open = true;
        self
    }

    /// Write an attribute on the currently opened element.
    pub fn attr(&mut self, key: &str, value: impl std::fmt::Display) -> &mut Self {
        debug_assert!(self.open, "attribute written outside of a start tag");
        let value = value.to_string();
        write!(self.buf, " {key}=\"{}\"", Escaped(&value)).unwrap();
        self
    }

    /// Close the most recently opened element.
    pub fn end(&mut self) -> &mut Self {
        let tag = self.stack.pop().expect("unbalanced end");
        if self.open {
            self.buf.push_str("/>");
            self.open = false;
        } else {
            self.buf.push_str("</");
            self.buf.push_str(tag);
            self.buf.push('>');
        }
        self
    }

    /// Write an empty element without attributes.
    pub fn leaf(&mut self, tag: &'static str) -> &mut Self {
        self.begin(tag).end()
    }

    /// Write escaped character data.
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.seal();
        write!(self.buf, "{}", Escaped(text)).unwrap();
        self
    }

    /// Inject a prebuilt XML fragment verbatim.
    pub fn raw(&mut self, xml: &str) -> &mut Self {
        self.seal();
        self.buf.push_str(xml);
        self
    }

    /// Finish building and return the XML string.
    pub fn finish(mut self) -> String {
        self.seal();
        debug_assert!(self.stack.is_empty(), "unclosed elements: {:?}", self.stack);
        self.buf
    }
}

/// Strip C0 control characters (except tab) from `text`.
///
/// These characters (`\u{0000}`–`\u{001F}`, tab excepted) are illegal in XML
/// 1.0 and are a common trigger for Word's repair dialog. Returns `Some` with
/// the cleaned string only if any character was removed, so callers can warn.
pub fn strip_c0_controls(text: &str) -> Option<String> {
    let bad = |c: char| (c as u32) < 0x20 && c != '\t';
    text.contains(bad).then(|| text.chars().filter(|&c| !bad(c)).collect())
}

/// Displays text with the five XML special characters escaped.
struct Escaped<'a>(&'a str);

impl std::fmt::Display for Escaped<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.0.chars() {
            match c {
                '&' => f.write_str("&amp;")?,
                '<' => f.write_str("&lt;")?,
                '>' => f.write_str("&gt;")?,
                '"' => f.write_str("&quot;")?,
                '\'' => f.write_str("&apos;")?,
                _ => std::fmt::Write::write_char(f, c)?,
            }
        }
        Ok(())
    }
}

/// The standard XML declaration written at the start of every part.
pub const XML_DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";

/// Parameters of an anchored drawing object.
pub struct Anchor<'a> {
    /// Page offset in EMU.
    pub x: i64,
    pub y: i64,
    /// Extent in EMU.
    pub cx: i64,
    pub cy: i64,
    /// Unique `wp:docPr` id and name.
    pub id: u32,
    pub name: &'a str,
    /// z-order; higher is on top.
    pub z: u32,
    /// Whether the drawing goes behind the text layer.
    pub behind: bool,
    /// The `a:graphicData` uri.
    pub uri: &'a str,
    /// The graphic fragment (`wps:wsp` or `pic:pic`).
    pub inner: &'a str,
}

/// Build a `<w:r><w:drawing><wp:anchor>…</w:r>` run around a graphic fragment.
pub fn drawing_run(anchor: &Anchor) -> String {
    let mut xml = Xml::new();
    xml.begin("w:r").begin("w:drawing");
    xml.begin("wp:anchor")
        .attr("distT", 0)
        .attr("distB", 0)
        .attr("distL", 0)
        .attr("distR", 0)
        .attr("simplePos", 0)
        .attr("relativeHeight", anchor.z)
        .attr("behindDoc", u8::from(anchor.behind))
        .attr("locked", 1)
        .attr("layoutInCell", 0)
        .attr("allowOverlap", 1);
    xml.begin("wp:simplePos").attr("x", 0).attr("y", 0).end();
    xml.begin("wp:positionH").attr("relativeFrom", "page");
    xml.begin("wp:posOffset").text(&anchor.x.to_string()).end();
    xml.end();
    xml.begin("wp:positionV").attr("relativeFrom", "page");
    xml.begin("wp:posOffset").text(&anchor.y.to_string()).end();
    xml.end();
    xml.begin("wp:extent")
        .attr("cx", anchor.cx.max(1))
        .attr("cy", anchor.cy.max(1))
        .end();
    xml.begin("wp:effectExtent")
        .attr("l", 0)
        .attr("t", 0)
        .attr("r", 0)
        .attr("b", 0)
        .end();
    xml.leaf("wp:wrapNone");
    xml.begin("wp:docPr").attr("id", anchor.id).attr("name", anchor.name).end();
    xml.leaf("wp:cNvGraphicFramePr");
    xml.begin("a:graphic");
    xml.begin("a:graphicData").attr("uri", anchor.uri).raw(anchor.inner).end();
    xml.end();
    xml.end();
    xml.end().end();
    xml.finish()
}

/// Write `<a:srgbClr val="RRGGBB">[<a:alpha…/>]</a:srgbClr>`.
pub fn srgb(xml: &mut Xml, color: &Color) {
    xml.begin("a:srgbClr").attr("val", hex6(color));
    let hex = color.to_hex();
    if hex.len() == 9
        && let Ok(alpha) = u8::from_str_radix(&hex[7..9], 16)
    {
        let val = (f64::from(alpha) / 255.0 * 100000.0).round() as i64;
        xml.begin("a:alpha").attr("val", val).end();
    }
    xml.end();
}

/// Write `<a:solidFill>` with the given color.
pub fn solid_fill(xml: &mut Xml, color: &Color) {
    xml.begin("a:solidFill");
    srgb(xml, color);
    xml.end();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        assert_eq!(emu(Abs::pt(1.0)), 12700);
        assert_eq!(emu(Abs::pt(72.0)), 914400);
        assert_eq!(twips(Abs::pt(595.3)), 11906);
        assert_eq!(twips_ceil(Abs::pt(12.541)), 251);
        assert_eq!(half_points(Abs::pt(11.0)), 22);
        assert_eq!(half_points(Abs::pt(0.1)), 1);
        // Non-finite lengths collapse to zero rather than saturating.
        assert_eq!(emu(Abs::pt(f64::NAN)), 0);
        assert_eq!(twips(Abs::pt(f64::INFINITY)), 0);
        assert_eq!(twips_ceil(Abs::pt(f64::NEG_INFINITY)), 0);
    }

    #[test]
    fn control_chars() {
        assert_eq!(strip_c0_controls("a\u{7}b\tc"), Some("ab\tc".to_string()));
        assert_eq!(strip_c0_controls("\u{0}\u{1f}"), Some(String::new()));
        assert_eq!(strip_c0_controls("abc"), None);
        assert_eq!(strip_c0_controls("tab\tkept"), None);
    }

    #[test]
    fn builder() {
        let mut xml = Xml::new();
        xml.begin("w:p")
            .begin("w:r")
            .attr("w:val", "a<b&\"c\"")
            .begin("w:t")
            .text("x < y & z")
            .end()
            .end()
            .end();
        assert_eq!(
            xml.finish(),
            "<w:p><w:r w:val=\"a&lt;b&amp;&quot;c&quot;\">\
             <w:t>x &lt; y &amp; z</w:t></w:r></w:p>"
        );
    }
}
