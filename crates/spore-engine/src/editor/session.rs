//! # `CreatorSession` — the editor's one source of truth, made mutable.
//!
//! [`CreatorStage`](crate::creator::CreatorStage) calls its document
//! "unmutated", because a stage is a *result*: it answers "what does this
//! document project to", and a result that changed underneath the caller would
//! make every number in it a lie. That is right for a loader and wrong for an
//! editor, because an editor's whole job is changing the document.
//!
//! So the mutable document does not go *into* the stage. It sits beside it, in
//! [`CreatorSession`], and the stage is recomputed from it after every accepted
//! edit. One document, one projection, one history:
//!
//! ```text
//!   CreatorSession.document   <- the only mutable authoring state
//!          |
//!          |  apply_add / apply_replace / apply_transform / undo / redo
//!          v
//!   CreatorHistory            <- snapshot checkpoints
//!          |
//!          |  reproject(), always, after every accepted edit
//!          v
//!   CreatorSession.stage      <- derived, disposable, never edited in place
//! ```
//!
//! # Why re-resolving the whole document is the honest cost
//!
//! Every `apply_*` call re-runs the full
//! `blockref -> rigblock -> prop -> part pair` chain over **every** block, not
//! just the one that changed. That is wasteful when only one part moved, and it
//! is still the right call, for two reasons.
//!
//! 1. **It is the only way "the stage describes the document" is established
//!    rather than hoped for.** A partial re-resolve is a second code path with
//!    its own private idea of which caches are stale, and that is where "the
//!    catalogue says 1864 triangles and the screen shows 1896" comes from.
//! 2. **A mutation can invalidate more than the block it named.** `remove_block`
//!    renumbers every index above the hole, so every stored index — a selection,
//!    a handle to a child — is now a different block. Re-resolving the world
//!    means the numbers reported afterwards are the numbers of the document as
//!    it now is.
//!
//! # The `Matrix3::default()` trap, and what it actually does
//!
//! `add_block` takes a whole [`Block`], and [`Block::default`]'s
//! `transform.orientation` is [`Matrix3::default`] — **all zeros, not the
//! identity**. `spore_core::record` says so in as many words, and
//! [`spore_creator_core::edit`]'s doc comment on `add_block` warns about it and
//! declines to guess.
//!
//! It bites hardest here, because every added part goes through
//! `Block { block_id: Some(..), ..Default::default() }`.
//!
//! **Measured, not assumed.** The first draft of this file claimed the zero
//! matrix produces a `NaN` quaternion and takes the subtree's transform with it.
//! That was a plausible story about a degenerate matrix, and it was wrong.
//! [`crate::loader::orientation_to_quat`] sends a general orientation to
//! `Quat::from_mat3`, and for the zero matrix that returns
//! `Quat(0.5, 0.0, 0.0, 0.0)` — perfectly **finite**, and of **length 0.5**.
//!
//! So the real consequence is quieter and is now the documented one: a
//! non-unit quaternion. Bevy composes rotations assuming unit length, so the part
//! is drawn scaled toward a point rather than rotated into place, and a part
//! small enough to begin with ends up **visually absent** while the runtime state
//! reports it resolved, visible, and on screen.
//!
//! That is why this is worth a constructor discipline and a test rather than a
//! comment: a part that is "there" and cannot be seen is exactly the class of
//! defect that gets reported as a rendering bug and debugged in the renderer.
//! Every constructor here sets [`Matrix3::IDENTITY`] explicitly, and
//! [`added_blocks_carry_a_usable_orientation`] asserts the quaternion comes out
//! **unit**, with [`a_default_orientation_is_not_a_rotation`] pinning what the
//! failure actually looks like.
//!
//! # Why the editing *semantics* are not here
//!
//! Add, replace, remove, re-parent and undo are `spore-creator-core`'s, and they
//! are exercised there against the real corpus with no window and no GPU. What
//! cannot leave the engine is the one thing that needs a live package mapping:
//! re-resolving an *edited* document. This module is that seam and nothing more.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use spore_creator_core::document::{Block, BlockId, BlockIndex, BlockTransform, Matrix3, Vec3};
use spore_creator_core::edit::{IndexRemap, OnLastChild};
use spore_creator_core::history::{CreatorHistory, SelectionStep};
use spore_creator_core::{CreatorDocument, EditError, Remap};
use spore_morphology::RecordSource;

