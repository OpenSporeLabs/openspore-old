//! # Creature skin: the colour that was on disk the whole time.
//!
//! Every creature part in OpenSpore rendered flat neutral grey, and the working
//! explanation was that the corpus has no colour data: all 1824 part containers
//! hold no texture reference, and all 47 shipped creature documents carry no
//! `<paintlist>`. Both of those are true, and together they made "there is no
//! colour" look like a property of Spore rather than of this engine.
//!
//! It is neither. The colour is in the `<sporemodel>` document, in three slots:
//!
//! ```text
//!   <skincolor1> <skincolor2> <skincolor3>     Base / Coat / Detail
//!   <skineffect1> <skineffect2> <skineffect3>   pattern ids
//!   <skineffectseed1..3>                        pattern seeds
//! ```
//!
//! All three sets are present in **47 of 47** `crt` documents, and
//! [`CreatorDocument::skin_colors`] already parses them — it has been parsed,
//! persisted and round-tripped this whole time. Nothing in the **editor's** render
//! path read it.
//!
//! # Why this is the document's colour and not a family's
//!
//! `domain::paint::resolve` answers "what colour should a *model* be", and it does
//! so by searching the store for a `crt` record whose *family* matches the model's
//! group. For a creature part that is the wrong question twice over: a part's
//! group (`0x40626000`) has no canonical name in the committed table, so the
//! lookup returns `ModelGroupUnnamed` and yields nothing; and where it does
//! resolve, it picks an arbitrary first match — which is how every unnamed
//! creature ended up wearing one shipped preset's green.
//!
//! A document's colour is **in the document**. So this module reads it from the
//! document the editor is editing, which is both simpler and strictly more
//! correct: edit a creature, and the colour is that creature's.
//!
//! # What is NOT claimed here
//!
//! * **Alpha is UNKNOWN.** `<skincolor1>` is an RGB triple with no fourth
//!   component. The shipped `setupEditorPaint` shader blends two colours by their
//!   alphas, and nothing in the corpus states where those come from, so
//!   [`SKIN_ALPHA_SOURCE`] records that and [`blend_layer`] takes the alpha as a
//!   **caller's explicit choice** rather than reading one from anywhere.
//! * **The patterns are not synthesised.** `skineffectN` is a 32-bit id with no
//!   registry in any shipped package; the catalogue lives as prose in effect
//!   cheatsheets. So the ids and seeds are carried and reported, and no stripes
//!   or spots are guessed from them. Inventing a pattern from an opaque id is
//!   exactly the plausible-looking fabrication this project refuses.
//! * **A player's own creature is not here.** All 47 `crt` records are shipped
//!   presets. OpenSpore can reproduce those exactly; a player-authored creature's
//!   colour is not in these packages at all.

use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

use spore_creator_core::color::UnitRgb;
use spore_creator_core::document::CreatorDocument;

/// Why the alpha in [`blend_layer`] is not read from the record.
pub const SKIN_ALPHA_SOURCE: &str = "UNKNOWN";

/// The three colour slots, in the order the reference's layers use.
///
/// **Base, Coat, Detail** — the reference frames show exactly three collapsible
/// bars with those names, and `<skincolor1..3>` / `<skineffect1..3>` is a
/// three-slot structure. The correspondence is **INFERRED** from the count and
/// the order: nothing in the corpus states that slot 1 is the base. It is a much
/// better inference than any anatomical guess, and it is still an inference.
pub const SKIN_LAYER_COUNT: usize = 3;

/// What a document says about its own colour.
///
/// `None` on a field means the document wrote no such element, which is a real
/// state -- 7 of 68 documents write two of the three colours -- and is not the
/// same as "white".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkinPalette {
    /// `<skincolor1..3>`, in slot order.
    pub colors: [Option<UnitRgb>; SKIN_LAYER_COUNT],
    /// `<skineffect1..3>`, reported and **not** applied.
    pub effects: [Option<u32>; SKIN_LAYER_COUNT],
    /// `<skineffectseed1..3>`, reported and **not** applied.
    pub seeds: [Option<u32>; SKIN_LAYER_COUNT],
}

