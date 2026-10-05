//! Real authoring operations on a [`CreatorDocument`].
//!
//! Six operations, each of which either does what it says or refuses with a
//! typed [`EditError`] and leaves the document **byte-identical** to how it
//! arrived. There is no repair path, no clamping and no "probably fine" mode:
//! an editor that silently fixes a bad request cannot answer the only question
//! an undo stack and a corpus test both need answered, which is what the tool
//! actually did.
//!
//! # The index-space policy: removal renumbers, and hands back the renumbering
//!
//! [`BlockIndex`] is a positional index into `document.blocks`, and that
//! position *is* the identity: the corpus writes `<childid>8</childid>` to mean
//! "the ninth `<blockref>`", and `docs/analysis/creator/05-serialization.md`
//! F14 records that **reordering is not a valid edit**. So an operation that
//! removes a block renumbers every block after it, and every `<childid>`,
//! `<symmetric>` and caller-held index in existence becomes potentially wrong.
//!
//! The choice made here is a **complete index remap**, over a tombstone:
//!
//! * `remove_block` compacts `blocks` (`len` drops by one, later indices shift
//!   down by one), rewrites every in-document reference, and returns an
//!   [`IndexRemap`] covering **every** index of the pre-removal document. The
//!   caller cannot be handed a document with a hole in it, and cannot be handed
//!   a remap with a gap in it.
//! * A tombstone policy — leaving a dead slot so nothing shifts — was **not**
//!   chosen, and the difference is pinned by a test rather than left to prose.
//!   It is rejected on three counts. It needs a second meaning for a slot that
//!   `Block` does not have, which would leak into `roots()`, `parent_of()` and
//!   [`mod@crate::validate`] — the query layer shared with the decoder. It produces
//!   a `<blockref>` with no `<blockid>`, a shape the corpus has never shown, so
//!   the writer that will eventually consume this would need a second answer.
//!   And it would move the cost, not remove it: every caller-held index would
//!   still need to know which slots are dead.
//!
//! So the invariant is one-directional and checkable: **removal shifts indices
//! strictly down by one after the removed slot, and returns the map that says
//! so.** Every index of the returned remap is populated; the removed index maps
//! to [`Remap::Removed`], which is a distinct answer and not an index, because
//! "this names nothing now" must never be spelled as a number.
//!
//! `add_block` needs no such map because it **appends**. A new block takes index
//! `blocks.len()`, so every existing index is untouched — the strongest form of
//! the preserve-ordering rule, and the reason inserting mid-document is not
//! offered. Insertion at a slot is an ordering change, which F14 rules out.
//!
//! # Absent is not empty, and no op may quietly convert between them
//!
//! `children: None` (the element was not written — 160 of 680 corpus blocks) and
//! `children: Some(vec![])` (an empty list *was* written — which the corpus never
//! does) are different bytes and different facts. Exactly one op is allowed to
//! create the list: `add_block`, which always creates it with at least one child
//! in it. Exactly one thing can empty it again — a removal or a reparent — and
//! because the corpus expresses "leaf" by *omitting* `<childlist>`, both ops take
//! an explicit [`OnLastChild`] saying which shape to leave behind. The choice is
//! in the signature rather than in a default, so a caller who empties a child
//! list has said which fact they meant.
//!
//! `set_paint` takes `Option` for the same reason: `None` omits `<paintlist>`,
//! `Some(vec![])` writes an empty one, and `validate`'s `empty_paint_list`
//! warning is the report of the difference, not this op's business.
//!
//! # What these ops deliberately do not do
//!
//! * **They do not touch transforms on a reparent.** Whether a block's
//!   `<position>` is local to its parent or absolute is still
//!   [`crate::claims`]' `transform_frame` question, and answering it here with a
//!   rewrite would be inventing the answer. A reparent moves the edge and leaves
//!   every stored value bit-for-bit, which is the only defensible choice while
//!   the frame is open. See the `reparent_transform_consequence` claim.
//! * **They do not repair.** Out-of-range references the document already had
//!   are reported by [`mod@crate::validate`]; an op neither fixes nor inherits them.
//! * **They do not re-derive `symmetric`.** Reciprocity is a check in
//!   `validate`, not a rewrite. The one place an op *must* touch `symmetric` is
//!   removal, where leaving a partner pointing at a block that no longer exists
//!   would be a dangling reference — and even there it clears the reference and
//!   **reports which blocks it cleared** ([`IndexRemap::severed_symmetry`])
//!   instead of losing the fact.
//!
//! # Grade
//!
//! The *operations* are `INFERRED` — they are what the document grammar plus the
//! forest measurements make correct, derived by reasoning rather than observed
//! in a game session. The *data* they move is [`crate::document::DOCUMENT_EVIDENCE`]
//! (`OBSERVED`): the bytes came out of real records.

use crate::document::{Block, BlockId, BlockIndex, BlockTransform, CreatorDocument, PaintRegion};
use crate::error::{EditError, EditResult};

/// What an op does to a `<childlist>` that has just lost its last child.
///
/// The two options are not stylistic. `None` is how all 160 corpus leaves are
/// written; `Some(vec![])` is how no corpus block is written, and `validate`
/// reports it as `empty_child_list`. Only the caller knows which fact is meant,
/// so the op asks rather than picks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnLastChild {
    /// Leave the written list in place, now empty (`Some(vec![])`).
    ///
    /// `validate` then reports one `empty_child_list` warning per affected
    /// block, which is the honest report of a shape the corpus does not use.
    KeepWrittenList,
    /// Drop the element (`None`), which is the shape the corpus writes for a
    /// leaf.
    ///
    /// This is a claim about the *new* document, not about an old record: after
    /// an edit there is no previous version whose omission this would misstate.
    OmitElement,
}