use crate::creator::{CreatorLoadError, CreatorStage, DocumentSource};
use crate::domain::world::ResourceWorld;

/// Where a session's records come from.
///
/// An enum rather than a trait object, and the reason is `Sync`.
/// [`StoreSource`](crate::creator::StoreSource) counts its lookups in a
/// [`Cell`](core::cell::Cell), which is `Send` but **not** `Sync`, so an
/// `Arc<StoreSource>` cannot cross into the Bevy world at all. Erasing the
/// source to `Arc<dyn RecordSource + Send + Sync>` is then refused by the
/// compiler rather than discovered at runtime as a data race.
///
/// Holding the [`ResourceWorld`] and building a fresh borrowing
/// [`StoreSource`](crate::creator::StoreSource) per re-projection sidesteps this
/// honestly: every projection gets its own counters, so `lookups` and `misses`
/// are per-projection facts rather than a running total that never resets.
enum SessionRecords {
    /// Live, memory-mapped packages.
    Packages(Arc<ResourceWorld>),
    /// Synthetic records, for tests and the corpus suite.
    Synthetic(Arc<dyn RecordSource + Send + Sync>),
}

impl core::fmt::Debug for SessionRecords {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Packages(world) => f
                .debug_struct("Packages")
                .field("packages", &world.package_names().len())
                .finish(),
            Self::Synthetic(_) => f
                .debug_struct("Synthetic")
                .field("note", &"erased: printed as its kind, not its contents")
                .finish(),
        }
    }
}

impl SessionRecords {
    /// Projects `document`, counting this projection's lookups.
    fn project(
        &self,
        document: CreatorDocument,
        source: &DocumentSource,
        packages: usize,
    ) -> Result<CreatorStage, CreatorLoadError> {
        match self {
            Self::Packages(world) => {
                let records = crate::creator::StoreSource::new(world.store());
                crate::creator::project_document(document, source, &records, packages)
            }
            Self::Synthetic(records) => {
                crate::creator::project_document(document, source, records.as_ref(), packages)
            }
        }
    }

    /// Reads the document itself, which for a `Record` source needs the packages.
    fn read(&self, source: &DocumentSource) -> Result<CreatorDocument, CreatorLoadError> {
        match self {
            Self::Packages(world) => crate::creator::read_document(source, world.store()),
            // A synthetic session has no packages. Reading a *record* is
            // therefore refused with the reason rather than answered with a
            // "no such record" that would be a statement about the session
            // rather than about the corpus. `is_searchable` exists for the same
            // reason in `spore_morphology`.
            Self::Synthetic(_) => match source {
                // A saved file needs a filesystem and nothing else, so it does
                // not need a store to stand in for.
                DocumentSource::SavedFile(path) => crate::creator::read_saved_document(path),
                DocumentSource::Record { key, .. } => Err(CreatorLoadError::Packages(format!(
                    "this session was opened on synthetic records, so it cannot read record {key}; \
                     open it against packages to read a record"
                ))),
            },
        }
    }
}

/// Why an edit did not happen.
///
/// Three families, deliberately not merged: an [`EditError`] means the document
/// refused the edit and did **not** change; a [`SessionError::Reproject`] means
/// the edit was applied and then the re-projection failed; and
/// [`SessionError::Io`] is neither. The first leaves the session as it was, the
/// second leaves the document ahead of the stage, and reporting them alike is
/// how an editor ends up lying about which creature is on screen.
#[derive(Debug)]
pub enum SessionError {
    /// The document refused the edit. Nothing changed.
    Edit(EditError),
    /// The edit was accepted but the document would not re-project.
    ///
    /// The document now holds the edit and the stage still describes the
    /// previous one. [`CreatorSession::revision`] therefore does **not** move,
    /// so the scene keeps showing the last state that resolved. The
    /// alternative — rendering a document whose parts cannot be resolved —
    /// produces a creature with holes and calls it correct.
    Reproject {
        /// The underlying failure.
        cause: CreatorLoadError,
    },
    /// A save failed.
    Io {
        /// Why.
        reason: String,
    },
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Edit(error) => write!(f, "{error}"),
            Self::Reproject { cause } => {
                write!(f, "the edit applied but will not re-project: {cause}")
            }
            Self::Io { reason } => write!(f, "save failed: {reason}"),
        }
    }
}