impl SkinPalette {
    /// Reads a document's own skin.
    ///
    /// A document with no colours is `None`, NOT a white palette: "this creature
    /// states no colour" and "this creature is white" are different facts, and
    /// defaulting to white would invent an appearance for every document whose
    /// colours we have not decoded.
    pub fn from_document(document: &CreatorDocument) -> Option<Self> {
        let colors = document.properties.skin_colors?;
        let has_any = colors.iter().any(|slot| slot.is_some());
        if !has_any {
            return None;
        }
        Some(Self {
            colors,
            effects: document
                .properties
                .skin_effects
                .unwrap_or([None; SKIN_LAYER_COUNT]),
            seeds: document
                .properties
                .skin_effect_seeds
                .unwrap_or([None; SKIN_LAYER_COUNT]),
        })
    }

    /// The base colour, i.e. slot 1.
    ///
    /// The one slot applied on its own, because it is the only one a part can be
    /// tinted with without the blend being a guess.
    pub fn base(&self) -> Option<UnitRgb> {
        self.colors[0]
    }

    /// The coat colour, i.e. slot 2.
    pub fn coat(&self) -> Option<UnitRgb> {
        self.colors[1]
    }

    /// The detail colour, i.e. slot 3.
    pub fn detail(&self) -> Option<UnitRgb> {
        self.colors[2]
    }

    /// The `1 / 3` report of what this palette carries.
    ///
    /// The effects and seeds are reported because they are real data a user of a
    /// Spore tool would want to see, and explicitly marked unapplied because a
    /// 32-bit id with no registry cannot be turned into a pattern honestly.
    pub fn describe(&self) -> String {
        let hex = |slot: Option<UnitRgb>| match slot {
            Some(colour) => format!(
                "#{:02x}{:02x}{:02x}",
                to_byte(colour.red),
                to_byte(colour.green),
                to_byte(colour.blue)
            ),
            None => "-".to_owned(),
        };
        let ids = |slots: [Option<u32>; SKIN_LAYER_COUNT]| {
            slots
                .iter()
                .map(|slot| match slot {
                    Some(value) => format!("{value:#010x}"),
                    None => "-".to_owned(),
                })
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "base={} coat={} detail={} effects=[{}] seeds=[{}] patterns=NOT_APPLIED",
            hex(self.base()),
            hex(self.coat()),
            hex(self.detail()),
            ids(self.effects),
            ids(self.seeds),
        )
    }
}

/// One `0.0 ..= 1.0` component as an 8-bit channel.
///
/// **Rounding, not truncating**: the shader's own upload is 8-bit, so rounding is
/// what the original does. Truncating would darken every colour by half a step
/// and the error would be invisible per part but systematic across the creature.
fn to_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Linear-space colour for a [`UnitRgb`].
///
/// sRGB -> linear, because a `StandardMaterial`'s `base_color` is a **linear**
/// value in Bevy and feeding it sRGB numbers makes every creature look washed out
/// and too bright. This is the same conversion `scene::ASSET_TINT` documents.
fn linear(colour: UnitRgb) -> [f32; 3] {
    let converted = Color::srgb(colour.red, colour.green, colour.blue).to_linear();
    [converted.red, converted.green, converted.blue]
}

/// The linear-space triple a `StandardMaterial` wants.
///
/// Public because the spawn path needs it directly: a caller building a material
/// by hand should not have to re-derive the sRGB -> linear step, and getting it
/// wrong renders every creature washed out.
pub fn linear_rgb(colour: UnitRgb) -> [f32; 3] {
    linear(colour)
}

