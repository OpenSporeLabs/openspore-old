//! The authoring model: a creator document and the parts it assembles.
//!
//! These are plain Rust values. Nothing here is an ECS entity, a Bevy type, a
//! GPU handle, or a window — a document must be buildable, comparable,
//! serialisable and assertable with nothing else running.
//!
//! # Field-for-field against the corpus
//!
//! Every type here mirrors something measured in the 68 real creator documents
//! (`docs/CREATOR-CORE.md` §1). The rules that produced this shape:
//!
//! * **Block order is the index space.** The corpus addresses parts
//!   positionally: `<childid>8</childid>` means "the ninth `<blockref>` in this
//!   document". So the document holds a `Vec<Block>` and nothing else
//!   identifies a block. A map keyed by [`BlockId`] would be a second identity
//!   scheme, and the same instance id is legitimately used by many blocks.
//! * **`Option<T>` means "the corpus did not write this".** It never means
//!   "the default". 160 blocks carry no `<childlist>`; those are leaves, not
//!   blocks with an empty child list.
//! * **No single root.** 79 roots over 68 documents (one document has six), so
//!   roots are a *derived* [`CreatorDocument::roots`], never a stored field.
//! * **Symmetry is an index, not a transform.** `<symmetric>7</symmetric>` names
//!   the partner block. No negative scale, no flip flag: the corpus has none,
//!   and the binary has none either.

use core::cmp::Ordering;

use spore_core::{parse_id, EvidenceLevel, EvidenceState, Provenance};

use crate::color::UnitRgb;
use crate::error::{CreatorError, Result};

/// A position or direction: three `f32`s, in the order the corpus writes them.
#[derive(Debug, Clone, Copy, Default)]
pub struct Vec3 {
    /// The first component.
    pub x: f32,
    /// The second component.
    pub y: f32,
    /// The third component.
    pub z: f32,
}

impl Vec3 {
    /// The components in order.
    pub const fn to_array(self) -> [f32; 3] {
        [self.x, self.y, self.z]
    }
}

/// A row-major 3×3 matrix: the block's `orientation`.
///
/// Stored as three rows because that is the order `<row0>`, `<row1>`, `<row2>`
/// appear in, and converting to a transposed form at the boundary is how a
/// rotation silently becomes its inverse.
#[derive(Debug, Clone, Copy, Default)]
pub struct Matrix3 {
    /// The first row.
    pub row0: [f32; 3],
    /// The second row.
    pub row1: [f32; 3],
    /// The third row.
    pub row2: [f32; 3],
}

impl Matrix3 {
    /// The identity rotation, which is what an unwritten orientation would be.
    pub const IDENTITY: Self = Self {
        row0: [1.0, 0.0, 0.0],
        row1: [0.0, 1.0, 0.0],
        row2: [0.0, 0.0, 1.0],
    };

    /// The nine components in row-major order.
    pub const fn to_array(self) -> [f32; 9] {
        [
            self.row0[0],
            self.row0[1],
            self.row0[2],
            self.row1[0],
            self.row1[1],
            self.row1[2],
            self.row2[0],
            self.row2[1],
            self.row2[2],
        ]
    }
}

/// Which part a block places, as the `(group, instance)` pair `<blockid>` holds.
///
/// Deliberately **not** a [`spore_core::ResourceKey`]: the corpus omits the type
/// word, and filling it in from a table would be a claim. The resolved key
/// belongs to the resolver's answer, not to the block's identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct BlockId {
    /// The rigblock record's group id.
    pub group_id: u32,
    /// The rigblock record's instance id.
    pub instance_id: u32,
}

impl BlockId {
    /// Builds a block id from its two components.
    pub const fn new(group_id: u32, instance_id: u32) -> Self {
        Self {
            group_id,
            instance_id,
        }
    }

    /// Parses the corpus spelling `0x40626000, 0xd05c53a3`.
    ///
    /// Both components are parsed with [`spore_core::parse_id`], so a `0x`
    /// prefix is optional on each — the corpus is not consistent about it, which
    /// is the AGENTS.md address-spelling trap in a new place.
    pub fn parse(text: &str, block: usize) -> Result<Self> {
        let mut parts = text.split(',');
        let (Some(group), Some(instance), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(CreatorError::MalformedBlockId {
                block,
                value: text.to_owned(),
            });
        };
        let group = parse_id(group).map_err(|_| CreatorError::MalformedBlockId {
            block,
            value: text.to_owned(),
        })?;
        let instance = parse_id(instance).map_err(|_| CreatorError::MalformedBlockId {
            block,
            value: text.to_owned(),
        })?;
        Ok(Self::new(group, instance))
    }
}

