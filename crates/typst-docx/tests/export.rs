//! End-to-end export tests: compile Typst source in memory, export DOCX,
//! and check structural invariants of the emitted XML.

use std::io::Read;

use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, LibraryExt, World};
use typst_layout::PagedDocument;

/// A minimal in-memory world with the embedded fonts.
struct TestWorld {
    library: LazyHash<Library>,
    book: LazyHash<FontBook>,
    fonts: Vec<Font>,
    source: Source,
}

impl TestWorld {
    fn new(text: &str) -> Self {
        let fonts: Vec<Font> =
            typst_kit::fonts::embedded().map(|(font, _)| font).collect();
        let book = FontBook::from_fonts(&fonts);
        let main = RootedPath::new(
            VirtualRoot::Project,
            VirtualPath::new("main.typ").unwrap(),
        )
        .intern();
        Self {
            library: LazyHash::new(Library::default()),
            book: LazyHash::new(book),
            fonts,
            source: Source::new(main, text.into()),
        }
    }
}

impl World for TestWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }

    fn book(&self) -> &LazyHash<FontBook> {
        &self.book
    }

    fn main(&self) -> FileId {
        self.source.id()
    }

    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.source.id() {
            Ok(self.source.clone())
        } else {
            Err(FileError::NotFound(id.vpath().get_without_slash().into()))
        }
    }

    fn file(&self, id: FileId) -> FileResult<Bytes> {
        Err(FileError::NotFound(id.vpath().get_without_slash().into()))
    }

    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.get(index).cloned()
    }

    fn today(&self, _: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Compile source and export it as a full DOCX output (bytes + warnings).
fn export_full(text: &str) -> typst_docx::DocxOutput {
    let world = TestWorld::new(text);
    let document = typst::compile::<PagedDocument>(&world)
        .output
        .expect("compilation failed");
    typst_docx::docx(&document)
}

/// Compile source and export it as DOCX bytes.
fn export(text: &str) -> Vec<u8> {
    export_full(text).bytes
}

/// Read a part from the DOCX zip as a string.
fn part(bytes: &[u8], name: &str) -> String {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut file = archive.by_name(name).unwrap_or_else(|_| panic!("missing {name}"));
    let mut out = String::new();
    file.read_to_string(&mut out).unwrap();
    out
}

/// Assert that a part parses as XML and return convenient accessors.
fn parse(xml: &str) -> roxmltree::Document<'_> {
    roxmltree::Document::parse(xml).expect("part is not well-formed XML")
}

#[test]
fn blank_page() {
    let bytes = export("#set page(width: 100pt, height: 50pt)\n");
    for name in [
        "[Content_Types].xml",
        "_rels/.rels",
        "word/document.xml",
        "word/_rels/document.xml.rels",
        "word/styles.xml",
        "word/settings.xml",
        "word/fontTable.xml",
    ] {
        parse(&part(&bytes, name));
    }

    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);

    // The last body child is the section properties with the exact page size
    // and schema-mandated child order.
    let body = doc
        .descendants()
        .find(|n| n.has_tag_name("body"))
        .expect("no w:body");
    let last = body
        .children()
        .filter(|n| n.is_element())
        .last()
        .expect("empty body");
    assert_eq!(last.tag_name().name(), "sectPr");
    let children: Vec<_> = last
        .children()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name().to_string())
        .collect();
    assert_eq!(children, ["type", "pgSz", "pgMar"]);
    let pgsz = last.children().find(|n| n.has_tag_name("pgSz")).unwrap();
    let w_attr = pgsz
        .attributes()
        .find(|a| a.name() == "w")
        .expect("no w:w on pgSz");
    assert_eq!(w_attr.value(), "2000"); // 100pt = 2000 twips
}

