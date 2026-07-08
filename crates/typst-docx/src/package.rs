//! Assembling the OPC package (the `.docx` ZIP file).

use std::io::{Cursor, Write as _};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::Exporter;
use crate::font::obfuscate;
use crate::write::{NS_A, NS_MC, NS_PIC, NS_R, NS_W, NS_W14, NS_WP, NS_WPS, XML_DECL, Xml};

const REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const REL_TYPE_BASE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_BASE: &str = "application/vnd.openxmlformats-officedocument.wordprocessingml";

/// Pack all parts into the final `.docx` bytes.
pub fn pack(exporter: &Exporter, body: &str) -> Vec<u8> {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let deflate =
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    // Image formats are already compressed.
    let store =
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    let mut put = |name: &str, data: &[u8], options: SimpleFileOptions| {
        zip.start_file(name, options).expect("failed to start zip entry");
        zip.write_all(data).expect("failed to write zip entry");
    };

    put("[Content_Types].xml", content_types(exporter).as_bytes(), deflate);
    put("_rels/.rels", package_rels().as_bytes(), deflate);
    put("word/document.xml", document(body).as_bytes(), deflate);
    put(
        "word/_rels/document.xml.rels",
        document_rels(exporter).as_bytes(),
        deflate,
    );
    put("word/styles.xml", styles().as_bytes(), deflate);
    put("word/settings.xml", settings(exporter).as_bytes(), deflate);
    put("word/fontTable.xml", exporter.fonts.font_table().as_bytes(), deflate);

    if exporter.fonts.has_embeds() {
        put(
            "word/_rels/fontTable.xml.rels",
            exporter.fonts.font_table_rels().as_bytes(),
            deflate,
        );
        for (index, embed) in exporter.fonts.embeds.iter().enumerate() {
            put(
                &format!("word/fonts/font{}.odttf", index + 1),
                &obfuscate(&embed.data, &embed.guid),
                deflate,
            );
        }
    }

    for (index, media) in exporter.media.items.iter().enumerate() {
        put(
            &format!("word/media/image{}.{}", index + 1, media.ext),
            &media.data,
            store,
        );
    }

    zip.finish().expect("failed to finish zip").into_inner()
}

/// Build `[Content_Types].xml`.
fn content_types(exporter: &Exporter) -> String {
    let mut xml = Xml::new();
    xml.begin("Types")
        .attr("xmlns", "http://schemas.openxmlformats.org/package/2006/content-types");

    let mut default = |ext: &str, ty: &str| {
        xml.begin("Default")
            .attr("Extension", ext)
            .attr("ContentType", ty)
            .end();
    };
    default("rels", "application/vnd.openxmlformats-package.relationships+xml");
    default("xml", "application/xml");
    if exporter.fonts.has_embeds() {
        default("odttf", "application/vnd.openxmlformats-officedocument.obfuscatedFont");
    }
    for (ext, content_type) in exporter.media.content_types() {
        default(ext, content_type);
    }

    let mut over = |part: &str, ty: String| {
        xml.begin("Override")
            .attr("PartName", part)
            .attr("ContentType", ty)
            .end();
    };
    over("/word/document.xml", format!("{CT_BASE}.document.main+xml"));
    over("/word/styles.xml", format!("{CT_BASE}.styles+xml"));
    over("/word/settings.xml", format!("{CT_BASE}.settings+xml"));
    over("/word/fontTable.xml", format!("{CT_BASE}.fontTable+xml"));

    xml.end();
    format!("{XML_DECL}{}", xml.finish())
}

/// Build `_rels/.rels`.
fn package_rels() -> String {
    let mut xml = Xml::new();
    xml.begin("Relationships").attr("xmlns", REL_NS);
    xml.begin("Relationship")
        .attr("Id", "rId1")
        .attr("Type", format!("{REL_TYPE_BASE}/officeDocument"))
        .attr("Target", "word/document.xml")
        .end();
    xml.end();
    format!("{XML_DECL}{}", xml.finish())
}

/// Build `word/document.xml` around the body content.
fn document(body: &str) -> String {
    let mut xml = Xml::new();
    xml.begin("w:document")
        .attr("xmlns:w", NS_W)
        .attr("xmlns:r", NS_R)
        .attr("xmlns:wp", NS_WP)
        .attr("xmlns:a", NS_A)
        .attr("xmlns:pic", NS_PIC)
        .attr("xmlns:wps", NS_WPS)
        .attr("xmlns:mc", NS_MC)
        .attr("xmlns:w14", NS_W14)
        .attr("mc:Ignorable", "w14");
    xml.begin("w:body").raw(body).end();
    xml.end();
    format!("{XML_DECL}{}", xml.finish())
}

/// Build `word/_rels/document.xml.rels`.
fn document_rels(exporter: &Exporter) -> String {
    let mut xml = Xml::new();
    xml.begin("Relationships").attr("xmlns", REL_NS);
    let mut rel = |id: String, ty: String, target: String| {
        xml.begin("Relationship")
            .attr("Id", id)
            .attr("Type", ty)
            .attr("Target", target)
            .end();
    };
    rel(
        "rId1".into(),
        format!("{REL_TYPE_BASE}/styles"),
        "styles.xml".into(),
    );
    rel(
        "rId2".into(),
        format!("{REL_TYPE_BASE}/settings"),
        "settings.xml".into(),
    );
    rel(
        "rId3".into(),
        format!("{REL_TYPE_BASE}/fontTable"),
        "fontTable.xml".into(),
    );
    for (index, media) in exporter.media.items.iter().enumerate() {
        rel(
            format!("rIdImg{}", index + 1),
            format!("{REL_TYPE_BASE}/image"),
            format!("media/image{}.{}", index + 1, media.ext),
        );
    }
    xml.end();
    format!("{XML_DECL}{}", xml.finish())
}

/// Build a minimal `word/styles.xml`.
fn styles() -> String {
    let mut xml = Xml::new();
    xml.begin("w:styles").attr("xmlns:w", NS_W);
    xml.begin("w:docDefaults");
    xml.begin("w:rPrDefault");
    xml.begin("w:rPr");
    xml.begin("w:kern").attr("w:val", 0).end();
    xml.end();
    xml.end();
    xml.begin("w:pPrDefault");
    xml.begin("w:pPr");
    xml.begin("w:spacing").attr("w:before", 0).attr("w:after", 0).end();
    xml.end();
    xml.end();
    xml.end();
    xml.begin("w:style")
        .attr("w:type", "paragraph")
        .attr("w:default", 1)
        .attr("w:styleId", "Normal");
    xml.begin("w:name").attr("w:val", "Normal").end();
    xml.end();
    xml.end();
    format!("{XML_DECL}{}", xml.finish())
}

/// Build `word/settings.xml`.
///
/// Element order follows the `CT_Settings` sequence: `embedTrueTypeFonts`
/// comes before `compat`.
fn settings(exporter: &Exporter) -> String {
    let mut xml = Xml::new();
    xml.begin("w:settings").attr("xmlns:w", NS_W);
    if exporter.fonts.has_embeds() {
        xml.leaf("w:embedTrueTypeFonts");
    }
    xml.begin("w:compat");
    xml.begin("w:compatSetting")
        .attr("w:name", "compatibilityMode")
        .attr("w:uri", "http://schemas.microsoft.com/office/word")
        .attr("w:val", 15)
        .end();
    xml.end();
    xml.end();
    format!("{XML_DECL}{}", xml.finish())
}