/// Where one pre-removal block index went.
///
/// Two answers, not a number and a sentinel: "it is here now" and "it names
/// nothing any more" must not be spelled the same way, or a caller that forgets
/// to check will edit whichever block happens to land on the dead index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Remap {
    /// The block survived and now sits at this index.
    Moved(BlockIndex),
    /// This was the removed block. There is no index that names it.
    Removed,
}

/// The complete index-space rewrite performed by a removal.
///
/// Every index of the **pre-removal** document appears exactly once, in
/// `after`, so the remap cannot have a gap. `detached` and `severed_symmetry`
/// are in that same pre-removal space; run them through [`Self::map`] to reach
/// the surviving document.
///
/// This type exists because `BlockIndex` has no meaning outside a document
/// state. Any index a caller was holding across a removal is stale the moment
/// the removal returns, and this is the receipt that says by how much.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRemap {
    removed: BlockIndex,
    after: Vec<Option<BlockIndex>>,
    detached: Vec<BlockIndex>,
    severed_symmetry: Vec<BlockIndex>,
}

impl IndexRemap {
    /// The index of the block that was removed, in the pre-removal space.
    pub fn removed(&self) -> BlockIndex {
        self.removed
    }

    /// How many indices this remap covers: the pre-removal block count.
    ///
    /// Exactly one more than the document's new block count, which is what
    /// makes a drop of exactly one checkable by the caller.
    pub fn len(&self) -> usize {
        self.after.len()
    }

    /// Whether the remap covers no indices — which is the state of a document
    /// that had no blocks, and so could not have had one removed. Never a real
    /// outcome of [`CreatorDocument::remove_block`].
    pub fn is_empty(&self) -> bool {
        self.after.is_empty()
    }

    /// The removed block's former children, in the pre-removal space.
    ///
    /// They are now **roots**, not orphans pointing at nothing: the edge to the
    /// removed block went away with it, and every other edge is untouched. A
    /// caller that wants the subtree kept intact should reparent these; a caller
    /// that wants it deleted should remove them too.
    pub fn detached(&self) -> &[BlockIndex] {
        &self.detached
    }

    /// Blocks whose `<symmetric>` named the removed block, in the pre-removal
    /// space.
    ///
    /// Their mirror reference was cleared, because leaving it would leave an
    /// index naming a block that does not exist. This list is how that is
    /// reported rather than lost.
    pub fn severed_symmetry(&self) -> &[BlockIndex] {
        &self.severed_symmetry
    }

    /// Where one pre-removal index went.
    ///
    /// An index past the end of the pre-removal document maps to itself: it was
    /// never a block, so nothing about it changed. Only the removed block maps to
    /// [`Remap::Removed`].
    pub fn map(&self, index: BlockIndex) -> Remap {
        match self.after.get(index) {
            Some(Some(moved)) => Remap::Moved(*moved),
            Some(None) => Remap::Removed,
            None => Remap::Moved(index),
        }
    }

    /// Rewrites a caller-held list of targets into the surviving index space.
    ///
    /// **All or nothing**: if any target was the removed block, nothing is
    /// written and [`EditError::RemovedTarget`] names the stale index. A
    /// half-rewritten selection is a selection pointing at the wrong part, and
    /// no error is louder than that.
    pub fn rewrite_targets(&self, targets: &mut [BlockIndex]) -> EditResult<()> {
        let mut mapped = Vec::with_capacity(targets.len());
        for &target in targets.iter() {
            mapped.push(match self.map(target) {
                Remap::Moved(index) => index,
                Remap::Removed => {
                    return Err(EditError::RemovedTarget { index: target });
                }
            });
        }
        targets.copy_from_slice(&mapped);
        Ok(())
    }
}

impl CreatorDocument {
    /// Appends `block` and returns the index it landed at.
    ///
    /// **Appends, always.** The new block takes index `blocks.len()`, so every
    /// existing index — and every index a caller is holding — stays exactly
    /// where it was. Inserting at a slot would renumber everything after it,
    /// which is an ordering change, and `docs/analysis/creator/05-serialization.md`
    /// F14 records that reordering is not a valid edit. There is therefore no
    /// `insert_at`, and its absence is the policy.
    ///
    /// `parent` is where the new block hangs. `None` makes it a root, which is
    /// legitimate: the corpus has 79 roots over 68 documents and one document
    /// has six.
    ///
    /// # What it refuses, and why each refusal is an index-space fact
    ///
    /// * no `<blockid>` — 680 of 680 corpus blocks write one, and a block that
    ///   names no part is not a thing this format has shown;
    /// * an incoming `<childlist>` — those `<childid>` numbers were chosen
    ///   before the caller could know this block's index, so they cannot mean
    ///   what they appear to mean. Add a subtree bottom-up instead, passing the
    ///   new index as the `parent` of the next call;
    /// * a `<symmetric>` partner at or past the new index — that partner is the
    ///   block itself or past the end of the document.
    ///
    /// Reciprocity is **not** checked here; `validate` reports a one-sided
    /// pairing as a warning, and a second opinion would be a second source of
    /// truth about the same record.
    ///
    /// One trap worth naming: [`Block::default`]'s orientation is
    /// [`crate::document::Matrix3::default`] — all zeros, **not** the identity.
    /// Pass [`crate::document::Matrix3::IDENTITY`] explicitly if identity is
    /// wanted; this op will not guess which of the two you meant.
    pub fn add_block(
        &mut self,
        block: Block,
        parent: Option<BlockIndex>,
    ) -> EditResult<BlockIndex> {
        if block.block_id.is_none() {
            return Err(EditError::MissingBlockIdentity);
        }
        if let Some(children) = &block.children {
            return Err(EditError::NewBlockCarriesChildren {
                children: children.len(),
            });
        }
        if let Some(parent) = parent {
            if parent >= self.blocks.len() {
                return Err(EditError::NoSuchBlock {
                    index: parent,
                    len: self.blocks.len(),
                });
            }
        }

        let index = self.blocks.len();
        if let Some(partner) = block.symmetric {
            if partner >= index {
                return Err(EditError::DanglingSymmetry {
                    partner,
                    len: index,
                });
            }
        }

        self.blocks.push(block);
        if let Some(parent) = parent {
            match &mut self.blocks[parent].children {
                Some(children) => children.push(index),
                // The one place a `<childlist>` comes into existence, and it is
                // always created holding its new child: `Some(vec![])` is never
                // produced here.
                None => self.blocks[parent].children = Some(vec![index]),
            }
        }
        Ok(index)
    }