#[test]
fn text_anchor_structure() {
    let bytes = export(
        "#set page(width: 200pt, height: 100pt, margin: 10pt)\nHello World\n",
    );
    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);

    let anchors: Vec<_> =
        doc.descendants().filter(|n| n.has_tag_name("anchor")).collect();
    assert!(!anchors.is_empty(), "no wp:anchor for text");

    for anchor in &anchors {
        // Schema-mandated wp:anchor child order.
        let children: Vec<_> = anchor
            .children()
            .filter(|n| n.is_element())
            .map(|n| n.tag_name().name().to_string())
            .collect();
        assert_eq!(
            children,
            [
                "simplePos",
                "positionH",
                "positionV",
                "extent",
                "effectExtent",
                "wrapNone",
                "docPr",
                "cNvGraphicFramePr",
                "graphic"
            ]
        );

        // Extents must be positive.
        let extent = anchor.children().find(|n| n.has_tag_name("extent")).unwrap();
        for attr in ["cx", "cy"] {
            let value: i64 = extent
                .attributes()
                .find(|a| a.name() == attr)
                .unwrap()
                .value()
                .parse()
                .unwrap();
            assert!(value > 0, "non-positive extent {attr}");
        }
    }

    // Every docPr id is unique.
    let mut ids: Vec<&str> = doc
        .descendants()
        .filter(|n| n.has_tag_name("docPr"))
        .map(|n| n.attributes().find(|a| a.name() == "id").unwrap().value())
        .collect();
    let len = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), len, "duplicate docPr ids");

    // The run properties inside the text box follow the schema order.
    let rpr = doc
        .descendants()
        .filter(|n| n.has_tag_name("rPr"))
        .find(|n| n.children().any(|c| c.has_tag_name("rFonts")))
        .expect("no run properties with fonts");
    let names: Vec<_> = rpr
        .children()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name().to_string())
        .collect();
    let expected = [
        "rFonts", "b", "i", "noProof", "color", "kern", "sz", "szCs", "lang",
        "ligatures",
    ];
    let mut last_index = 0;
    for name in &names {
        let index = expected
            .iter()
            .position(|e| e == name)
            .unwrap_or_else(|| panic!("unexpected rPr child {name}"));
        assert!(index >= last_index, "rPr child {name} out of order");
        last_index = index;
    }
}

#[test]
fn multi_page_sections() {
    let bytes = export(
        "#set page(width: 100pt, height: 80pt)\nOne\n#pagebreak()\nTwo\n#pagebreak()\nThree\n",
    );
    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);

    // Three pages: two sectPr in paragraph properties + one final.
    let sects: Vec<_> =
        doc.descendants().filter(|n| n.has_tag_name("sectPr")).collect();
    assert_eq!(sects.len(), 3);
    let in_ppr = sects
        .iter()
        .filter(|n| n.parent().is_some_and(|p| p.has_tag_name("pPr")))
        .count();
    assert_eq!(in_ppr, 2);
}

#[test]
fn shapes_and_background() {
    let bytes = export(
        "#set page(width: 200pt, height: 200pt, margin: 0pt, fill: rgb(\"eeddcc\"))\n\
         #place(rect(width: 50pt, height: 40pt, fill: blue, stroke: 2pt + red))\n\
         #place(dy: 100pt, line(length: 150pt, stroke: 1pt + green))\n",
    );
    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);

    // Page background: a behindDoc anchor exists.
    let behind = doc.descendants().filter(|n| n.has_tag_name("anchor")).any(|n| {
        n.attributes().any(|a| a.name() == "behindDoc" && a.value() == "1")
    });
    assert!(behind, "no page background rectangle");

    // The line is emitted as a custom geometry.
    assert!(xml.contains("custGeom"), "no custGeom for line");
    assert!(xml.contains("prstGeom"), "no prstGeom for rect");
    // custGeom child order.
    let cust = doc
        .descendants()
        .find(|n| n.has_tag_name("custGeom"))
        .unwrap();
    let children: Vec<_> = cust
        .children()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name().to_string())
        .collect();
    assert_eq!(children, ["avLst", "gdLst", "ahLst", "cxnLst", "rect", "pathLst"]);
}

#[test]
fn linear_gradient_shape_fill() {
    let out = export_full(
        "#set page(width: 120pt, height: 80pt, margin: 0pt)\n\
         #place(rect(width: 80pt, height: 40pt, fill: gradient.linear(\
           (rgb(\"112233\"), 0%),\
           (rgb(\"44556680\"), 50%),\
           (rgb(\"778899\"), 100%),\
           angle: 45deg,\
         )))\n",
    );
    assert!(out.warnings.is_empty(), "unexpected warnings: {:?}", out.warnings);

    let xml = part(&out.bytes, "word/document.xml");
    let doc = parse(&xml);
    let grad = doc
        .descendants()
        .find(|n| n.has_tag_name("gradFill"))
        .expect("no gradient fill");
    assert_eq!(grad.attribute("rotWithShape"), Some("1"));

    let children: Vec<_> = grad
        .children()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name().to_string())
        .collect();
    assert_eq!(children, ["gsLst", "lin"]);

    let lin = grad.children().find(|n| n.has_tag_name("lin")).unwrap();
    assert_eq!(lin.attribute("ang"), Some("2700000"));
    assert_eq!(lin.attribute("scaled"), Some("1"));

    let stops: Vec<_> = grad.descendants().filter(|n| n.has_tag_name("gs")).collect();
    let positions: Vec<_> = stops.iter().map(|n| n.attribute("pos").unwrap()).collect();
    assert_eq!(positions, ["0", "50000", "100000"]);

    let colors: Vec<_> = stops
        .iter()
        .map(|stop| {
            stop.descendants()
                .find(|n| n.has_tag_name("srgbClr"))
                .unwrap()
                .attribute("val")
                .unwrap()
        })
        .collect();
    assert_eq!(colors, ["112233", "445566", "778899"]);

    let alpha = stops[1]
        .descendants()
        .find(|n| n.has_tag_name("alpha"))
        .expect("transparent stop lost alpha");
    assert_eq!(alpha.attribute("val"), Some("50196"));
}

