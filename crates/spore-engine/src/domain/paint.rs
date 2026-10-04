//! # Where a Spore creature's colour comes from, and how much of it is proven
//!
//! `docs/COLOUR-SOURCES.md` measured the answer: a model's colour is **two
//! unit-range float RGB triples per paint** in the model's `sporemodel` XML
//! record, plus **three model-level `skincolor` slots**. It is *not* in the
//! textures — the `DXT5` families are masks and the `0x15` family is a
//! tangent-space normal map, so there is no albedo record anywhere in this
//! corpus.
//!
//! The shipped creature-skin shader (`smt` `0x0469A3F7`, function
//! `setupEditorPaint`) is the game's own statement of how the pieces combine:
//!
//! ```text
//! mixedColor = lerp(diffuseTexture.rgb, editorColors.mColor1.rgb, editorColors.mColor1.a);
//! mixedColor = lerp(mixedColor,         editorColors.mColor2.rgb, editorColors.mColor2.a * diffuseTexture.a);
//! Current.color.rgb = mixedColor * In.color.rgb;
//! ```
//!
//! This module resolves the colours and reduces that formula to what a modern
//! renderer can honestly reproduce. The split matters, so it is a table and not
//! a paragraph:
//!
//! | term | status |
//! |---|---|
//! | `mColor1.rgb` / `mColor2.rgb` are the record's `skincolor*` / `color1` / `color2` | **INFERRED** — `spore_assets::color::claims::SKIN_COLOR_TO_SHADER`. The shader's names, the effect cheatsheet's `SPSkinPaintClear -diffuse color1` and the XML's `<color1>` agree; no record states the assignment. |
//! | the values themselves | **OBSERVED**, read verbatim out of the record by `spore_assets::color::extract`. Never normalised, never clamped, never rounded to bytes. |
//! | `mColor1.a` / `mColor2.a` | **UNKNOWN**. `claims::COLOR_ALPHA_SOURCE`: the record stores three components and nothing in the corpus says where the fourth comes from. |
//! | the `lerp` weights over the raster | **not reproduced**, for the same reason. `claims::MASK_LAYER_PURPOSE` is `UNKNOWN`, so no plane is treated as a weight. |
//! | `In.color.rgb` (vertex colour) | **not available**: `spore_gmdl` yields positions, normals and uvs, and no vertex colour channel is decoded. |
//!
//! So the reduced form this build applies is **one constant colour per model**,
//! which is the whole of `mColor1.rgb` with no invented weight. That is an
//! *approximation* of the original graph and is graded as one; the colour
//! itself is real.
//!
//! # `paintregion`: structurally absent for a creature
//!
//! The brief's hard question — what does a `paintregion` bit select? — has an
//! answer that is better than an unknown, and it was measured rather than
//! guessed:
//!
//! * **0 of the 47 `crt` (creature) records and 0 of the 13 `cll` (cell)
//!   records contain a `<paint>` element at all.** They carry `skincolor1..3`
//!   and no paint list.
//! * The 12 records that do carry `<paintlist>` are `vcl` (vehicle), `ufo` and
//!   `bld` (building) — 4 + 2 + 1 records.
//!
//! So for a creature the region mask **does not exist**. There is no bit to
//! decode and inventing one would be inventing a field the record does not
//! have. [`RegionMapping::AbsentFromRecord`] records that as a
//! `Fact`-backed observation rather than as a zero mask.
//!
//! For the vehicle/ufo/building shapes that *do* have paints,
//! [`RegionMapping::Unmapped`] applies the first paint's `color1` to the whole
//! model and carries the masks verbatim with the meaning left `UNKNOWN` —
//! option (c) of the brief, typed.
//!
//! # The model → `sporemodel` link, and its grade
//!
//! Measured over the 3.1.0.22 install:
//!
//! * **PROVEN.** All 97 `gmdl` instance ids in `CreatureModelsHQ` (`0x40627100`)
//!   are also `CreatureModels` (`0x40626200`) instance ids, and the committed
//!   group-name table spells the relation: `0x40627100` is `CreatureModelsHQ`
//!   and `0x40626200` is `CreatureModels`. [`asset_family`] turns that into a
//!   comparison two group ids can be tested on.
//! * **PROVEN.** All 46 `crt` records that write `<modeltype>` write
//!   `0x9ea3031a`, and that hash appears verbatim as a little-endian word in
//!   exactly one `prop` record of group `0x40626000` — the block every one of
//!   them names in `<blockid>`. Two independent routes to the same record.
//! * **REFUTED, and this is the load-bearing negative.** *No* record in the
//!   corpus names a creature-part `gmdl` instance from a `sporemodel`, a block,
//!   or a `prop`: 0 of 97 creature `gmdl` records contain the model's own
//!   instance id's block instance, and no base-group `prop` references any
//!   instance of `0x40627100` or `0x40626200`.
//!
//! Therefore there is **no byte-level model → `sporemodel` identity**, and this
//! module does not claim one. [`PaintOrigin::FamilyDefault`] resolves at the
//! level of the *asset family* and is graded
//! [`EvidenceLevel::Inferred`]; the colours it reads are
//! [`EvidenceLevel::Observed`].
//!
//! Where the several records of a family disagree — 47 creature presets share
//! one family — the choice is **deterministic from the data**: the lowest
//! `(type, group, instance)` identity, so it does not depend on how a package
//! file happens to be ordered. `--sporemodel` overrides it, and the report
//! line prints the record's full identity and every colour it carries, so the
//! choice is never invisible.
//!
//! # Unavailable is not grey
//!
//! [`PaintBinding::Unavailable`] means *we looked and found nothing*. It is
//! not a colour, not a zero and not a default tint: the renderer substitutes
//! [`crate::texture::UNTEXURED_TINT`] and the report line says
//! `state=unavailable reason=...`. That is the only place a neutral colour
//! appears, and it appears because nothing was found rather than because
//! nothing was looked for.