    /// Removes the block at `index`, renumbers, and returns the full remap.
    ///
    /// Compacts `blocks` — the count drops by one and every later index shifts
    /// down by one — and rewrites, in this document, **every** index-valued
    /// reference:
    ///
    /// * every `<childid>` that named `index` is removed from its parent's list;
    /// * every `<childid>` pointing **above** the removed index is shifted down
    ///   by one, because it now names a different slot;
    /// * every `<symmetric>` that named `index` is cleared, because an index
    ///   naming a block that does not exist is a dangling reference;
    /// * every `<symmetric>` pointing above the removed index is likewise
    ///   shifted.
    ///
    /// The shift is the part that is easy to forget and impossible to spot: an
    /// edge that keeps its number after a compaction silently re-parents the
    /// document, which is exactly the failure
    /// `docs/analysis/creator/05-serialization.md` F14 warns about. So it is
    /// done here rather than left to the caller, and the corpus test asserts
    /// the edge set is isomorphic across a removal.
    ///
    /// Both **facts** that the removal destroys are reported in the
    /// [`IndexRemap`] rather than silently absorbed — `detached` for the removed
    /// block's children (now roots) and `severed_symmetry` for the mirror
    /// relations that were cleared — and both lists are in the pre-removal index
    /// space.
    ///
    /// `on_last_child` says what a parent that just lost its final child looks
    /// like; see [`OnLastChild`]. It applies to every affected parent, because a
    /// malformed document may list the same index twice.
    ///
    /// # What it does not do
    ///
    /// It does not delete the removed block's children, which become **roots**;
    /// it does not require the document to be valid beforehand (`validate`
    /// reports pre-existing problems, and refusing to edit a document with one
    /// would make the op unusable on real input); and it does not renumber
    /// anything a caller is holding — that is what the returned remap is for.
    ///
    /// # Ordering note
    ///
    /// Do not defer the rewrite of caller-held indices until "later". A
    /// `BlockIndex` is a position, not a handle, and the remap is the only
    /// record that the position moved.
    pub fn remove_block(
        &mut self,
        index: BlockIndex,
        on_last_child: OnLastChild,
    ) -> EditResult<IndexRemap> {
        let len = self.blocks.len();
        if index >= len {
            return Err(EditError::NoSuchBlock { index, len });
        }

        let detached: Vec<BlockIndex> = self.blocks[index]
            .child_indices()
            .iter()
            .copied()
            .filter(|&child| child != index && child < len)
            .collect();
        let parents: Vec<BlockIndex> = (0..len)
            .filter(|&parent| {
                parent != index && self.blocks[parent].child_indices().contains(&index)
            })
            .collect();
        let severed_symmetry: Vec<BlockIndex> = (0..len)
            .filter(|&other| other != index && self.blocks[other].symmetric == Some(index))
            .collect();

        for &parent in &parents {
            detach_child(self, parent, index, on_last_child);
        }
        for &partner in &severed_symmetry {
            self.blocks[partner].symmetric = None;
        }
        // Every surviving index reference above the hole shifts down by one.
        for block in &mut self.blocks {
            if let Some(children) = &mut block.children {
                for child in children.iter_mut() {
                    if *child > index {
                        *child -= 1;
                    }
                }
            }
            if let Some(partner) = &mut block.symmetric {
                if *partner > index {
                    *partner -= 1;
                }
            }
        }

        let mut after = Vec::with_capacity(len - 1);
        for old in 0..len {
            after.push(if old == index {
                None
            } else {
                Some(old - usize::from(old > index))
            });
        }
        self.blocks.remove(index);

        Ok(IndexRemap {
            removed: index,
            after,
            detached,
            severed_symmetry,
        })
    }

    /// Points the block at `index` at a different part, changing nothing else.
    ///
    /// **Every other field is preserved bit-for-bit**: `transform` (position,
    /// scale, orientation, both triangle fields), `children`, `symmetric`,
    /// `snapped`, `paint`, `handles`, `limb`, `user_orientation` and `bbox`.
    /// This is the operation an end-to-end milestone depends on — swap a head
    /// for another and the placement must not move by a float epsilon — so the
    /// implementation is a single field assignment and the test asserts the
    /// whole block, not the diff.
    ///
    /// Deliberately *not* implemented as "replace the whole `Block`": that
    /// would make the caller restate every field it did not mean to change, and
    /// a restated field is a field this crate would silently accept wrong.
    ///
    /// `<symmetric>` is left alone even when the old and new parts differ in
    /// handedness, because whether a mirror relation must survive a part swap is
    /// a question about the part and not about the document: nothing in the
    /// corpus answers it. `validate` will still report a one-sided relation.
    pub fn replace_block(&mut self, index: BlockIndex, block_id: BlockId) -> EditResult<()> {
        let len = self.blocks.len();
        if index >= len {
            return Err(EditError::NoSuchBlock { index, len });
        }
        self.blocks[index].block_id = Some(block_id);
        Ok(())
    }