impl std::error::Error for SessionError {}

impl From<EditError> for SessionError {
    fn from(error: EditError) -> Self {
        Self::Edit(error)
    }
}

impl From<CreatorLoadError> for SessionError {
    fn from(cause: CreatorLoadError) -> Self {
        Self::Reproject { cause }
    }
}

/// A live, editable creator document.
///
/// See the module docs for why this sits beside
/// [`CreatorStage`](crate::creator::CreatorStage) rather than inside it.
#[derive(Debug)]
pub struct CreatorSession {
    /// The authoring source of truth. The only mutable state here.
    document: CreatorDocument,
    /// Snapshot checkpoints, owned by `spore-creator-core`.
    history: CreatorHistory,
    /// The selected block, or `None`.
    ///
    /// A `BlockIndex` is a **position**, so it is only meaningful against
    /// `document` as it stands right now. Every operation that renumbers
    /// carries it through the core's own index rule, or clears it.
    selection: Option<BlockIndex>,
    /// Derived from `document`. Recomputed after every accepted edit.
    stage: CreatorStage,
    /// Live packages, so an edit can be re-resolved.
    records: SessionRecords,
    /// Where the document came from, for reporting and for save.
    doc_source: DocumentSource,
    /// How many packages `records` spans.
    packages: usize,
    /// The world scale applied to the root entity.
    scale: f32,
    /// Bumped whenever `stage` changes.
    ///
    /// The spawn system compares this against the revision it last built from, so
    /// "the scene matches the document" is one integer comparison rather than a
    /// structural guess. It does **not** move on a refused edit, which is what
    /// lets a failed edit leave the scene alone.
    revision: u64,
}

impl CreatorSession {
    /// Opens a document from a record or a saved file, with real packages behind it.
    ///
    /// The package mapping is held for the session's lifetime, which is the
    /// deliberate price of being an editor: without it a mutation could not be
    /// re-resolved, and an editor that cannot resolve what it just wrote is a
    /// viewer with a save button.
    pub fn open(
        source: &DocumentSource,
        packages: &[PathBuf],
        scale: f32,
    ) -> Result<Self, SessionError> {
        if packages.is_empty() {
            return Err(SessionError::Reproject {
                cause: CreatorLoadError::NoPackages,
            });
        }
        let world = ResourceWorld::open(packages).map_err(|error| SessionError::Reproject {
            cause: CreatorLoadError::Packages(crate::loader::describe(&error)),
        })?;
        let records = SessionRecords::Packages(Arc::new(world));
        let packages = records.package_count();
        let document = records.read(source)?;
        Self::assemble(document, source, records, packages, scale)
    }

    /// Builds a session over any record source.
    ///
    /// The seam every test and the corpus suite uses: [`MemorySource`] and the
    /// package-backed source produce the same [`CreatorSession`], so a behaviour
    /// proven against synthetic records is the behaviour the editor has.
    pub fn from_parts(
        document: CreatorDocument,
        source: &DocumentSource,
        records: Arc<dyn RecordSource + Send + Sync>,
        packages: usize,
        scale: f32,
    ) -> Result<Self, SessionError> {
        Self::assemble(
            document,
            source,
            SessionRecords::Synthetic(records),
            packages,
            scale,
        )
    }

    fn assemble(
        document: CreatorDocument,
        source: &DocumentSource,
        records: SessionRecords,
        packages: usize,
        scale: f32,
    ) -> Result<Self, SessionError> {
        let stage = records
            .project(document.clone(), source, packages)
            .map_err(|cause| SessionError::Reproject { cause })?;
        Ok(Self {
            document,
            // A freshly opened document has NO history. See `load_from_file`:
            // undo that can reach the previous creature is not an undo.
            history: CreatorHistory::default(),
            selection: None,
            stage,
            records,
            doc_source: source.clone(),
            packages,
            scale,
            revision: 0,
        })
    }

    /// The document, read-only.
    ///
    /// Every change goes through an `apply_*` call, so history and the stage
    /// cannot fall out of step with it. A `&mut CreatorDocument` reaching a UI
    /// system would put those three back under anyone's control.
    pub fn document(&self) -> &CreatorDocument {
        &self.document
    }

    /// The current projection of the current document.
    pub fn stage(&self) -> &CreatorStage {
        &self.stage
    }