/// A block's position in its own document. This *is* its identity.
pub type BlockIndex = usize;

/// One part placement: where it is, how big, how turned, and what hangs off it.
#[derive(Debug, Clone, Default)]
pub struct Block {
    /// Which part this block places.
    pub block_id: Option<BlockId>,
    /// The placement.
    pub transform: BlockTransform,
    /// `<snapped>` — present on all 680 corpus blocks, but optional here
    /// because a document that omitted it would mean something different.
    pub snapped: Option<bool>,
    /// `<childlist>` — the blocks that hang off this one.
    ///
    /// `None` = the element was not written (a leaf, 160 of 680).
    /// `Some(vec![])` = an empty list *was* written. Those are different bytes
    /// and different facts.
    pub children: Option<Vec<BlockIndex>>,
    /// `<symmetric>` — the index of this block's mirror partner.
    ///
    /// Reciprocal in 84 of 84 corpus relations, so it is stored on both sides
    /// and reciprocity is a *check*, not a rewrite.
    pub symmetric: Option<BlockIndex>,
    /// `<paintlist>` — absent in all 47 creature documents.
    pub paint: Option<Vec<PaintRegion>>,
    /// `<handles>` — named-channel weights, addressed by an opaque word.
    pub handles: Option<Vec<ChannelWeight>>,
    /// `<limbtype>`, `<musclescale>`, `<basemusclescale>` — creature and flora
    /// only. Opaque values; meaning UNKNOWN, bytes preserved.
    pub limb: Option<Limb>,
    /// `<userorientation>` — vehicle documents only.
    pub user_orientation: Option<Matrix3>,
    /// `<blockbbox>` — present only in the one `formatversion 11` document.
    pub bbox: Option<(Vec3, Vec3)>,
}

impl Block {
    /// Whether this block writes a `<childlist>` at all.
    pub fn writes_children(&self) -> bool {
        self.children.is_some()
    }

    /// The child indices, or an empty slice for a leaf.
    pub fn child_indices(&self) -> &[BlockIndex] {
        self.children.as_deref().unwrap_or(&[])
    }
}

/// The three `crt`/`flr`-only block fields, kept together because they only
/// ever appear together and their meaning is unknown.
#[derive(Debug, Clone, Copy, Default)]
pub struct Limb {
    /// `<limbtype>` — an opaque word.
    pub limb_type: Option<u32>,
    /// `<musclescale>`.
    pub muscle_scale: Option<f32>,
    /// `<basemusclescale>`.
    pub base_muscle_scale: Option<f32>,
}

/// A block's transform, exactly as the corpus writes it.
///
/// `scale` is a **scalar**: 680 blocks, range 0.28 … 2.254181, no vector scale
/// anywhere. Non-uniform scale is absent from the corpus and so absent here.
#[derive(Debug, Clone, Copy, Default)]
pub struct BlockTransform {
    /// `<position>`.
    pub position: Vec3,
    /// `<scale>`.
    pub scale: f32,
    /// `<orientation>`.
    pub orientation: Matrix3,
    /// `<triangledirection>` — a picking aid, present on all 680 blocks.
    ///
    /// It is **not** derived from `orientation`, and the two disagree on 102 of
    /// 680. Three separate facts, kept as three fields; deriving one from
    /// another would delete those 102 findings.
    pub triangle_direction: Option<Vec3>,
    /// `<trianglepickorigin>` — absent on the 10 blocks of the `formatversion 11`
    /// document.
    pub triangle_pick_origin: Option<Vec3>,
}

/// One `<paint>` entry: a region, a swatch word, and two colours.
///
/// Two colours per region is what the corpus writes. Whether that is a base and
/// a highlight, or two layers of a skin, is **UNKNOWN** — so the fields are
/// named by position and nothing more is claimed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaintRegion {
    /// `<paintregion>` — an opaque word; 23 distinct values corpus-wide.
    pub region: u32,
    /// `<paintid>` — an opaque word; 6 distinct values corpus-wide.
    pub paint_id: u32,
    /// `<color1>`. A [`UnitRgb`], so a component outside `0.0 ..= 1.0` is a
    /// decode failure rather than a colour this build will draw.
    pub color1: UnitRgb,
    /// `<color2>`.
    pub color2: UnitRgb,
}