use spore_assets::{ContentStore, ModelColors, PaintRegion, UnitRgb};
use spore_core::evidence::{EvidenceLevel, EvidenceState, Fact, Provenance};
use spore_core::record::{group_name, GROUP_NAMES};
use spore_core::ResourceKey;

/// The five DBPF type ids measured to hold `sporemodel` records, in the order
/// the resolver consults them.
///
/// `docs/COLOUR-SOURCES.md` §4.2: 47 `crt`, 13 `cll`, 4 `vcl`, 2 `ufo`, 1 `bld`
/// per package. Creature first because it is the family with the most records
/// and the one this engine renders.
pub const SPOREMODEL_TYPES: [(u32, &str); 5] = [
    (0x2B97_8C46, "crt"),
    (0x3D97_A8E4, "cll"),
    (0x2468_2294, "vcl"),
    (0x476A_98C7, "ufo"),
    (0x2399_BE55, "bld"),
];

/// Documented variant suffixes on a canonical group name.
///
/// Exactly one exists in the committed table: `CreatureModelsHQ`
/// (`0x40627100`). Its 97 records are the same records as `CreatureModels`
/// (`0x40626200`) — the instance ids agree 97 for 97 — under a name that
/// differs only by `HQ`, so removing the suffix expresses a family relation
/// that was measured rather than a guess about what `HQ` means.
///
/// Nothing else is listed. A suffix with no canonical name behind it is a
/// coincidence, and [`asset_family`] refuses to strip one.
pub const VARIANT_SUFFIXES: [&str; 1] = ["HQ"];

/// A group id's asset family: its canonical name with a documented variant
/// suffix removed.
///
/// Two groups are the same family when their [`AssetFamily`] values are equal.
/// This is the *only* family relation the corpus establishes between the group
/// a creature's geometry lives in and the group its `sporemodel` records live
/// in; see the module docs for what is proven and what is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetFamily(&'static str);

impl AssetFamily {
    /// The canonical name, e.g. `"CreatureModels"`.
    pub const fn name(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for AssetFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// The asset family of a group id, or `None` when the committed name table has
/// no name for it.
///
/// `None` is an absence, never an empty family: a group the table does not name
/// cannot be shown to belong to any family, so the resolver refuses it rather
/// than guessing. That refusal is reported as
/// [`PaintUnavailableReason::ModelGroupUnnamed`].
pub fn asset_family(group_id: u32) -> Option<AssetFamily> {
    let name = group_name(group_id)?;
    for suffix in VARIANT_SUFFIXES {
        let Some(stem) = name.strip_suffix(suffix) else {
            continue;
        };
        // The stem must itself be a canonical group name. Without this a suffix
        // that happens to trim any name would manufacture a family.
        if let Some((_, canonical)) = GROUP_NAMES.iter().find(|(_, other)| *other == stem) {
            return Some(AssetFamily(canonical));
        }
    }
    Some(AssetFamily(name))
}

/// How the `sporemodel` record for a model was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintOrigin {
    /// The caller named the record on the command line.
    ///
    /// The record's colours are still [`EvidenceLevel::Observed`]; what the
    /// caller asserts is that it is *this* record that colours *this* model,
    /// which no byte in the corpus establishes. The association is therefore
    /// [`EvidenceLevel::Inferred`], exactly as the family default's is.
    Explicit,
    /// Chosen by matching the model's asset family against the families the
    /// `sporemodel` records live in, then taking the lowest identity in that
    /// family.
    ///
    /// The family comparison rests on a measured name relation
    /// ([`asset_family`]); the step from "same family" to "colours this model"
    /// is [`EvidenceLevel::Inferred`] and is graded that way everywhere it is
    /// reported.
    FamilyDefault,
}

impl PaintOrigin {
    /// The canonical spelling, for a report line.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::FamilyDefault => "family-default",
        }
    }
}

/// Why no `sporemodel` record was found.
///
/// A **closed** vocabulary. A renderer that grew a reason here would be
/// reporting an outcome nobody designed, and the set is small because the
/// search has exactly four steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintUnavailableReason {
    /// No opened package held a record of any `sporemodel` type.
    ///
    /// Almost always a missing `--sporemodel-package`: the records live in
    /// `Spore_Game.package` and `PatchData.package`, not in `Spore_Content`.
    NoSporemodelRecords,
    /// `sporemodel` records were found, but none lives in the model's asset
    /// family.
    NoFamilyMatch,
    /// The model's group id has no canonical name in the committed table, so no
    /// family could be computed and none was invented.
    ModelGroupUnnamed,
    /// Records of the right family exist and every one of them refused to
    /// decode.
    DecodeRefused,
}

impl PaintUnavailableReason {
    /// The canonical spelling, for a report line.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoSporemodelRecords => "no_sporemodel_records",
            Self::NoFamilyMatch => "no_family_match",
            Self::ModelGroupUnnamed => "model_group_unnamed",
            Self::DecodeRefused => "decode_refused",
        }
    }
}

impl std::fmt::Display for PaintUnavailableReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The colour source bound to one model.
#[derive(Debug, Clone, PartialEq)]
pub enum PaintBinding {
    /// A `sporemodel` record was found, read and decoded.
    Bound(ModelPaint),
    /// None was found.
    ///
    /// **Not** a colour. The renderer substitutes its neutral tint and the
    /// report line states this reason, so "we did not look" can never be read
    /// as "we looked and found grey".
    Unavailable {
        /// Which of the four steps of the search came up empty.
        reason: PaintUnavailableReason,
        /// The same statement as a `Fact`: `UNKNOWN` / `MISSING`, no
        /// provenance, because a non-finding has no source to repeat.
        fact: Fact<()>,
    },
}