    /// The selected block, or `None`.
    pub fn selection(&self) -> Option<BlockIndex> {
        self.selection
    }

    /// Selects a block, or clears the selection.
    ///
    /// An index past the end is **refused**, not clamped and not ignored.
    /// Selecting the last block when the user meant nothing would highlight the
    /// wrong part and look like success.
    pub fn select(&mut self, index: Option<BlockIndex>) -> Result<(), SessionError> {
        if let Some(index) = index {
            let len = self.document.blocks.len();
            if index >= len {
                return Err(SessionError::Edit(EditError::NoSuchBlock { index, len }));
            }
        }
        self.selection = index;
        Ok(())
    }

    /// Whether an undo is available, for greying out a button.
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// Whether a redo is available, for greying out a button.
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// How many checkpoints are behind the current state.
    pub fn undo_depth(&self) -> usize {
        self.history.depth()
    }

    /// The revision the stage is currently at.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The world scale applied to the root entity.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// How many packages this session resolves against.
    pub fn package_count(&self) -> usize {
        self.packages
    }

    // ---- editing ---------------------------------------------------------

    /// Adds a block naming `part`, as a child of `parent` (or a root).
    ///
    /// The transform is the identity, set **explicitly** — see the module docs
    /// on [`Matrix3::default()`].
    ///
    /// The new block is **selected**, because an editor that adds a part and
    /// leaves the previous selection highlighted has added something the user
    /// cannot then manipulate.
    pub fn apply_add(
        &mut self,
        part: BlockId,
        parent: Option<BlockIndex>,
    ) -> Result<BlockIndex, SessionError> {
        self.history.record(&self.document);
        let block = Block {
            block_id: Some(part),
            transform: BlockTransform {
                scale: 1.0,
                orientation: Matrix3::IDENTITY,
                ..BlockTransform::default()
            },
            ..Block::default()
        };
        let index = self.document.add_block(block, parent)?;
        self.finish_edit()?;
        self.selection = Some(index);
        Ok(index)
    }

    /// Removes a block, its children re-parented onto its own parent.
    ///
    /// # The children policy, stated because it is a decision and not a fact
    ///
    /// `remove_block` **detaches** the children rather than deleting them. This
    /// session keeps them: each is re-parented to the removed block's own
    /// parent, so a limb's lower segment survives its upper segment being
    /// swapped out. Deleting a forearm should not silently delete the hand.
    ///
    /// `remove_block` itself does not re-parent — it returns the detached indices
    /// in the [`IndexRemap`] — so this is *this module's* policy. The
    /// re-parenting happens **before** the removal, because afterwards every
    /// index above the hole means a different block and the policy would be
    /// applied to the wrong pair.
    ///
    /// # Why the last block cannot be removed
    ///
    /// `project_document` refuses a document with no blocks, so removing the
    /// last one would accept the edit and then fail to re-project, leaving the
    /// document and the stage disagreeing. An editor with no parts is not a
    /// state this projection layer can represent, so the edit is refused up
    /// front, with a reason.
    pub fn apply_remove(&mut self, index: BlockIndex) -> Result<IndexRemap, SessionError> {
        let len = self.document.blocks.len();
        if index >= len || len == 1 {
            return Err(SessionError::Edit(EditError::NoSuchBlock { index, len }));
        }
        let parent = self.document.parent_of(index);
        let children: Vec<BlockIndex> = self.document.blocks[index].child_indices().to_vec();
        for child in &children {
            self.document
                .reparent_block(*child, parent, OnLastChild::OmitElement)?;
        }
        // The checkpoint for a removal is the document the undo must RESTORE, so
        // it is the pre-removal one -- and it must be captured before the call,
        // because `remove_block` renumbers in place and hands back only the
        // receipt. `record_removal` asserts the pairing, which is how the first
        // draft of this method got it backwards and was caught immediately.
        let before_removal = self.document.clone();
        let remap = self
            .document
            .remove_block(index, OnLastChild::OmitElement)?;
        self.history.record_removal(&before_removal, &remap);
        self.finish_edit()?;
        // Carry the selection through the renumbering, using the SAME function
        // `IndexRemap::rewrite_targets` uses.
        //
        // The first draft derived the new selection from the *removed* index,
        // which is always `Remap::Removed`, so deleting any block cleared the
        // selection outright. That silently defeats the point of returning a
        // remap: the block the user was looking at was still there, under a
        // lower index, and they should still be looking at it.
        self.selection = self
            .selection
            .and_then(|selected| match remap.map(selected) {
                Remap::Moved(moved) => Some(moved),
                Remap::Removed => None,
            });
        Ok(remap)
    }

