//! Where a Spore creature's colour comes from.
//!
//! # The short answer, and how it is known
//!
//! A Spore model's colour is **two unit-range float RGB triples per paint**,
//! written as decimal text inside the model's `sporemodel` XML record:
//!
//! ```text
//! <properties>
//!   <skincolor1>0.341176,0.360784,0.576470</skincolor1>
//!   <skincolor2>1,1,1</skincolor2>
//!   <skincolor3>1,1,1</skincolor3>
//! </properties>
//! ...
//! <blockref>
//!   <blockid>0x40626000, 0xd05c53a3</blockid>
//!   ...
//!   <paintlist count="3">
//!     <paint>
//!       <paintregion>0x00000005</paintregion>
//!       <paintid>0xf5967f30</paintid>
//!       <color1>0.450979,0.450979,0.450979</color1>
//!       <color2>0.749020,0.749020,0.749020</color2>
//!     </paint>
//!   </paintlist>
//! </blockref>
//! ```
//!
//! It is **not** in the textures. The creature skin shader is shipped as
//! readable source in the `smt` records (`0x0469A3F7`), and it takes the two
//! colours as a **constant** shader struct and uses the greyscale DXT5 raster
//! only as a blend *mask*:
//!
//! ```text
//! struct { float4 mColor1; float4 mColor2; float4 mParams; } editorColors;
//!
//! mixedColor = lerp(diffuseTexture.rgb, editorColors.mColor1.rgb, editorColors.mColor1.a);
//! mixedColor = lerp(mixedColor,         editorColors.mColor2.rgb, editorColors.mColor2.a * diffuseTexture.a);
//! Current.color.rgb = mixedColor * In.color.rgb;
//! ```
//!
//! so the on-disk raster supplies *where* each colour goes and the record
//! supplies *what* the colours are. That is why a Spore creature renders
//! uniform grey when only its textures are read, and it is the whole answer.
//! `docs/COLOUR-SOURCES.md` carries the measurement that eliminated every other
//! candidate.
//!
//! # Which records carry it
//!
//! Measured over the installed corpus, five DBPF type ids hold `sporemodel`
//! records and every one of them carries colour. The counts are per package;
//! `PatchData.package` holds the same records a second time.
//!
//! | type id | name | records | group | colour elements |
//! |---|---|---|---|---|
//! | `0x2B978C46` | `crt` (creature) | 47 | `0x40626200` | 141 × `skincolor1..3` |
//! | `0x3D97A8E4` | `cll` (cell) | 13 | `0x40616200` | 39 × `skincolor1..3` |
//! | `0x24682294` | `vcl` (vehicle) | 4 | `0x40646200` | 280 × `color1`/`color2` |
//! | `0x476A98C7` | `ufo` | 2 | `0x40656200` | 70 × `color1`/`color2` |
//! | `0x2399BE55` | `bld` (building) | 1 | `0x40636200` | 40 × `color1`/`color2` |
//!
//! # What this module does and does not do
//!
//! It decodes **only** the colour-bearing fields, by scanning for the exact
//! element names the corpus uses, and it refuses anything that does not match
//! the measured grammar. It is deliberately not an XML parser: this crate has
//! no XML dependency, and a general parser would accept documents this format
//! never produces, which is the wrong way for a decoder to be permissive.
//!
//! What it deliberately does **not** produce:
//!
//! * **No alpha.** The shader blends with `mColor1.a` / `mColor2.a`, and the
//!   record stores three components per colour. Where the fourth comes from is
//!   [`claims::COLOR_ALPHA_SOURCE`] — a real, unresolved question, and
//!   [`UnitRgb`] has no field to hide it in.
//! * **No resolution of `paintid`.** Six distinct ids are observed and none of
//!   them is a record instance id in any installed package. See
//!   [`claims::PAINT_ID_RESOLUTION`].
//! * **No meaning for a `paintregion` bit.** The value is carried verbatim as
//!   an opaque mask. See [`claims::PAINT_REGION_BITS`].
//! * **No claim about a player's own creature.** Every `sporemodel` record in
//!   the install is a shipped preset (`PaintSwatch`, `PaintBuddy`,
//!   `gencar`, ...). See [`claims::USER_CREATURE_COLOR_LOCATION`].
//!
//! # Evidence discipline
//!
//! Absent is absent. [`ModelColors::skin_colors`] is `[Option<UnitRgb>; 3]`, not
//! `[UnitRgb; 3]`, because "this record has no `skincolor2`" and "this record
//! has `skincolor2` equal to `0,0,0`" are different facts and only one of them
//! is a zero. Every remaining uncertainty is a graded
//! [`Fact`](spore_core::evidence::Fact) in [`claims`].

use core::fmt;

/// The XML declaration every `sporemodel` record opens with, byte for byte.
///
/// Measured: all 134 records in **each** of `Spore_Game.package` and
/// `PatchData.package` begin with exactly this and end with `</sporemodel>` —
/// 134 records, twice over, with no exception. A record that does not is refused
/// by name rather than scanned anyway.
pub const XML_DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";

/// The root element's opening tag.
pub const ROOT_OPEN: &str = "<sporemodel>";