impl PaintBinding {
    /// Whether a record was found. Not "whether a colour exists".
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound(_))
    }

    /// The bound record, if any.
    pub fn paint(&self) -> Option<&ModelPaint> {
        match self {
            Self::Bound(paint) => Some(paint),
            Self::Unavailable { .. } => None,
        }
    }

    /// The `OPENSPORE-PAINT v1` line for one record.
    ///
    /// Takes the geometry record's identity as a parameter because on the
    /// unavailable path there is no `ModelPaint` to read it from, and the line
    /// must still name the model whose colour is missing.
    ///
    /// The unavailable line is deliberately **not** shaped like the available
    /// one: there is no `base=` value on it beyond `base=absent`, because a
    /// neutral tint is not this record's colour.
    pub fn describe(&self, model: &ResourceKey, index: usize, records: usize) -> String {
        match self {
            Self::Bound(paint) => paint.describe(index, records),
            Self::Unavailable { reason, .. } => format!(
                "OPENSPORE-PAINT v1 record={index}/{records} key={model} state=unavailable \
                 reason={} link={} base=absent",
                reason.as_str(),
                EvidenceLevel::Unknown.as_str(),
            ),
        }
    }
}

/// One `sporemodel` record, bound to one model.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPaint {
    /// The geometry record this colour was bound to.
    pub model: ResourceKey,
    /// The `sporemodel` record the colours were read from.
    pub record: ResourceKey,
    /// Which package answered for `record`.
    pub package_name: String,
    /// The record's canonical type name: `crt`, `cll`, `vcl`, `ufo`, `bld`.
    pub type_name: &'static str,
    /// How `record` was chosen.
    pub origin: PaintOrigin,
    /// How much to believe "this record colours this model".
    pub association: EvidenceLevel,
    /// Everything `spore_assets::color::extract` read, unmodified.
    pub colors: ModelColors,
    /// How many records of the family were candidates. `1` means the choice
    /// was forced, which is a materially different fact from "picked one of 47".
    pub family_candidates: usize,
}

impl ModelPaint {
    /// The grade of the record's own numbers. Always
    /// [`EvidenceLevel::Observed`]: they were read out of bytes.
    fn contents_fact() -> Fact<&'static str> {
        Fact::new(
            EvidenceLevel::Observed,
            EvidenceState::Derived,
            vec![Provenance::derived("crates/spore-assets/src/color.rs")],
            "the colours are read verbatim out of the sporemodel record",
        )
    }

    /// The colour this build applies, and where it came from.
    ///
    /// `None` when the record decoded but carried no colour at all, which is a
    /// real corpus shape: a `crt` always has `skincolor1`, but nothing in the
    /// format guarantees it.
    pub fn application(&self) -> Option<PaintApplication> {
        // A paint list is the shape that carries `color1`/`color2` and a region
        // mask; a skin record carries three independent colour slots. Prefer
        // the explicit paints when they exist, because a `vcl` record has no
        // skin slots at all and the two shapes never coexist in this corpus.
        if let Some(list) = self.colors.paint_lists.first() {
            if let Some(paint) = list.paints.first() {
                return Some(PaintApplication {
                    base: paint.color1,
                    source: PaintSource::PaintColor1 { list: 0, paint: 0 },
                });
            }
        }
        self.colors
            .skin_colors
            .iter()
            .position(Option::is_some)
            .and_then(|index| {
                self.colors.skin_colors[index].map(|colour| PaintApplication {
                    base: colour,
                    // The corpus writes `skincolor1..3`; report the 1-based
                    // spelling the record uses, not a 0-based array index.
                    source: PaintSource::SkinSlot(index + 1),
                })
            })
    }

    /// What this build did about `paintregion`.
    pub fn regions(&self) -> RegionMapping {
        let elements: usize = self
            .colors
            .paint_lists
            .iter()
            .map(|list| list.paints.len())
            .sum();
        let masks: Vec<PaintRegion> = self
            .colors
            .paint_lists
            .iter()
            .flat_map(|list| list.paints.iter())
            .map(|paint| paint.region)
            .collect();
        match masks.first() {
            // Measured: 0 of 47 `crt` and 0 of 13 `cll` records hold a `<paint>`.
            // The field is absent, not unread, so this is an observation and not
            // a non-finding.
            None => RegionMapping::AbsentFromRecord { elements },
            Some(_) => RegionMapping::Unmapped { elements, masks },
        }
    }

    /// The base colour as linear floats, or `None` when there is none.
    pub fn base_rgb(&self) -> Option<[f32; 3]> {
        self.application()
            .map(|application| application.base.components())
    }

    /// How many of the three skin slots the record carries.
    ///
    /// Distinct from "how many are white": `1,1,1` is a recorded colour.
    pub fn skin_slots_present(&self) -> usize {
        self.colors
            .skin_colors
            .iter()
            .filter(|c| c.is_some())
            .count()
    }

    /// How many colour slots the record carries but this build did **not**
    /// apply, because the weight that would select them is
    /// [`spore_assets::color::claims::COLOR_ALPHA_SOURCE`].
    pub fn unapplied_colours(&self) -> usize {
        let applied = usize::from(self.application().is_some());
        self.colors.distinct_colors().len().saturating_sub(applied)
    }

    /// The base colour as a `#rrggbb` string, for a report line.
    ///
    /// A display convenience only: `UnitRgb::to_corpus_text` remains the
    /// canonical spelling of the value and this never feeds the renderer.
    pub fn base_hex(&self) -> Option<String> {
        self.base_rgb().map(|rgb| {
            let channel = |v: f32| {
                let scaled = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                format!("{scaled:02x}")
            };
            format!("#{}{}{}", channel(rgb[0]), channel(rgb[1]), channel(rgb[2]))
        })
    }

    /// The `OPENSPORE-PAINT v1` line.
    pub fn describe(&self, index: usize, records: usize) -> String {
        let application = self.application();
        let (base_hex, base_rgb, base_source) = match &application {
            Some(application) => (
                application.base_hex(),
                application.base.to_corpus_text(),
                application.source.as_str(),
            ),
            None => ("absent".to_owned(), "absent".to_owned(), "absent"),
        };
        let paints: usize = self
            .colors
            .paint_lists
            .iter()
            .map(|list| list.paints.len())
            .sum();
        let (regions, region_reason) = match self.regions() {
            RegionMapping::AbsentFromRecord { elements } => {
                ("absent".to_owned(), format!("paint_elements={elements}"))
            }
            RegionMapping::Unmapped {
                elements,
                ref masks,
            } => (
                "unmapped".to_owned(),
                format!(
                    "paint_elements={elements} distinct_masks={} bits=UNKNOWN",
                    {
                        let mut distinct: Vec<PaintRegion> = Vec::new();
                        for mask in masks {
                            if !distinct.contains(mask) {
                                distinct.push(*mask);
                            }
                        }
                        distinct.len()
                    }
                ),
            ),
        };
        format!(
            "OPENSPORE-PAINT v1 record={index}/{records} key={} state=available \
             sporemodel={} package={} type={} origin={} link={} candidates={} \
             base={base_hex} base_source={base_source} base_rgb={base_rgb} \
             skin_slots={} paints={paints} regions={regions} {region_reason} \
             unapplied_colours={} alpha_source=UNKNOWN mask_purpose=UNKNOWN",
            self.model,
            self.record,
            self.package_name,
            self.type_name,
            self.origin.as_str(),
            self.association.as_str(),
            self.family_candidates,
            self.skin_slots_present(),
            self.unapplied_colours(),
        )
    }
}