    /// Points the block at `index` at a different part, changing nothing else.
    pub fn apply_replace(&mut self, index: BlockIndex, part: BlockId) -> Result<(), SessionError> {
        self.history.record(&self.document);
        self.document.replace_block(index, part)?;
        self.finish_edit()
    }

    /// Replaces a block's whole transform.
    ///
    /// The whole [`BlockTransform`], not a field: the corpus writes position,
    /// scale and orientation as one group, and `set_block_transform` refuses to
    /// half-write it.
    pub fn apply_transform(
        &mut self,
        index: BlockIndex,
        transform: BlockTransform,
    ) -> Result<(), SessionError> {
        self.history.record(&self.document);
        self.document.set_block_transform(index, transform)?;
        self.finish_edit()
    }

    /// Moves a block under a new parent, or to the root for `None`.
    pub fn apply_reparent(
        &mut self,
        index: BlockIndex,
        parent: Option<BlockIndex>,
    ) -> Result<(), SessionError> {
        self.history.record(&self.document);
        self.document
            .reparent_block(index, parent, OnLastChild::OmitElement)?;
        self.finish_edit()
    }

    /// Sets a block's `<paintlist>`. `None` omits the element.
    ///
    /// The region and paint-id words stay **opaque**:
    /// [`spore_creator_core::claims`] records that what `paintregion` means is
    /// UNKNOWN, so this stores what it is given and claims nothing about it.
    pub fn apply_paint(
        &mut self,
        index: BlockIndex,
        paint: Option<Vec<spore_creator_core::document::PaintRegion>>,
    ) -> Result<(), SessionError> {
        self.history.record(&self.document);
        self.document.set_paint(index, paint)?;
        self.finish_edit()
    }

    /// Scales one block, leaving position and orientation alone.
    ///
    /// A named convenience because it is the operation the reference editor
    /// performs with a mouse wheel, and because a caller who wants to scale
    /// should not have to reconstruct the rest of the transform — which is also
    /// the only way to keep the fields the corpus says are independent
    /// (`triangledirection` disagrees with `orientation` on 102 of 680 blocks).
    pub fn apply_scale(&mut self, index: BlockIndex, scale: f32) -> Result<(), SessionError> {
        let mut transform = self.transform_of(index)?;
        transform.scale = scale;
        self.apply_transform(index, transform)
    }

    /// Moves one block, leaving scale and orientation alone.
    pub fn apply_position(
        &mut self,
        index: BlockIndex,
        position: Vec3,
    ) -> Result<(), SessionError> {
        let mut transform = self.transform_of(index)?;
        transform.position = position;
        self.apply_transform(index, transform)
    }

    /// Undoes one step, carrying the selection through the core's own index rule.
    ///
    /// Returns `false` when there was nothing to undo, which is a real answer
    /// the UI needs for a greyed-out button and not an error.
    pub fn undo(&mut self) -> Result<bool, SessionError> {
        let Some(step) = self.history.undo(&self.document, self.selection) else {
            return Ok(false);
        };
        let (document, selection) = step.into_parts();
        self.adopt(document, selection)
    }

    /// Redoes one step.
    pub fn redo(&mut self) -> Result<bool, SessionError> {
        let Some(step) = self.history.redo(&self.document, self.selection) else {
            return Ok(false);
        };
        let (document, selection) = step.into_parts();
        self.adopt(document, selection)
    }

    // ---- persistence -----------------------------------------------------

    /// Writes the document through `spore-creator-doc/1`.
    pub fn save(&self, path: &Path) -> Result<(), SessionError> {
        crate::creator::save_document(path, &self.document).map_err(|error| SessionError::Io {
            reason: error.to_string(),
        })
    }