/// One `<weight channel="…">` entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelWeight {
    /// The `channel` attribute: an opaque word, 14 distinct values corpus-wide.
    /// **Not** an enum and not nameable — no registry maps these.
    pub channel: u32,
    /// The weight.
    pub weight: f32,
}

/// A whole creator document.
#[derive(Debug, Clone, Default)]
pub struct CreatorDocument {
    /// `<formatversion>`.
    ///
    /// A label, **not** a schema gate: the corpus refutes gating on it, because
    /// `v16` documents both write and omit `<handles>`, and so does `v15`.
    pub format_version: Option<u32>,
    /// `<properties>`.
    pub properties: DocumentProperties,
    /// `<blocks>` in document order. **Order is the index space.**
    pub blocks: Vec<Block>,
}

impl CreatorDocument {
    /// How many blocks the document places.
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Whether the document places no blocks.
    ///
    /// Separate from "has blocks" on purpose: a document with none is not a
    /// creature with zero parts, it is a document that failed to load.
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// The indices no other block lists as a child — the roots of the forest.
    ///
    /// **Derived, never stored.** 79 roots over 68 documents: 62 documents have
    /// one, four have two, one has three, one has six. A single `root` field
    /// would be wrong for 9% of the corpus.
    pub fn roots(&self) -> Vec<BlockIndex> {
        let mut is_child = vec![false; self.blocks.len()];
        for block in &self.blocks {
            for &child in block.child_indices() {
                if let Some(slot) = is_child.get_mut(child) {
                    *slot = true;
                }
            }
        }
        is_child
            .iter()
            .enumerate()
            .filter_map(|(index, &child)| (!child).then_some(index))
            .collect()
    }

    /// The chain of parents above `index`, nearest first.
    ///
    /// Returns an empty vector for a root, and stops rather than looping if the
    /// document is malformed — validation reports the cycle, this does not
    /// hang on it.
    pub fn ancestors(&self, index: BlockIndex) -> Vec<BlockIndex> {
        let mut chain = Vec::new();
        if index >= self.blocks.len() {
            return chain;
        }
        let mut seen = vec![false; self.blocks.len()];
        let mut current = index;
        while let Some(parent) = self.parent_of(current) {
            if seen[parent] {
                break;
            }
            seen[parent] = true;
            chain.push(parent);
            current = parent;
        }
        chain
    }

    /// The parent of `index`, derived by scanning for who lists it.
    pub fn parent_of(&self, index: BlockIndex) -> Option<BlockIndex> {
        self.blocks
            .iter()
            .position(|block| block.child_indices().contains(&index))
    }
}

// ---------------------------------------------------------------------------
// Equality
//
// Float equality is bitwise, not `==`. The property the save format has to
// satisfy is `load(save(doc)) == doc`, and `0.0 == -0.0` plus `NaN != NaN` mean
// a derived `PartialEq` cannot express it: a round trip that flipped one sign
// bit, or produced a NaN, would test as equal and ship as a different document.
//
// ---------------------------------------------------------------------------

impl PartialEq for Vec3 {
    fn eq(&self, other: &Self) -> bool {
        bits(self.x) == bits(other.x)
            && bits(self.y) == bits(other.y)
            && bits(self.z) == bits(other.z)
    }
}

impl PartialEq for Matrix3 {
    fn eq(&self, other: &Self) -> bool {
        self.row0.map(bits) == other.row0.map(bits)
            && self.row1.map(bits) == other.row1.map(bits)
            && self.row2.map(bits) == other.row2.map(bits)
    }
}

impl PartialEq for BlockTransform {
    fn eq(&self, other: &Self) -> bool {
        self.position == other.position
            && bits(self.scale) == bits(other.scale)
            && self.orientation == other.orientation
            && self.triangle_direction == other.triangle_direction
            && self.triangle_pick_origin == other.triangle_pick_origin
    }
}

impl PartialEq for Limb {
    fn eq(&self, other: &Self) -> bool {
        self.limb_type == other.limb_type
            && self.muscle_scale.map(bits) == other.muscle_scale.map(bits)
            && self.base_muscle_scale.map(bits) == other.base_muscle_scale.map(bits)
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.block_id == other.block_id
            && self.transform == other.transform
            && self.snapped == other.snapped
            && self.children == other.children
            && self.symmetric == other.symmetric
            && self.paint == other.paint
            && self.handles == other.handles
            && self.limb == other.limb
            && self.user_orientation == other.user_orientation
            && self.bbox == other.bbox
    }
}