/// What this build did about `paintregion`.
///
/// Deliberately typed rather than summarised, because the two cases have
/// different evidence behind them and collapsing them would make an absent
/// field look like an unread one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegionMapping {
    /// The record holds **no** `<paint>` element, so there is no region mask.
    ///
    /// `elements` is the count that was measured, and it is `0`. For a creature
    /// this is the real shape of the data: 0 of 47 `crt` records and 0 of 13
    /// `cll` records carry a `<paint>` at all.
    AbsentFromRecord {
        /// How many `<paint>` elements were found. `0` for every creature and
        /// cell record measured.
        elements: usize,
    },
    /// `<paint>` elements exist and each carries a region mask whose bit
    /// meaning no table in the corpus states.
    ///
    /// The masks are carried verbatim in document order, deduplicated for the
    /// report, and **no bit is decoded**. One colour is applied to the whole
    /// model, which is the brief's option (c): a visibly correct-coloured model
    /// plus an explicit `UNKNOWN` instead of a fake bit decoder.
    Unmapped {
        /// How many `<paint>` elements the record holds.
        elements: usize,
        /// Every element's mask, in document order, duplicates included.
        masks: Vec<PaintRegion>,
    },
}

impl RegionMapping {
    /// The canonical spelling, for a report line.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AbsentFromRecord { .. } => "absent",
            Self::Unmapped { .. } => "unmapped",
        }
    }

    /// Whether a bit was decoded. Always `false`, and asserted as such: a `true`
    /// here would mean this build had invented a mapping.
    pub const fn is_decoded(&self) -> bool {
        false
    }

    /// How many `<paint>` elements were seen.
    pub const fn elements(&self) -> usize {
        match self {
            Self::AbsentFromRecord { elements } | Self::Unmapped { elements, .. } => *elements,
        }
    }
}

/// The colour actually applied, and which record field it came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintApplication {
    /// The colour, verbatim from the record.
    pub base: UnitRgb,
    /// Which field supplied it.
    pub source: PaintSource,
}

impl PaintApplication {
    /// The colour as a `#rrggbb` string, for a report line.
    pub fn base_hex(&self) -> String {
        let channel = |v: f32| {
            let scaled = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("{scaled:02x}")
        };
        format!(
            "#{}{}{}",
            channel(self.base.red),
            channel(self.base.green),
            channel(self.base.blue)
        )
    }
}

/// Which record field a colour came from.
///
/// Spelled the way the record spells it, so a reader can grep the corpus text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintSource {
    /// `<skincolorN>` with `N` 1-based, exactly as written.
    SkinSlot(usize),
    /// A `<paint>`'s `<color1>`, by paint-list and paint index.
    PaintColor1 {
        /// Which `<paintlist>`, 0-based, in document order.
        list: usize,
        /// Which `<paint>` inside it, 0-based.
        paint: usize,
    },
}

impl PaintSource {
    /// The canonical spelling, for a report line.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SkinSlot(slot) => match slot {
                1 => "skincolor1",
                2 => "skincolor2",
                _ => "skincolorN",
            },
            Self::PaintColor1 { .. } => "color1",
        }
    }
}

/// One mesh's resolved base colour.
#[derive(Debug, Clone, PartialEq)]
pub struct MeshPaint {
    /// Which mesh of the model this applies to.
    pub mesh_index: u32,
    /// Linear RGB, `None` when no `sporemodel` colour was found.
    ///
    /// An absence, and the renderer substitutes
    /// [`crate::texture::UNTEXURED_TINT`] for it while the report says why. It
    /// is never `(0,0,0)`: black is a colour a record can carry, and this is
    /// not one.
    pub base_color: Option<[f32; 3]>,
    /// Which record field supplied `base_color`.
    pub source: Option<PaintSource>,
    /// How much to believe the record colours this mesh's model.
    pub association: EvidenceLevel,
    /// What was done about `paintregion`.
    pub regions: RegionMapping,
}