    /// Moves the block at `index` so its single parent is `new_parent`.
    ///
    /// `None` detaches it to a root. `Some(same)` is already in that state and
    /// is a no-op, not an error.
    ///
    /// # The multi-parent question, answered
    ///
    /// Max in-degree is 1 across all 680 corpus blocks, so a block has at most
    /// one parent and "move" is unambiguous. If the block already has a parent
    /// this op **moves** it — the old edge is removed and the new one added, in
    /// one call, so in-degree never exceeds 1 even transiently and the caller
    /// never has to synthesise an intermediate root. The corpus's shape is
    /// preserved as an invariant rather than checked as a hope.
    ///
    /// # Refusals
    ///
    /// * a parent that is `index` itself, or a descendant of it — refused as
    ///   [`EditError::Cycle`], because 0 corpus cycles is what the forest claim
    ///   rests on. Ancestry is walked from the proposed parent upwards;
    /// * an ancestor chain that already cycles, so "is this safe" cannot be
    ///   established — refused as [`EditError::CyclicAncestry`] rather than
    ///   allowed on an unproven guess.
    ///
    /// # The thing it does not touch
    ///
    /// The moved block's `transform` is **not** rewritten, and neither is any
    /// descendant's. Whether `<position>` is parent-local or absolute is
    /// [`crate::claims`]'s open `transform_frame` question; a rescale here would
    /// be an answer invented in a function that has no evidence for one. What
    /// the op does is the one thing the document grammar decides: move the edge.
    ///
    /// `on_last_child` applies to the parent that just lost a child; see
    /// [`OnLastChild`].
    pub fn reparent_block(
        &mut self,
        index: BlockIndex,
        new_parent: Option<BlockIndex>,
        on_last_child: OnLastChild,
    ) -> EditResult<()> {
        let len = self.blocks.len();
        if index >= len {
            return Err(EditError::NoSuchBlock { index, len });
        }
        let Some(new_parent) = new_parent else {
            if let Some(old) = self.parent_of(index) {
                detach_child(self, old, index, on_last_child);
            }
            return Ok(());
        };
        if new_parent >= len {
            return Err(EditError::NoSuchBlock {
                index: new_parent,
                len,
            });
        }
        if self.is_descendant(new_parent, index)? {
            return Err(EditError::Cycle {
                block: index,
                would_be_parent: new_parent,
            });
        }
        if self.parent_of(index) == Some(new_parent) {
            return Ok(());
        }
        if let Some(old) = self.parent_of(index) {
            detach_child(self, old, index, on_last_child);
        }
        match &mut self.blocks[new_parent].children {
            Some(children) => children.push(index),
            None => self.blocks[new_parent].children = Some(vec![index]),
        }
        Ok(())
    }

    /// Replaces the block at `index`'s whole transform.
    ///
    /// The whole [`BlockTransform`], not a sub-field: the corpus writes
    /// `<position>`, `<scale>`, `<orientation>`, `<triangledirection>` and
    /// `<trianglepickorigin>` as one group, and `<triangledirection>` is
    /// explicitly **not** derivable from `<orientation>` — the two disagree on
    /// 102 of 680 blocks, so deriving one from the other would delete 102
    /// findings. A caller who wants only the position changed should read the
    /// transform, change that field, and pass it back.
    pub fn set_block_transform(
        &mut self,
        index: BlockIndex,
        transform: BlockTransform,
    ) -> EditResult<()> {
        let len = self.blocks.len();
        if index >= len {
            return Err(EditError::NoSuchBlock { index, len });
        }
        self.blocks[index].transform = transform;
        Ok(())
    }

    /// Sets the block at `index`'s `<paintlist>`.
    ///
    /// `None` omits the element; `Some(regions)` writes it, and `Some(vec![])`
    /// writes an **empty** list — which `validate` reports as
    /// `empty_paint_list`, and which is a different fact from omitting it.
    /// That distinction is preserved rather than smoothed over: the corpus has
    /// 195 `<paint>` entries in 80 blocks and no empty list anywhere.
    ///
    /// # What it cannot check, and why that is not a gap here
    ///
    /// `validate` warns `paint_in_a_family_without_paint` when a `crt` document
    /// carries paint — all 47 creature documents carry none, while `bld`, `ufo`
    /// and `vcl` do. This op **cannot** apply that rule, because
    /// [`CreatorDocument`] does not record its own [`crate::sporemodel::Family`];
    /// the record type is known at the package level, not in the XML. So the
    /// check stays in `validate`, which is told the family by the caller that has
    /// it, and a warning — not a refusal — is the right severity anyway: the
    /// paint is kept and reported, never dropped.
    ///
    /// Region and paint-id words are opaque ([`crate::claims`]
    /// `paintregion_meaning`, `paintid_meaning`), so this op validates neither
    /// the values nor their order. It stores what it is given.
    pub fn set_paint(
        &mut self,
        index: BlockIndex,
        paint: Option<Vec<PaintRegion>>,
    ) -> EditResult<()> {
        let len = self.blocks.len();
        if index >= len {
            return Err(EditError::NoSuchBlock { index, len });
        }
        self.blocks[index].paint = paint;
        Ok(())
    }

    /// Whether `candidate` sits inside `ancestor`'s subtree.
    ///
    /// Walks parent links upwards from `candidate`, which is the only direction
    /// a forest has. Returns `Err` when the chain already cycles: an unprovable
    /// answer must not be read as "no", because that is how a cycle gets added
    /// to a document that only had none.
    fn is_descendant(&self, candidate: BlockIndex, ancestor: BlockIndex) -> EditResult<bool> {
        let mut current = candidate;
        let mut seen = vec![false; self.blocks.len()];
        while current < self.blocks.len() {
            if current == ancestor {
                return Ok(true);
            }
            if seen[current] {
                return Err(EditError::CyclicAncestry { block: ancestor });
            }
            seen[current] = true;
            match self.parent_of(current) {
                Some(parent) => current = parent,
                None => return Ok(false),
            }
        }
        Ok(false)
    }
}