#[test]
fn linear_gradient_angles_use_typst_direction() {
    let bytes = export(
        "#set page(width: 160pt, height: 160pt, margin: 0pt)\n\
         #place(rect(width: 20pt, height: 20pt, fill: gradient.linear(red, blue, angle: 0deg)))\n\
         #place(dy: 30pt, rect(width: 20pt, height: 20pt, fill: gradient.linear(red, blue, angle: 90deg)))\n\
         #place(dy: 60pt, rect(width: 20pt, height: 20pt, fill: gradient.linear(red, blue, angle: 180deg)))\n\
         #place(dy: 90pt, rect(width: 20pt, height: 20pt, fill: gradient.linear(red, blue, angle: 270deg)))\n",
    );

    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);
    let angles: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("lin"))
        .map(|n| n.attribute("ang").unwrap())
        .collect();
    assert_eq!(angles, ["0", "5400000", "10800000", "16200000"]);
}

#[test]
fn radial_gradient_shape_fill() {
    let out = export_full(
        "#set page(width: 120pt, height: 80pt, margin: 0pt)\n\
         #place(rect(width: 80pt, height: 40pt, fill: gradient.radial(\
           red,\
           blue,\
           center: (25%, 75%),\
         )))\n",
    );
    assert!(out.warnings.is_empty(), "unexpected warnings: {:?}", out.warnings);

    let xml = part(&out.bytes, "word/document.xml");
    let doc = parse(&xml);
    let grad = doc
        .descendants()
        .find(|n| n.has_tag_name("gradFill"))
        .expect("no gradient fill");
    assert_eq!(grad.attribute("rotWithShape"), Some("1"));

    let children: Vec<_> = grad
        .children()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name().to_string())
        .collect();
    assert_eq!(children, ["gsLst", "path"]);

    let path = grad.children().find(|n| n.has_tag_name("path")).unwrap();
    assert_eq!(path.attribute("path"), Some("circle"));
    let fill_to = path
        .children()
        .find(|n| n.has_tag_name("fillToRect"))
        .expect("no fillToRect");
    assert_eq!(fill_to.attribute("l"), Some("25000"));
    assert_eq!(fill_to.attribute("t"), Some("75000"));
    assert_eq!(fill_to.attribute("r"), Some("75000"));
    assert_eq!(fill_to.attribute("b"), Some("25000"));
}

#[test]
fn conic_gradient_shape_fill_warns_and_uses_solid_fallback() {
    let out = export_full(
        "#set page(width: 120pt, height: 80pt, margin: 0pt)\n\
         #place(rect(width: 80pt, height: 40pt, fill: gradient.conic(red, blue)))\n",
    );
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("conic gradients require raster fallback")),
        "no conic-gradient warning: {:?}",
        out.warnings
    );

    let xml = part(&out.bytes, "word/document.xml");
    let doc = parse(&xml);
    assert!(
        doc.descendants().all(|n| !n.has_tag_name("gradFill")),
        "conic gradient should not emit misleading native gradient OOXML"
    );
    assert!(
        doc.descendants().any(|n| n.has_tag_name("solidFill")),
        "conic fallback should emit a solid fill"
    );
}

#[test]
fn font_embedding() {
    let bytes = export("Hello\n");
    let table = part(&bytes, "word/fontTable.xml");
    assert!(table.contains("embedRegular"), "no embedded regular font");
    assert!(table.contains("fontKey"), "no font key");
    parse(&part(&bytes, "word/_rels/fontTable.xml.rels"));

    // The odttf part exists and differs from a raw font in its first bytes
    // (obfuscated header).
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes[..])).unwrap();
    let mut file = archive.by_name("word/fonts/font1.odttf").expect("no odttf");
    let mut head = [0u8; 4];
    std::io::Read::read_exact(&mut file, &mut head).unwrap();
    // A raw TrueType/CFF font starts with 00 01 00 00 or 'OTTO'; the
    // obfuscated header must not.
    assert_ne!(&head, &[0x00, 0x01, 0x00, 0x00]);
    assert_ne!(&head, b"OTTO");

    // Content types declare the odttf extension.
    let types = part(&bytes, "[Content_Types].xml");
    assert!(types.contains("obfuscatedFont"));
}

