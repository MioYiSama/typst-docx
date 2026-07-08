//! A visually faithful DOCX exporter for Typst paged documents.
//!
//! This crate consumes a laid-out [`PagedDocument`] and produces an OOXML
//! WordprocessingML package in which every frame item is emitted as an
//! absolutely positioned drawing object. The goal is that the result is
//! visually indistinguishable from Typst's own PDF output at 100% zoom in
//! Microsoft Word 2013 or later; it is *not* a semantic conversion and the
//! output is not meant to be edited.

mod font;
mod frame;
mod image;
mod package;
mod page;
mod paint;
mod shape;
mod text;
mod write;

use ecow::EcoString;
use typst_layout::PagedDocument;

use crate::font::FontCollection;
use crate::image::MediaCollection;

/// The result of exporting a document.
pub struct DocxOutput {
    /// The bytes of the `.docx` file.
    pub bytes: Vec<u8>,
    /// Non-fatal problems encountered during export.
    pub warnings: Vec<EcoString>,
}

/// Export a paged document as a DOCX file.
pub fn docx(document: &PagedDocument) -> DocxOutput {
    let mut exporter = Exporter::new();
    let body = page::body(&mut exporter, document);
    let bytes = package::pack(&exporter, &body);
    DocxOutput { bytes, warnings: exporter.warnings }
}

/// Shared state accumulated over the whole export.
pub(crate) struct Exporter {
    pub(crate) fonts: FontCollection,
    pub(crate) media: MediaCollection,
    pub(crate) warnings: Vec<EcoString>,
    /// The last `wp:docPr` id handed out. Ids must be unique in the document.
    docpr_id: u32,
    /// The last `relativeHeight` handed out. Higher values are drawn on top.
    rel_height: u32,
}

impl Exporter {
    fn new() -> Self {
        Self {
            fonts: FontCollection::new(),
            media: MediaCollection::new(),
            warnings: Vec::new(),
            docpr_id: 0,
            rel_height: 251658240,
        }
    }

    /// Record a warning. Identical messages are deduplicated.
    pub(crate) fn warn(&mut self, message: impl Into<EcoString>) {
        let message = message.into();
        if !self.warnings.contains(&message) {
            self.warnings.push(message);
        }
    }

    /// The next unique `wp:docPr` id.
    pub(crate) fn next_docpr(&mut self) -> u32 {
        self.docpr_id += 1;
        self.docpr_id
    }

    /// The next z-order value, increasing in paint order.
    pub(crate) fn next_rel_height(&mut self) -> u32 {
        self.rel_height += 1;
        self.rel_height
    }
}
