//! The `<sporemodel>` decoder: Spore's own creator-document format.
//!
//! # This is a format that exists, not one that was designed here
//!
//! 68 records in `Spore_Game.package` hold a creator document as text: `crt` 47,
//! `cll` 13, `vcl` 4, `ufo` 2, `bld` 1, `flr` 1. All parse, and between them
//! they place 680 `<blockref>` blocks. The grammar is catalogued in
//! `docs/CREATOR-CORE.md` §1; this module is that catalogue in Rust.
//!
//! # What the decoder refuses to do
//!
//! * **It does not skip what it does not know.** An element or attribute outside
//!   the grammar is [`CreatorError::UnexpectedElement`] /
//!   [`CreatorError::UnexpectedAttribute`], never a discarded child. A corpus
//!   version that adds a field must be adopted deliberately, in a diff a reader
//!   can see.
//! * **It does not fill absences.** A missing `<childlist>` is `None`; a missing
//!   `<trianglepickorigin>` is `None`; a missing `<paintlist>` is `None`. See
//!   [`crate::document`] for why that distinction is the whole ballgame.
//! * **It does not gate on `formatversion`.** The corpus refutes gating on it:
//!   `v16` documents both write and omit `<handles>`, and so do `v15` documents.
//! * **It does not default a number.** A field that will not parse is an error
//!   with its element name and block index, not a zero.

use spore_core::parse_id;

use crate::document::{
    Block, BlockId, BlockTransform, ChannelWeight, CreatorDocument, DocumentProperties, Limb,
    Matrix3, PaintRegion, Vec3,
};
use crate::error::{CreatorError, Result};
use crate::xml::{self, Element};

/// The record families a creator document arrives as.
///
/// The type ids are the ones the DBPF index actually holds. An earlier draft of
/// `docs/CREATOR-CORE.md` carried hand-converted hex that matched **no record in
/// the tree**; these were re-read from the index
/// (`docs/analysis/creator/07-architecture-review.md` §1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    /// `0x2B978C46` — 47 records, group `0x40626200`.
    Creature,
    /// `0x3D97A8E4` — 13 records, group `0x40616200`.
    Cell,
    /// `0x24682294` — 4 records, group `0x40646200`.
    Vehicle,
    /// `0x476A98C7` — 2 records, group `0x40656200`. Borrows vehicle-family parts.
    Ufo,
    /// `0x2399BE55` — 1 record, group `0x40636200`.
    Building,
    /// `0x438F6347` — 1 record, group `0x40666200`.
    Flora,
}

impl Family {
    /// Every family, in census order.
    pub const ALL: [Self; 6] = [
        Self::Creature,
        Self::Cell,
        Self::Vehicle,
        Self::Ufo,
        Self::Building,
        Self::Flora,
    ];

    /// The DBPF type id.
    pub const fn type_id(self) -> u32 {
        match self {
            Self::Creature => 0x2B97_8C46,
            Self::Cell => 0x3D97_A8E4,
            Self::Vehicle => 0x2468_2294,
            Self::Ufo => 0x476A_98C7,
            Self::Building => 0x2399_BE55,
            Self::Flora => 0x438F_6347,
        }
    }

    /// The group's low 16 bits, which is where these families live.
    pub const fn family_bits(self) -> u16 {
        match self {
            Self::Cell => 0x6100,
            Self::Creature => 0x6200,
            Self::Building => 0x6300,
            Self::Vehicle => 0x6400,
            Self::Ufo => 0x6500,
            Self::Flora => 0x6600,
        }
    }

    /// The family a record type id belongs to, if this build knows it.
    pub fn from_type_id(type_id: u32) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.type_id() == type_id)
    }

    /// The four-letter name the asset tooling uses.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Creature => "crt",
            Self::Cell => "cll",
            Self::Vehicle => "vcl",
            Self::Ufo => "ufo",
            Self::Building => "bld",
            Self::Flora => "flr",
        }
    }

    /// Whether the corpus writes `<paintlist>` in this family at all.
    ///
    /// Measured, not assumed: paint appears in 80 blocks across `bld` (8), `ufo`
    /// (15) and `vcl` (57), and in **none** of the 47 creature documents. That
    /// is a real difference between creator domains and the reason paint is an
    /// `Option` rather than a defaulted list.
    pub const fn carries_paint(self) -> bool {
        matches!(self, Self::Vehicle | Self::Ufo | Self::Building)
    }
}