impl PartialEq for DocumentProperties {
    fn eq(&self, other: &Self) -> bool {
        self.model_type == other.model_type
            && self.name == other.name
            && self.description == other.description
            && self.skin_effects == other.skin_effects
            && self.skin_effect_seeds == other.skin_effect_seeds
            && self.skin_colors == other.skin_colors
            && self.required_pack == other.required_pack
            && self.zcorp_score == other.zcorp_score
            && self.has_thumbnail == other.has_thumbnail
            && self.cache_id == other.cache_id
    }
}

impl PartialEq for CreatorDocument {
    fn eq(&self, other: &Self) -> bool {
        self.format_version == other.format_version
            && self.properties == other.properties
            && self.blocks == other.blocks
    }
}

fn bits(value: f32) -> u32 {
    value.to_bits()
}

/// Document-level properties, every one of them optional.
///
/// No field is present in all 68 documents: `modeltype` 68, `requiredpack` 66,
/// the skin triples 61, `zcorpscore` 59, `name` 9, `hasthumbnail` 1. A struct
/// of required fields would be a struct that fails to load the corpus.
#[derive(Debug, Clone, Default)]
pub struct DocumentProperties {
    /// `<modeltype>`.
    ///
    /// Written **both ways in the same corpus** — `0x9ea3031a` in 67 documents
    /// and `2661483290` in the `formatversion 11` one. Parsed accordingly.
    pub model_type: Option<u32>,
    /// `<name>` — 9 of 68.
    pub name: Option<String>,
    /// `<description>` — 9 of 68, and empty in every one of them.
    pub description: Option<String>,
    /// `<skineffect1..3>` — 61 of 68. Opaque words.
    pub skin_effects: Option<[Option<u32>; 3]>,
    /// `<skineffectseed1..3>` — 61 of 68.
    pub skin_effect_seeds: Option<[Option<u32>; 3]>,
    /// `<skincolor1..3>` — 61 of 68. RGB triples in `0.0 ..= 1.0`, each held as
    /// a [`UnitRgb`]. The slots are independent `Option`s because a document may
    /// write two of the three; 7 of 68 do.
    pub skin_colors: Option<[Option<UnitRgb>; 3]>,
    /// `<requiredpack>` — 66 of 68.
    pub required_pack: Option<u32>,
    /// `<zcorpscore>` — 59 of 68.
    pub zcorp_score: Option<u32>,
    /// `<hasthumbnail>` — 1 of 68.
    pub has_thumbnail: Option<bool>,
    /// `<cacheid>` — 1 of 68.
    pub cache_id: Option<u32>,
}

/// Evidence grade for a fact this crate reads out of a real record.
///
/// `OBSERVED` and not `VERIFIED`: the bytes were read and are reported exactly
/// as written, but no differential against the original binary exists for the
/// creator document. Raising this needs the game's own reader, which is quoted
/// in `docs/analysis/creator/06-semantic-evidence.md` (`FUN_004bf9f0`) and
/// implemented in `06` only as a description.
pub const DOCUMENT_EVIDENCE: EvidenceLevel = EvidenceLevel::Observed;

/// The evidence state a decoded field carries.
pub const DOCUMENT_EVIDENCE_STATE: EvidenceState = EvidenceState::Derived;

/// Builds the [`Provenance`] row a decoded document field carries.
pub fn document_provenance(record_type: u32) -> Provenance {
    Provenance::derived(format!("sporemodel:{record_type:08x}"))
}