/// The root element's closing tag.
pub const ROOT_CLOSE: &str = "</sporemodel>";

/// A `<paintregion>` mask, carried verbatim.
///
/// The corpus uses 23 distinct values across 390 `<paint>` elements and nothing
/// in these packages says what a bit selects, so this is a `u32` with a name and
/// no accessor pretending to know more. See [`claims::PAINT_REGION_BITS`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaintRegion(u32);

impl PaintRegion {
    /// Wraps a raw mask.
    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    /// The raw mask, exactly as stored.
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl fmt::Display for PaintRegion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{:08x}", self.0)
    }
}

/// One `<paint>`: a region mask, a paint id, and the two colours.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    /// Which body regions this paint covers. Meaning of the bits: unknown.
    pub region: PaintRegion,
    /// The `paintid` hash. Resolution: unknown.
    pub paint_id: u32,
    /// First colour, blended in by `mColor1.a` in the shader.
    pub color1: UnitRgb,
    /// Second colour, blended in by `mColor2.a * diffuse.a` in the shader.
    pub color2: UnitRgb,
}

/// One `<paintlist>`, in document order.
#[derive(Debug, Clone, PartialEq)]
pub struct PaintList {
    /// The `count` attribute as written, and the number of `<paint>` children
    /// that were read. The extractor refuses a record where they differ, so the
    /// two can never drift here.
    pub declared_count: u32,
    /// The paints, in document order.
    pub paints: Vec<Paint>,
}

/// Every colour-bearing field of one `sporemodel` record.
///
/// `Option` is load-bearing throughout. A `crt` record has `skincolor1..3` and
/// no `paintlist`; a `vcl` record has `paintlist`s and no `skincolor`. Neither
/// is a zero-length list of colours.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelColors {
    /// `<formatversion>`, when present. Measured values: 11, 15, 16.
    pub format_version: Option<u32>,
    /// `<skincolor1>`, `<skincolor2>`, `<skincolor3>`, in that order. `None` per
    /// slot means the element is absent from the record.
    pub skin_colors: [Option<UnitRgb>; 3],
    /// `<skineffect1..3>`, in that order. `Some(0)` is a real recorded zero and
    /// is kept distinct from `None`.
    pub skin_effects: [Option<u32>; 3],
    /// `<skineffectseed1..3>`, in that order.
    pub skin_effect_seeds: [Option<u32>; 3],
    /// Every `<paintlist>` in the record, in document order. One per `<blockref>`
    /// that has one; `Spore_Content`'s own models have none.
    pub paint_lists: Vec<PaintList>,
}

impl ModelColors {
    /// Every distinct colour in the record, `skincolor*` first, then each
    /// paint's `color1` and `color2` in document order.
    ///
    /// Deduplicated by value, not by slot, because the question this exists to
    /// answer is "how many colours does this model have", and a model that
    /// writes `1,1,1` in four places has one colour, not four.
    pub fn distinct_colors(&self) -> Vec<UnitRgb> {
        let mut out: Vec<UnitRgb> = Vec::new();
        let mut push = |c: UnitRgb| {
            if !out.contains(&c) {
                out.push(c);
            }
        };
        for slot in self.skin_colors.iter().flatten() {
            push(*slot);
        }
        for list in &self.paint_lists {
            for paint in &list.paints {
                push(paint.color1);
                push(paint.color2);
            }
        }
        out
    }
}

/// The canonical colour type and grammar — **owned by `spore-creator-core`**.
///
/// # This module is a projection, not an owner
///
/// A second implementation of the `sporemodel` colour grammar used to live
/// here. It is gone: [`UnitRgb`] and its component grammar are defined once, in
/// `spore_creator_core::color`, and re-exported below. What remains in this
/// module is the part that is genuinely `spore-assets`' business — reading the
/// colour-bearing fields out of raw record bytes — plus the UTF-8, declaration
/// and integer-id errors that a byte-level reader has and a document-level one
/// does not.
///
/// The dependency direction is the documented one:
///
/// ```text
/// Spore XML -> spore-creator-core -> canonical representation -> spore-assets
/// ```
///
/// so `spore-creator-core` knows nothing about this crate and the graph stays
/// acyclic. See `docs/CREATOR-CORE.md` §3.
pub use spore_creator_core::color::UnitRgb;

/// Translate a canonical colour failure into this module's error shape.
///
/// The *grammar* decision has already been made by `spore-creator-core`; this
/// only re-labels it with the element name the byte-level reader knows about,
/// so an error surfaced through [`extract`] still points at a record field.
fn colour_error(error: spore_creator_core::color::ColorError, tag: &str) -> ColorError {
    use spore_creator_core::color::ColorError as Canonical;
    let tag = tag.to_owned();
    match error {
        Canonical::ComponentCount { text, found, .. } => {
            ColorError::ComponentCount { tag, text, found }
        }
        Canonical::MalformedComponent {
            text, component, ..
        } => ColorError::MalformedComponent {
            tag,
            text,
            component,
        },
        Canonical::OutOfRange { text, .. } => ColorError::OutOfRange { tag, text },
    }
}

