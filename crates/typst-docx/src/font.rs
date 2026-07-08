//! Font referencing, deduplication, and ODTTF embedding.

use std::collections::{BTreeMap, HashMap};

use ecow::{EcoString, eco_format};
use ttf_parser::name_id;
use typst_library::foundations::Bytes;
use typst_library::text::{Font, FontStyle, FontWeight};

use crate::Exporter;
use crate::write::{XML_DECL, Xml};

/// How a run refers to a font in `w:rPr`.
#[derive(Clone)]
pub struct FontRef {
    /// The value for all four `w:rFonts` slots.
    pub name: EcoString,
    /// Whether the run sets `w:b`.
    pub bold: bool,
    /// Whether the run sets `w:i`.
    pub italic: bool,
}

/// The RIBBI slot a face occupies within a `w:font` entry.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
enum Slot {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

/// A font file prepared for embedding.
pub struct Embed {
    /// Raw (unobfuscated) font file data.
    pub data: Bytes,
    /// The obfuscation GUID, derived deterministically from the data.
    pub guid: [u8; 16],
}

/// Per-family record in the font table.
#[derive(Default)]
struct Family {
    /// Indices into `FontCollection::embeds` per slot.
    slots: Vec<(Slot, usize)>,
}

/// Collects all fonts referenced by the document.
pub struct FontCollection {
    /// Cache of per-font resolution results.
    map: HashMap<Font, FontRef>,
    /// Families keyed by the `w:name` under which they appear in fontTable.
    families: BTreeMap<EcoString, Family>,
    /// Embedded font files, in registration order (`font{i+1}.odttf`).
    pub embeds: Vec<Embed>,
}

impl FontCollection {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            families: BTreeMap::new(),
            embeds: Vec::new(),
        }
    }

    /// Whether any fonts will be embedded.
    pub fn has_embeds(&self) -> bool {
        !self.embeds.is_empty()
    }

    /// Build `word/fontTable.xml`.
    pub fn font_table(&self) -> String {
        let mut xml = Xml::new();
        xml.begin("w:fonts")
            .attr("xmlns:w", crate::write::NS_W)
            .attr("xmlns:r", crate::write::NS_R);
        for (name, family) in &self.families {
            xml.begin("w:font").attr("w:name", name);
            xml.begin("w:charset").attr("w:val", "00").end();
            xml.begin("w:family").attr("w:val", "auto").end();
            xml.begin("w:pitch").attr("w:val", "variable").end();
            for &(slot, index) in &family.slots {
                let tag = match slot {
                    Slot::Regular => "w:embedRegular",
                    Slot::Bold => "w:embedBold",
                    Slot::Italic => "w:embedItalic",
                    Slot::BoldItalic => "w:embedBoldItalic",
                };
                xml.begin(tag)
                    .attr("r:id", format!("rId{}", index + 1))
                    .attr(
                        "w:fontKey",
                        format!("{{{}}}", guid_string(&self.embeds[index].guid)),
                    )
                    .end();
            }
            xml.end();
        }
        xml.end();
        format!("{XML_DECL}{}", xml.finish())
    }

    /// Build `word/_rels/fontTable.xml.rels`.
    pub fn font_table_rels(&self) -> String {
        let mut xml = Xml::new();
        xml.begin("Relationships").attr(
            "xmlns",
            "http://schemas.openxmlformats.org/package/2006/relationships",
        );
        for (index, _) in self.embeds.iter().enumerate() {
            xml.begin("Relationship")
                .attr("Id", format!("rId{}", index + 1))
                .attr(
                    "Type",
                    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/font",
                )
                .attr("Target", format!("fonts/font{}.odttf", index + 1))
                .end();
        }
        xml.end();
        format!("{XML_DECL}{}", xml.finish())
    }
}

impl Exporter {
    /// Resolve how runs should reference this font and register it for
    /// embedding.
    pub(crate) fn font_ref(&mut self, font: &Font) -> FontRef {
        if let Some(existing) = self.fonts.map.get(font) {
            return existing.clone();
        }

        let info = font.info();
        let family = EcoString::from(info.family.as_str());
        let weight = info.variant.weight;
        let italic = info.variant.style != FontStyle::Normal;

        // Regular (400) and Bold (700) weights map onto Word's four RIBBI
        // slots of the typographic family. Any other weight is referenced by
        // the face's full name, which Word treats as its own family.
        let (name, slot, bold) = if weight == FontWeight::REGULAR {
            (family.clone(), if italic { Slot::Italic } else { Slot::Regular }, false)
        } else if weight == FontWeight::BOLD {
            (family.clone(), if italic { Slot::BoldItalic } else { Slot::Bold }, true)
        } else {
            let full = full_name(font).unwrap_or_else(|| family.clone());
            (full, Slot::Regular, false)
        };

        let font_ref = FontRef {
            // For full-name references the name already identifies the exact
            // face, so no style flags are set.
            italic: italic && name == family,
            name: name.clone(),
            bold,
        };
        self.fonts.map.insert(font.clone(), font_ref.clone());

        if let Some(embed_index) = self.embeddable(font, &name) {
            let family_entry = self.fonts.families.entry(name.clone()).or_default();
            if family_entry.slots.iter().any(|&(s, _)| s == slot) {
                self.warn(format!(
                    "font \"{name}\": multiple faces map to the same slot, \
                     embedding only the first"
                ));
            } else {
                family_entry.slots.push((slot, embed_index));
            }
        } else {
            // Still list the family in the font table without embedding.
            self.fonts.families.entry(name.clone()).or_default();
        }

        font_ref
    }