/// Decodes one `<sporemodel>` record.
///
/// `family` is recorded alongside the document because several measurements
/// differ per family (paint presence, `<handles>`, `<limbtype>`), and a caller
/// that does not know the family cannot interpret those differences.
pub fn parse_sporemodel(family: Family, input: &str) -> Result<CreatorDocument> {
    let root = xml::parse(input)?;
    if root.name != "sporemodel" {
        return Err(CreatorError::NotACreatorDocument { found: root.name });
    }
    check_attributes(&root, 0)?;

    let mut format_version = None;
    let mut properties = DocumentProperties::default();
    let mut blocks: Option<Vec<Block>> = None;

    for child in &root.children {
        match child.name.as_str() {
            "formatversion" => format_version = Some(number(child, "formatversion", 0)?),
            "properties" => properties = self::properties(child)?,
            "blocks" => blocks = Some(self::blocks(child, family)?),
            other => {
                return Err(CreatorError::UnexpectedElement {
                    element: other.to_owned(),
                    block: 0,
                })
            }
        }
    }

    let blocks = blocks.ok_or(CreatorError::MissingBlocks)?;
    check_counts(&root, "blocks", 0, blocks.len())?;
    Ok(CreatorDocument {
        format_version,
        properties,
        blocks,
    })
}

// ---------------------------------------------------------------------------
// properties
// ---------------------------------------------------------------------------

fn properties(element: &Element) -> Result<DocumentProperties> {
    check_attributes(element, 0)?;
    let mut out = DocumentProperties::default();
    let mut effects = [None; 3];
    let mut seeds = [None; 3];
    let mut colors = [None; 3];

    for child in &element.children {
        let name = child.name.as_str();
        check_attributes(child, 0)?;
        match name {
            "modeltype" => out.model_type = Some(number(child, name, 0)?),
            "name" => out.name = Some(child.text.trim().to_owned()),
            // Present in 9 documents and empty in all 9. Kept as `Some("")`,
            // because "wrote nothing" and "wrote an empty string" are different
            // bytes and the difference is the corpus's, not ours.
            "description" => out.description = Some(child.text.trim().to_owned()),
            "requiredpack" => out.required_pack = Some(number(child, name, 0)?),
            "zcorpscore" => out.zcorp_score = Some(number(child, name, 0)?),
            "hasthumbnail" => out.has_thumbnail = Some(boolean(child, name, 0)?),
            "cacheid" => out.cache_id = Some(number(child, name, 0)?),
            _ if effect_index(name).is_some() => {
                let index = effect_index(name).expect("checked");
                effects[index] = Some(number(child, name, 0)?);
            }
            _ if seed_index(name).is_some() => {
                let index = seed_index(name).expect("checked");
                seeds[index] = Some(number(child, name, 0)?);
            }
            _ if color_index(name).is_some() => {
                let index = color_index(name).expect("checked");
                colors[index] = Some(colour(child, name, 0)?);
            }
            other => {
                return Err(CreatorError::UnexpectedElement {
                    element: other.to_owned(),
                    block: 0,
                })
            }
        }
    }

    if effects.iter().any(Option::is_some) {
        out.skin_effects = Some(effects);
    }
    if seeds.iter().any(Option::is_some) {
        out.skin_effect_seeds = Some(seeds);
    }
    if colors.iter().any(Option::is_some) {
        out.skin_colors = Some(colors);
    }
    Ok(out)
}

fn effect_index(name: &str) -> Option<usize> {
    name.strip_prefix("skineffect")
        .and_then(|rest| rest.parse::<usize>().ok())
        .filter(|index| (1..=3).contains(index))
        .map(|index| index - 1)
}

fn seed_index(name: &str) -> Option<usize> {
    name.strip_prefix("skineffectseed")
        .and_then(|rest| rest.parse::<usize>().ok())
        .filter(|index| (1..=3).contains(index))
        .map(|index| index - 1)
}

fn color_index(name: &str) -> Option<usize> {
    name.strip_prefix("skincolor")
        .and_then(|rest| rest.parse::<usize>().ok())
        .filter(|index| (1..=3).contains(index))
        .map(|index| index - 1)
}

// ---------------------------------------------------------------------------
// blocks
// ---------------------------------------------------------------------------