impl MeshPaint {
    /// One line for a log.
    pub fn describe(&self) -> String {
        let (colour, source) = match (self.base_color, self.source) {
            (Some(rgb), Some(source)) => (
                format!(
                    "#{:02x}{:02x}{:02x}",
                    (rgb[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (rgb[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (rgb[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                ),
                source.as_str(),
            ),
            _ => ("absent".to_owned(), "absent"),
        };
        format!(
            "mesh {} base_color={colour} from={source} link={} regions={} elements={}",
            self.mesh_index,
            self.association.as_str(),
            self.regions.as_str(),
            self.regions.elements(),
        )
    }
}

/// Where the model ↔ `sporemodel` association's grade comes from.
///
/// One function so the grade cannot drift between the resolver and the report.
fn association_grade() -> EvidenceLevel {
    // There is no byte-level model -> sporemodel identity in this corpus; see
    // the module docs for the negative that establishes it. Every association
    // this build makes is therefore inferred, whether the record was named by
    // the caller or found through the asset family.
    EvidenceLevel::Inferred
}

fn unavailable(reason: PaintUnavailableReason, message: &'static str) -> PaintBinding {
    PaintBinding::Unavailable {
        reason,
        fact: Fact::unavailable(message),
    }
}

/// Resolves the colour source for one model.
///
/// `explicit` is the `--sporemodel` value, if any. Resolution order:
///
/// 1. **Explicit.** The named record, read whatever its group.
/// 2. **Family default.** The model's [`asset_family`], then the lowest
///    `(type, group, instance)` `sporemodel` identity in that family that
///    decodes and carries a colour.
///
/// Never a silent failure: every outcome is one of [`PaintBinding`]'s two
/// variants, and the unavailable variant carries a closed reason.
pub fn resolve(
    store: &ContentStore,
    model: &ResourceKey,
    explicit: Option<ResourceKey>,
) -> PaintBinding {
    if let Some(record) = explicit {
        return match read(store, model, record, PaintOrigin::Explicit, 1) {
            Ok(paint) => PaintBinding::Bound(paint),
            Err(reason) => unavailable(reason, "the named sporemodel record did not yield colour"),
        };
    }

    let Some(family) = asset_family(model.group_id) else {
        return unavailable(
            PaintUnavailableReason::ModelGroupUnnamed,
            "the model's group id has no canonical name in the committed table",
        );
    };

    // The candidate list is computed over the store's own index, so it is a
    // measurement of the packages actually opened rather than a constant table
    // that could disagree with them.
    let candidates = family_candidates(store, family);
    if candidates.is_empty() {
        return if any_sporemodel_record_exists(store) {
            unavailable(
                PaintUnavailableReason::NoFamilyMatch,
                "sporemodel records exist, none in the model's asset family",
            )
        } else {
            unavailable(
                PaintUnavailableReason::NoSporemodelRecords,
                "no opened package holds a record of a sporemodel type; \
                 Spore_Game.package and PatchData.package carry them",
            )
        };
    }

    let total = candidates.len();
    let mut last_refusal: Option<String> = None;
    for record in candidates {
        match read(store, model, record, PaintOrigin::FamilyDefault, total) {
            Ok(paint) => return PaintBinding::Bound(paint),
            Err(reason) => {
                last_refusal.get_or_insert_with(|| reason.as_str().to_owned());
            }
        }
    }
    unavailable(
        PaintUnavailableReason::DecodeRefused,
        // Kept as a static reason rather than a formatted one so the closed
        // vocabulary stays closed; the specific refusal is on the log.
        match last_refusal {
            Some(_) => "every sporemodel record in the family refused to decode",
            None => "the family's sporemodel records were all absent",
        },
    )
}

/// Reads and decodes one `sporemodel` record into a [`ModelPaint`].
///
/// `Err` is one of the four closed reasons. `DecodeRefused` carries the
/// decoder's own message on the log rather than in the enum, so the vocabulary
/// cannot grow a variant per parser error.
fn read(
    store: &ContentStore,
    model: &ResourceKey,
    record: ResourceKey,
    origin: PaintOrigin,
    family_candidates: usize,
) -> Result<ModelPaint, PaintUnavailableReason> {
    let package_name = store
        .find(&record)
        .map(|found| found.package_name.to_owned())
        .unwrap_or_else(|_| "unresolved".to_owned());
    let bytes = store
        .read(&record)
        .map_err(|_| PaintUnavailableReason::DecodeRefused)?;
    // `eprintln!` rather than Bevy's `error!`: this module is below the
    // renderer and must not grow a Bevy dependency to say something.
    let colors = spore_assets::extract_model_colors(&bytes).map_err(|error| {
        eprintln!("openspore: sporemodel {record} refused to decode: {error}");
        PaintUnavailableReason::DecodeRefused
    })?;
    let type_name = SPOREMODEL_TYPES
        .iter()
        .find(|(type_id, _)| *type_id == record.type_id)
        .map_or("unknown", |(_, name)| *name);
    let paint = ModelPaint {
        model: *model,
        record,
        package_name,
        type_name,
        origin,
        association: association_grade(),
        colors,
        family_candidates,
    };
    // A record that decoded but carries no colour is not a colour, and saying
    // so is better than painting with the first default tint it happens to hold.
    if paint.application().is_none() {
        return Err(PaintUnavailableReason::DecodeRefused);
    }
    Ok(paint)
}

/// Every `sporemodel` identity in `family`, lowest identity first.
///
/// `ContentStore::identities_of_type` already returns package-then-index order;
/// this sorts by the identity so the choice does not depend on how a package
/// file happens to be laid out. That matters because the family's choice is
/// arbitrary by nature — 47 creature presets share one family — and an
/// arbitrary choice has to at least be reproducible.
fn family_candidates(store: &ContentStore, family: AssetFamily) -> Vec<ResourceKey> {
    let mut out: Vec<ResourceKey> = Vec::new();
    for (type_id, _) in SPOREMODEL_TYPES {
        for key in store.identities_of_type(type_id) {
            if asset_family(key.group_id) == Some(family) && !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out.sort();
    out
}

/// Whether any opened package holds a record of a `sporemodel` type.
///
/// Used only to choose between [`PaintUnavailableReason::NoFamilyMatch`] and
/// [`PaintUnavailableReason::NoSporemodelRecords`], because "I looked and
/// there is nothing to look at" and "I looked and it is not yours" are
/// different diagnoses with different fixes.
fn any_sporemodel_record_exists(store: &ContentStore) -> bool {
    SPOREMODEL_TYPES
        .iter()
        .any(|(type_id, _)| !store.identities_of_type(*type_id).is_empty())
}

/// The paint plan for one mesh of one model.
///
/// A pure function of the binding, so the whole colour path is assertable with
/// no window, no GPU and no game install — which is the honest headless test.
pub fn mesh_paint(binding: &PaintBinding, mesh_index: u32) -> MeshPaint {
    match binding {
        PaintBinding::Bound(paint) => MeshPaint {
            mesh_index,
            base_color: paint.base_rgb(),
            source: paint.application().map(|application| application.source),
            association: paint.association,
            regions: paint.regions(),
        },
        PaintBinding::Unavailable { .. } => MeshPaint {
            mesh_index,
            base_color: None,
            source: None,
            association: EvidenceLevel::Unknown,
            regions: RegionMapping::AbsentFromRecord { elements: 0 },
        },
    }
}

/// The provenance record for a bound colour.
///
/// Exposed so the renderer, the report and a test all quote the same string
/// rather than each inventing one.
pub fn contents_provenance() -> Fact<&'static str> {
    ModelPaint::contents_fact()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREATURE_MODELS: u32 = 0x4062_6200;
    const CREATURE_MODELS_HQ: u32 = 0x4062_7100;
    const CELL_MODELS: u32 = 0x4061_6200;
    const CELL_IMAGES: u32 = 0x4061_6201;
    const PALETTES: u32 = 0x406B_6A00;
    const PALETTE_MODELS: u32 = 0x406B_6200;

    #[test]
    fn every_named_group_resolves_to_exactly_one_family() {
        for (group_id, name) in GROUP_NAMES {
            let family = asset_family(*group_id)
                .unwrap_or_else(|| panic!("{name} ({group_id:#010x}) has no family"));
            assert!(!family.name().is_empty());
        }
        assert_eq!(asset_family(0xDEAD_BEEF), None);
    }

    #[test]
    fn the_hq_group_shares_a_family_with_its_non_hq_twin_and_nothing_else_does() {
        // Measured: 0x40627100 (CreatureModelsHQ) holds the same 97 instance
        // ids as 0x40626200 (CreatureModels). The names differ by `HQ` and
        // nothing else.
        assert_eq!(
            asset_family(CREATURE_MODELS_HQ),
            asset_family(CREATURE_MODELS)
        );
        assert_eq!(
            asset_family(CREATURE_MODELS_HQ).map(|f| f.name()),
            Some("CreatureModels")
        );
        // The near-miss pairs must NOT merge. `CellModels`/`CellImages` share a
        // first word, and `Palettes`/`PaletteModels` share a stem; merging
        // either would put a cell's colour on a texture record.
        assert_ne!(asset_family(CELL_MODELS), asset_family(CELL_IMAGES));
        assert_ne!(asset_family(PALETTES), asset_family(PALETTE_MODELS));
    }

    #[test]
    fn stripping_a_variant_suffix_never_invents_a_family() {
        // The only stripped suffix is `HQ`, and it only applies where the stem
        // is itself a canonical name. Adding a suffix with no name behind it
        // would silently create a family that no group belongs to.
        for (group_id, name) in GROUP_NAMES {
            for suffix in VARIANT_SUFFIXES {
                let Some(stem) = name.strip_suffix(suffix) else {
                    continue;
                };
                let stem_is_canonical = GROUP_NAMES.iter().any(|(_, other)| *other == stem);
                assert!(
                    stem_is_canonical,
                    "{name} strips to {stem}, which is not a canonical group name"
                );
                assert!(asset_family(*group_id).is_some());
            }
        }
    }

    fn creature_record(body: &str) -> ModelColors {
        spore_assets::extract_model_colors(
            format!(
                "{}<sporemodel>{body}</sporemodel>",
                r#"<?xml version="1.0" encoding="UTF-8"?>"#
            )
            .as_bytes(),
        )
        .expect("synthetic record decodes")
    }

    fn bound(colors: ModelColors) -> PaintBinding {
        PaintBinding::Bound(ModelPaint {
            model: ResourceKey::new(0x00E6_BCE5, CREATURE_MODELS_HQ, 0x067C_79D2),
            record: ResourceKey::new(0x2B97_8C46, CREATURE_MODELS, 0x05E3_DF13),
            package_name: "Spore_Game".to_owned(),
            type_name: "crt",
            origin: PaintOrigin::FamilyDefault,
            association: association_grade(),
            colors,
            family_candidates: 47,
        })
    }

    #[test]
    fn a_creature_record_takes_skincolor1_verbatim_with_no_invented_weight() {
        // The measured lowest-identity creature preset:
        // 0x2b978c46:0x40626200:0x05e3df13, skincolor1 = 0.498039,0.803922,0.474510.
        let binding = bound(creature_record(
            "<properties><skincolor1>0.498039,0.803922,0.474510</skincolor1>\
             <skincolor2>1,1,1</skincolor2><skincolor3>1,1,1</skincolor3></properties>",
        ));
        let paint = binding.paint().expect("bound");
        let application = paint.application().expect("a colour");
        assert_eq!(application.base.red, 0.498_039);
        assert_eq!(application.base.green, 0.803_922);
        assert_eq!(application.base.blue, 0.474_510);
        assert_eq!(application.source, PaintSource::SkinSlot(1));
        assert_eq!(paint.base_hex().as_deref(), Some("#7fcd79"));
        // Never normalised, clamped or rounded on the way to the renderer.
        assert_eq!(paint.base_rgb().unwrap(), [0.498_039, 0.803_922, 0.474_510]);
        assert_eq!(paint.skin_slots_present(), 3);
        // `1,1,1` is a recorded white, not an absent slot: two distinct colours
        // were read and one of them is not applied because the weight that
        // would select it is UNKNOWN.
        assert_eq!(paint.unapplied_colours(), 1);
    }

    #[test]
    fn a_creature_record_has_no_region_mask_to_map_and_says_absent_not_zero() {
        let binding = bound(creature_record(
            "<properties><skincolor1>0.498039,0.803922,0.474510</skincolor1></properties>",
        ));
        let paint = binding.paint().unwrap();
        let regions = paint.regions();
        assert_eq!(regions, RegionMapping::AbsentFromRecord { elements: 0 });
        assert!(!regions.is_decoded());
        // Absent is a distinct value from a zero mask: no mask exists at all.
        assert_ne!(
            regions,
            RegionMapping::Unmapped {
                elements: 0,
                masks: Vec::new()
            }
        );
    }

    #[test]
    fn a_paintlist_record_keeps_its_masks_verbatim_and_decodes_no_bit() {
        let binding = bound(creature_record(
            "<blocks><blockref><paintlist count=\"2\">\
             <paint><paintregion>0x00000005</paintregion><paintid>0xf5967f30</paintid>\
             <color1>0.694117,0.352941,0.223529</color1>\
             <color2>0.749020,0.749020,0.749020</color2></paint>\
             <paint><paintregion>0x00000007</paintregion><paintid>0x7da7e22c</paintid>\
             <color1>0,0,0</color1><color2>1,0.937255,0.690196</color2></paint>\
             </paintlist></blockref></blocks>",
        ));
        let paint = binding.paint().unwrap();
        let regions = paint.regions();
        let RegionMapping::Unmapped { elements, masks } = &regions else {
            panic!("a record with paints must not report the mask as absent: {regions:?}");
        };
        assert_eq!(*elements, 2);
        // Carried verbatim, in document order, duplicates kept: no bit decoded.
        assert_eq!(masks[0].bits(), 0x0000_0005);
        assert_eq!(masks[1].bits(), 0x0000_0007);
        assert!(!regions.is_decoded());
        // The applied colour is the first paint's `color1`, spelled as the
        // record spells it.
        let application = paint.application().expect("a colour");
        assert_eq!(
            application.source,
            PaintSource::PaintColor1 { list: 0, paint: 0 }
        );
        assert_eq!(application.base.green, 0.352_941);
    }

    #[test]
    fn a_record_that_carries_no_colour_yields_no_colour_and_never_black() {
        // `<zcorpscore>` alone: a real `sporemodel` field, and no colour in it.
        // `read` turns this into the unavailable variant rather than painting
        // with a default; the binding built directly here is the shape the
        // renderer would receive if it did not, and it still yields `None`.
        let binding = bound(creature_record(
            "<properties><zcorpscore>255</zcorpscore></properties>",
        ));
        let paint = binding.paint().expect("the record itself decoded");
        assert_eq!(paint.application(), None);
        assert_eq!(paint.base_rgb(), None);
        assert_eq!(paint.base_hex(), None);
        assert_eq!(paint.skin_slots_present(), 0);
        assert_eq!(mesh_paint(&binding, 0).base_color, None);
        assert_ne!(mesh_paint(&binding, 0).base_color, Some([0.0, 0.0, 0.0]));
        // And the report says `absent`, not a tint.
        let line = binding.describe(
            &ResourceKey::new(0x00E6_BCE5, CREATURE_MODELS_HQ, 0x067C_79D2),
            0,
            1,
        );
        assert!(line.contains("base=absent"), "{line}");
        assert!(line.contains("base_rgb=absent"), "{line}");
    }

    #[test]
    fn every_mesh_of_a_bound_model_gets_the_same_colour_and_says_why() {
        let binding = bound(creature_record(
            "<properties><skincolor1>0.498039,0.803922,0.474510</skincolor1></properties>",
        ));
        let first = mesh_paint(&binding, 0);
        let second = mesh_paint(&binding, 1);
        assert_eq!(first.base_color, second.base_color);
        assert_eq!(first.base_color, Some([0.498_039, 0.803_922, 0.474_510]));
        // The per-mesh cut is an inference in this build and is graded as one,
        // because no bit of a region mask selects a mesh.
        assert_eq!(first.association, EvidenceLevel::Inferred);
        assert!(first.describe().contains("regions=absent"));
    }

    #[test]
    fn an_unavailable_binding_grades_unknown_and_names_its_reason() {
        let binding = unavailable(
            PaintUnavailableReason::ModelGroupUnnamed,
            "the model's group id has no canonical name in the committed table",
        );
        let mesh = mesh_paint(&binding, 0);
        assert_eq!(mesh.base_color, None, "never black, never a default tint");
        assert_eq!(mesh.source, None);
        assert_eq!(mesh.association, EvidenceLevel::Unknown);
        let line = binding.describe(
            &ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0),
            0,
            1,
        );
        assert!(line.starts_with("OPENSPORE-PAINT v1 "), "{line}");
        assert!(line.contains("state=unavailable"), "{line}");
        assert!(line.contains("reason=model_group_unnamed"), "{line}");
        assert!(line.contains("base=absent"), "{line}");
        assert!(line.contains("link=UNKNOWN"), "{line}");
        // The available line must be shaped differently: a neutral tint is not
        // this record's colour.
        assert!(!line.contains("skin_slots="), "{line}");
    }

    #[test]
    fn an_unavailable_fact_is_unknown_missing_and_carries_no_provenance() {
        let binding = unavailable(PaintUnavailableReason::NoFamilyMatch, "nothing here");
        let PaintBinding::Unavailable { fact, .. } = binding else {
            panic!("expected the unavailable variant");
        };
        assert!(fact.is_unavailable());
        assert_eq!(fact.level(), EvidenceLevel::Unknown);
        assert_eq!(fact.evidence_state(), EvidenceState::Missing);
        assert!(fact.provenance().is_empty());
        assert_eq!(fact.value(), None);
    }

    #[test]
    fn a_bound_colour_is_observed_but_its_association_to_the_model_is_inferred() {
        // These are two different claims and the module must not let them merge:
        // the numbers came out of bytes, while "these are this model's colours"
        // is an argument, not an observation.
        assert_eq!(contents_provenance().level(), EvidenceLevel::Observed);
        assert!(contents_provenance().is_available());
        assert_eq!(association_grade(), EvidenceLevel::Inferred);
        assert_ne!(contents_provenance().level(), association_grade());
    }

    #[test]
    fn the_bound_report_line_carries_every_colour_it_chose_between() {
        let binding = bound(creature_record(
            "<properties><skincolor1>0.498039,0.803922,0.474510</skincolor1>\
             <skincolor2>1,1,1</skincolor2></properties>",
        ));
        let line = binding.describe(
            &ResourceKey::new(0x00E6_BCE5, CREATURE_MODELS_HQ, 0x067C_79D2),
            2,
            5,
        );
        assert!(line.starts_with("OPENSPORE-PAINT v1 record=2/5 "), "{line}");
        assert!(line.contains("state=available"), "{line}");
        assert!(line.contains("type=crt"), "{line}");
        assert!(line.contains("origin=family-default"), "{line}");
        assert!(line.contains("link=INFERRED"), "{line}");
        assert!(line.contains("base=#7fcd79"), "{line}");
        assert!(
            line.contains("base_rgb=0.498039,0.803922,0.474510"),
            "{line}"
        );
        assert!(line.contains("base_source=skincolor1"), "{line}");
        assert!(line.contains("regions=absent"), "{line}");
        assert!(line.contains("paint_elements=0"), "{line}");
        // The two unknowns that stopped this build from reproducing the shader
        // are named on the line, not left in a comment.
        assert!(line.contains("alpha_source=UNKNOWN"), "{line}");
        assert!(line.contains("mask_purpose=UNKNOWN"), "{line}");
        // And the candidate count, so "one of 47" is visible.
        assert!(line.contains("candidates=47"), "{line}");
    }

    #[test]
    fn the_sporemodel_type_list_matches_the_recorded_vocabulary() {
        // docs/COLOUR-SOURCES.md §4.2. Pinned because adding a type without
        // measuring it would be a guess about a record nobody has read.
        assert_eq!(
            SPOREMODEL_TYPES,
            [
                (0x2B97_8C46, "crt"),
                (0x3D97_A8E4, "cll"),
                (0x2468_2294, "vcl"),
                (0x476A_98C7, "ufo"),
                (0x2399_BE55, "bld"),
            ]
        );
        for (type_id, name) in SPOREMODEL_TYPES {
            assert_eq!(
                spore_core::record::RecordType::new(type_id).name(),
                Some(name),
                "type 0x{type_id:08x} must be named by the committed table"
            );
        }
    }

    #[test]
    fn a_store_with_no_packages_yields_unavailable_not_a_colour() {
        let store = ContentStore::new();
        let model = ResourceKey::new(0x00E6_BCE5, CREATURE_MODELS_HQ, 0x067C_79D2);
        let binding = resolve(&store, &model, None);
        assert!(!binding.is_bound());
        assert_eq!(
            binding,
            PaintBinding::Unavailable {
                reason: PaintUnavailableReason::NoSporemodelRecords,
                fact: Fact::unavailable(
                    "no opened package holds a record of a sporemodel type; \
                     Spore_Game.package and PatchData.package carry them"
                ),
            }
        );
        assert_eq!(mesh_paint(&binding, 0).base_color, None);
    }

    #[test]
    fn an_explicit_record_that_is_absent_is_unavailable_with_its_own_reason() {
        let store = ContentStore::new();
        let model = ResourceKey::new(0x00E6_BCE5, CREATURE_MODELS_HQ, 0x067C_79D2);
        let named = ResourceKey::new(0x2B97_8C46, CREATURE_MODELS, 0x05E3_DF13);
        assert!(!resolve(&store, &model, Some(named)).is_bound());
    }

    #[test]
    fn a_model_group_the_table_does_not_name_is_refused_rather_than_guessed() {
        // 0x40637E03 has no canonical name, so no family can be computed. The
        // resolver must say so instead of widening the search.
        let store = ContentStore::new();
        let model = ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0);
        let binding = resolve(&store, &model, None);
        assert_eq!(
            binding,
            PaintBinding::Unavailable {
                reason: PaintUnavailableReason::ModelGroupUnnamed,
                fact: Fact::unavailable(
                    "the model's group id has no canonical name in the committed table"
                ),
            }
        );
    }

    #[test]
    fn every_reason_has_a_stable_spelling_and_there_are_only_four() {
        let spellings = [
            PaintUnavailableReason::NoSporemodelRecords.as_str(),
            PaintUnavailableReason::NoFamilyMatch.as_str(),
            PaintUnavailableReason::ModelGroupUnnamed.as_str(),
            PaintUnavailableReason::DecodeRefused.as_str(),
        ];
        assert_eq!(
            spellings,
            [
                "no_sporemodel_records",
                "no_family_match",
                "model_group_unnamed",
                "decode_refused",
            ],
            "the vocabulary is closed; a new spelling is a new outcome and must \
             be designed, not discovered"
        );
    }

    #[test]
    fn a_paint_source_names_the_record_spelling_not_an_array_index() {
        assert_eq!(PaintSource::SkinSlot(1).as_str(), "skincolor1");
        assert_eq!(PaintSource::SkinSlot(2).as_str(), "skincolor2");
        // Beyond the measured slots the name stays positional rather than
        // inventing a third element name.
        assert_eq!(PaintSource::SkinSlot(3).as_str(), "skincolorN");
        assert_eq!(
            PaintSource::PaintColor1 { list: 0, paint: 0 }.as_str(),
            "color1"
        );
    }

    #[test]
    fn the_hex_formatter_rounds_rather_than_truncates_and_clamps_the_range() {
        let binding = bound(creature_record(
            "<properties><skincolor1>1,0.5,0</skincolor1></properties>",
        ));
        assert_eq!(
            binding.paint().unwrap().base_hex().as_deref(),
            Some("#ff8000")
        );
    }
}