/// Orders two floats by bit pattern, for a total order over documents.
///
/// `f32` has no total order (`NaN` is unordered, `-0.0 == 0.0`), so any
/// comparison that needs one must define it; this is that definition, and it is
/// the same one `spore_core::ResourceKey` uses for its own determinism.
pub fn total_cmp(a: f32, b: f32) -> Ordering {
    bits(a).cmp(&bits(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(id: u32) -> Block {
        Block {
            block_id: Some(BlockId::new(0x4062_6000, id)),
            ..Block::default()
        }
    }

    #[test]
    fn a_block_id_parses_the_corpus_spelling_in_both_radices() {
        let hex = BlockId::parse("0x40626000, 0xd05c53a3", 0).unwrap();
        assert_eq!(hex, BlockId::new(0x4062_6000, 0xd05c_53a3));
        // The same pair written decimally is the same identity, not a second one.
        let dec = BlockId::parse("1080188928, 3495711651", 0).unwrap();
        assert_eq!(dec, hex);
    }

    #[test]
    fn a_block_id_with_the_wrong_shape_is_refused_not_guessed() {
        assert!(BlockId::parse("0x40626000", 3).is_err());
        assert!(BlockId::parse("0x40626000, 0x1, 0x2", 3).is_err());
        assert!(BlockId::parse("nonsense, 0x1", 3).is_err());
    }

    #[test]
    fn a_leaf_writes_no_children_and_an_empty_list_is_a_different_value() {
        let mut leaf = leaf(1);
        assert!(!leaf.writes_children());
        assert_eq!(leaf.child_indices(), &[] as &[BlockIndex]);

        leaf.children = Some(Vec::new());
        assert!(leaf.writes_children());
        assert_eq!(leaf.child_indices(), &[] as &[BlockIndex]);

        // Same children, different bytes, different fact.
        assert_ne!(leaf, Block::default());
    }

    #[test]
    fn roots_are_derived_and_a_six_root_document_does_not_pick_one() {
        let mut document = CreatorDocument {
            blocks: vec![leaf(1), leaf(2), leaf(3)],
            ..CreatorDocument::default()
        };
        assert_eq!(document.roots(), vec![0, 1, 2]);

        // 0 parents 1 and 2; 1 and 2 stay roots, and 3 becomes a child.
        document.blocks[0].children = Some(vec![1, 2]);
        assert_eq!(document.roots(), vec![0]);
    }

    #[test]
    fn a_root_field_would_have_been_wrong_and_there_is_none() {
        let mut document = CreatorDocument {
            blocks: vec![leaf(1), leaf(2)],
            ..CreatorDocument::default()
        };
        document.blocks[0].children = Some(vec![1]);
        assert_eq!(document.roots(), vec![0]);
        assert_eq!(document.parent_of(1), Some(0));
        assert_eq!(document.parent_of(0), None);
    }

    #[test]
    fn an_out_of_range_child_is_not_a_panic() {
        let mut block = leaf(1);
        block.children = Some(vec![99]);
        let document = CreatorDocument {
            blocks: vec![block],
            ..CreatorDocument::default()
        };
        assert_eq!(document.roots(), vec![0], "an unknown child marks nobody");
        assert_eq!(document.ancestors(99), Vec::new());
    }

    #[test]
    fn a_cycle_terminates_instead_of_looping_forever() {
        let mut a = leaf(1);
        let mut b = leaf(2);
        a.children = Some(vec![1]);
        b.children = Some(vec![0]);
        let document = CreatorDocument {
            blocks: vec![a, b],
            ..CreatorDocument::default()
        };
        // Both are named as children, so there is no root: reported, not hidden.
        assert!(document.roots().is_empty());
        assert!(document.ancestors(0).len() <= document.blocks.len());
    }

    #[test]
    fn float_equality_is_bitwise_so_a_sign_flip_is_a_different_document() {
        let a = Vec3 {
            x: 0.0,
            y: 1.0,
            z: -0.0,
        };
        let b = Vec3 {
            x: 0.0,
            y: 1.0,
            z: 0.0,
        };
        assert_ne!(a, b, "0.0 == -0.0 numerically, and that is not enough");

        let nan = Vec3 {
            x: f32::NAN,
            ..Vec3::default()
        };
        assert_eq!(nan, nan, "a NaN round trip is still equal to itself");
    }

    #[test]
    fn an_empty_document_says_it_places_nothing_rather_than_being_a_creature() {
        let document = CreatorDocument::default();
        assert!(document.is_empty());
        assert_eq!(document.len(), 0);
        assert!(document.roots().is_empty());
    }

    #[test]
    fn the_identity_matrix_is_what_an_unwritten_orientation_would_be() {
        let m = Matrix3::IDENTITY;
        assert_eq!(m.to_array()[0], 1.0);
        assert_eq!(m.to_array()[4], 1.0);
        assert_eq!(m.to_array()[8], 1.0);
        assert_eq!(m.to_array().iter().filter(|&&v| v == 1.0).count(), 3);
    }
}