fn blocks(element: &Element, _family: Family) -> Result<Vec<Block>> {
    check_attributes(element, 0)?;
    let mut out = Vec::new();
    for (index, child) in element.children_named("blockref").enumerate() {
        out.push(block(child, index)?);
    }
    // Any child that is not a `<blockref>` is a grammar change, not noise.
    for child in &element.children {
        if child.name != "blockref" {
            return Err(CreatorError::UnexpectedElement {
                element: child.name.clone(),
                block: 0,
            });
        }
    }
    Ok(out)
}

fn block(element: &Element, index: usize) -> Result<Block> {
    check_attributes(element, index)?;
    let mut out = Block::default();
    let mut limb_type = None;
    let mut muscle_scale = None;
    let mut base_muscle_scale = None;

    for child in &element.children {
        let name = child.name.as_str();
        check_attributes(child, index)?;
        match name {
            "blockid" => {
                out.block_id = Some(BlockId::parse(child.text.trim(), index)?);
            }
            "transform" => {
                let parsed = transform(child, index)?;
                out.transform = parsed.transform;
                out.bbox = parsed.bbox;
                out.user_orientation = parsed.user_orientation;
                limb_type = parsed.limb_type;
                muscle_scale = parsed.muscle_scale;
                base_muscle_scale = parsed.base_muscle_scale;
            }
            "snapped" => out.snapped = Some(boolean(child, name, index)?),
            // The distinction this crate exists to preserve: a `<childlist>` that
            // was not written is `None`, and an empty one that *was* written is
            // `Some([])`. 160 of 680 corpus blocks take the first branch.
            "childlist" => out.children = Some(child_list(child, index)?),
            "symmetric" => out.symmetric = Some(index_value(child, name, index)?),
            "paintlist" => out.paint = Some(paint_list(child, index)?),
            "handles" => out.handles = Some(handles(child, index)?),
            other => {
                return Err(CreatorError::UnexpectedElement {
                    element: other.to_owned(),
                    block: index,
                })
            }
        }
    }

    if limb_type.is_some() || muscle_scale.is_some() || base_muscle_scale.is_some() {
        out.limb = Some(Limb {
            limb_type,
            muscle_scale,
            base_muscle_scale,
        });
    }
    Ok(out)
}

fn child_list(element: &Element, index: usize) -> Result<Vec<usize>> {
    check_attributes(element, index)?;
    let mut out = Vec::new();
    for child in &element.children {
        if child.name != "childid" {
            return Err(CreatorError::UnexpectedElement {
                element: child.name.clone(),
                block: index,
            });
        }
        out.push(index_value(child, "childid", index)?);
    }
    check_counts(element, "childlist", index, out.len())?;
    Ok(out)
}

fn paint_list(element: &Element, index: usize) -> Result<Vec<PaintRegion>> {
    check_attributes(element, index)?;
    let mut out = Vec::new();
    for child in &element.children {
        if child.name != "paint" {
            return Err(CreatorError::UnexpectedElement {
                element: child.name.clone(),
                block: index,
            });
        }
        check_attributes(child, index)?;
        let mut region = None;
        let mut paint_id = None;
        let mut color1 = None;
        let mut color2 = None;
        for field in &child.children {
            let name = field.name.as_str();
            check_attributes(field, index)?;
            match name {
                "paintregion" => region = Some(number(field, name, index)?),
                "paintid" => paint_id = Some(number(field, name, index)?),
                "color1" => color1 = Some(colour(field, name, index)?),
                "color2" => color2 = Some(colour(field, name, index)?),
                other => {
                    return Err(CreatorError::UnexpectedElement {
                        element: other.to_owned(),
                        block: index,
                    })
                }
            }
        }
        // Both colours are required *within* a `<paint>`; the corpus writes both
        // in all 140 of them. A half-written paint entry is a grammar change,
        // not something to paper over with a default colour.
        let (Some(region), Some(paint_id), Some(color1), Some(color2)) =
            (region, paint_id, color1, color2)
        else {
            return Err(CreatorError::UnexpectedElement {
                element: "paint/partial".to_owned(),
                block: index,
            });
        };
        out.push(PaintRegion {
            region,
            paint_id,
            color1,
            color2,
        });
    }
    check_counts(element, "paintlist", index, out.len())?;
    Ok(out)
}