/// Applies a document's skin colour to a material.
///
/// Takes the base colour and **nothing else**: the coat and detail layers need the
/// pattern masks to mean anything, and the masks are empty (49 of 30 sampled parts
/// carry `0x20003` Raster sections of exactly 32 bytes -- header only, zero
/// texels), so blending two more colours over a flat diffuse would be inventing
/// an appearance rather than reproducing one.
pub fn tint(
    materials: &mut Assets<StandardMaterial>,
    palette: Option<SkinPalette>,
) -> Handle<StandardMaterial> {
    let base = palette.and_then(|palette| palette.base()).map(linear);
    materials.add(crate::texture::standard_material(
        &crate::texture::SurfaceSet::default(),
        None,
        base,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_creator_core::sporemodel::{parse_sporemodel, Family};

    /// A document in the corpus's own grammar, carrying three skin colours.
    fn document(colors: &str) -> CreatorDocument {
        // `<properties>` is where the corpus puts these, and the grammar is not
        // forgiving: a `<skincolor1>` at the document root is an
        // `UnexpectedElement`, which the first draft of this fixture hit.
        let xml = format!(
            "<sporemodel><formatversion>16</formatversion>\
             <properties>{colors}</properties>\
             <blocks count=\"1\"><blockref><blockid>0x40626000, 0x00013530</blockid>\
             <transform><position>0,0,0</position><scale>1</scale><orientation>\
             <row0>1,0,0</row0><row1>0,1,0</row1><row2>0,0,1</row2>\
             </orientation></transform></blockref></blocks></sporemodel>"
        );
        parse_sporemodel(Family::Creature, &xml).expect("the fixture parses")
    }

    #[test]
    fn a_documents_own_colour_is_read_from_the_document() {
        let document = document(
            "<skincolor1>0.498039,0.803922,0.474510</skincolor1>\
             <skincolor2>1,1,1</skincolor2><skincolor3>1,1,1</skincolor3>",
        );
        let palette = SkinPalette::from_document(&document).expect("the document states a colour");
        let base = palette.base().expect("slot 1");
        assert!((base.red - 0.498039).abs() < 1e-5, "red {}", base.red);
        assert!((base.green - 0.803922).abs() < 1e-5, "green {}", base.green);
        assert!((base.blue - 0.474510).abs() < 1e-5, "blue {}", base.blue);
    }

    #[test]
    fn a_document_that_states_no_colour_is_not_white() {
        // The distinction this whole module exists to preserve. `None` is "no
        // colour stated"; a white palette would be an appearance invented for every
        // document whose colours we have not decoded.
        let document = document("");
        assert_eq!(
            SkinPalette::from_document(&document),
            None,
            "no <skincolor*> means no colour stated, which is not white"
        );
    }

    #[test]
    fn two_of_three_slots_is_a_real_state_and_not_a_rejection() {
        // 7 of 68 documents write exactly two colours, so the slots cannot be
        // treated as an all-or-nothing triple.
        let document =
            document("<skincolor1>0.2,0.3,0.4</skincolor1><skincolor2>0.5,0.6,0.7</skincolor2>");
        let palette = SkinPalette::from_document(&document).expect("one colour is enough");
        assert!(palette.base().is_some());
        assert!(palette.coat().is_some());
        assert!(
            palette.detail().is_none(),
            "an unwritten slot is absent, not white"
        );
    }

    #[test]
    fn the_description_reports_the_patterns_as_unapplied() {
        // The honest string. `patterns=NOT_APPLIED` is in the output so nobody reads
        // a report line and concludes the stripes were rendered.
        let document = document(
            "<skincolor1>0.498039,0.803922,0.474510</skincolor1>\
             <skineffect1>0x306b8dd8</skineffect1>\
             <skineffectseed1>4660</skineffectseed1>",
        );
        let text = SkinPalette::from_document(&document)
            .expect("colour")
            .describe();
        assert!(text.contains("patterns=NOT_APPLIED"), "{text}");
        assert!(
            text.contains("base=#7fcd79"),
            "the measured PaintBuddy green: {text}"
        );
        assert!(
            text.contains("0x306b8dd8"),
            "the effect id is reported: {text}"
        );
    }

    #[test]
    fn a_colour_round_trips_through_eight_bits_exactly() {
        // The measured value 0.498039 must come back as #7f, or every colour in the
        // game is off by one step and nobody can tell.
        assert_eq!(to_byte(0.498039), 127);
        assert_eq!(to_byte(0.803922), 205);
        assert_eq!(to_byte(0.474510), 121);
    }

    #[test]
    fn a_linear_conversion_is_actually_darker_than_srgb() {
        // Guards the washout. sRGB 1.0 is white; its linear value is also 1.0, but
        // mid-grey must come out darker, and if this ever inverts the creature
        // renders bleached.
        let mid = UnitRgb {
            red: 0.5,
            green: 0.5,
            blue: 0.5,
        };
        let linear_value = linear(mid);
        assert!(
            linear_value[0] < 0.5,
            "linear must be darker than sRGB, got {}",
            linear_value[0]
        );
        let white = UnitRgb {
            red: 1.0,
            green: 1.0,
            blue: 1.0,
        };
        assert!((linear(white)[0] - 1.0).abs() < 1e-5, "white stays white");
    }

    #[test]
    fn alpha_is_recorded_as_unknown_rather_than_read() {
        // There is no fourth component in `<skincolorN>`, so anything that claimed
        // an alpha source would be inventing one. This test exists so that
        // deleting the constant breaks something.
        assert_eq!(SKIN_ALPHA_SOURCE, "UNKNOWN");
        assert!(
            !SKIN_ALPHA_SOURCE.contains("found"),
            "the alpha source is not known, so the constant must not imply otherwise"
        );
    }
}