/// Every way reading a `sporemodel` colour can fail.
///
/// Each variant names the element and carries the offending text, because a
/// colour decoder whose error says "bad parse" costs more time than it saves.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ColorError {
    /// The record is not valid UTF-8.
    #[error("`sporemodel` record is not valid UTF-8 at byte {offset}")]
    NotUtf8 {
        /// Byte offset of the first invalid sequence.
        offset: usize,
    },
    /// The XML declaration is missing or differs from the measured one.
    #[error("expected the record to open with `{XML_DECLARATION}`, found `{found}`")]
    DeclarationMismatch {
        /// The first bytes actually present.
        found: String,
    },
    /// The root element is missing.
    #[error("expected a `{ROOT_OPEN}` root element")]
    MissingRoot,
    /// An element carried a number this build will not read.
    #[error("<{tag}> is not an unsigned decimal integer: `{text}`")]
    MalformedInteger {
        /// The element name.
        tag: String,
        /// The text found.
        text: String,
    },
    /// An element carried a hex id that is not `0x` + 8 lowercase hex digits.
    #[error("<{tag}> is not 0x + 8 lowercase hex digits: `{text}`")]
    MalformedHex {
        /// The element name.
        tag: String,
        /// The text found.
        text: String,
    },
    /// A colour did not have exactly three components.
    #[error("<{tag}> colour `{text}` has {found} components, not 3")]
    ComponentCount {
        /// The element name.
        tag: String,
        /// The text found.
        text: String,
        /// How many components were found.
        found: usize,
    },
    /// A colour component was not a bare decimal.
    #[error("<{tag}> colour `{text}` has malformed component `{component}`")]
    MalformedComponent {
        /// The element name.
        tag: String,
        /// The whole colour text.
        text: String,
        /// The offending component.
        component: String,
    },
    /// A colour component was outside `0.0 ..= 1.0`.
    #[error("<{tag}> colour `{text}` is outside the unit range")]
    OutOfRange {
        /// The element name.
        tag: String,
        /// The text found.
        text: String,
    },
    /// A `<paintlist count="N">` held a number of `<paint>` children other
    /// than `N`.
    #[error("<paintlist count=\"{declared}\"> holds {found} <paint> children")]
    PaintCountMismatch {
        /// The `count` attribute.
        declared: u32,
        /// The children actually present.
        found: usize,
    },
}

/// Reads the colour-bearing fields of a `sporemodel` record.
///
/// # Strictness
///
/// The record must open with [`XML_DECLARATION`] and contain
/// [`ROOT_OPEN`]/[`ROOT_CLOSE`]. Every colour, count and id must match the
/// measured grammar; anything else is an error, never a skipped field. A
/// decoder that quietly drops the one colour it could not read is how a model
/// renders a wrong colour instead of no model.
pub fn extract(record: &[u8]) -> Result<ModelColors, ColorError> {
    let text = std::str::from_utf8(record).map_err(|e| ColorError::NotUtf8 {
        offset: e.valid_up_to(),
    })?;
    let head_len = text.len().min(XML_DECLARATION.len());
    if text.as_bytes().get(..head_len) != XML_DECLARATION.as_bytes().get(..head_len) {
        return Err(ColorError::DeclarationMismatch {
            found: text
                .get(..head_len.min(text.len()))
                .unwrap_or_default()
                .to_owned(),
        });
    }
    if !text.contains(ROOT_OPEN) || !text.contains(ROOT_CLOSE) {
        return Err(ColorError::MissingRoot);
    }

    let mut out = ModelColors {
        format_version: element(text, "formatversion").map(decimal).transpose()?,
        ..ModelColors::default()
    };

    for (slot, tag) in ["skincolor1", "skincolor2", "skincolor3"]
        .into_iter()
        .enumerate()
    {
        out.skin_colors[slot] = element(text, tag)
            .map(|v| UnitRgb::parse(v, tag).map_err(|e| colour_error(e, tag)))
            .transpose()?;
    }
    for (slot, tag) in ["skineffect1", "skineffect2", "skineffect3"]
        .into_iter()
        .enumerate()
    {
        out.skin_effects[slot] = element(text, tag).map(|v| hex32(v, tag)).transpose()?;
    }
    for (slot, tag) in ["skineffectseed1", "skineffectseed2", "skineffectseed3"]
        .into_iter()
        .enumerate()
    {
        out.skin_effect_seeds[slot] = element(text, tag).map(decimal).transpose()?;
    }
    out.paint_lists = paint_lists(text)?;
    Ok(out)
}

/// The first `<tag>value</tag>` in `text`, or `None`.
///
/// A repeated element is not an error here: `<color1>` legitimately appears
/// once per `<paint>`, and each occurrence is read in its own
/// [`paint_lists`] pass. This helper is only used for elements the corpus
/// writes at most once per record.
fn element<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(&text[start..end])
}