    /// Reads a document from disk, adopts it, and **clears the history**.
    ///
    /// The clear is load-bearing. Keeping the previous document's checkpoints
    /// means undo walks the user out of the creature they just opened and into
    /// the one they were editing — which is not undo, it is a different
    /// document reached by a control the user reads as "the last edit".
    pub fn load_from_file(&mut self, path: &Path) -> Result<(), SessionError> {
        let document = crate::creator::read_saved_document(path)
            .map_err(|cause| SessionError::Reproject { cause })?;
        self.doc_source = DocumentSource::SavedFile(path.to_path_buf());
        self.adopt(document, None)?;
        self.history.clear();
        Ok(())
    }

    // ---- internals -------------------------------------------------------

    /// A block's current transform, or the core's "no such block" error.
    fn transform_of(&self, index: BlockIndex) -> Result<BlockTransform, SessionError> {
        self.document
            .blocks
            .get(index)
            .map(|block| block.transform)
            .ok_or(EditError::NoSuchBlock {
                index,
                len: self.document.blocks.len(),
            })
            .map_err(SessionError::Edit)
    }

    /// Re-projects after an accepted edit and bumps the revision.
    ///
    /// On failure the document keeps the edit and the revision does **not** move,
    /// so the scene continues to show the last state that resolved. Reporting
    /// this as an error, rather than as a success with holes on screen, is the
    /// whole point of separating `Edit` from `Reproject`.
    fn finish_edit(&mut self) -> Result<(), SessionError> {
        self.reproject()?;
        self.revision += 1;
        Ok(())
    }

    /// Adopts a document that arrived from history or from disk.
    fn adopt(
        &mut self,
        document: CreatorDocument,
        selection: Option<SelectionStep>,
    ) -> Result<bool, SessionError> {
        self.document = document;
        self.selection = match selection {
            Some(SelectionStep::Kept(index)) => Some(index),
            Some(SelectionStep::Removed) | None => None,
        };
        self.finish_edit()?;
        Ok(true)
    }

    /// Recomputes the stage from the document.
    fn reproject(&mut self) -> Result<(), CreatorLoadError> {
        self.stage =
            self.records
                .project(self.document.clone(), &self.doc_source, self.packages)?;
        Ok(())
    }
}