/// Removes `child` from `parent`'s written child list, applying the policy.
fn detach_child(
    document: &mut CreatorDocument,
    parent: BlockIndex,
    child: BlockIndex,
    on_last_child: OnLastChild,
) {
    let emptied = document.blocks[parent]
        .children
        .as_mut()
        .is_some_and(|children| {
            children.retain(|&index| index != child);
            children.is_empty()
        });
    if emptied && on_last_child == OnLastChild::OmitElement {
        document.blocks[parent].children = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::UnitRgb;
    use crate::document::{ChannelWeight, Limb, Matrix3};
    use crate::validate::{validate, Severity};

    fn leaf(instance: u32) -> Block {
        Block {
            block_id: Some(BlockId::new(0x4062_6000, instance)),
            ..Block::default()
        }
    }

    fn rgb(value: f32) -> UnitRgb {
        UnitRgb::new(value, value, value).expect("the test value is in 0.0 ..= 1.0")
    }

    /// A block with every corpus-writable field populated, so "nothing else
    /// changed" can be asserted on all of them at once.
    fn fully_populated() -> Block {
        Block {
            block_id: Some(BlockId::new(0x4062_6000, 0xd05c_53a3)),
            transform: BlockTransform {
                position: crate::document::Vec3 {
                    x: -0.0,
                    y: 1.5,
                    z: f32::MIN_POSITIVE,
                },
                scale: 2.254_181,
                orientation: Matrix3 {
                    row0: [0.5, 0.25, 0.125],
                    row1: [-0.0, 1.0, 2.0],
                    row2: [3.0, 4.0, 5.0],
                },
                triangle_direction: Some(crate::document::Vec3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                }),
                triangle_pick_origin: Some(crate::document::Vec3 {
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                }),
            },
            snapped: Some(true),
            children: Some(vec![1, 2]),
            symmetric: Some(3),
            paint: Some(vec![crate::document::PaintRegion {
                region: 5,
                paint_id: 0xf596_7f30,
                color1: rgb(0.0),
                color2: rgb(1.0),
            }]),
            handles: Some(vec![ChannelWeight {
                channel: 7,
                weight: 0.25,
            }]),
            limb: Some(Limb {
                limb_type: Some(3),
                muscle_scale: Some(1.5),
                base_muscle_scale: Some(-0.0),
            }),
            user_orientation: Some(Matrix3::IDENTITY),
            bbox: Some((
                crate::document::Vec3 {
                    x: -1.0,
                    y: -1.0,
                    z: -1.0,
                },
                crate::document::Vec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
            )),
        }
    }

    /// A parent with two children, and a three-block-deep chain for cycle tests:
    /// 0 -> 1 -> 2.
    fn forest() -> CreatorDocument {
        let mut root = leaf(1);
        root.children = Some(vec![1]);
        let mut middle = leaf(2);
        middle.children = Some(vec![2]);
        CreatorDocument {
            format_version: Some(16),
            blocks: vec![root, middle, leaf(3)],
            ..CreatorDocument::default()
        }
    }

    // -- add_block ---------------------------------------------------------

    #[test]
    fn adding_a_block_appends_and_leaves_every_existing_index_alone() {
        let before = forest();
        let mut document = before.clone();

        let index = document
            .add_block(leaf(9), Some(0))
            .expect("a leaf with an identity is added");
        assert_eq!(index, 3, "append only: the new block is last");

        // Blocks 1 and 2 are byte-identical: an append moved nothing.
        assert_eq!(document.blocks[1], before.blocks[1]);
        assert_eq!(document.blocks[2], before.blocks[2]);

        // Block 0 changed in exactly one field: the edge naming the new child.
        let mut expected_root = before.blocks[0].clone();
        expected_root.children = Some(vec![1, index]);
        assert_eq!(document.blocks[0], expected_root);

        assert_eq!(document.blocks[0].child_indices(), &[1, index]);
        assert_eq!(document.parent_of(1), Some(0));
        assert_eq!(document.parent_of(index), Some(0));
        assert_eq!(document.len(), 4);
    }

    #[test]
    fn adding_a_block_without_a_parent_makes_a_root_because_the_corpus_has_several() {
        let mut document = forest();
        let index = document.add_block(leaf(9), None).expect("added");
        assert_eq!(document.parent_of(index), None);
        assert_eq!(document.roots(), vec![0, index]);
    }

    #[test]
    fn adding_a_child_to_a_leaf_creates_the_child_list_with_a_child_in_it() {
        let mut document = CreatorDocument {
            blocks: vec![leaf(1)],
            ..CreatorDocument::default()
        };
        assert!(!document.blocks[0].writes_children());
        let index = document.add_block(leaf(2), Some(0)).expect("added");
        assert_eq!(
            document.blocks[0].children,
            Some(vec![index]),
            "the list is created holding its child; Some(vec![]) is never produced here"
        );
        assert!(validate(&document, None).is_empty());
    }

    #[test]
    fn a_block_with_no_identity_or_with_children_is_refused_and_the_document_is_untouched() {
        let before = forest();
        let mut document = before.clone();

        let anonymous = Block::default();
        assert_eq!(
            document.add_block(anonymous, None).unwrap_err(),
            EditError::MissingBlockIdentity
        );

        let mut with_children = leaf(9);
        with_children.children = Some(vec![0, 1]);
        assert_eq!(
            document.add_block(with_children, None).unwrap_err(),
            EditError::NewBlockCarriesChildren { children: 2 }
        );

        assert_eq!(document, before, "a refused add writes nothing");
    }

    #[test]
    fn adding_under_a_parent_that_does_not_exist_is_refused() {
        let mut document = forest();
        assert_eq!(
            document.add_block(leaf(9), Some(7)).unwrap_err(),
            EditError::NoSuchBlock { index: 7, len: 3 }
        );
        assert_eq!(document.len(), 3);
    }

    #[test]
    fn a_symmetric_partner_at_or_past_the_new_index_is_refused() {
        let mut document = forest();
        let mut self_partner = leaf(9);
        self_partner.symmetric = Some(3);
        assert_eq!(
            document.add_block(self_partner, None).unwrap_err(),
            EditError::DanglingSymmetry { partner: 3, len: 3 }
        );

        let mut past_end = leaf(9);
        past_end.symmetric = Some(99);
        assert_eq!(
            document.add_block(past_end, None).unwrap_err(),
            EditError::DanglingSymmetry {
                partner: 99,
                len: 3
            }
        );
        assert_eq!(document.len(), 3);
    }

    // -- remove_block ------------------------------------------------------

    #[test]
    fn the_index_policy_is_a_complete_remap_not_a_tombstone_and_that_is_pinned() {
        let mut document = forest();
        let remap = document
            .remove_block(0, OnLastChild::KeepWrittenList)
            .expect("removed");

        // The observable difference from a tombstone policy: the array COMPACTS.
        // A dead slot would leave `len() == 3` and block 1 still at index 1.
        assert_eq!(document.len(), 2, "a tombstone would leave the count at 3");
        assert_eq!(
            document.blocks[0].block_id,
            Some(BlockId::new(0x4062_6000, 2)),
            "block 1 shifted down into slot 0"
        );

        // Every pre-removal index appears exactly once: complete, no gaps.
        assert_eq!(remap.len(), 3);
        assert_eq!(
            (0..3).map(|index| remap.map(index)).collect::<Vec<_>>(),
            vec![Remap::Removed, Remap::Moved(0), Remap::Moved(1)]
        );
        assert_eq!(remap.removed(), 0);
        assert!(!remap.is_empty());

        // And the remap is *correct*, not merely present: for every surviving
        // index, old == new, and old == new + 1 after the hole.
        for old in 1..remap.len() {
            match remap.map(old) {
                Remap::Moved(new) => {
                    assert_eq!(new + 1, old, "everything after the hole shifts down one");
                }
                Remap::Removed => panic!("only the removed index maps to Removed"),
            }
        }
        assert!(validate(&document, None).is_empty());
    }

    #[test]
    fn removal_rewrites_every_in_document_reference_to_the_removed_block() {
        // 0 -> [1, 3], 1 -> [2], 3 -> [2]; 2 <-> 3 mirror each other.
        let mut document = forest();
        document.blocks[0].children = Some(vec![1, 3]);
        document.blocks[1].children = Some(vec![2]);
        document.blocks.push(leaf(4));
        document.blocks[2].symmetric = Some(3);
        document.blocks[3].symmetric = Some(2);
        assert!(validate(&document, None).is_empty());

        let remap = document
            .remove_block(2, OnLastChild::OmitElement)
            .expect("removed");

        // Both edges into block 2 are gone, and the edge that pointed *above*
        // the hole followed the block it named down one slot: block 3 moved to
        // index 2, so block 0's list reads [1, 2] and still names block 3.
        assert_eq!(document.blocks[0].children, Some(vec![1, 2]));
        assert_eq!(
            document.blocks[2].child_indices(),
            &[] as &[BlockIndex],
            "3 wrote a child list holding only the removed block"
        );
        assert_eq!(
            document.blocks[2].children, None,
            "with OmitElement, the emptied list is dropped"
        );
        assert_eq!(
            document.blocks[2].block_id,
            Some(BlockId::new(0x4062_6000, 4)),
            "index 2 now holds former block 3"
        );
        // The mirror relation is reported, not silently left dangling.
        assert_eq!(remap.severed_symmetry(), &[3]);
        assert_eq!(document.blocks[2].symmetric, None);

        for (index, block) in document.blocks.iter().enumerate() {
            for &child in block.child_indices() {
                assert!(child < document.len(), "edge {index} -> {child} dangles");
            }
        }
        let errors: Vec<_> = validate(&document, None)
            .into_iter()
            .filter(|issue| issue.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(remap.detached(), &[], "block 2 had no children");
    }

    #[test]
    fn a_removed_blocks_children_become_roots_and_are_reported_not_deleted() {
        let mut document = forest();
        let remap = document
            .remove_block(0, OnLastChild::KeepWrittenList)
            .expect("removed");
        assert_eq!(remap.detached(), &[1], "in the pre-removal index space");
        assert_eq!(remap.map(1), Remap::Moved(0));
        assert_eq!(document.parent_of(0), None, "the former child is a root");
        assert_eq!(document.parent_of(1), Some(0), "the subtree is intact");
        assert_eq!(document.len(), 2);
    }

    #[test]
    fn the_last_child_policy_is_the_callers_choice_and_both_shapes_are_reachable() {
        let mut kept = forest();
        kept.remove_block(1, OnLastChild::KeepWrittenList)
            .expect("removed");
        assert_eq!(
            kept.blocks[0].children,
            Some(vec![]),
            "an empty list was written, which the corpus never does"
        );
        assert!(validate(&kept, None)
            .iter()
            .any(|issue| issue.code == "empty_child_list"));

        let mut omitted = forest();
        omitted
            .remove_block(1, OnLastChild::OmitElement)
            .expect("removed");
        assert_eq!(
            omitted.blocks[0].children, None,
            "the element was dropped, which is how all 160 corpus leaves read"
        );
        assert!(validate(&omitted, None).is_empty());
    }

    #[test]
    fn a_rewritten_target_list_is_all_or_nothing() {
        let mut document = forest();
        let remap = document
            .remove_block(1, OnLastChild::OmitElement)
            .expect("removed");

        let mut targets = vec![0, 2];
        remap
            .rewrite_targets(&mut targets)
            .expect("neither target was removed");
        assert_eq!(targets, vec![0, 1]);

        let mut stale = vec![0, 1, 2];
        assert_eq!(
            remap.rewrite_targets(&mut stale).unwrap_err(),
            EditError::RemovedTarget { index: 1 }
        );
        assert_eq!(stale, vec![0, 1, 2], "nothing is written when one is stale");
    }

    #[test]
    fn removing_a_block_that_does_not_exist_is_refused() {
        let mut document = forest();
        assert_eq!(
            document
                .remove_block(9, OnLastChild::OmitElement)
                .unwrap_err(),
            EditError::NoSuchBlock { index: 9, len: 3 }
        );
        assert_eq!(document.len(), 3);
    }

    // -- replace_block -----------------------------------------------------

    #[test]
    fn replace_preserves_every_other_field_of_the_block_bit_for_bit() {
        let before = fully_populated();
        let mut block = before.clone();
        block.children = Some(vec![1]);
        let mut document = CreatorDocument {
            blocks: vec![block, leaf(2), leaf(3), leaf(4)],
            ..CreatorDocument::default()
        };

        let expected = document.blocks[0].clone();
        document
            .replace_block(0, BlockId::new(0x4061_6000, 0x1234_5678))
            .expect("replaced");

        let after = &document.blocks[0];
        assert_eq!(after.block_id, Some(BlockId::new(0x4061_6000, 0x1234_5678)));

        // Field by field, by bit pattern: `PartialEq` on these types is already
        // bitwise, and the float assertions below say so out loud for the
        // values where a sign bit is the whole point.
        assert_eq!(after.transform, expected.transform);
        assert_eq!(after.transform.position.x.to_bits(), (-0.0f32).to_bits());
        assert_eq!(
            after.transform.position.z.to_bits(),
            f32::MIN_POSITIVE.to_bits()
        );
        assert_eq!(
            after.transform.orientation.row1[0].to_bits(),
            (-0.0f32).to_bits(),
            "a negative zero orientation component survives the swap"
        );
        assert_eq!(
            after.transform.scale.to_bits(),
            expected.transform.scale.to_bits()
        );
        assert_eq!(
            after.transform.triangle_direction,
            expected.transform.triangle_direction
        );
        assert_eq!(
            after.transform.triangle_pick_origin,
            expected.transform.triangle_pick_origin
        );
        assert_eq!(after.snapped, Some(true));
        assert_eq!(after.children, Some(vec![1]));
        assert_eq!(after.symmetric, Some(3));
        assert_eq!(after.paint, expected.paint);
        assert_eq!(after.handles, expected.handles);
        assert_eq!(after.limb, expected.limb);
        assert_eq!(
            after
                .limb
                .map(|limb| limb.base_muscle_scale.map(f32::to_bits)),
            Some(Some((-0.0f32).to_bits()))
        );
        assert_eq!(after.user_orientation, expected.user_orientation);
        assert_eq!(after.bbox, expected.bbox);

        // And the whole document is unchanged apart from that one word: no
        // neighbour moved and no other field drifted.
        assert_eq!(document.blocks[1], leaf(2));
        assert_eq!(document.blocks[3], leaf(4));
        assert_eq!(document.len(), 4);
        assert_eq!(document.parent_of(1), Some(0));
    }

    #[test]
    fn replacing_a_block_that_does_not_exist_is_refused() {
        let mut document = forest();
        assert_eq!(
            document.replace_block(3, BlockId::new(1, 2)).unwrap_err(),
            EditError::NoSuchBlock { index: 3, len: 3 }
        );
    }

    // -- reparent_block ----------------------------------------------------

    #[test]
    fn a_reparent_that_would_create_a_cycle_is_refused_and_changes_nothing() {
        let before = forest();
        let mut document = before.clone();

        // 2 is a descendant of 1, and 0 is a descendant of 1 too.
        assert_eq!(
            document
                .reparent_block(1, Some(2), OnLastChild::OmitElement)
                .unwrap_err(),
            EditError::Cycle {
                block: 1,
                would_be_parent: 2
            }
        );
        assert_eq!(
            document
                .reparent_block(0, Some(2), OnLastChild::OmitElement)
                .unwrap_err(),
            EditError::Cycle {
                block: 0,
                would_be_parent: 2
            }
        );
        assert_eq!(
            document
                .reparent_block(1, Some(1), OnLastChild::OmitElement)
                .unwrap_err(),
            EditError::Cycle {
                block: 1,
                would_be_parent: 1
            },
            "a block cannot be its own parent"
        );

        assert_eq!(document, before, "a refused reparent writes nothing");
        assert!(validate(&document, None).is_empty());
    }

    #[test]
    fn a_reparent_moves_a_block_that_already_has_a_parent_and_never_exceeds_in_degree_one() {
        let mut document = forest();
        // Give 0 a second, independent child so the reparent has a destination
        // that is not an ancestor of the block being moved.
        document.blocks.push(leaf(4));
        document.blocks[0].children = Some(vec![1, 3]);

        document
            .reparent_block(1, Some(3), OnLastChild::KeepWrittenList)
            .expect("moved");

        assert_eq!(
            document.blocks[0].children,
            Some(vec![3]),
            "the old edge is gone"
        );
        assert_eq!(
            document.blocks[3].children,
            Some(vec![1]),
            "the new edge is there"
        );
        assert_eq!(document.parent_of(1), Some(3));
        assert_eq!(document.roots(), vec![0]);

        let mut in_degree = vec![0usize; document.len()];
        for block in &document.blocks {
            for &child in block.child_indices() {
                in_degree[child] += 1;
            }
        }
        assert!(
            in_degree.iter().all(|&degree| degree <= 1),
            "max in-degree is 1 in the corpus and stays 1 here: {in_degree:?}"
        );
        assert!(validate(&document, None).is_empty());
    }

    #[test]
    fn detaching_with_none_makes_a_root_and_moving_to_the_current_parent_is_a_no_op() {
        let mut document = forest();
        document
            .reparent_block(1, None, OnLastChild::OmitElement)
            .expect("detached");
        assert_eq!(document.parent_of(1), None);
        assert_eq!(document.blocks[0].children, None, "the element was dropped");
        assert_eq!(document.roots(), vec![0, 1]);

        // Already a child of 1: asking for it again changes nothing at all.
        let before = document.clone();
        document
            .reparent_block(2, Some(1), OnLastChild::OmitElement)
            .expect("unchanged");
        assert_eq!(document, before);
    }

    #[test]
    fn a_reparent_does_not_rewrite_a_single_transform_bit() {
        let mut placed = fully_populated();
        placed.children = None;
        let mut document = CreatorDocument {
            blocks: vec![placed, leaf(2)],
            ..CreatorDocument::default()
        };
        let before = document.blocks[1].transform;

        document
            .reparent_block(1, Some(0), OnLastChild::OmitElement)
            .expect("reparented");
        assert_eq!(document.blocks[1].transform, before);
        assert_eq!(
            document.blocks[1].transform.orientation, before.orientation,
            "the frame question is open, so nothing here composes a transform"
        );
    }

    #[test]
    fn a_reparent_under_an_already_cycling_document_is_refused_rather_than_guessed() {
        let mut a = leaf(1);
        let mut b = leaf(2);
        a.children = Some(vec![1]);
        b.children = Some(vec![0]);
        let mut document = CreatorDocument {
            blocks: vec![a, b, leaf(3), leaf(4)],
            ..CreatorDocument::default()
        };
        // Block 3 is a second child of block 0, and 0 <-> 1 still cycle.
        document.blocks[0].children = Some(vec![1, 3]);
        assert!(
            validate(&document, None)
                .iter()
                .any(|issue| issue.code == "cycle"),
            "the pre-existing cycle is validate's to report"
        );
        assert_eq!(
            document
                .reparent_block(3, Some(1), OnLastChild::OmitElement)
                .unwrap_err(),
            EditError::CyclicAncestry { block: 3 },
            "ancestry cannot be established, so the reparent is refused"
        );
        assert_eq!(document.parent_of(3), Some(0), "the edge is untouched");
    }

    #[test]
    fn reparenting_to_a_block_that_does_not_exist_is_refused() {
        let mut document = forest();
        assert_eq!(
            document
                .reparent_block(1, Some(9), OnLastChild::OmitElement)
                .unwrap_err(),
            EditError::NoSuchBlock { index: 9, len: 3 }
        );
        assert_eq!(
            document
                .reparent_block(9, Some(0), OnLastChild::OmitElement)
                .unwrap_err(),
            EditError::NoSuchBlock { index: 9, len: 3 }
        );
    }

    // -- set_block_transform / set_paint -----------------------------------

    #[test]
    fn a_transform_is_replaced_whole_and_triangle_direction_is_not_derived() {
        let mut document = forest();
        let replacement = BlockTransform {
            position: crate::document::Vec3 {
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            scale: 0.28,
            orientation: Matrix3::IDENTITY,
            triangle_direction: Some(crate::document::Vec3 {
                x: -1.0,
                y: -2.0,
                z: -3.0,
            }),
            triangle_pick_origin: None,
        };
        document.set_block_transform(1, replacement).expect("set");

        assert_eq!(document.blocks[1].transform, replacement);
        assert_eq!(
            document.blocks[1].transform.triangle_direction,
            Some(crate::document::Vec3 {
                x: -1.0,
                y: -2.0,
                z: -3.0
            }),
            "kept as written, not recomputed from the orientation"
        );
        assert_eq!(document.blocks[1].transform.triangle_pick_origin, None);
        assert_eq!(document.blocks[0].transform, BlockTransform::default());
    }

    #[test]
    fn paint_absent_and_paint_empty_are_different_values_and_the_op_keeps_them_apart() {
        let region = crate::document::PaintRegion {
            region: 5,
            paint_id: 0xf596_7f30,
            color1: rgb(0.25),
            color2: rgb(0.75),
        };

        let mut document = forest();
        document.set_paint(1, Some(vec![region])).expect("set");
        assert_eq!(document.blocks[1].paint, Some(vec![region]));
        document.set_paint(1, None).expect("cleared");
        assert_eq!(document.blocks[1].paint, None, "<paintlist> is omitted");
        document.set_paint(1, Some(Vec::new())).expect("emptied");
        assert_eq!(
            document.blocks[1].paint,
            Some(Vec::new()),
            "an empty list was written, which is a different fact"
        );
        assert!(validate(&document, None)
            .iter()
            .any(|issue| issue.code == "empty_paint_list"));
    }

    #[test]
    fn paint_on_a_block_that_does_not_exist_is_refused() {
        let mut document = forest();
        assert_eq!(
            document.set_paint(9, Some(Vec::new())).unwrap_err(),
            EditError::NoSuchBlock { index: 9, len: 3 }
        );
        assert_eq!(
            document
                .set_block_transform(9, BlockTransform::default())
                .unwrap_err(),
            EditError::NoSuchBlock { index: 9, len: 3 }
        );
    }

    // -- the combination an agent actually performs -------------------------

    #[test]
    fn an_add_replace_reparent_remove_sequence_keeps_the_document_a_forest() {
        let mut document = forest();

        // Build a two-block limb on the root, swap one of its parts, move it,
        // then delete it and remap a caller-held selection.
        let upper = document.add_block(leaf(0xaa), Some(0)).expect("added");
        let lower = document.add_block(leaf(0xbb), Some(upper)).expect("added");
        document
            .replace_block(lower, BlockId::new(0x4062_6000, 0xcccc_dddd))
            .expect("replaced");
        document
            .reparent_block(lower, Some(0), OnLastChild::OmitElement)
            .expect("reparented");
        assert!(validate(&document, None).is_empty());

        let mut selection = vec![upper];
        let remap = document
            .remove_block(lower, OnLastChild::OmitElement)
            .expect("removed");
        remap
            .rewrite_targets(&mut selection)
            .expect("the selection did not name the removed block");

        // `lower` was appended after `upper`, so removing it leaves `upper`
        // where it is: the remap proves it rather than assuming it.
        assert_eq!(selection, vec![upper]);
        assert_eq!(remap.map(upper), Remap::Moved(upper));
        assert_eq!(document.parent_of(selection[0]), Some(0));
        assert!(validate(&document, None).is_empty());
        assert_eq!(document.roots(), vec![0]);
    }
}