fn paint_lists(text: &str) -> Result<Vec<PaintList>, ColorError> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(found) = text[cursor..].find("<paintlist") {
        let head = cursor + found;
        // Only a real element, not a longer name that starts with this one.
        let after = text[head + "<paintlist".len()..].chars().next();
        if !matches!(after, Some(' ')) {
            cursor = head + "<paintlist".len();
            continue;
        }
        let gt = match text[head..].find('>') {
            Some(i) => head + i,
            None => return Err(ColorError::MissingRoot),
        };
        let attributes = &text[head + "<paintlist".len()..gt];
        let declared = attribute(attributes, "count")
            .and_then(|v| decimal(v).ok())
            .ok_or(ColorError::MalformedInteger {
                tag: "paintlist/@count".to_owned(),
                text: attributes.to_owned(),
            })?;
        let close = match text[gt..].find("</paintlist>") {
            Some(i) => gt + i,
            None => return Err(ColorError::MissingRoot),
        };
        let body = &text[gt + 1..close];

        let mut paints = Vec::new();
        let mut inner = 0usize;
        while let Some(found) = body[inner..].find("<paint>") {
            let start = inner + found + "<paint>".len();
            let end = match body[start..].find("</paint>") {
                Some(i) => start + i,
                None => return Err(ColorError::MissingRoot),
            };
            let paint = &body[start..end];
            paints.push(Paint {
                region: PaintRegion::new(hex32(required(paint, "paintregion")?, "paintregion")?),
                paint_id: hex32(required(paint, "paintid")?, "paintid")?,
                color1: UnitRgb::parse(required(paint, "color1")?, "color1")
                    .map_err(|e| colour_error(e, "color1"))?,
                color2: UnitRgb::parse(required(paint, "color2")?, "color2")
                    .map_err(|e| colour_error(e, "color2"))?,
            });
            inner = end + "</paint>".len();
        }
        if paints.len() as u32 != declared {
            return Err(ColorError::PaintCountMismatch {
                declared,
                found: paints.len(),
            });
        }
        out.push(PaintList {
            declared_count: declared,
            paints,
        });
        cursor = close + "</paintlist>".len();
    }
    Ok(out)
}

fn required<'a>(text: &'a str, tag: &str) -> Result<&'a str, ColorError> {
    element(text, tag).ok_or_else(|| ColorError::MalformedInteger {
        tag: tag.to_owned(),
        text: String::from("<absent>"),
    })
}

fn attribute<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let start = text.find(&key)? + key.len();
    let end = text[start..].find('"')? + start;
    Some(&text[start..end])
}

fn decimal(text: &str) -> Result<u32, ColorError> {
    let trimmed = text.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ColorError::MalformedInteger {
            tag: "integer".to_owned(),
            text: text.to_owned(),
        });
    }
    trimmed
        .parse::<u32>()
        .map_err(|_| ColorError::MalformedInteger {
            tag: "integer".to_owned(),
            text: text.to_owned(),
        })
}

/// `0x` + exactly eight lowercase hex digits, which is the only spelling the
/// corpus uses for `paintid`, `paintregion` and `skineffect*`.
fn hex32(text: &str, tag: &str) -> Result<u32, ColorError> {
    let body = text
        .strip_prefix("0x")
        .ok_or_else(|| ColorError::MalformedHex {
            tag: tag.to_owned(),
            text: text.to_owned(),
        })?;
    if body.len() != 8
        || !body
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(ColorError::MalformedHex {
            tag: tag.to_owned(),
            text: text.to_owned(),
        });
    }
    u32::from_str_radix(body, 16).map_err(|_| ColorError::MalformedHex {
        tag: tag.to_owned(),
        text: text.to_owned(),
    })
}

/// Every claim this module makes that is **not** verified, with its grade.
///
/// The grades are the repository's canonical scale from
/// [`spore_core::EvidenceLevel`]. Nothing here is upgraded because a
/// convenient answer would be nicer: the open questions about `paintid`
/// resolution, the `paintregion` bits and the missing alpha component are
/// [`spore_core::evidence::Fact::unavailable`] — *we looked and found nothing* — and
/// stay that way.
pub mod claims {
    use std::sync::OnceLock;

    use spore_core::evidence::{EvidenceLevel, EvidenceState, Fact, Provenance};

    /// That `skincolor1..3` are colours, and per-slot absence is preserved.
    pub const SKIN_COLOR_SLOTS: &str = "skin_color_slots";

    /// That `skincolor*` feed the shader's `color1`/`color2`.
    pub const SKIN_COLOR_TO_SHADER: &str = "skin_color_to_shader";

    /// Where the shader's fourth colour component (`mColor1.a`) comes from.
    pub const COLOR_ALPHA_SOURCE: &str = "color_alpha_source";

    /// What one bit of `paintregion` selects.
    pub const PAINT_REGION_BITS: &str = "paint_region_bits";

    /// How `paintid` resolves to a texture or effect.
    pub const PAINT_ID_RESOLUTION: &str = "paint_id_resolution";

    /// What `skineffectseed*` seeds.
    pub const SKIN_EFFECT_SEED_MEANING: &str = "skin_effect_seed_meaning";

    /// Where a player's own creature colour is stored.
    pub const USER_CREATURE_COLOR_LOCATION: &str = "user_creature_color_location";