fn handles(element: &Element, index: usize) -> Result<Vec<ChannelWeight>> {
    check_attributes(element, index)?;
    let mut out = Vec::new();
    for child in &element.children {
        if child.name != "weight" {
            return Err(CreatorError::UnexpectedElement {
                element: child.name.clone(),
                block: index,
            });
        }
        let channel =
            child
                .attribute("channel")
                .ok_or_else(|| CreatorError::UnexpectedAttribute {
                    element: "weight".to_owned(),
                    attribute: "channel".to_owned(),
                    block: index,
                })?;
        // The channel is an opaque word. 14 distinct values corpus-wide, and no
        // registry names them — so it is parsed as a number and nothing more is
        // claimed about it.
        let channel = parse_id(channel).map_err(|_| CreatorError::NotANumber {
            element: "weight@channel".to_owned(),
            block: index,
            value: channel.to_owned(),
            reason: "not a 32-bit id".to_owned(),
        })?;
        out.push(ChannelWeight {
            channel,
            weight: scalar(child, "weight", index)?,
        });
    }
    check_counts(element, "handles", index, out.len())?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// transform
// ---------------------------------------------------------------------------

/// What one `<transform>` holds: the placement, plus the two elements that live
/// *inside* it in the corpus — `<blockbbox>` (the `formatversion 11` document)
/// and `<userorientation>` (vehicle documents).
struct ParsedTransform {
    transform: BlockTransform,
    bbox: Option<(Vec3, Vec3)>,
    user_orientation: Option<Matrix3>,
    limb_type: Option<u32>,
    muscle_scale: Option<f32>,
    base_muscle_scale: Option<f32>,
}

fn transform(element: &Element, index: usize) -> Result<ParsedTransform> {
    check_attributes(element, index)?;
    let mut out = BlockTransform {
        scale: 1.0,
        orientation: Matrix3::IDENTITY,
        ..BlockTransform::default()
    };
    let mut has_scale = false;
    let mut has_orientation = false;
    let mut bbox = None;
    let mut user_orientation = None;
    let mut limb_type = None;
    let mut muscle_scale = None;
    let mut base_muscle_scale = None;

    for child in &element.children {
        let name = child.name.as_str();
        check_attributes(child, index)?;
        match name {
            "position" => out.position = vec3(triple(child, name, index)?),
            "scale" => {
                out.scale = scalar(child, name, index)?;
                has_scale = true;
            }
            "orientation" => {
                out.orientation = orientation(child, index, "orientation")?;
                has_orientation = true;
            }
            "triangledirection" => {
                out.triangle_direction = Some(vec3(triple(child, name, index)?));
            }
            "trianglepickorigin" => {
                out.triangle_pick_origin = Some(vec3(triple(child, name, index)?));
            }
            "blockbbox" => bbox = Some(block_bbox(child, index)?),
            // These three live inside `<transform>`, not inside `<blockref>`.
            // Measured: parent→child over all 68 documents puts `limbtype` 5,
            // `musclescale` 5 and `basemusclescale` 8 under `<transform>`, and
            // zero under `<blockref>`.
            "limbtype" => limb_type = Some(number(child, name, index)?),
            "musclescale" => muscle_scale = Some(scalar(child, name, index)?),
            "basemusclescale" => base_muscle_scale = Some(scalar(child, name, index)?),
            // `<userorientation>` spells its rows differently from
            // `<orientation>`: `userorientation_row0..2`, 11 of them, vehicle
            // documents only. Same shape, different names — which is exactly why
            // the decoder matches element names rather than positions.
            "userorientation" => {
                user_orientation = Some(orientation(child, index, "userorientation")?);
            }
            other => {
                return Err(CreatorError::UnexpectedElement {
                    element: other.to_owned(),
                    block: index,
                })
            }
        }
    }

    // A `<transform>` with no `<scale>` or no `<orientation>` is a grammar
    // change. Defaulting either would make a missing element indistinguishable
    // from a written identity, which is the exact collapse this crate forbids.
    if !has_scale {
        return Err(CreatorError::UnexpectedElement {
            element: "transform/scale".to_owned(),
            block: index,
        });
    }
    if !has_orientation {
        return Err(CreatorError::UnexpectedElement {
            element: "transform/orientation".to_owned(),
            block: index,
        });
    }
    Ok(ParsedTransform {
        transform: out,
        bbox,
        user_orientation,
        limb_type,
        muscle_scale,
        base_muscle_scale,
    })
}

/// Reads a three-row matrix, spelling the row elements from `prefix`.
///
/// `<orientation>` holds `row0..2`; `<userorientation>` holds
/// `userorientation_row0..2`. Both are present in the corpus and both are
/// row-major, so one reader serves both and neither is transposed on the way.
fn orientation(element: &Element, index: usize, prefix: &str) -> Result<Matrix3> {
    check_attributes(element, index)?;
    Ok(Matrix3 {
        row0: row(element, prefix, 0, index)?,
        row1: row(element, prefix, 1, index)?,
        row2: row(element, prefix, 2, index)?,
    })
}

fn row(element: &Element, prefix: &str, which: usize, index: usize) -> Result<[f32; 3]> {
    let name = if prefix == "orientation" {
        format!("row{which}")
    } else {
        format!("{prefix}_row{which}")
    };
    let child = element
        .child(&name)
        .ok_or_else(|| CreatorError::UnexpectedElement {
            element: format!("{prefix}/{name}"),
            block: index,
        })?;
    triple(child, &name, index)
}

fn block_bbox(element: &Element, index: usize) -> Result<(Vec3, Vec3)> {
    check_attributes(element, index)?;
    let min_element =
        element
            .child("blockbbox_min")
            .ok_or_else(|| CreatorError::UnexpectedElement {
                element: "blockbbox/blockbbox_min".to_owned(),
                block: index,
            })?;
    let max_element =
        element
            .child("blockbbox_max")
            .ok_or_else(|| CreatorError::UnexpectedElement {
                element: "blockbbox/blockbbox_max".to_owned(),
                block: index,
            })?;
    Ok((
        vec3(triple(min_element, "blockbbox_min", index)?),
        vec3(triple(max_element, "blockbbox_max", index)?),
    ))
}

// ---------------------------------------------------------------------------
// scalars
// ---------------------------------------------------------------------------

/// The attributes each element in the grammar carries.
///
/// Exactly three elements have any: the three that declare a child count. Every
/// other element in all 68 documents carries none, so an attribute appearing on
/// one is a grammar change and is refused rather than ignored.
fn allowed_attributes(name: &str) -> &'static [&'static str] {
    match name {
        "blocks" | "childlist" | "paintlist" | "handles" => &["count"],
        _ => &[],
    }
}

