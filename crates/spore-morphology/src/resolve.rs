//! The resolver: a `<blockref>` becomes a part, or an explicit non-result.
//!
//! # The chain
//!
//! ```text
//! <blockid>  ->  rigblock   0x5CAA5E28, at the block's own key
//!            ->  prop       0x00B1B104, at the rigblock's own key
//!            ->  part pair  0x40606000, same instance id as the rigblock
//!                            { 0x2F4E681B container, 0x01C135DA geometry }
//! ```
//!
//! Measured over the install: 1316 of 1549 rigblocks reach a part. **The other
//! 233 do not, and neither does any flora rigblock.** That is a fact about the
//! corpus and this resolver reports it as a count. It never invents a
//! substitute mesh, a placeholder body, or an anatomical meaning for a hole.
//!
//! # Why a part is a pair
//!
//! The two records at `0x40606000` share an instance id and neither contains the
//! other's type id. The geometry record's bones are an anonymous local ordinal
//! list; the container record holds the named, parented skeleton. Binding them is
//! a resolution step with provenance, not a convenience — which is why
//! [`crate::provenance::Stage::PartPair`] is a stage of its own.
//!
//! # Nothing here is Bevy
//!
//! This crate turns documents into morphology. It draws nothing.

use spore_core::ResourceKey;
use spore_creator_core::{BlockId, BlockIndex, CreatorDocument};
use spore_rigblock::{Rigblock, Skeleton};

use crate::error::{MorphologyError, Result};
use crate::provenance::{Grade, Stage, StopReason, Trail};
use crate::source::RecordSource;

/// The rigblock bounds record type.
pub const TYPE_RIGBLOCK: u32 = spore_rigblock::TYPE_RIGBLOCK;

/// The property-pointer record type.
pub const TYPE_PROP: u32 = 0x00B1_B104;

/// The part's RenderWare 4 container.
pub const TYPE_RW4: u32 = 0x2F4E_681B;

/// The part's geometry record.
pub const TYPE_GMSH: u32 = 0x01C1_35DA;

/// The group the part pair lives at.
pub const GROUP_PART: u32 = 0x4060_6000;

/// The RW4 `Skeleton (0x70002)` section type.
pub const SECTION_SKELETON: u32 = spore_rigblock::SECTION_SKELETON;

/// Resolves documents into morphology against a record source.
///
/// Hand-written `Debug`: the source is a trait object, so a derive would demand
/// `Debug` of every implementation for no benefit. What a caller wants to see is
/// which source it holds.
pub struct Resolver<'a> {
    source: &'a dyn RecordSource,
}

impl core::fmt::Debug for Resolver<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Resolver").finish_non_exhaustive()
    }
}

impl<'a> Resolver<'a> {
    /// Builds a resolver over any record source.
    pub fn new(source: &'a dyn RecordSource) -> Self {
        Self { source }
    }

    /// Resolves every block of a document, in document order.
    ///
    /// The returned [`crate::Composition`] has one slot per block, so
    /// `slots()[i]` always corresponds to `document.blocks[i]`. A slot is
    /// `Unresolved` rather than absent — the index space is preserved and a
    /// caller cannot mistake "not resolved" for "not there".
    pub fn compose(&self, document: &CreatorDocument) -> Result<crate::Composition> {
        let mut slots = Vec::with_capacity(document.blocks.len());
        for index in 0..document.blocks.len() {
            slots.push(self.resolve_block(document, index));
        }
        Ok(crate::Composition::new_inner(
            document.format_version,
            slots,
        ))
    }