#[test]
fn justified_text_fragments() {
    // Narrow justified column forces stretched spaces; the exporter must
    // split fragments instead of emitting one drifting box.
    let bytes = export(
        "#set page(width: 120pt, height: 200pt, margin: 5pt)\n\
         #set par(justify: true)\n\
         one two three four five six seven eight nine ten\n",
    );
    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);
    let boxes = doc
        .descendants()
        .filter(|n| n.has_tag_name("txbx"))
        .count();
    assert!(boxes > 3, "expected multiple text boxes, got {boxes}");
}

#[test]
fn east_asian_and_ligature_props() {
    // Mixed CJK/Latin text must disable Word's re-shaping so exact positions
    // survive: autoSpace off, no re-ligation, no spell-check reflow.
    let bytes = export("#set page(width: 200pt, height: 100pt, margin: 10pt)\n中文 abc\n");
    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);

    // Paragraph properties order autoSpaceDE -> autoSpaceDN -> spacing, both off.
    let ppr = doc
        .descendants()
        .filter(|n| n.has_tag_name("pPr"))
        .find(|n| n.children().any(|c| c.has_tag_name("autoSpaceDE")))
        .expect("no pPr with autoSpaceDE");
    let names: Vec<_> = ppr
        .children()
        .filter(|n| n.is_element())
        .map(|n| n.tag_name().name().to_string())
        .collect();
    let de = names.iter().position(|n| n == "autoSpaceDE").unwrap();
    let dn = names.iter().position(|n| n == "autoSpaceDN").unwrap();
    let sp = names.iter().position(|n| n == "spacing").unwrap();
    assert!(de < dn && dn < sp, "pPr order wrong: {names:?}");
    for tag in ["autoSpaceDE", "autoSpaceDN"] {
        let node = ppr.children().find(|n| n.has_tag_name(tag)).unwrap();
        let val = node.attributes().find(|a| a.name() == "val").unwrap().value();
        assert_eq!(val, "0", "{tag} not disabled");
    }

    // The run turns off proofing and ligatures.
    assert!(
        doc.descendants().any(|n| n.has_tag_name("noProof")),
        "no w:noProof in run"
    );
    let ligatures = doc
        .descendants()
        .find(|n| n.has_tag_name("ligatures"))
        .expect("no w14:ligatures in run");
    let val = ligatures.attributes().find(|a| a.name() == "val").unwrap().value();
    assert_eq!(val, "none");
}

#[test]
fn math_box_merging() {
    // A multi-glyph script such as the "n+1" superscript must render as one
    // merged box, not one anchor per glyph. (Typst positions each script as a
    // separate translated text run rather than via glyph y-offsets, so the
    // merging happens in `render_texts`' segment accumulator.)
    let bytes = export(
        "#set page(width: 200pt, height: 60pt, margin: 5pt)\n$x^2 + y_i^2 = z^(n+1)$\n",
    );
    let xml = part(&bytes, "word/document.xml");
    let doc = parse(&xml);
    let texts: Vec<String> = doc
        .descendants()
        .filter(|n| n.has_tag_name("txbx"))
        .map(|b| {
            b.descendants()
                .filter(|n| n.has_tag_name("t"))
                .flat_map(|n| n.text())
                .collect::<String>()
        })
        .collect();
    // The superscript's three glyphs (n, +, 1) share one box.
    assert!(
        texts.iter().any(|t| t.ends_with("+1") && t.chars().count() == 3),
        "n+1 superscript not merged into one box: {texts:?}"
    );
    // The whole line stays well below one box per glyph.
    assert!(texts.len() <= 9, "too many boxes: {}", texts.len());
}

#[test]
fn oversize_page_warns() {
    // A page wider than Word's 22 inch limit is written as-is with a warning.
    let out = export_full("#set page(width: 30in, height: 5in)\nHi\n");
    assert!(
        out.warnings.iter().any(|w| w.contains("22 inch")),
        "no oversize-page warning: {:?}",
        out.warnings
    );
    let xml = part(&out.bytes, "word/document.xml");
    let doc = parse(&xml);
    let pgsz = doc.descendants().find(|n| n.has_tag_name("pgSz")).unwrap();
    let w: i64 = pgsz
        .attributes()
        .find(|a| a.name() == "w")
        .unwrap()
        .value()
        .parse()
        .unwrap();
    assert_eq!(w, 43200, "30in should still be written as 43200 twips");
}