    /// Check embedding gates and register the embed. Returns the index into
    /// `embeds` if the font file will be embedded.
    fn embeddable(&mut self, font: &Font, name: &str) -> Option<usize> {
        let data = font.data();

        if data.starts_with(b"ttcf") {
            self.warn(format!(
                "font \"{name}\": TrueType collections cannot be embedded; the \
                 document will only render correctly where the font is installed"
            ));
            return None;
        }

        let ttf = ttf_parser::Face::parse(data, font.index()).ok()?;

        if let Some(os2) = ttf.tables().os2
            && os2.permissions() == Some(ttf_parser::Permissions::Restricted)
        {
            self.warn(format!(
                "font \"{name}\": license restricts embedding (fsType), skipping; \
                 the document will only render correctly where the font is installed"
            ));
            return None;
        }

        if ttf.tables().cff.is_some() || ttf.tables().cff2.is_some() {
            self.warn(format!(
                "font \"{name}\": PostScript (CFF) outlines are embedded as-is; \
                 some Word versions may not render them"
            ));
        }

        if ttf.is_variable() {
            self.warn(format!(
                "font \"{name}\": variable font is embedded; Word will use its \
                 default instance"
            ));
        }

        let guid = derive_guid(data, font.index());
        self.fonts.embeds.push(Embed { data: data.clone(), guid });
        Some(self.fonts.embeds.len() - 1)
    }
}

/// The full name of a face from its `name` table.
fn full_name(font: &Font) -> Option<EcoString> {
    let ttf = ttf_parser::Face::parse(font.data(), font.index()).ok()?;
    let name = ttf
        .names()
        .into_iter()
        .find(|n| n.name_id == name_id::FULL_NAME && n.is_unicode())?;
    let name = name.to_string()?;
    (!name.is_empty()).then(|| EcoString::from(name))
}

/// Derive a deterministic obfuscation GUID from the font data via FNV-1a.
fn derive_guid(data: &[u8], index: u32) -> [u8; 16] {
    fn fnv1a(data: &[u8], mut hash: u64) -> u64 {
        for &byte in data {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    let seed = 0xcbf29ce484222325u64 ^ u64::from(index);
    let a = fnv1a(data, seed);
    let b = fnv1a(data, a ^ 0x9e3779b97f4a7c15);

    let mut guid = [0u8; 16];
    guid[..8].copy_from_slice(&a.to_be_bytes());
    guid[8..].copy_from_slice(&b.to_be_bytes());
    // Stamp RFC 4122 version 4 / variant bits so the GUID looks well-formed.
    guid[6] = (guid[6] & 0x0f) | 0x40;
    guid[8] = (guid[8] & 0x3f) | 0x80;
    guid
}

/// Format a GUID as `XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX` (no braces).
pub fn guid_string(guid: &[u8; 16]) -> EcoString {
    let hex = |range: std::ops::Range<usize>| {
        guid[range].iter().map(|b| format!("{b:02X}")).collect::<String>()
    };
    eco_format!(
        "{}-{}-{}-{}-{}",
        hex(0..4),
        hex(4..6),
        hex(6..8),
        hex(8..10),
        hex(10..16)
    )
}

/// Obfuscate font data according to ECMA-376 Part 1 §15.2.13.
///
/// The first 32 bytes are XORed with the GUID's 16 bytes in reverse order,
/// repeated twice.
pub fn obfuscate(data: &[u8], guid: &[u8; 16]) -> Vec<u8> {
    let mut out = data.to_vec();
    let n = out.len().min(32);
    for (i, byte) in out[..n].iter_mut().enumerate() {
        *byte ^= guid[15 - (i % 16)];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odttf_obfuscation() {
        // Hand-computed vector: GUID bytes 00..0f, data bytes all 0xAA.
        let guid: [u8; 16] = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
            0x0c, 0x0d, 0x0e, 0x0f,
        ];
        let data = vec![0xAA; 40];
        let out = obfuscate(&data, &guid);
        // Byte 0 is XORed with guid[15] = 0x0f.
        assert_eq!(out[0], 0xAA ^ 0x0f);
        // Byte 15 with guid[0], byte 16 with guid[15] again.
        assert_eq!(out[15], 0xAA ^ 0x00);
        assert_eq!(out[16], 0xAA ^ 0x0f);
        assert_eq!(out[31], 0xAA ^ 0x00);
        // Bytes past 32 are untouched.
        assert_eq!(out[32], 0xAA);
        // Applying the XOR twice restores the original.
        let round = obfuscate(&out, &guid);
        assert_eq!(round, data);
    }

    #[test]
    fn guid_is_deterministic_and_formatted() {
        let a = derive_guid(b"some font data", 0);
        let b = derive_guid(b"some font data", 0);
        let c = derive_guid(b"some font data", 1);
        assert_eq!(a, b);
        assert_ne!(a, c);
        let s = guid_string(&a);
        assert_eq!(s.len(), 36);
        assert_eq!(s.chars().filter(|&c| c == '-').count(), 4);
    }
}