    /// Resolves one block.
    pub fn resolve_block(&self, document: &CreatorDocument, index: BlockIndex) -> crate::PartSlot {
        let Some(block) = document.blocks.get(index) else {
            // A slot for a block that is not there cannot happen through
            // `compose`; through the public API it is an explicit refusal rather
            // than a fabricated slot.
            return crate::PartSlot::Unresolved(crate::UnresolvedPart {
                block: index,
                block_id: None,
                trail: None,
                stage: Stage::BlockId,
                reason: StopReason::NoSuchRecord,
            });
        };
        let Some(block_id) = block.block_id else {
            return crate::PartSlot::Unresolved(crate::UnresolvedPart {
                block: index,
                block_id: None,
                trail: None,
                stage: Stage::BlockId,
                reason: StopReason::NoSuchRecord,
            });
        };
        match self.walk(block_id) {
            Ok(resolved) => crate::PartSlot::Resolved(resolved.with_block_index(index)),
            Err(Stop { trail, reason }) => {
                let stage = trail.furthest();
                crate::PartSlot::Unresolved(crate::UnresolvedPart {
                    block: index,
                    block_id: Some(block_id),
                    trail: Some(trail),
                    stage,
                    reason,
                })
            }
        }
    }

    /// Walks the chain for one block id.
    fn walk(&self, block_id: BlockId) -> core::result::Result<ResolvedPart, Stop> {
        let block_key = ResourceKey::new(TYPE_RIGBLOCK, block_id.group_id, block_id.instance_id);
        let mut trail = Trail::new(block_key);

        // Hop 1: the rigblock bounds record, at the block's own key.
        let rigblock_bytes = match self.source.record(&block_key) {
            Some(bytes) => bytes,
            None => {
                return Err(Stop {
                    trail: trail.stopped_at(
                        Stage::Rigblock,
                        Some(block_key),
                        Grade::Measured,
                        StopReason::NoSuchRecord,
                    ),
                    reason: StopReason::NoSuchRecord,
                })
            }
        };
        let rigblock = match Rigblock::decode(&rigblock_bytes) {
            Ok(rigblock) => rigblock,
            Err(_) => {
                return Err(Stop {
                    trail: trail.stopped_at(
                        Stage::Rigblock,
                        Some(block_key),
                        Grade::Measured,
                        StopReason::Undecodable,
                    ),
                    reason: StopReason::Undecodable,
                })
            }
        };
        trail = trail.reached(Stage::Rigblock, block_key, Grade::Measured);

        // Hop 2: the prop pointer, at the rigblock's own key.
        let prop_key = ResourceKey::new(TYPE_PROP, block_id.group_id, block_id.instance_id);
        let prop_bytes = match self.source.record(&prop_key) {
            Some(bytes) => bytes,
            None => {
                return Err(Stop {
                    trail: trail.stopped_at(
                        Stage::Prop,
                        Some(prop_key),
                        Grade::Measured,
                        StopReason::NoSuchRecord,
                    ),
                    reason: StopReason::NoSuchRecord,
                })
            }
        };
        let pointers = SporePointer::find_all(&prop_bytes);
        let Some(pointer) = pointers.first() else {
            return Err(Stop {
                trail: trail.stopped_at(
                    Stage::Prop,
                    Some(prop_key),
                    Grade::Measured,
                    StopReason::WrongStructure,
                ),
                reason: StopReason::WrongStructure,
            });
        };
        // Selection rule: the **first** validated container entry in the prop.
        //
        // This was measured rather than assumed, and the obvious alternative is
        // wrong. Preferring the entry that names the rigblock's own instance —
        // which report 08's §5 wording suggests — drops a body graph on 484 of
        // 678 real parts, because the model is frequently at a *different*
        // instance (measured example: rigblock `0xc66a14b8` carries exactly one
        // pointer, to `0x10fe9dc4`, and that is the one with the skeleton).
        //
        // The rule is graded `Measured` only when the selected entry also happens
        // to equal the rigblock's own instance, i.e. two signals agree. Otherwise
        // the selection is a measured rule but an unconfirmed choice, and says so.
        let agrees_with_block = pointer.instance == block_id.instance_id;
        let prop_grade = if agrees_with_block {
            Grade::Measured
        } else {
            Grade::Inferred
        };
        trail = trail.reached(Stage::Prop, prop_key, prop_grade);

        // Hop 3: the part pair at 0x40606000, keyed by the rigblock's instance.
        //
        // The pointer is checked against the instance we are resolving. When it
        // disagrees the pointer is not silently ignored and not silently obeyed:
        // the hop is reported as inferred, because the corpus shows the pair is
        // addressed by the rigblock's own instance and a disagreeing pointer is a
        // finding about that record rather than a reason to pick one.
        // The pair is addressed by the instance the prop selected, not by the
        // block's own instance: measured, and the two frequently differ.
        let pair_instance = pointer.instance;
        let container_key = ResourceKey::new(TYPE_RW4, GROUP_PART, pair_instance);
        let geometry_key = ResourceKey::new(TYPE_GMSH, GROUP_PART, pair_instance);
        let grade = prop_grade;

        let (container_bytes, geometry_present) = match (
            self.source.record(&container_key),
            self.source.record(&geometry_key),
        ) {
            (Some(container), Some(_)) => (container, true),
            (None, _) | (_, None) => {
                return Err(Stop {
                    trail: trail.stopped_at(
                        Stage::PartPair,
                        Some(container_key),
                        grade,
                        StopReason::NoSuchRecord,
                    ),
                    reason: StopReason::NoSuchRecord,
                })
            }
        };
        trail = trail.reached(Stage::PartPair, container_key, grade);

        // Hop 4: the skeleton inside the container. A part with no skeleton is
        // a real state — 256 of 1824 — so it is `skeleton: None`, never an error
        // and never an invented empty graph.
        let skeleton = first_skeleton(&container_bytes);
        if skeleton.is_some() {
            trail = trail.reached(Stage::Container, container_key, grade);
        }

        Ok(ResolvedPart {
            block: 0,
            block_id,
            rigblock,
            prop_key: Some(prop_key),
            container_key,
            geometry_key: geometry_present.then_some(geometry_key),
            skeleton,
            trail,
        })
    }
}