fn only_known_attributes(
    element: &Element,
    name: &str,
    block: usize,
    allowed: &[&str],
) -> Result<()> {
    for (attribute, _) in &element.attributes {
        if !allowed.contains(&attribute.as_str()) {
            return Err(CreatorError::UnexpectedAttribute {
                element: name.to_owned(),
                attribute: attribute.clone(),
                block,
            });
        }
    }
    Ok(())
}

/// The one attribute check: what the grammar allows on this element.
fn check_attributes(element: &Element, block: usize) -> Result<()> {
    only_known_attributes(
        element,
        &element.name,
        block,
        allowed_attributes(&element.name),
    )
}

fn check_counts(element: &Element, name: &str, block: usize, actual: usize) -> Result<()> {
    let Some(declared) = element.attribute("count") else {
        return Ok(());
    };
    let declared = declared
        .trim()
        .parse::<usize>()
        .map_err(|_| CreatorError::NotANumber {
            element: format!("{name}@count"),
            block,
            value: declared.to_owned(),
            reason: "not a count".to_owned(),
        })?;
    // The attribute is the corpus's own claim about its contents. Disagreeing
    // with it is a finding, so it is an error and not a repair.
    if declared != actual {
        return Err(CreatorError::CountMismatch {
            element: name.to_owned(),
            block,
            declared,
            actual,
        });
    }
    Ok(())
}

fn number(element: &Element, name: &str, block: usize) -> Result<u32> {
    let text = element.text.trim();
    parse_id(text).map_err(|_| CreatorError::NotANumber {
        element: name.to_owned(),
        block,
        value: text.to_owned(),
        reason: "not a 32-bit id in either radix".to_owned(),
    })
}

fn scalar(element: &Element, name: &str, block: usize) -> Result<f32> {
    let text = element.text.trim();
    // No `NaN`, no `inf`, no thousands separators: a float that does not
    // round-trip is a float we cannot save.
    if !is_float_literal(text) {
        return Err(CreatorError::NotANumber {
            element: name.to_owned(),
            block,
            value: text.to_owned(),
            reason: "not a finite decimal literal".to_owned(),
        });
    }
    text.parse::<f32>().map_err(|_| CreatorError::NotANumber {
        element: name.to_owned(),
        block,
        value: text.to_owned(),
        reason: "not an f32".to_owned(),
    })
}