impl SessionRecords {
    /// How many packages this source spans.
    fn package_count(&self) -> usize {
        match self {
            Self::Packages(world) => world.package_names().len(),
            // A synthetic source spans none, which is a fact about the session
            // and is reported as 0 rather than as the caller's `packages`.
            Self::Synthetic(_) => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_core::Orientation;
    use spore_creator_core::sporemodel::{parse_sporemodel, Family};
    use spore_morphology::MemorySource;

    /// The identity every fixture block names.
    const FIXTURE: &str = "0x40626000, 0x00013530";
    /// A second identity, so a replace is observably a change.
    const OTHER: &str = "0x40626000, 0x00013531";

    fn id(text: &str) -> BlockId {
        BlockId::parse(text, 0).expect("fixture identity parses")
    }

    /// A document of `blocks` siblings at one identity, in the corpus's grammar.
    fn session(blocks: usize) -> CreatorSession {
        let mut body = String::new();
        for _ in 0..blocks {
            body.push_str(&format!(
                "<blockref><blockid>{FIXTURE}</blockid><transform>\
                 <position>0,0,0</position><scale>1</scale><orientation>\
                 <row0>1,0,0</row0><row1>0,1,0</row1><row2>0,0,1</row2>\
                 </orientation></transform></blockref>"
            ));
        }
        let xml = format!(
            "<sporemodel><formatversion>16</formatversion>\
             <blocks count=\"{blocks}\">{body}</blocks></sporemodel>"
        );
        let document = parse_sporemodel(Family::Creature, &xml).expect("the fixture parses");
        CreatorSession::from_parts(
            document,
            &DocumentSource::Record {
                key: spore_core::ResourceKey::new(1, 2, 3),
                family: Family::Creature,
            },
            Arc::new(MemorySource::new()) as Arc<dyn RecordSource + Send + Sync>,
            0,
            1.0,
        )
        .expect("a session")
    }

    #[test]
    fn a_fresh_session_has_no_history_so_undo_cannot_reach_another_creature() {
        let mut session = session(2);
        assert!(
            !session.can_undo(),
            "a newly opened document has nothing behind it"
        );
        assert!(!session.can_redo());
        assert_eq!(session.undo_depth(), 0);
        assert!(
            !session.undo().expect("undo is answerable"),
            "an empty history answers \"nothing to undo\", which is what a greyed-out button needs"
        );
    }

    #[test]
    fn an_accepted_edit_moves_the_revision_and_a_refused_one_does_not() {
        let mut session = session(2);
        let before = session.revision();
        session.apply_replace(0, id(OTHER)).expect("accepted");
        assert_eq!(session.revision(), before + 1);

        let refused = session.apply_replace(99, id(OTHER));
        assert!(refused.is_err(), "an index past the end is refused");
        assert_eq!(
            session.revision(),
            before + 1,
            "a refused edit must not move the revision, or the scene respawns for nothing"
        );
    }

    #[test]
    fn added_blocks_carry_a_usable_orientation() {
        // The `Matrix3::default()` trap, as a test. A zero orientation matrix is
        // degenerate, so `Quat::from_mat3` does not yield a rotation, and the
        // added part would take its whole subtree's transform with it.
        let mut session = session(1);
        let index = session.apply_add(id(OTHER), None).expect("added");
        let orientation = session.document.blocks[index].transform.orientation;
        assert_eq!(
            orientation.to_array(),
            Matrix3::IDENTITY.to_array(),
            "a new block must be given the identity explicitly, because \
             Block::default() carries an all-zero matrix"
        );
        let quat = crate::loader::orientation_to_quat(Orientation::General(orientation));
        assert!(
            quat.is_finite(),
            "a degenerate orientation must not reach the quaternion: {quat:?}"
        );
        assert!(
            (quat.length() - 1.0).abs() < 1e-5,
            "and it must be a unit quaternion, got length {}",
            quat.length()
        );
    }

    #[test]
    fn a_default_orientation_is_not_a_rotation_so_the_trap_is_real() {
        // Without this, the test above could be asserting a distinction that does
        // not exist: if `Matrix3::default()` happened to be the identity,
        // `added_blocks_carry_a_usable_orientation` would prove nothing.
        //
        // The measured failure, not the assumed one: the zero matrix yields a
        // FINITE quaternion of length 0.5. Not NaN, not a panic -- a non-unit
        // quaternion, which Bevy composes as a scaled-toward-a-point rotation.
        // So the part is drawn collapsed and, if it was small to begin with, is
        // simply not visible while the runtime state says it is.
        let quat = crate::loader::orientation_to_quat(Orientation::General(Matrix3::default()));
        assert!(
            quat.is_finite(),
            "the zero matrix does NOT produce NaN; if this now fails the \
             behaviour changed and the module docs must be corrected"
        );
        assert!(
            (quat.length() - 1.0).abs() > 0.1,
            "but it does produce a non-unit quaternion: length {} is not a rotation",
            quat.length()
        );
    }

    #[test]
    fn an_added_block_is_selected_so_it_can_be_manipulated_next() {
        let mut session = session(2);
        session.select(Some(0)).expect("selected");
        let index = session.apply_add(id(OTHER), Some(0)).expect("added");
        assert_eq!(
            session.selection(),
            Some(index),
            "adding a part must leave it selected, or the user cannot then move it"
        );
    }

    #[test]
    fn removing_a_block_keeps_its_children_and_carries_the_selection() {
        let mut session = session(3);
        // 0 -> 1 -> 2, so removing 1 must orphan 2 onto 0 rather than delete it.
        session
            .document
            .reparent_block(1, Some(0), OnLastChild::OmitElement)
            .unwrap();
        session
            .document
            .reparent_block(2, Some(1), OnLastChild::OmitElement)
            .unwrap();
        session.select(Some(2)).expect("selected");

        session.apply_remove(1).expect("removed");

        assert_eq!(
            session.document.blocks.len(),
            2,
            "only the named block went"
        );
        assert_eq!(
            session.document.parent_of(1),
            Some(0),
            "the orphaned child moved up to the removed block's own parent"
        );
        assert_eq!(
            session.selection(),
            Some(1),
            "the selection below the hole followed the renumbering"
        );
    }

    #[test]
    fn removing_a_leaf_leaves_its_siblings_untouched() {
        let mut session = session(3);
        session
            .document
            .reparent_block(1, Some(0), OnLastChild::OmitElement)
            .unwrap();
        let before: Vec<Option<BlockId>> =
            session.document.blocks.iter().map(|b| b.block_id).collect();
        session.apply_remove(1).expect("removed a leaf");
        let after: Vec<Option<BlockId>> =
            session.document.blocks.iter().map(|b| b.block_id).collect();
        assert_eq!(
            after,
            vec![before[0], before[2]],
            "the other two survived in order"
        );
    }

    #[test]
    fn the_last_block_cannot_be_removed_because_nothing_would_be_left_to_project() {
        let mut session = session(1);
        let error = session
            .apply_remove(0)
            .expect_err("refused rather than accepted-then-unprojectable");
        assert!(
            matches!(error, SessionError::Edit(_)),
            "an Edit error, not a Reproject one: {error}"
        );
        assert_eq!(
            session.document.blocks.len(),
            1,
            "the document is untouched"
        );
    }

    #[test]
    fn a_synthetic_session_refuses_to_read_a_record_and_says_why() {
        // "No such record" from a session that has no packages is a statement
        // about the session, not about the corpus, and reading it as the latter
        // is exactly the unavailable-is-not-a-miss bug this repo keeps catching.
        let session = session(1);
        let source = DocumentSource::Record {
            key: spore_core::ResourceKey::new(1, 2, 3),
            family: Family::Creature,
        };
        let error = session
            .records
            .read(&source)
            .expect_err("a synthetic session cannot read a record");
        assert!(
            error.to_string().contains("synthetic"),
            "the reason must name the cause: {error}"
        );
    }

    #[test]
    fn undo_and_redo_walk_the_document_and_the_selection_together() {
        let mut session = session(2);
        let original = session.document.blocks[0].block_id;
        session.apply_replace(0, id(OTHER)).expect("replaced");
        session.select(Some(0)).expect("selected");
        assert!(session.can_undo());

        assert!(session.undo().expect("undo runs"));
        assert_eq!(
            session.document.blocks[0].block_id, original,
            "the replace came back"
        );
        assert_eq!(
            session.selection(),
            Some(0),
            "the selection survived the undo"
        );
        assert!(session.can_redo());

        assert!(session.redo().expect("redo runs"));
        assert_eq!(session.document.blocks[0].block_id, Some(id(OTHER)));
    }

    #[test]
    fn undo_after_an_add_removes_the_added_block() {
        let mut session = session(2);
        let before = session.document.blocks.len();
        let added = session.apply_add(id(OTHER), Some(0)).expect("added");
        assert_eq!(session.document.blocks.len(), before + 1);
        session.undo().expect("undo runs");
        assert_eq!(session.document.blocks.len(), before, "the add was undone");
        assert_ne!(session.selection(), Some(added), "nothing added survives");
    }

    #[test]
    fn a_sequence_of_edits_unwinds_all_the_way_and_replays_all_the_way() {
        let mut session = session(3);
        let original = session.document.clone();
        session.apply_replace(0, id(OTHER)).expect("replaced");
        session.apply_scale(1, 1.5).expect("scaled");
        session.apply_add(id(OTHER), Some(0)).expect("added");
        session.apply_remove(2).expect("removed");
        assert_eq!(session.document.blocks.len(), 3, "add then remove nets out");

        while session.undo().expect("undo runs") {}
        assert_eq!(
            session.document.blocks, original.blocks,
            "back to the opening document"
        );
        while session.redo().expect("redo runs") {}
        assert_eq!(session.document.blocks.len(), 3);
        assert!(
            session
                .document
                .blocks
                .iter()
                .any(|b| b.block_id == Some(id(OTHER))),
            "the replacement replayed"
        );
        // Whatever it settled on, it must name a block that exists -- a stale
        // index past the end is the exact failure this module is guarding.
        if let Some(selected) = session.selection() {
            assert!(
                selected < session.document.blocks.len(),
                "selection {selected} is past the end of {} blocks",
                session.document.blocks.len()
            );
        }
    }

    #[test]
    fn scale_leaves_the_other_transform_fields_exactly_as_they_were() {
        let mut session = session(2);
        let before = session.document.blocks[0].transform;
        session.apply_scale(0, 2.0).expect("scaled");
        let after = session.document.blocks[0].transform;
        assert_eq!(after.scale, 2.0);
        assert_eq!(
            after.position.to_array(),
            before.position.to_array(),
            "scale must not disturb position"
        );
        assert_eq!(
            after.orientation.to_array(),
            before.orientation.to_array(),
            "scale must not disturb orientation: the corpus's two fields are independent"
        );
    }
}