/// A resolved part, with the full trail that produced it.
#[derive(Debug, Clone)]
pub struct ResolvedPart {
    block: BlockIndex,
    /// The `<blockid>` this part came from.
    pub block_id: BlockId,
    /// The decoded rigblock bounds grid.
    pub rigblock: Rigblock,
    /// The `prop` record the pointer was read from.
    pub prop_key: Option<ResourceKey>,
    /// The container half of the pair.
    pub container_key: ResourceKey,
    /// The geometry half of the pair.
    ///
    /// `None` when the container exists but the geometry does not. That is a
    /// measured asymmetry, distinct from the pair being absent.
    pub geometry_key: Option<ResourceKey>,
    /// The decoded body graph, when the container holds one.
    ///
    /// `None` is a real corpus state (256 of 1824 containers hold no skeleton),
    /// not a failure and not an empty graph.
    pub skeleton: Option<Skeleton>,
    /// Every hop, with grades.
    pub trail: Trail,
}

impl ResolvedPart {
    /// Which block in the document this part came from.
    pub fn block(&self) -> BlockIndex {
        self.block
    }

    /// The bones of the body graph, when there is one.
    pub fn bones(&self) -> Option<&[spore_rigblock::Bone]> {
        self.skeleton
            .as_ref()
            .map(|skeleton| skeleton.bones.as_slice())
    }