/// Whether a float field is a finite decimal literal this crate will accept.
///
/// The corpus writes **both** plain decimals (`0.450979`) and scientific
/// notation (`7.26504e-9`, in a vehicle `row0`), so the exponent form is part of
/// the grammar and not a tolerance. What is refused: `NaN`, `inf`, a second
/// `.`, a second `e`, a bare `.`, and thousands separators — every one of which
/// is a value we could not save and reload unchanged.
fn is_float_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        index = 1;
    }
    let mut mantissa_digits = 0;
    let mut dots = 0;
    let mut exponents = 0;
    let mut seen_exponent = false;
    while index < bytes.len() {
        match bytes[index] {
            b'0'..=b'9' => {
                mantissa_digits += 1;
                index += 1;
            }
            b'.' => {
                dots += 1;
                index += 1;
            }
            b'e' | b'E' => {
                if seen_exponent {
                    return false;
                }
                seen_exponent = true;
                exponents += 1;
                index += 1;
                // A signed exponent is ordinary and appears in the corpus.
                if matches!(bytes.get(index), Some(b'+' | b'-')) {
                    index += 1;
                }
            }
            _ => return false,
        }
    }
    let total_digits: usize = mantissa_digits;
    total_digits > 0 && dots <= 1 && exponents <= 1
}

/// A `r,g,b` colour field.
///
/// The arity check is kept here so a wrong component count is still reported
/// against the block it belongs to; the *grammar* of a component is
/// [`crate::color`]'s, which is canonical for the whole workspace.
fn colour(element: &Element, name: &str, block: usize) -> Result<crate::color::UnitRgb> {
    let text = element.text.trim();
    let found = text.split(',').count();
    if found != 3 {
        return Err(CreatorError::WrongArity {
            element: name.to_owned(),
            block,
            expected: 3,
            got: found,
        });
    }
    crate::color::UnitRgb::parse(text, name).map_err(|error| CreatorError::NotANumber {
        element: name.to_owned(),
        block,
        value: text.to_owned(),
        reason: error.to_string(),
    })
}

/// A three-float tuple that is **not** a colour — a position, a direction, a
/// bbox corner, a `userorientation` row.
///
/// This keeps the general float grammar, exponent forms included, because the
/// corpus writes those in these fields (`7.26504e-9`) and does not write them in
/// a colour. Keeping the two apart is the point: one grammar per field class.
fn triple(element: &Element, name: &str, block: usize) -> Result<[f32; 3]> {
    let text = element.text.trim();
    let parts: Vec<&str> = text.split(',').map(str::trim).collect();
    if parts.len() != 3 {
        return Err(CreatorError::WrongArity {
            element: name.to_owned(),
            block,
            expected: 3,
            got: parts.len(),
        });
    }
    let mut out = [0.0; 3];
    for (slot, part) in out.iter_mut().zip(parts) {
        if !is_float_literal(part) {
            return Err(CreatorError::NotANumber {
                element: name.to_owned(),
                block,
                value: part.to_owned(),
                reason: "not a finite decimal literal".to_owned(),
            });
        }
        *slot = part.parse::<f32>().map_err(|_| CreatorError::NotANumber {
            element: name.to_owned(),
            block,
            value: part.to_owned(),
            reason: "not an f32".to_owned(),
        })?;
    }
    Ok(out)
}

fn index_value(element: &Element, name: &str, block: usize) -> Result<usize> {
    let text = element.text.trim();
    // `<childid>` and `<symmetric>` are decimal positions in the block list.
    // A `0x` prefix would be a different claim about the same field, so it is
    // accepted by `parse_id` but the value is still a list index.
    parse_id(text)
        .map(|value| value as usize)
        .map_err(|_| CreatorError::NotANumber {
            element: name.to_owned(),
            block,
            value: text.to_owned(),
            reason: "not a block index".to_owned(),
        })
}

fn boolean(element: &Element, name: &str, block: usize) -> Result<bool> {
    match element.text.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(CreatorError::NotANumber {
            element: name.to_owned(),
            block,
            value: other.to_owned(),
            reason: "not `true` or `false`".to_owned(),
        }),
    }
}

fn vec3(values: [f32; 3]) -> Vec3 {
    Vec3 {
        x: values[0],
        y: values[1],
        z: values[2],
    }
}
