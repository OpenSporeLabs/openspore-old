//! A composition: one slot per block, resolved or explicitly not.
//!
//! # The rule this type exists to enforce
//!
//! **A composed creature must never look complete when it is not.** Every block
//! of the document gets a slot; a slot is either [`PartSlot::Resolved`] or
//! [`PartSlot::Unresolved`]. There is no third state, no `skip`, no `default`, and
//! no way for a caller to iterate only the resolved parts without first seeing
//! the count of the ones that are missing.
//!
//! That is why [`Composition::counts`] and [`Composition::unresolved`] are part of
//! the API rather than something a caller assembles: a renderer that draws
//! partial compositions can do so, but it cannot do so without having been told.

use spore_creator_core::BlockIndex;

use crate::provenance::{Stage, StopReason, Trail};
use crate::resolve::ResolvedPart;

/// One block's slot.
///
/// The resolved half is boxed because a [`ResolvedPart`] carries a decoded mesh —
/// 110 037 vertices across a whole composition — and an unboxed variant would
/// make every slot as large as the heaviest part while the unresolved half needs
/// a few dozen bytes.
#[derive(Debug, Clone)]
pub enum PartSlot {
    /// The block resolved to a part.
    Resolved(Box<ResolvedPart>),
    /// The block did not resolve, and says where it stopped.
    Unresolved(UnresolvedPart),
}

impl PartSlot {
    /// Whether this slot resolved.
    pub fn is_resolved(&self) -> bool {
        matches!(self, Self::Resolved(_))
    }

    /// The resolved part, if any.
    pub fn resolved(&self) -> Option<&ResolvedPart> {
        match self {
            Self::Resolved(part) => Some(part.as_ref()),
            Self::Unresolved(_) => None,
        }
    }

    /// The unresolved record, if any.
    pub fn unresolved(&self) -> Option<&UnresolvedPart> {
        match self {
            Self::Resolved(_) => None,
            Self::Unresolved(unresolved) => Some(unresolved),
        }
    }
}

/// A block that did not resolve.
///
/// Deliberately not an `Err`. The chain failing for one part is a finding about
/// the corpus, and 233 of 1549 blocks are expected to fail. Returning an error
/// would make the normal case exceptional and would tempt a caller to treat the
/// whole composition as unusable instead of reporting it honestly.
#[derive(Debug, Clone)]
pub struct UnresolvedPart {
    /// Which block in the document.
    pub block: BlockIndex,
    /// The `<blockid>`, when the document carried one.
    pub block_id: Option<spore_creator_core::BlockId>,
    /// Every hop that was taken, including the one that failed.
    ///
    /// `None` only when the block had no readable id, in which case there was
    /// nothing to walk.
    pub trail: Option<Trail>,
    /// The furthest stage reached.
    pub stage: Stage,
    /// Why it stopped.
    pub reason: StopReason,
}

impl UnresolvedPart {
    /// A one-line account, for a report a human reads.
    pub fn summary(&self) -> String {
        let id = match self.block_id {
            Some(id) => format!("0x{:08x}:0x{:08x}", id.group_id, id.instance_id),
            None => "<no block id>".to_owned(),
        };
        format!(
            "block {} ({id}) unresolved at {} because {}",
            self.block, self.stage, self.reason
        )
    }
}

/// How many blocks resolved and how many did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    /// Blocks that resolved to a part.
    pub resolved: usize,
    /// Blocks that did not.
    pub unresolved: usize,
}

impl Counts {
    /// The total, which always equals the document's block count.
    pub fn total(self) -> usize {
        self.resolved + self.unresolved
    }

    /// Whether the composition is complete.
    ///
    /// A caller that needs "no silent gaps" checks this, not `resolved > 0`.
    pub fn is_complete(self) -> bool {
        self.unresolved == 0
    }
}

/// A whole document, resolved.
#[derive(Debug, Clone)]
pub struct Composition {
    /// The document's `formatversion`, carried through unexamined.
    ///
    /// It is a label, not a schema gate, and it is kept only so a report can
    /// quote it.
    pub document_format_version: Option<u32>,
    /// One slot per block, in document order.
    slots: Vec<PartSlot>,
}

impl Composition {
    pub(crate) fn new_inner(format_version: Option<u32>, slots: Vec<PartSlot>) -> Self {
        Self {
            document_format_version: format_version,
            slots,
        }
    }

    /// Every slot, in document order. `slots()[i]` is `document.blocks[i]`.
    pub fn slots(&self) -> &[PartSlot] {
        &self.slots
    }

    /// The counts, so completeness is one field read away.
    pub fn counts(&self) -> Counts {
        let resolved = self.slots.iter().filter(|slot| slot.is_resolved()).count();
        Counts {
            resolved,
            unresolved: self.slots.len() - resolved,
        }
    }

    /// Every unresolved slot.
    pub fn unresolved(&self) -> impl Iterator<Item = &UnresolvedPart> {
        self.slots.iter().filter_map(PartSlot::unresolved)
    }

    /// Every resolved slot.
    pub fn resolved(&self) -> impl Iterator<Item = &ResolvedPart> {
        self.slots.iter().filter_map(PartSlot::resolved)
    }

    /// The bones across every resolved part that has a body graph.
    ///
    /// This is the morphological whole, and it is only meaningful next to
    /// [`Composition::counts`]: 43 bones across 12 resolved parts is a very
    /// different claim from 43 bones across 12 parts out of 20.
    pub fn all_bones(&self) -> Vec<(&'static str, BlockIndex)> {
        let mut out = Vec::new();
        for slot in &self.slots {
            if let PartSlot::Resolved(part) = slot {
                for bone in part.bones().unwrap_or_default() {
                    if let Some(name) = bone.name() {
                        out.push((name, slot_index(slot)));
                    }
                }
            }
        }
        out
    }

    /// How many resolved parts carry a body graph at all.
    pub fn parts_with_skeleton(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.resolved().is_some_and(|part| part.skeleton.is_some()))
            .count()
    }
}

fn slot_index(slot: &PartSlot) -> BlockIndex {
    match slot {
        PartSlot::Resolved(part) => part.block(),
        PartSlot::Unresolved(unresolved) => unresolved.block,
    }
}