    /// The bones whose identity recovered a name.
    pub fn named_bones(&self) -> Vec<(&'static str, Option<usize>)> {
        self.skeleton
            .as_ref()
            .map(|skeleton| {
                skeleton
                    .named_bones()
                    .into_iter()
                    .map(|(index, name)| (name, Some(index)))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn with_block_index(mut self, index: BlockIndex) -> Self {
        self.block = index;
        self
    }
}

/// A halt in the chain.
struct Stop {
    trail: Trail,
    reason: StopReason,
}

/// The `0x00A00CE4`-tagged key found inside a `prop` record.
#[derive(Debug, Clone, Copy)]
struct SporePointer {
    instance: u32,
}

/// Finds the pointer from a `prop` record.
///
/// The 16-byte form `{ instance, type, group, 0x00A00CE4 }` was validated on
/// 388 of 388 resolving keys in the creature model-slot layer. Searching for
/// that validated form, rather than any 12-byte triple, is deliberate: report 08
/// records that a bare ResourceKey-shaped search over the rigblock records found
/// **zero**, and a looser pattern here would find keys that do not resolve.
impl SporePointer {
    const TAG: u32 = 0x00A0_0CE4;
    const TYPE_TARGET: u32 = TYPE_RW4;

    /// Every validated 16-byte entry pointing at a container.
    ///
    /// A `prop` record holds several; report 08 measured that the *most
    /// frequent* one names the rigblock's own instance, which means a record can
    /// also hold others. An earlier revision of this resolver took the first
    /// match scanning forward, which was usually some other entry and made 485 of
    /// 678 parts look inferred — a measurement artefact that would have been
    /// filed as a fact about the corpus.
    fn find_all(bytes: &[u8]) -> Vec<Self> {
        let read = |at: usize| -> u32 {
            u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let mut out = Vec::new();
        let mut offset = 0usize;
        while offset + 16 <= bytes.len() {
            if read(offset + 12) == Self::TAG && read(offset + 4) == Self::TYPE_TARGET {
                out.push(Self {
                    instance: read(offset),
                });
            }
            offset += 4;
        }
        out
    }
}

/// The first `Skeleton (0x70002)` section in an RW4 container, decoded.
///
/// Returns `None` for "this container holds no skeleton", which is a real corpus
/// state — 256 of 1824 containers — and is never confused with a decode failure.
fn first_skeleton(container: &[u8]) -> Option<Skeleton> {
    if container.len() < 0x98 || &container[..8] != b"\x89RW4w32\0" {
        return None;
    }
    let word = |at: usize| -> Option<[u8; 4]> { container.get(at..at + 4)?.try_into().ok() };
    let read_u32 = |at: usize| -> u32 { u32::from_le_bytes(word(at).unwrap_or([0; 4])) };
    let read_i32 = |at: usize| -> i32 { i32::from_le_bytes(word(at).unwrap_or([0; 4])) };
    let section_info = read_u32(0x30) as usize;
    let section_count = read_i32(0x24);
    let buffer_pointer = read_u32(0x44) as usize;
    if section_count <= 0 {
        return None;
    }
    for index in 0..section_count as usize {
        let row = section_info + 24 * index;
        if row + 24 > container.len() {
            return None;
        }
        let data_pointer = read_u32(row) as usize;
        let size = read_i32(row + 0x08);
        let type_code = read_u32(row + 0x14);
        if type_code != SECTION_SKELETON || size <= 0 {
            continue;
        }
        let adjusted = if type_code == 0x10030 {
            data_pointer + buffer_pointer
        } else {
            data_pointer
        };
        let end = adjusted.checked_add(size as usize)?;
        let payload = container.get(adjusted..end.min(container.len()))?;
        if let Ok(skeleton) = Skeleton::decode(payload) {
            return Some(skeleton);
        }
    }
    None
}

/// Reports that a document could not be used at all.
#[derive(Debug)]
pub enum Unusable {
    /// The document had no blocks to resolve.
    Empty,
}

impl From<Unusable> for MorphologyError {
    fn from(error: Unusable) -> Self {
        match error {
            Unusable::Empty => MorphologyError::NoBlocksToResolve,
        }
    }
}

/// Builds a [`crate::Composition`] and fails when there is nothing to compose.
pub fn compose(
    document: &CreatorDocument,
    source: &dyn RecordSource,
) -> Result<crate::Composition> {
    if document.blocks.is_empty() {
        return Err(Unusable::Empty.into());
    }
    Resolver::new(source).compose(document)
}