    /// One recorded claim.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ColorClaim {
        /// Stable identifier.
        pub subject: &'static str,
        /// The claim, in one sentence, in the repository's evidence vocabulary.
        pub statement: &'static str,
        /// The envelope: grade, how it was obtained, where it came from.
        pub fact: Fact<&'static str>,
    }

    /// The claims, in declaration order.
    pub fn all() -> &'static [ColorClaim] {
        static CLAIMS: OnceLock<Vec<ColorClaim>> = OnceLock::new();
        CLAIMS.get_or_init(|| {
            vec![
                claim_of(
                    SKIN_COLOR_SLOTS,
                    "The three <skincolorN> elements are three independent colour slots, written \
                     together or not at all: 120 of the 134 corpus records carry all three and 14 carry \
                     none, and no record carries one without the other two. A record may therefore omit \
                     the slot entirely, which is why ModelColors::skin_colors is [Option<UnitRgb>; 3] \
                     and not [UnitRgb; 3].",
                    EvidenceLevel::Observed,
                    "docs/COLOUR-SOURCES.md",
                ),
                claim_of(
                    SKIN_COLOR_TO_SHADER,
                    "skincolor1..3 / color1 / color2 are the source of the `editorColors` struct's \
                     mColor1/mColor2, which the shipped setupEditorPaint shader consumes. The shader names \
                     are read from the corpus's own shader source and the game names `color1` in its \
                     SPSkinPaintClear effect cheatsheet; the assignment of a given XML slot to a given \
                     struct member is not stated anywhere in these packages.",
                    EvidenceLevel::Inferred,
                    "docs/COLOUR-SOURCES.md",
                ),
                unavailable_claim(
                    COLOR_ALPHA_SOURCE,
                    "The record stores three components per colour while the shader blends with \
                     mColor1.a / mColor2.a. Where the fourth component comes from is not stated by \
                     any record or shader in the installed packages.",
                ),
                unavailable_claim(
                    PAINT_REGION_BITS,
                    "23 distinct paintregion masks appear across 390 <paint> elements and no table \
                     in the corpus maps a bit to a body region.",
                ),
                unavailable_claim(
                    PAINT_ID_RESOLUTION,
                    "Six distinct paintid values appear. None of them is the instance id of any record \
                     in any installed package, so the id does not resolve by DBPF identity and the \
                     structure it selects is not determined here.",
                ),
                unavailable_claim(
                    SKIN_EFFECT_SEED_MEANING,
                    "skineffectseed1..3 are unsigned integers (32 distinct values observed). Nothing \
                     in the corpus says what algorithm consumes them.",
                ),
                unavailable_claim(
                    USER_CREATURE_COLOR_LOCATION,
                    "Every sporemodel record in the install is a shipped preset. Where a player's own \
                     creature colour is persisted is outside these packages and is not determined here.",
                ),
            ]
        })
    }

    /// The one claim with a subject, or `None` when this build records none.
    ///
    /// `None` means "no claim recorded", which is not the same as a claim of
    /// grade `Unknown`.
    pub fn claim(subject: &str) -> Option<&'static ColorClaim> {
        all().iter().find(|c| c.subject == subject)
    }

    fn claim_of(
        subject: &'static str,
        statement: &'static str,
        level: EvidenceLevel,
        reference: &'static str,
    ) -> ColorClaim {
        ColorClaim {
            subject,
            statement,
            fact: Fact::new(
                level,
                EvidenceState::Derived,
                vec![Provenance::derived(reference)],
                statement,
            ),
        }
    }

    /// Builds an unavailable claim with an explicit subject.
    pub fn unavailable_claim(subject: &'static str, reason: &'static str) -> ColorClaim {
        ColorClaim {
            subject,
            statement: reason,
            fact: Fact::unavailable(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::claims::*;
    use super::*;
    use spore_core::evidence::{EvidenceLevel, EvidenceState};

    fn synthetic(body: &str) -> Vec<u8> {
        format!("{XML_DECLARATION}{ROOT_OPEN}{body}{ROOT_CLOSE}").into_bytes()
    }

    const CREATURE: &str = r#"
  <formatversion>16</formatversion>
  <properties>
    <modeltype>0x9ea3031a</modeltype>
    <skineffect1>0x791c8201</skineffect1>
    <skineffect2>0xcf48a38f</skineffect2>
    <skineffect3>0xdd5391a5</skineffect3>
    <skineffectseed1>1234</skineffectseed1>
    <skineffectseed2>3176</skineffectseed2>
    <skineffectseed3>7976</skineffectseed3>
    <skincolor1>0.341176,0.360784,0.576470</skincolor1>
    <skincolor2>1,1,1</skincolor2>
    <skincolor3>1,1,1</skincolor3>
  </properties>
"#;

    #[test]
    fn a_creature_record_yields_three_skin_colours_and_three_effects() {
        let got = extract(&synthetic(CREATURE)).expect("record parses");
        assert_eq!(got.format_version, Some(16));
        let colors: Vec<UnitRgb> = got.skin_colors.iter().flatten().copied().collect();
        assert_eq!(colors.len(), 3);
        assert_eq!(colors[0].red, 0.341_176);
        assert_eq!(colors[0].green, 0.360_784);
        assert_eq!(colors[0].blue, 0.576_470);
        // `1,1,1` is a recorded white, not an absent slot.
        assert_eq!(colors[1], UnitRgb::new(1.0, 1.0, 1.0).unwrap());
        assert_eq!(got.skin_effects[0], Some(0x791c_8201));
        assert_eq!(got.skin_effect_seeds, [Some(1234), Some(3176), Some(7976)]);
        assert!(got.paint_lists.is_empty());
    }

    #[test]
    fn a_vehicle_block_yields_a_paint_list_in_document_order() {
        let body = r#"
  <blocks count="1">
    <blockref>
      <blockid>0x40646000, 0xab10c9b7</blockid>
      <paintlist count="2">
        <paint>
          <paintregion>0x00000005</paintregion>
          <paintid>0xf5967f30</paintid>
          <color1>0.450979,0.450979,0.450979</color1>
          <color2>0.749020,0.749020,0.749020</color2>
        </paint>
        <paint>
          <paintregion>0x00000007</paintregion>
          <paintid>0x7da7e22c</paintid>
          <color1>0,0,0</color1>
          <color2>1,0.937255,0.690196</color2>
        </paint>
      </paintlist>
    </blockref>
  </blocks>
"#;
        let got = extract(&synthetic(body)).expect("record parses");
        assert_eq!(got.paint_lists.len(), 1);
        let list = &got.paint_lists[0];
        assert_eq!(list.declared_count, 2);
        assert_eq!(list.paints.len(), 2);
        assert_eq!(list.paints[0].region, PaintRegion::new(0x0000_0005));
        assert_eq!(list.paints[0].paint_id, 0xf596_7f30);
        assert_eq!(list.paints[0].color1.red, 0.450_979);
        assert_eq!(list.paints[1].paint_id, 0x7da7_e22c);
        assert_eq!(list.paints[1].color2.blue, 0.690_196);
        // A `crt`-shaped record has no skin colours and a `vcl`-shaped one has
        // no paint list; neither absence may read as a black colour.
        assert_eq!(got.skin_colors, [None, None, None]);
    }

    #[test]
    fn distinct_colours_deduplicates_by_value_not_by_slot() {
        let body = format!(
            "{CREATURE}<paintlist count=\"1\"><paint>\
            <paintregion>0x00000001</paintregion><paintid>0x00000001</paintid>\
            <color1>1,1,1</color1><color2>0,0,0</color2></paint></paintlist>"
        );
        let got = extract(&synthetic(&body)).expect("record parses");
        // skincolor2 and skincolor3 are both `1,1,1`; the paint's color1 is too.
        assert_eq!(got.distinct_colors().len(), 3);
    }

    #[test]
    fn a_paintlist_count_that_disagrees_with_its_children_is_refused() {
        let body = r#"<paintlist count="3"><paint>
            <paintregion>0x00000001</paintregion><paintid>0x00000001</paintid>
            <color1>0.5,0.5,0.5</color1><color2>0.25,0.25,0.25</color2>
        </paint></paintlist>"#;
        assert_eq!(
            extract(&synthetic(body)),
            Err(ColorError::PaintCountMismatch {
                declared: 3,
                found: 1
            })
        );
    }

    /// The colour **grammar** is no longer tested here, because it is no longer
    /// implemented here. Its canonical tests live in
    /// `spore_creator_core::color`, and they are the ones that count: this
    /// module only projects.
    ///
    /// What is tested *here* is the part `spore-assets` alone owns — that a
    /// byte-level read of a real record reaches the same colour the canonical
    /// parser reaches. Duplicating the grammar assertions would have recreated
    /// the second implementation this migration removed, just as tests.
    #[test]
    fn the_colour_grammar_is_owned_by_creator_core_not_reimplemented_here() {
        // If this ever stops compiling because a grammar rule changed, the change
        // belongs in `spore_creator_core::color`, not in a copy over here.
        let via_owner = spore_creator_core::UnitRgb::parse("0.5,0.5,0.5", "color1");
        assert!(via_owner.is_ok());
        assert!(spore_creator_core::UnitRgb::parse("1e-3,0,0", "color1").is_err());
    }

    #[test]
    fn a_colour_error_names_the_element_it_came_from() {
        let body = "<properties><skincolor1>0.5,0.5</skincolor1></properties>";
        match extract(&synthetic(body)) {
            Err(ColorError::ComponentCount { tag, found, .. }) => {
                assert_eq!(tag, "skincolor1");
                assert_eq!(found, 2);
            }
            other => panic!("expected a named component-count error, got {other:?}"),
        }
    }

    #[test]
    fn hex_ids_must_be_the_spelling_the_corpus_uses() {
        for bad in ["791c8201", "0x791C8201", "0x791c820", "0x791c82011"] {
            let body = format!("<properties><skineffect1>{bad}</skineffect1></properties>");
            assert!(
                matches!(
                    extract(&synthetic(&body)),
                    Err(ColorError::MalformedHex { .. })
                ),
                "{bad:?} should be refused"
            );
        }
        let ok = "<properties><skineffect1>0x00000000</skineffect1></properties>";
        assert_eq!(
            extract(&synthetic(ok)).unwrap().skin_effects[0],
            Some(0),
            "a recorded zero effect id is a value, not an absence"
        );
    }

    #[test]
    fn a_record_without_the_measured_declaration_or_root_is_refused() {
        assert!(matches!(
            extract(b"<sporemodel></sporemodel>"),
            Err(ColorError::DeclarationMismatch { .. })
        ));
        assert!(matches!(
            extract(format!("{XML_DECLARATION}<other/>").as_bytes()),
            Err(ColorError::MissingRoot)
        ));
        assert!(matches!(
            extract(&[0xff, 0xfe, 0x00]),
            Err(ColorError::NotUtf8 { .. })
        ));
    }

    #[test]
    fn corpus_text_round_trips_through_the_parser() {
        for text in [
            "0.341176,0.360784,0.576470",
            "1,1,1",
            "0,0,0",
            "0.937255,0.964706,0.925490",
            "0.949020,0.541176,0.301961",
        ] {
            let colour = UnitRgb::parse(text, "color1").expect("corpus text parses");
            assert_eq!(colour.to_corpus_text(), text);
        }
    }

    #[test]
    fn a_seven_digit_corpus_component_lands_within_one_unit_in_the_last_place() {
        // Six of the corpus' 1 710 colour components are written with seven
        // fractional digits. The canonical writer is six, so it cannot be a
        // bit-exact round trip for those six; it is within 1e-6, and this pins
        // that bound rather than pretending the precision is free.
        let seven = UnitRgb::parse("0.1234567,0,0", "color1").unwrap();
        let back = UnitRgb::parse(&seven.to_corpus_text(), "color1").unwrap();
        assert!(
            (back.red - seven.red).abs() <= 1e-6,
            "{back:?} vs {seven:?}"
        );
        assert_eq!(back.green, seven.green);
        assert_eq!(back.blue, seven.blue);
    }

    #[test]
    fn every_subject_has_exactly_one_claim_and_unavailable_ones_stay_unavailable() {
        for subject in [
            SKIN_COLOR_SLOTS,
            SKIN_COLOR_TO_SHADER,
            COLOR_ALPHA_SOURCE,
            PAINT_REGION_BITS,
            PAINT_ID_RESOLUTION,
            SKIN_EFFECT_SEED_MEANING,
            USER_CREATURE_COLOR_LOCATION,
        ] {
            assert!(claim(subject).is_some(), "{subject} has no claim");
        }
        let alpha = claim(COLOR_ALPHA_SOURCE).expect("recorded");
        assert!(!alpha.fact.is_available());
        assert_eq!(alpha.fact.level(), EvidenceLevel::Unknown);
        assert_eq!(alpha.fact.evidence_state(), EvidenceState::Missing);
    }

    #[test]
    fn a_longer_element_name_is_not_mistaken_for_a_paintlist() {
        // `<paintlistfoo>` starts with `<paintlist` and is not a paint list.
        let body = "<paintlistfoo count=\"1\"></paintlistfoo>";
        assert!(extract(&synthetic(body)).unwrap().paint_lists.is_empty());
    }
}

/// Corpus tests over the real install. `#[ignore]`d by default; they skip
/// cleanly when `SPORE/` is absent, and they never assert a colour value — only
/// that the measured shape still holds.
#[cfg(test)]
mod corpus {
    use super::*;
    use spore_dbpf::{extract_record, PackageIndex};

    /// The five type ids measured to hold `sporemodel` records.
    const SPOREMODEL_TYPES: [u32; 5] = [
        0x2b97_8c46, // crt   — creature
        0x3d97_a8e4, // cll   — cell
        0x476a_98c7, // ufo
        0x2399_be55, // bld   — building
        0x2468_2294, // vcl   — vehicle
    ];

    /// Reads every `sporemodel` record out of the two packages that hold them.
    ///
    /// Returns an empty vector when `SPORE/` is absent, which the callers below
    /// treat as "skip" rather than "fail" — an absent install is not a defect in
    /// the data or in this decoder.
    fn corpus_records() -> Vec<Vec<u8>> {
        // `cargo test` runs with the crate directory as the working directory,
        // so the install is addressed from the manifest rather than from `.`.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .map(std::path::Path::to_path_buf);
        let mut out = Vec::new();
        for name in ["Spore_Game.package", "PatchData.package"] {
            let Some(root) = root.as_ref() else { continue };
            let path = root.join("SPORE/Data").join(name);
            let Ok(image) = std::fs::read(&path) else {
                continue;
            };
            let Ok(index) = PackageIndex::parse(&image) else {
                continue;
            };
            for entry in index.entries() {
                if !SPOREMODEL_TYPES.contains(&entry.type_id) {
                    continue;
                }
                if let Ok(bytes) = extract_record(&image, entry) {
                    out.push(bytes);
                }
            }
        }
        out
    }

    #[test]
    #[ignore = "needs SPORE/"]
    fn every_sporemodel_record_decodes_and_every_colour_is_in_range() {
        let records = corpus_records();
        if records.is_empty() {
            eprintln!("SKIP: no SPORE/Data packages present");
            return;
        }
        assert!(!records.is_empty(), "no sporemodel records found");

        let mut skin_slots = 0usize;
        let mut paint_elements = 0usize;
        let mut distinct: Vec<UnitRgb> = Vec::new();
        for bytes in &records {
            let model = extract(bytes).expect("every corpus record decodes");
            for slot in model.skin_colors.iter().flatten() {
                skin_slots += 1;
                if !distinct.contains(slot) {
                    distinct.push(*slot);
                }
            }
            for list in &model.paint_lists {
                assert_eq!(list.declared_count as usize, list.paints.len());
                for paint in &list.paints {
                    paint_elements += 1;
                    for colour in [paint.color1, paint.color2] {
                        if !distinct.contains(&colour) {
                            distinct.push(colour);
                        }
                    }
                }
            }
        }
        // Measured on the 3.1.0.22 install: 120 skincolour slots, 390 paint
        // elements, 59 distinct triples.
        assert!(skin_slots >= 120, "only {skin_slots} skin colour slots");
        assert!(
            paint_elements >= 390,
            "only {paint_elements} paint elements"
        );
        assert!(
            distinct.len() >= 59,
            "only {} distinct colours; the channel spans are the point",
            distinct.len()
        );
        // The whole point: the colours are not greyscale. A record set where
        // every channel spans only one value would mean the answer moved.
        for channel in 0..3 {
            let values: Vec<f32> = distinct.iter().map(|c| c.components()[channel]).collect();
            let lo = values.iter().cloned().fold(f32::INFINITY, f32::min);
            let hi = values.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
            assert!(
                hi - lo > 0.9,
                "channel {channel} spans only {lo}..{hi}; Spore creature colour is not greyscale"
            );
        }
    }

    /// Bit-for-bit comparison of two RGB triples.
    ///
    /// `f32` equality would accept `-0.0 == 0.0`, which is exactly the kind of
    /// silent difference this test exists to catch.
    fn same_bits(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter()
            .zip(b.iter())
            .all(|(x, y)| x.to_bits() == y.to_bits())
    }

    /// **The equivalence test this migration had to earn.**
    ///
    /// Two readers of the same colour fields must never disagree: the canonical
    /// one (`spore_creator_core::parse_sporemodel`, which parses the whole
    /// document) and this crate's byte-level projection (`extract`, which reads
    /// only the colour-bearing fields out of raw bytes). Before the migration
    /// they were two implementations; nothing asserted they agreed.
    ///
    /// Over every `<sporemodel>` record in the install, every skin colour and
    /// every paint colour must come out **bit-identical** from both paths.
    #[test]
    #[ignore = "needs SPORE/"]
    fn the_projection_and_the_canonical_parser_agree_on_every_colour() {
        let records = corpus_records();
        if records.is_empty() {
            eprintln!("SKIP: no SPORE/Data packages present");
            return;
        }

        let mut compared_skin = 0usize;
        let mut compared_paint = 0usize;

        for bytes in &records {
            let projected = extract(bytes).expect("the projection decodes");
            let text = std::str::from_utf8(bytes).expect("a sporemodel record is utf-8");
            // The family only labels the document; colour reading is
            // family-independent, and every family is exercised by the loop.
            let canonical =
                spore_creator_core::parse_sporemodel(spore_creator_core::Family::Creature, text)
                    .expect("the canonical parser decodes the same record");

            // Skin colours: `<skincolor1..3>`, compared **slot by slot**.
            //
            // Flattening both sides and zipping them would also pass when one
            // reader thinks slot 2 is absent and the other thinks slot 3 is —
            // the vectors would still line up. Presence is part of the claim,
            // so presence is compared.
            let canonical_skin = canonical
                .properties
                .skin_colors
                .unwrap_or([None, None, None]);
            let projected_skin = projected.skin_colors;
            for slot in 0..3 {
                match (canonical_skin[slot], projected_skin[slot]) {
                    (None, None) => {}
                    (Some(mine), Some(theirs)) => {
                        assert!(
                            same_bits(mine.components(), theirs.components()),
                            "skincolor{} disagrees: canonical {mine} vs projection {theirs}",
                            slot + 1
                        );
                        compared_skin += 1;
                    }
                    (mine, theirs) => panic!(
                        "skincolor{} presence disagrees: canonical {mine:?} vs projection {theirs:?}",
                        slot + 1
                    ),
                }
            }

            // Paint colours: every `<paint>` in document order, per block.
            let canonical_paint: Vec<_> = canonical
                .blocks
                .iter()
                .filter_map(|block| block.paint.as_ref())
                .flatten()
                .flat_map(|paint| [paint.color1, paint.color2])
                .collect();
            let projected_paint: Vec<_> = projected
                .paint_lists
                .iter()
                .flat_map(|list| &list.paints)
                .flat_map(|paint| [paint.color1, paint.color2])
                .collect();
            assert_eq!(
                canonical_paint.len(),
                projected_paint.len(),
                "the two readers found a different number of paint colours"
            );
            for (mine, theirs) in canonical_paint.iter().zip(&projected_paint) {
                assert!(
                    same_bits(mine.components(), theirs.components()),
                    "paint colour {mine} vs {theirs} disagrees between the canonical parser and the projection"
                );
                compared_paint += 1;
            }
        }

        // A zero here would mean the comparison proved nothing, which is the
        // failure mode this repository treats as worse than a failing test.
        assert!(compared_skin > 0, "compared no skin colours at all");
        assert!(compared_paint > 0, "compared no paint colours at all");
        eprintln!("agreed on {compared_skin} skin and {compared_paint} paint colours");
    }
}
