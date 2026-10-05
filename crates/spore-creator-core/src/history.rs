//! Undo/redo for a [`CreatorDocument`], by snapshot.
//!
//! This is the first undo stack in the workspace, and it exists because
//! [`mod@crate::edit`]'s header names what one needs: its six mutators "either do
//! what it says or refuse with a typed [`crate::EditError`] and leave the
//! document **byte-identical** to how it arrived", because "an editor that
//! silently fixes a bad request cannot answer the only question an undo stack and
//! a corpus test both need answered". That question is *what did the tool
//! actually do*, and this module answers it by storing the document itself
//! rather than a claim about it.
//!
//! # Snapshot, not inverse operation
//!
//! Each checkpoint is a whole [`CreatorDocument`] clone, and undo returns a
//! stored document rather than running an operation backwards.
//!
//! The alternative — one inverse operation per mutator — was rejected for a
//! concrete reason, not a philosophical one. **Undoing a removal means
//! re-inserting a block at a slot, which means re-deriving the whole index-space
//! rewrite in the other direction**: every `<childid>` above the hole shifts
//! back **up** by one, and `edit.rs` documents the forward shift as "the part
//! that is easy to forget and impossible to spot — an edge that keeps its number
//! after a compaction silently re-parents the document". That sentence is about
//! the forward direction, where the code and its test both exist. The reverse
//! direction has no such guard, so it would be written once and trusted forever,
//! and the class of bug it produces is a creature that looks fine and is
//! re-parented.
//!
//! Worse, inverse undo makes the stack *depend on the mutators staying
//! invertible*. `add_block` appends, so its inverse is a truncation — but only
//! while nothing else has appended since. Two operations interleave and the
//! history is already wrong. A snapshot of a [`Clone`] document has no such
//! coupling: it cannot desynchronise from the document by construction, because
//! it **is** a document.
//!
//! # The bound is a ring, and what it costs
//!
//! [`CreatorHistory::new`] takes a `max_depth`. When the undo stack is full,
//! recording a checkpoint **drops the oldest** — so it is a ring buffer, and the
//! accepted cost is stated plainly:
//!
//! * **Redo is unavailable across an overflow.** Recording a checkpoint clears
//!   the redo stack (a new mutation invalidates the branch), and an overflow
//!   additionally destroys the undo entry that a redo would have needed. Past
//!   `max_depth` edits the user cannot go forward again without redoing the work.
//!   A ring of 64 keeps that window wide enough to cover any single editing
//!   burst; a longer one would need a different structure (see
//!   [`CreatorHistory::clear`] on why the bound is per-history and not global).
//! * **The dropped entry is gone, not shadowed.** `depth()` shrinks back to
//!   `max_depth` and the earliest reachable state is the one that survived. There
//!   is no "undo all the way to load" once the ring has turned over.
//!
//! [`DEFAULT_MAX_DEPTH`] is 64. That is a choice, and the reason it is not a
//! measurement is that **nothing in this workspace has watched a creator session
//! long enough to count one**: no game session has been traced here, so any
//! "deepest observed sequence" would be invented. 64 is chosen to sit above any
//! plausible single editing burst and to keep `record` a `VecDeque::push_back`;
//! a client that knows its own budget should pass its own number to
//! [`CreatorHistory::new`] rather than rely on this one.
//!
//! # Selection is the whole problem, and it is the reason this is not a `Vec<Option<Document>>`
//!
//! A [`BlockIndex`] is a **position**, not a handle: `remove_block` compacts
//! `blocks`, so every index above the removed slot shifts down by one and every
//! index the UI is holding silently names a different part. `edit.rs` returns an
//! [`IndexRemap`] precisely so the caller cannot forget, and says so twice —
//! "do not defer the rewrite of caller-held indices until *later*" — but an undo
//! stack is exactly where the caller forgets, because the removal happened
//! several frames ago and the code doing the undoing did not perform it.
//!
//! So the remap is **carried, not requested**. [`CreatorHistory::record`] and
//! [`CreatorHistory::record_removal`] store the relationship between a step's
//! two index spaces alongside the two documents, and [`CreatorHistory::undo`] /
//! [`CreatorHistory::redo`] hand the remapped selection back with the document.
//! There is no API by which the history itself hands back a stale index.
//!
//! What it cannot do is repair one the caller already corrupted: a caller who
//! skips `rewrite_targets` on the way *forward* is holding an index that names
//! the wrong part, and no history can know what was meant. The rule is applied
//! to whatever number it is given, which keeps the two directions inverses; it
//! is not silently corrected, because correcting it would be inventing a fact
//! about what the user had selected. `a_selection_the_caller_corrupted_is_carried_consistently_not_repaired`
//! pins that.
//!
//! # Two ways to record, because there are exactly two kinds of edit
//!
//! `edit.rs` has six mutators, and they divide exactly:
//!
//! | mutator | renumbers? | recorded with |
//! |---|---|---|
//! | `add_block` | no — appends at `blocks.len()` | [`record`](CreatorHistory::record) |
//! | `replace_block` | no | [`record`](CreatorHistory::record) |
//! | `set_block_transform` | no | [`record`](CreatorHistory::record) |
//! | `set_paint` | no | [`record`](CreatorHistory::record) |
//! | `reparent_block` | no — moves an edge, touches no index | [`record`](CreatorHistory::record) |
//! | `remove_block` | **yes** — returns an [`IndexRemap`] | [`record_removal`](CreatorHistory::record_removal) |
//!
//! [`IndexRemap`] has exactly one producer in the whole workspace, and it is
//! `remove_block`. So the division above is exhaustive by construction rather
//! than by convention: there is no third mutator that could need a third
//! recording form, and a caller who holds an `IndexRemap` in their hand is
//! looking at a removal. The test
//! `every_mutator_that_does_not_renumber_can_be_recorded_and_moves_no_index`
//! walks the five index-preserving mutators to keep that list honest.
//!
//! The two forms cannot be confused by accident either: `record_removal` pairs a
//! document with a remap and its `debug_assert` checks that the remap covers
//! exactly that document's pre-removal indices. That is the weight the crate
//! gives a mismatched internal pairing — the same weight
//! `persist`'s encoder gives an unbalanced container, for the same reason:
//! `#![forbid(unsafe_code)]`, hand-checked, and a mistake surfaces as a wrong
//! document rather than as memory trouble.
//!
//! # What the history does not own, and why that shapes the signature
//!
//! **The caller owns the document.** This crate is Bevy-free and holds no editor
//! state, so the live [`CreatorDocument`] lives in the caller's variable and the
//! history is a stack of snapshots beside it. That is why [`undo`](CreatorHistory::undo)
//! and [`redo`](CreatorHistory::redo) *take* `current: &CreatorDocument` and
//! return an **owned** restored document rather than borrowing one out of the
//! stack:
//!
//! * the `current` document is the post-state of the step being undone, and it
//!   has to be pushed onto the redo stack — which a method that only reads the
//!   stack cannot do;
//! * returning `&CreatorDocument` would tie the caller's mutable borrow of the
//!   live document to an immutable borrow of the history, for as long as the step
//!   is alive. The editor loop would have to copy out of the step to write into
//!   the document anyway, so the borrow would buy nothing and cost the natural
//!   call site.
//!
//! The price is one [`Clone`] of the document per `undo`/`redo` — the same clone
//! `record` already pays per mutation, and the same one the stack holds
//! regardless. There is no way to swap instead of clone without the history
//! owning the document, which would put editor state in a crate whose whole
//! claim is that it holds none.
//!
//! # The steps themselves are exact, whatever `record` was given
//!
//! A useful property falls out of the above. `undo` does not *replay* anything:
//! it moves the caller's live document onto the redo stack and returns the
//! snapshot that was on top of the undo stack. So if a caller records the wrong
//! document, the *documents* still walk the sequence correctly — only the
//! selection's remap is wrong, and that is declared at record time rather than
//! inferred at undo time. A history that stored "what to do" would have the
//! opposite property and the opposite failure.
//!
//! # `clear()` is part of the contract, not a convenience
//!
//! Loading a document **must** be followed by [`clear`](CreatorHistory::clear).
//! Without it the previous creature's checkpoints stay reachable, and `Ctrl+Z`
//! in a freshly opened file silently replaces the user's work with another
//! creature. That is a data-loss bug that looks like a working undo stack, so it
//! is stated as an obligation here rather than left to the client to rediscover.
//! There is no `document_id` on the history that could enforce it: the crate
//! knows nothing about files.
//!
//! # What this does not do
//!
//! * **It does not coalesce.** Dragging a slider for ten frames records ten
//!   checkpoints. Coalescing needs a "did this edit mean the same as the last
//!   one" predicate, which is a client decision (and, for a transform drag, a
//!   decision about whether two positions are within a drag threshold). The
//!   bound is the only pressure on stack growth.
//! * **It does not detect no-op mutations.** `reparent_block` to the current
//!   parent is documented as a no-op, not an error; recording before it stores an
//!   identical checkpoint, and undoing it restores an identical document. That is
//!   harmless and truthful, and suppressing it would mean comparing documents on
//!   every `record` to save nothing.
//! * **It does not name anything.** A [`CreatorHistory`] holds no labels for its
//!   steps, so a UI cannot yet grey out "Undo: delete limb". [`CreatorHistory::depth`]
//!   and [`can_undo`](CreatorHistory::can_undo) are exact; the *label* is the
//!   client's, because only the client knows what it was doing.
//!
//! # Grade
//!
//! The history is **`INFERRED`**, like the operations it undoes: it is what the
//! index-space policy in `edit.rs` plus the fact that a [`CreatorDocument`] is
//! [`Clone`] make correct, by reasoning rather than by observation. The *data* it
//! moves is [`crate::document::DOCUMENT_EVIDENCE`] (`OBSERVED`) — the snapshots
//! are clones of bytes that came out of real records.
//!
//! # Memory, measured
//!
//! A snapshot costs one [`CreatorDocument`]. Over the 68 real creator documents
//! (680 `<blockref>` blocks, 601 `<childid>` edges, 79 roots) the largest places
//! **32** blocks and the median **9**; per family the maxima are `crt` 20,
//! `vcl` 19, `ufo` 32, `cll` 7, `bld` 8, `flr` 3. The whole corpus serialises to
//! 392,218 bytes in the canonical save format, and the largest document — the
//! 32-block `ufo` — to 17,733 bytes.
//!
//! So a [`DEFAULT_MAX_DEPTH`]-deep ring on the worst document in the corpus is
//! bounded by 64 × 17,733 B ≈ 1.1 MB *if a snapshot in RAM cost what its
//! canonical save costs*, which it does not: the save is a JSON string carrying
//! every field name. The bound is therefore not a memory concern for this
//! corpus. It is a **redo-availability** concern, which is why it is documented
//! as one above and why [`DEFAULT_MAX_DEPTH`] is chosen for editing bursts rather
//! than for bytes.

use std::collections::VecDeque;

use crate::document::{BlockIndex, CreatorDocument};
use crate::edit::{IndexRemap, Remap};

/// The depth [`CreatorHistory::default`] uses.
///
/// 64 checkpoints. The bound exists to cap *redo availability past an overflow*
/// (see the module docs), not memory: the largest of the 68 corpus documents is
/// 32 blocks and the median 9, so 64 snapshots of the worst one stay under a
/// megabyte even charging each snapshot its full canonical-save size. A depth no
/// user outruns in one sitting, and a `record` that stays a
/// `VecDeque::push_back`.
pub const DEFAULT_MAX_DEPTH: usize = 64;

/// A bounded undo/redo stack over whole [`CreatorDocument`] snapshots.
///
/// Construct with [`new`](CreatorHistory::new) for an explicit bound or
/// [`CreatorHistory::default`] for [`DEFAULT_MAX_DEPTH`].
///
/// # The editor loop
///
/// ```text
/// // before every mutation that moved no index:
/// history.record(&document);
/// let index = document.add_block(part, Some(parent))?;
///
/// // and before a removal, where the receipt is the point:
/// let before = document.clone();
/// let remap = document.remove_block(victim, OnLastChild::OmitElement)?;
/// history.record_removal(&before, &remap);
///
/// // undo and redo hand back the document and the selection together:
/// if let Some(step) = history.undo(&document, selection) {
///     document = step.into_document();
///     selection = step.selection().and_then(SelectionStep::kept_index);
/// }
/// ```
///
/// Two rules the type cannot enforce:
///
/// 1. `record` **before** the mutation, not after. Recording after stores the
///    state the mutation produced, and `undo` then returns the document the user
///    was already looking at.
/// 2. [`clear`](CreatorHistory::clear) on document load. Skipping it leaves the
///    previous creature reachable through `undo`, which is silent data loss.
#[derive(Debug)]
pub struct CreatorHistory {
    max_depth: usize,
    undo: VecDeque<Entry>,
    redo: VecDeque<Entry>,
}

/// One checkpoint: a document, and how its index space relates to the other
/// side of the same step.
#[derive(Debug)]
struct Entry {
    document: CreatorDocument,
    shift: StepShift,
}

/// The index-space relationship a step imposes between its two documents.
///
/// Two shapes, and the second is the first plus one deletion. This mirrors
/// `edit.rs`'s policy exactly rather than re-deriving it: the forward direction
/// calls [`IndexRemap::map`] — the same function `rewrite_targets` calls — so
/// "what redo does to an index" and "what `remove_block` did to it" are one
/// answer and cannot drift. Only the reverse direction is written out here, and
/// it is the exact inverse of what `remove_block` does:
///
/// ```text
/// later j  <  removed  ->  earlier j
/// later j  >= removed  ->  earlier j + 1
/// ```
#[derive(Debug, Clone)]
enum StepShift {
    /// No index moved. Every index of the earlier document is the same index of
    /// the later one.
    Identity,
    /// One block was removed, and this is the receipt `remove_block` returned
    /// for it.
    Removal(IndexRemap),
}

impl StepShift {
    /// Maps an index of the **later** document into the earlier one.
    ///
    /// Every index of the later document existed before the step, so the only
    /// way to answer [`SelectionStep::Removed`] is an index that named nothing
    /// in the document being replaced. That is checked against `later_len`
    /// because the caller supplied the index and a stale one is not this type's
    /// business to guess about.
    fn to_earlier(&self, later_len: usize, index: BlockIndex) -> SelectionStep {
        if index >= later_len {
            return SelectionStep::Removed;
        }
        match self {
            StepShift::Identity => SelectionStep::Kept(index),
            StepShift::Removal(remap) => {
                SelectionStep::Kept(index + usize::from(index >= remap.removed()))
            }
        }
    }

    /// Maps an index of the **earlier** document into the later one.
    ///
    /// Here [`SelectionStep::Removed`] has a second meaning the other direction
    /// cannot have: the index named the block the step deleted, so there is no
    /// index that names it any more. That is the same fact
    /// [`Remap::Removed`] and [`crate::EditError::RemovedTarget`] report, so the
    /// selection does not get a third spelling of it.
    fn to_later(&self, earlier_len: usize, index: BlockIndex) -> SelectionStep {
        if index >= earlier_len {
            return SelectionStep::Removed;
        }
        match self {
            StepShift::Identity => SelectionStep::Kept(index),
            StepShift::Removal(remap) => match remap.map(index) {
                Remap::Moved(moved) => SelectionStep::Kept(moved),
                Remap::Removed => SelectionStep::Removed,
            },
        }
    }
}

/// Where one selection index ended up when a step was applied.
///
/// Two answers, and the shape follows [`crate::edit::Remap`]: "it is here now"
/// and "it names nothing any more" must not be spelled the same way, or a UI
/// that forgets to check will act on whichever block happens to land on the dead
/// index.
///
/// `Kept` covers "did not move at all" as well as "moved", because a step that
/// only changed a transform leaves every index where it was and calling that
/// `Moved` would be a false claim. The index space is what is being reported,
/// not the block's fate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionStep {
    /// The selection names a block in the document being applied, at this index.
    Kept(BlockIndex),
    /// The selection names nothing in the document being applied.
    ///
    /// Either it named the block the step removed (the forward direction, a
    /// redo), or it was already stale — an index past the end of the document
    /// being replaced. Either way there is no index to carry, and no safe
    /// substitute: keeping the number would select whichever block now occupies
    /// it.
    Removed,
}

impl SelectionStep {
    /// The index to select in the document being applied, or `None` when the
    /// selection named nothing.
    ///
    /// The convenience for the common UI shape, and the reason
    /// [`SelectionStep`] is not a bare `Option<BlockIndex>`: `None` here means
    /// "the user's selection was deleted" and is a *different fact* from the
    /// `None` in [`HistoryStep::selection`], which means "the caller had no
    /// selection at all".
    pub const fn kept_index(self) -> Option<BlockIndex> {
        match self {
            SelectionStep::Kept(index) => Some(index),
            SelectionStep::Removed => None,
        }
    }
}

/// One restored document, with the selection that goes with it.
///
/// Returned by both [`CreatorHistory::undo`] and [`CreatorHistory::redo`]
/// because the two are the same operation on opposite stacks: a second type
/// would be a second spelling of one fact, and this crate's rule is that a fact
/// gets one name. Which direction produced a step is not something a caller
/// needs in order to use it, and [`CreatorHistory::redo_depth`] already says
/// which stack the step came off.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryStep {
    document: CreatorDocument,
    selection: Option<SelectionStep>,
}

impl HistoryStep {
    /// The document this step restored.
    pub fn document(&self) -> &CreatorDocument {
        &self.document
    }

    /// Takes the restored document, for the caller that wants to own it.
    ///
    /// Free where [`Clone`] is not: the snapshot is already a clone of the live
    /// document, and handing it over costs nothing rather than a second copy.
    pub fn into_document(self) -> CreatorDocument {
        self.document
    }

    /// Where the caller's selection ended up, or `None` if it passed none.
    ///
    /// `None` here is "no selection was carried", which is **not**
    /// [`SelectionStep::Removed`] — the caller's selection survived or did not;
    /// it was never asked about. See [`SelectionStep::kept_index`] for the
    /// narrower question.
    pub fn selection(&self) -> Option<SelectionStep> {
        self.selection
    }

    /// Both halves at once, for the editor loop's two assignments.
    pub fn into_parts(self) -> (CreatorDocument, Option<SelectionStep>) {
        (self.document, self.selection)
    }
}

impl Default for CreatorHistory {
    /// An empty history with [`DEFAULT_MAX_DEPTH`].
    fn default() -> Self {
        Self::new(DEFAULT_MAX_DEPTH)
    }
}

impl CreatorHistory {
    /// An empty history that keeps at most `max_depth` checkpoints.
    ///
    /// `max_depth == 0` is legal and means **undo disabled**: [`record`](Self::record) is a
    /// no-op and [`can_undo`](CreatorHistory::can_undo) is permanently `false`.
    /// It is honoured rather than clamped, because a client that computes a depth
    /// of 0 from its own memory budget should get "no undo", not "one undo" and
    /// not a panic.
    pub fn new(max_depth: usize) -> Self {
        Self {
            max_depth,
            undo: VecDeque::new(),
            redo: VecDeque::new(),
        }
    }

    /// The bound this history was built with.
    pub const fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// How many states [`undo`](Self::undo) can reach.
    ///
    /// Exact, and equal to `can_undo()`: both are `!self.undo.is_empty()`. A
    /// UI that greys out a button from this cannot be wrong.
    pub fn depth(&self) -> usize {
        self.undo.len()
    }

    /// How many states [`redo`](Self::redo) can reach, and [`can_redo`](Self::can_redo).
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Whether there is nothing to undo — and so, by symmetry, nothing to redo.
    pub fn is_empty(&self) -> bool {
        self.undo.is_empty() && self.redo.is_empty()
    }

    /// Whether [`undo`](Self::undo) will restore something.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether [`redo`](Self::redo) will restore something.
    ///
    /// Exactly `redo_depth() > 0`, which is **false** after a mutation following
    /// an undo and **false** after an overflow. Both are true answers about the
    /// same state: a new mutation ends the redo branch, and an overflow destroys
    /// the redo entry the branch needed.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Pushes a checkpoint for a mutation that moved no index.
    ///
    /// This is the right call for all five of `edit.rs`'s index-preserving
    /// mutators — `add_block` (which appends, so every existing index is
    /// untouched), `replace_block`, `set_block_transform`, `set_paint` and
    /// `reparent_block` — and the wrong one for `remove_block`.
    ///
    /// **Clears the redo stack.** A mutation that happened after an undo means
    /// the future the redo stack described no longer gets reached by editing; it
    /// is a different branch. Every editor implements this, and an implementation
    /// that did not would let `redo` apply a stale document.
    ///
    /// **Call it before the mutation.** Recording the state the mutation produced
    /// stores a document identical to the one the caller already has, and `undo`
    /// then appears to work while doing nothing.
    pub fn record(&mut self, document: &CreatorDocument) {
        self.push(StepShift::Identity, document);
    }

    /// Pushes a checkpoint for a [`remove_block`](crate::document::CreatorDocument::remove_block), carrying its
    /// [`IndexRemap`].
    ///
    /// `document` is the state **before** the removal and `remap` is the receipt
    /// that removal returned. The call therefore reads:
    ///
    /// ```text
    /// let before = document.clone();
    /// let remap = document.remove_block(victim, OnLastChild::OmitElement)?;
    /// history.record_removal(&before, &remap);
    /// ```
    ///
    /// which is the *only* order in which the two arguments cannot describe
    /// different states by accident — the document handed to `record_removal` is
    /// the one `remove_block` was handed, and the remap is the one it produced.
    /// Note the order versus [`record`](Self::record), which must precede its
    /// mutation. That asymmetry is not a wart: `remove_block` hands out its remap
    /// on the way out, so the receipt only exists after the call, while
    /// `record_removal` only needs it to describe a step that has already
    /// happened.
    ///
    /// Only the remap's **removed index** is used, because `IndexRemap::map` is
    /// consulted for everything else — the shift it reports is the same function
    /// the caller's own [`IndexRemap::rewrite_targets`] calls, so a redo puts an
    /// index where `rewrite_targets` would have and not somewhere merely similar.
    pub fn record_removal(&mut self, document: &CreatorDocument, remap: &IndexRemap) {
        debug_assert_eq!(
            remap.len(),
            document.len(),
            "record_removal pairs the pre-removal document with the remap of that same \
             removal: IndexRemap::len() is the pre-removal block count, and the two \
             disagree only when the caller hands over some other document"
        );
        self.push(StepShift::Removal(remap.clone()), document);
    }

    /// Drops every checkpoint, in both directions.
    ///
    /// **Call this after loading a document.** Not doing so leaves the previous
    /// creature reachable: `undo` would return a snapshot of a document the user
    /// has closed, and the editor would write it over the one they just opened.
    /// That is data loss wearing the costume of a working undo stack, and the
    /// only mitigation available here is the obligation being written down.
    ///
    /// `max_depth` is kept, so a loaded document's history is bounded the same
    /// way the old one was.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Restores the state before the most recent checkpoint, and returns the
    /// caller's `selection` carried into it.
    ///
    /// `current` is the document as it stands *after* the mutation being undone;
    /// it is pushed onto the redo stack, which is what makes
    /// [`redo`](Self::redo) return it. `selection` is a [`BlockIndex`] in
    /// `current`'s index space, and comes back in the restored document's.
    ///
    /// Returns `None` — writing nothing — when there is nothing to undo, which
    /// is also what [`can_undo`](Self::can_undo) already said. The caller keeps
    /// its document untouched in that case, so "no undo available" can never be
    /// confused with "undo produced nothing".
    pub fn undo(
        &mut self,
        current: &CreatorDocument,
        selection: Option<BlockIndex>,
    ) -> Option<HistoryStep> {
        let Entry { document, shift } = self.undo.pop_back()?;
        self.redo.push_back(Entry {
            document: current.clone(),
            shift: shift.clone(),
        });
        let selection = selection.map(|index| shift.to_earlier(current.len(), index));
        Some(HistoryStep {
            document,
            selection,
        })
    }

    /// Restores the state the last [`undo`](Self::undo) took back, and returns
    /// the caller's `selection` carried into it.
    ///
    /// The mirror of [`undo`](Self::undo): `current` is the document as it stands
    /// after the undo, `selection` is in *that* index space, and it comes back in
    /// the redone document's — where a selection that named the block the
    /// original operation removed is reported as
    /// [`SelectionStep::Removed`], because redoing the removal does not bring
    /// that block back.
    ///
    /// Returns `None` when there is nothing to redo, which is the normal state
    /// after a fresh mutation, after an overflow, and after
    /// [`clear`](Self::clear).
    pub fn redo(
        &mut self,
        current: &CreatorDocument,
        selection: Option<BlockIndex>,
    ) -> Option<HistoryStep> {
        let Entry { document, shift } = self.redo.pop_back()?;
        self.undo.push_back(Entry {
            document: current.clone(),
            shift: shift.clone(),
        });
        let selection = selection.map(|index| shift.to_later(current.len(), index));
        Some(HistoryStep {
            document,
            selection,
        })
    }

    /// The one place a checkpoint enters, so the ring and the redo-clearing live
    /// in a single function.
    fn push(&mut self, shift: StepShift, document: &CreatorDocument) {
        // Before the early return: a mutation at depth 0 still ends the redo
        // branch. It cannot be non-empty, but saying it once here means the
        // rule is not re-derived per call site.
        self.redo.clear();
        if self.max_depth == 0 {
            return;
        }
        // The ring: drop the OLDEST, so the most recent `max_depth` states stay
        // reachable. Dropping the newest instead would make `record` a no-op at
        // the bound, which reads as "undo works" and stops working.
        if self.undo.len() >= self.max_depth {
            self.undo.pop_front();
        }
        self.undo.push_back(Entry {
            document: document.clone(),
            shift,
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::UnitRgb;
    use crate::document::{Block, BlockId, BlockTransform, Matrix3, PaintRegion};
    use crate::edit::OnLastChild;
    use crate::error::EditError;
    use crate::validate::{validate, Severity};

    /// A leaf carrying a part identity, which `add_block` requires.
    fn leaf(instance: u32) -> Block {
        Block {
            block_id: Some(BlockId::new(0x4062_6000, instance)),
            ..Block::default()
        }
    }

    fn rgb(value: f32) -> UnitRgb {
        UnitRgb::new(value, value, value).expect("the test value is in 0.0 ..= 1.0")
    }

    /// A five-block chain, 0 -> 1 -> 2 -> 3 -> 4.
    ///
    /// Five, not three: a removal in the middle has to leave blocks on *both*
    /// sides of the hole, which is the only shape in which "the selection was
    /// carried up by one" and "the selection did not move" are distinguishable
    /// answers rather than the same arithmetic.
    fn chain(count: u32) -> CreatorDocument {
        let mut document = CreatorDocument {
            format_version: Some(16),
            blocks: (0..count).map(|n| leaf(n + 1)).collect(),
            ..CreatorDocument::default()
        };
        for index in 0..count as usize - 1 {
            document.blocks[index].children = Some(vec![index + 1]);
        }
        document
    }

    /// The same chain, told apart by one document-level field, so a snapshot can
    /// be identified by value without touching a single block index.
    fn tagged(document: &CreatorDocument, tag: u32) -> CreatorDocument {
        let mut tagged = document.clone();
        tagged.properties.zcorp_score = Some(tag);
        tagged
    }

    /// Every externally visible depth, and the two booleans, asserted together.
    ///
    /// The booleans are checked against the counts they must equal rather than
    /// against literals, because the requirement on them is *exactness*: a UI
    /// greys a button out from `can_undo`, so `can_undo` being merely
    /// approximately right is a bug with a user-visible symptom.
    fn assert_depths(history: &CreatorHistory, undo: usize, redo: usize) {
        assert_eq!(history.depth(), undo, "undo depth");
        assert_eq!(history.redo_depth(), redo, "redo depth");
        assert_eq!(
            history.can_undo(),
            undo > 0,
            "can_undo is exactly `depth() > 0`, not a guess"
        );
        assert_eq!(
            history.can_redo(),
            redo > 0,
            "can_redo is exactly `redo_depth() > 0`, not a guess"
        );
        assert_eq!(history.is_empty(), undo == 0 && redo == 0);
        assert!(
            history.depth() <= history.max_depth(),
            "the ring is a ring: {} > {}",
            history.depth(),
            history.max_depth()
        );
        assert!(
            history.redo_depth() <= history.max_depth(),
            "the ring is a ring in both directions: {} > {}",
            history.redo_depth(),
            history.max_depth()
        );
    }

    /// `record`, apply `edit`, undo — and require that `edit` really changed
    /// something.
    ///
    /// That last assertion is the load-bearing one. An undo test over a
    /// no-op mutation passes for free, which is exactly how a test that cannot
    /// fail gets written; here the harness refuses to run on one.
    fn assert_restored_by_undo(
        history: &mut CreatorHistory,
        before: &CreatorDocument,
        edit: impl FnOnce(&mut CreatorDocument),
        selection: Option<BlockIndex>,
        label: &str,
    ) {
        history.record(before);
        let mut edited = before.clone();
        edit(&mut edited);
        assert_ne!(
            &edited, before,
            "{label} changed nothing, so this assertion would pass without the history"
        );

        let step = history
            .undo(&edited, selection)
            .unwrap_or_else(|| panic!("{label}: recorded, so undo must produce a step"));
        assert_eq!(step.document(), before, "{label} restored the pre-state");
        assert_eq!(
            step.selection(),
            selection.map(SelectionStep::Kept),
            "{label}"
        );
    }

    // -- a history that has done nothing ------------------------------------

    #[test]
    fn a_fresh_history_can_do_nothing_and_says_so_without_touching_the_document() {
        let mut history = CreatorHistory::default();
        assert_depths(&history, 0, 0);

        let document = chain(4);
        assert!(history.undo(&document, Some(0)).is_none());
        assert!(history.redo(&document, Some(0)).is_none());
        assert_depths(&history, 0, 0);
        assert_eq!(document, chain(4), "a refused undo writes nothing anywhere");
    }

    #[test]
    fn can_undo_and_can_redo_are_exact_at_every_point_of_a_session() {
        let document = chain(4);
        let mut history = CreatorHistory::new(2);

        assert_depths(&history, 0, 0);
        history.record(&document);
        assert_depths(&history, 1, 0);
        history.record(&document);
        assert_depths(&history, 2, 0);
        // Full: the third record pushes the oldest out rather than growing.
        history.record(&document);
        assert_depths(&history, 2, 0);

        let step = history.undo(&document, None).expect("undo");
        assert_depths(&history, 1, 1);
        assert_eq!(step.document(), &document);

        // A fresh mutation ends the branch: redo is gone, not merely shadowed.
        history.record(&document);
        assert_depths(&history, 2, 0);
        assert!(history.redo(&document, None).is_none());

        history.undo(&document, None).expect("undo");
        assert_depths(&history, 1, 1);
        history.undo(&document, None).expect("undo");
        assert_depths(&history, 0, 2);
        assert!(history.undo(&document, None).is_none());
        assert_depths(&history, 0, 2);
    }

    #[test]
    fn the_default_history_is_empty_and_bounded_by_the_documented_constant() {
        let history = CreatorHistory::default();
        assert_eq!(history.max_depth(), DEFAULT_MAX_DEPTH);
        assert_eq!(DEFAULT_MAX_DEPTH, 64, "the module docs quote this number");
        assert_depths(&history, 0, 0);
    }

    // -- a snapshot is the document, bit for bit ----------------------------

    #[test]
    fn undo_restores_every_field_including_every_absence() {
        // One block per shape the corpus distinguishes, so "restored" is a claim
        // about all of them and not about the blocks that happened to differ.
        let mut leaf_none = leaf(1);
        leaf_none.transform.position.x = -0.0;
        leaf_none.transform.scale = 0.28;
        leaf_none.transform.triangle_direction = None;
        leaf_none.paint = None;

        let mut empty_child_list = leaf(2);
        empty_child_list.children = Some(vec![]);

        let mut empty_paint_list = leaf(3);
        empty_paint_list.paint = Some(vec![]);
        empty_paint_list.limb = Some(crate::document::Limb {
            limb_type: Some(3),
            muscle_scale: Some(1.5),
            base_muscle_scale: Some(-0.0),
        });

        let mut root = leaf(4);
        root.children = Some(vec![1]);
        root.handles = Some(vec![crate::document::ChannelWeight {
            channel: 7,
            weight: 0.25,
        }]);

        let before = CreatorDocument {
            // 59 corpus documents are version 16 and 8 are 15; `None` is the
            // shape a document with no `<formatversion>` at all would have, and
            // it must not be normalised into 16 on the way through the history.
            format_version: None,
            properties: crate::document::DocumentProperties::default(),
            blocks: vec![root, leaf_none, empty_child_list, empty_paint_list],
        };

        let mut edited = before.clone();
        edited.add_block(leaf(9), Some(0)).expect("added");
        assert_ne!(
            &edited, &before,
            "the mutation has to be real to prove anything"
        );

        let mut history = CreatorHistory::default();
        history.record(&before);
        let step = history.undo(&edited, None).expect("undo");
        let restored = step.document();
        assert_eq!(restored, &before, "the whole document, compared bitwise");

        // Now the absences by name, so a `Clone` that quietly normalised one
        // would fail here rather than being argued about.
        assert_eq!(restored.format_version, None);
        assert_eq!(restored.properties.model_type, None);
        assert_eq!(restored.blocks[1].children, None, "a leaf writes no list");
        assert_eq!(
            restored.blocks[2].children,
            Some(vec![]),
            "an empty list is a different fact from an absent one"
        );
        assert_eq!(restored.blocks[1].paint, None);
        assert_eq!(
            restored.blocks[3].paint,
            Some(vec![]),
            "an empty <paintlist> survived too"
        );
        assert_eq!(
            restored.blocks[1].transform.position.x.to_bits(),
            (-0.0f32).to_bits(),
            "a negative zero is not a zero"
        );
        assert_eq!(
            restored.blocks[3]
                .limb
                .as_ref()
                .and_then(|limb| limb.base_muscle_scale)
                .map(f32::to_bits),
            Some((-0.0f32).to_bits())
        );
        assert_eq!(
            restored.blocks[0].handles.as_ref().map(|h| h.len()),
            Some(1)
        );

        // The two shapes that are *deliberately* malformed are still malformed
        // after the round trip, and `validate` says which: a snapshot that
        // quietly normalised an empty list into an absent one would delete the
        // fact the corpus absence census is built on.
        let issues = validate(restored, None);
        assert!(
            issues.iter().all(|issue| issue.severity != Severity::Error),
            "no shape became structurally invalid: {issues:?}"
        );
        assert!(
            issues.iter().any(|issue| issue.code == "empty_child_list"),
            "an empty <childlist> is still an empty <childlist>: {issues:?}"
        );
        assert!(
            issues.iter().any(|issue| issue.code == "empty_paint_list"),
            "an empty <paintlist> is still an empty <paintlist>: {issues:?}"
        );
    }

    #[test]
    fn redo_reproduces_the_edited_document_bit_for_bit() {
        let before = chain(4);
        let mut history = CreatorHistory::default();
        history.record(&before);

        let mut edited = before.clone();
        edited
            .remove_block(1, OnLastChild::OmitElement)
            .expect("removed");
        edited
            .set_paint(
                0,
                Some(vec![PaintRegion {
                    region: 5,
                    paint_id: 0xf596_7f30,
                    color1: rgb(0.25),
                    color2: rgb(0.75),
                }]),
            )
            .expect("painted");
        let before_removal = edited.clone();
        let remap = edited
            .remove_block(2, OnLastChild::OmitElement)
            .expect("removed");
        history.record_removal(&before_removal, &remap);

        // Two steps were recorded, so it takes two undos to be back at the start.
        let step = history.undo(&edited, None).expect("undo");
        assert_eq!(step.document(), &before_removal);
        assert_depths(&history, 1, 1);
        let step = history.undo(step.document(), None).expect("undo");
        assert_eq!(step.document(), &before);
        assert_depths(&history, 0, 2);

        // And forward again, to the edited document exactly. `redo` hands back
        // the document the caller had at undo time, so the paint, the removal
        // and the compaction all have to come back bit for bit.
        let step = history.redo(&before, None).expect("redo");
        assert_eq!(step.document(), &before_removal);
        let step = history.redo(step.document(), None).expect("redo");
        assert_eq!(
            step.document(),
            &edited,
            "redo is the edited document, exactly"
        );
        assert_eq!(step.document().len(), 2);
        assert_depths(&history, 2, 0);
        assert!(history.redo(step.document(), None).is_none());
        assert!(validate(step.document(), None).is_empty());
    }

    // -- the selection, which is the point ----------------------------------

    #[test]
    fn a_selection_the_caller_never_held_stays_unheld() {
        // `None` in, `None` out. Collapsing it to `Some(Removed)` would tell the
        // UI its selection had been deleted, which is a different event entirely.
        let document = chain(4);
        let mut history = CreatorHistory::default();
        history.record(&document);
        assert_eq!(
            history.undo(&document, None).expect("undo").selection(),
            None
        );
    }

    #[test]
    fn undo_of_a_removal_carries_a_selection_above_the_hole_back_up_one() {
        let before = chain(5);
        let mut edited = before.clone();
        let remap = edited
            .remove_block(1, OnLastChild::OmitElement)
            .expect("removed");
        let mut history = CreatorHistory::default();
        history.record_removal(&before, &remap);

        // In the 4-block document, index 1 holds what used to be block 2.
        assert_eq!(edited.len(), 4);
        assert_eq!(remap.map(2), Remap::Moved(1));
        let step = history.undo(&edited, Some(1)).expect("undo");

        assert_eq!(step.document(), &before);
        assert_eq!(
            step.selection(),
            Some(SelectionStep::Kept(2)),
            "the selected block is back where it was, not where its number now points"
        );
        assert_eq!(
            step.selection().and_then(SelectionStep::kept_index),
            Some(2)
        );
    }

    #[test]
    fn a_selection_below_the_hole_does_not_move_across_an_undo() {
        let before = chain(5);
        let mut edited = before.clone();
        let remap = edited
            .remove_block(3, OnLastChild::OmitElement)
            .expect("removed");
        let mut history = CreatorHistory::default();
        history.record_removal(&before, &remap);

        // Index 1 is below the hole, so it names the same block in both
        // documents. A remap that shifted unconditionally would say 2, and that
        // is a *different block*, not a differently-spelled 1.
        assert_eq!(edited.len(), 4);
        let step = history.undo(&edited, Some(1)).expect("undo");
        assert_eq!(step.selection(), Some(SelectionStep::Kept(1)));
        assert_eq!(step.document().blocks[1], before.blocks[1]);
    }

    #[test]
    fn redo_of_a_removal_reports_the_block_it_deleted_as_removed() {
        let before = chain(5);
        let mut edited = before.clone();
        let remap = edited
            .remove_block(2, OnLastChild::OmitElement)
            .expect("removed");
        let mut history = CreatorHistory::default();
        history.record_removal(&before, &remap);

        let undone = history.undo(&edited, None).expect("undo");
        assert_eq!(undone.document(), &before);

        // Redoing deletes block 2 again, so a selection on block 2 has nothing
        // left to point at. Reporting an index anyway would select whichever
        // block now occupies slot 2.
        let redone = history.redo(undone.document(), Some(2)).expect("redo");
        assert_eq!(
            redone.selection(),
            Some(SelectionStep::Removed),
            "no index names a block the redo deleted"
        );
        assert_eq!(redone.selection().and_then(SelectionStep::kept_index), None);
        assert_eq!(redone.document(), &edited);
    }

    #[test]
    fn an_index_past_the_end_of_the_document_being_replaced_names_nothing() {
        let before = chain(5);
        let mut edited = before.clone();
        let remap = edited
            .remove_block(0, OnLastChild::OmitElement)
            .expect("removed");
        let mut history = CreatorHistory::default();
        history.record_removal(&before, &remap);

        // 99 was never a block in either document. Both directions must say so
        // rather than carrying it, and neither may index past the end doing it.
        let step = history.undo(&edited, Some(99)).expect("undo");
        assert_eq!(step.selection(), Some(SelectionStep::Removed));
        let redone = history.redo(step.document(), Some(99)).expect("redo");
        assert_eq!(redone.selection(), Some(SelectionStep::Removed));
    }

    #[test]
    fn the_core_remap_and_the_historys_remap_are_the_same_function() {
        // For every victim and every block: `rewrite_targets` forward, this
        // history backward, and the answer has to be the index we started with.
        // Loop A walks the surviving blocks; loop B handles the one the removal
        // took, which `rewrite_targets` refuses rather than rewriting.
        for victim in 0..5usize {
            let before = chain(5);
            let mut edited = before.clone();
            let remap = edited
                .remove_block(victim, OnLastChild::OmitElement)
                .expect("removed");
            let mut history = CreatorHistory::default();
            history.record_removal(&before, &remap);

            for original in 0..before.len() {
                if matches!(remap.map(original), Remap::Removed) {
                    continue;
                }
                let mut held = vec![original];
                remap
                    .rewrite_targets(&mut held)
                    .expect("this block survived, so the rewrite is defined");
                let forward = held[0];

                let step = history.undo(&edited, Some(forward)).expect("undo");
                assert_eq!(
                    step.selection(),
                    Some(SelectionStep::Kept(original)),
                    "victim {victim}: the core moved {original} to {forward}, and undo \
                     must bring it back to {original}"
                );
                assert_eq!(step.document(), &before);

                let back = history.redo(step.document(), Some(original)).expect("redo");
                assert_eq!(
                    back.selection(),
                    Some(SelectionStep::Kept(forward)),
                    "victim {victim}: redo must agree with rewrite_targets, not merely \
                     look like it"
                );
                assert_eq!(back.document(), &edited);
            }

            // The removed block: the core refuses to rewrite it, and the history
            // reports the same fact in its own vocabulary.
            let mut stale = vec![0, victim, before.len()];
            assert_eq!(
                remap.rewrite_targets(&mut stale).unwrap_err(),
                EditError::RemovedTarget { index: victim },
                "the core is all-or-nothing"
            );
            assert_eq!(stale, vec![0, victim, before.len()], "and it wrote nothing");

            let step = history.undo(&edited, None).expect("undo");
            assert_eq!(step.selection(), None);
            assert_eq!(step.document().blocks[victim], before.blocks[victim]);
            let back = history.redo(step.document(), Some(victim)).expect("redo");
            assert_eq!(back.selection(), Some(SelectionStep::Removed));
            assert_eq!(back.document(), &edited);
        }
    }

    #[test]
    fn a_selection_the_caller_corrupted_is_carried_consistently_not_repaired() {
        // The honest limit. A caller that skips `rewrite_targets` holds an index
        // that already names the wrong part, and no history can know what was
        // meant. What it can do — and does — is carry the held number through the
        // same rule every other index follows, so the two directions stay
        // inverses. Claiming to repair the caller's bug would be inventing a
        // fact about what the user had selected.
        let before = chain(5);
        let mut edited = before.clone();
        let remap = edited
            .remove_block(1, OnLastChild::OmitElement)
            .expect("removed");
        let mut history = CreatorHistory::default();
        history.record_removal(&before, &remap);

        // The caller forgot, and still holds 2, which in the 4-block document
        // names what used to be block 3.
        assert_eq!(remap.map(2), Remap::Moved(1), "the right index was 1");
        let step = history.undo(&edited, Some(2)).expect("undo");
        assert_eq!(
            step.selection(),
            Some(SelectionStep::Kept(3)),
            "carried by the same rule, not silently fixed to 2 and not silently kept"
        );
    }

    /// One of `edit.rs`'s index-preserving mutators, as a named plain function.
    ///
    /// A `fn` pointer rather than a closure, so each entry in the table below is the
    /// op its label names and cannot quietly capture a fixture that makes it a
    /// different op.
    type Mutator = fn(&mut CreatorDocument);

    /// (label, mutation). These are the five of `edit.rs`'s six mutators that leave
    /// every index alone, and the module docs assert the list is exhaustive; this
    /// table is where that assertion is checked rather than trusted.
    const INDEX_PRESERVING_MUTATORS: [(&str, Mutator); 5] = [
        ("add_block", add_block),
        ("replace_block", replace_block),
        ("set_block_transform", set_block_transform),
        ("set_paint", set_paint),
        ("reparent_block", reparent_block),
    ];

    fn add_block(document: &mut CreatorDocument) {
        document.add_block(leaf(9), Some(0)).expect("added");
    }

    fn replace_block(document: &mut CreatorDocument) {
        document
            .replace_block(2, BlockId::new(0x4061_6000, 0x1234_5678))
            .expect("replaced");
    }

    fn set_block_transform(document: &mut CreatorDocument) {
        document
            .set_block_transform(
                1,
                BlockTransform {
                    position: crate::document::Vec3 {
                        x: 1.0,
                        y: 2.0,
                        z: 3.0,
                    },
                    scale: 1.5,
                    orientation: Matrix3::IDENTITY,
                    triangle_direction: None,
                    triangle_pick_origin: None,
                },
            )
            .expect("set");
    }

    fn set_paint(document: &mut CreatorDocument) {
        document
            .set_paint(
                3,
                Some(vec![PaintRegion {
                    region: 5,
                    paint_id: 0xf596_7f30,
                    color1: rgb(0.0),
                    color2: rgb(1.0),
                }]),
            )
            .expect("set");
    }

    fn reparent_block(document: &mut CreatorDocument) {
        document
            .reparent_block(3, Some(1), OnLastChild::KeepWrittenList)
            .expect("reparented");
    }

    #[test]
    fn every_mutator_that_does_not_renumber_can_be_recorded_and_moves_no_index() {
        let before = chain(4);
        for (label, edit) in INDEX_PRESERVING_MUTATORS {
            // Block 3 is checked as well as block 0: a selection past where an
            // insertion would land is where an off-by-one would show.
            for selection in [0, 3] {
                let mut history = CreatorHistory::default();
                assert_restored_by_undo(&mut history, &before, edit, Some(selection), label);
                assert_depths(&history, 0, 1);
            }
        }
    }

    // -- the bound ----------------------------------------------------------

    #[test]
    fn the_ring_drops_the_oldest_checkpoint_and_keeps_the_newest() {
        let base = chain(4);
        // Four distinct states. `record` names a *pre*-state, so `undo` has to
        // be handed a document that is genuinely *after* it; passing the same
        // document would make the test an identity that proves nothing.
        let state_one = tagged(&base, 1);
        let state_two = tagged(&base, 2);
        let state_three = tagged(&base, 3);
        let final_state = tagged(&base, 4);

        let mut history = CreatorHistory::new(2);
        history.record(&state_one);
        history.record(&state_two);
        history.record(&state_three); // full: pushes state_one out
        assert_depths(&history, 2, 0);

        let step = history.undo(&final_state, None).expect("undo");
        assert_eq!(
            step.document(),
            &state_three,
            "the most recent checkpoint is the one on top"
        );
        let step = history.undo(step.document(), None).expect("undo");
        assert_eq!(step.document(), &state_two, "then the one before it");
        assert_depths(&history, 0, 2);
        assert!(
            history.undo(&state_two, None).is_none(),
            "state one was pushed out by the ring: unreachable, not shadowed"
        );

        // Forward again: the two survivors and nothing else. `redo` hands back
        // whatever `undo` was handed, so the first redo is `final_state`.
        let step = history.redo(&state_two, None).expect("redo");
        assert_eq!(step.document(), &state_three);
        let step = history.redo(&step.document().clone(), None).expect("redo");
        assert_eq!(step.document(), &final_state);
        assert_depths(&history, 2, 0);
        assert!(history.redo(step.document(), None).is_none());
    }

    #[test]
    fn the_ring_holds_its_bound_across_a_whole_undo_redo_walk() {
        let base = chain(4);
        let mut history = CreatorHistory::new(3);
        for tag in 0..10u32 {
            history.record(&tagged(&base, tag));
        }
        assert_depths(&history, 3, 0);

        // Ten records, three slots: walking all the way back lands on tag 7, the
        // oldest that survived, and tag 6 is one step past the end.
        let mut current = tagged(&base, 10);
        let mut visited = Vec::new();
        for _ in 0..3 {
            let step = history.undo(&current, None).expect("undo");
            visited.push(step.document().properties.zcorp_score);
            current = step.into_document();
            assert_depths(&history, history.depth(), history.redo_depth());
        }
        assert_eq!(visited, vec![Some(9), Some(8), Some(7)]);
        assert!(history.undo(&current, None).is_none());
        assert_depths(&history, 0, 3);

        for _ in 0..3 {
            let step = history.redo(&current, None).expect("redo");
            current = step.into_document();
            assert_depths(&history, history.depth(), history.redo_depth());
        }
        assert_eq!(current, tagged(&base, 10));
        assert_depths(&history, 3, 0);
    }

    #[test]
    fn an_overflow_costs_the_redo_branch_and_that_is_the_documented_price() {
        let base = chain(4);
        let mut history = CreatorHistory::new(2);
        history.record(&tagged(&base, 1));
        history.record(&tagged(&base, 2));
        history.record(&tagged(&base, 3)); // overflows, dropping tag 1
        let final_state = tagged(&base, 4);

        let step = history.undo(&final_state, None).expect("undo");
        assert_eq!(step.document().properties.zcorp_score, Some(3));
        let step = history.undo(step.document(), None).expect("undo");
        assert_eq!(
            step.document().properties.zcorp_score,
            Some(2),
            "the oldest surviving entry; tag 1 went out with the overflow"
        );
        assert_depths(&history, 0, 2);

        // Both surviving entries are still reachable forward...
        let step = history.redo(step.document(), None).expect("redo");
        assert_eq!(step.document().properties.zcorp_score, Some(3));
        // ...but the branch the overflow destroyed is gone for good, and the ring
        // does not grow to compensate.
        let step = history.redo(step.document(), None).expect("redo");
        assert_eq!(step.document(), &final_state);
        assert_depths(&history, 2, 0);
        assert!(history.redo(step.document(), None).is_none());

        // And a fresh mutation from here starts a new branch from tag 3, never
        // from the entry the overflow discarded.
        history.record(step.document());
        let step = history.undo(step.document(), None).expect("undo");
        assert_eq!(step.document().properties.zcorp_score, Some(4));
    }

    #[test]
    fn a_zero_depth_disables_undo_without_panicking() {
        let document = chain(4);
        let mut history = CreatorHistory::new(0);
        assert_eq!(history.max_depth(), 0);
        for _ in 0..5 {
            history.record(&document);
        }
        assert_depths(&history, 0, 0);
        assert!(!history.can_undo());
        assert!(history.undo(&document, Some(0)).is_none());
        assert_eq!(document, chain(4));
    }

    // -- document load ------------------------------------------------------

    #[test]
    fn a_freshly_loaded_document_has_no_history_and_undo_cannot_reach_the_old_one() {
        // The bug this exists to prevent: `Ctrl+Z` in a document opened after
        // another one replaces the user's work with the previous creature.
        let first = chain(4);
        let second = tagged(&chain(6), 0xdead_beef);

        let mut history = CreatorHistory::default();
        for tag in 1..=3u32 {
            history.record(&tagged(&first, tag));
        }
        let mut edited = first.clone();
        edited
            .remove_block(0, OnLastChild::OmitElement)
            .expect("removed");
        assert_depths(&history, 3, 0);
        assert!(history.can_undo(), "there really is history to lose");

        // Loading a document clears it.
        history.clear();
        assert_depths(&history, 0, 0);
        assert!(history.is_empty());
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(
            history.max_depth(),
            DEFAULT_MAX_DEPTH,
            "the bound survives a load, so the new document is bounded too"
        );

        // And no number of presses gets the old creature back.
        for _ in 0..10 {
            assert!(
                history.undo(&second, Some(0)).is_none(),
                "undo must not produce a step from another document's history"
            );
            assert!(history.redo(&second, Some(0)).is_none());
        }
        assert_eq!(second.len(), 6, "the loaded document is untouched");
    }

    #[test]
    fn clear_drops_the_redo_side_too() {
        let document = chain(4);
        let mut history = CreatorHistory::default();
        history.record(&document);
        let step = history.undo(&document, None).expect("undo");
        assert_depths(&history, 0, 1);

        history.clear();
        assert_depths(&history, 0, 0);
        assert!(history.redo(step.document(), None).is_none());
    }

    // -- a whole session, and the pairing check ------------------------------

    #[test]
    fn a_full_authoring_session_undoes_and_redoes_to_the_same_document() {
        let original = chain(4);
        let mut document = original.clone();
        let mut history = CreatorHistory::default();

        history.record(&document);
        let upper = document.add_block(leaf(0xaa), Some(0)).expect("added");

        history.record(&document);
        let lower = document.add_block(leaf(0xbb), Some(upper)).expect("added");

        history.record(&document);
        document
            .replace_block(lower, BlockId::new(0x4062_6000, 0xcccc_dddd))
            .expect("replaced");

        history.record(&document);
        document
            .set_block_transform(
                lower,
                BlockTransform {
                    position: crate::document::Vec3 {
                        x: 0.5,
                        y: -0.25,
                        z: 2.0,
                    },
                    scale: 1.5,
                    orientation: Matrix3::IDENTITY,
                    triangle_direction: None,
                    triangle_pick_origin: None,
                },
            )
            .expect("transform written");

        history.record(&document);
        let before_removal = document.clone();
        let remap = document
            .remove_block(lower, OnLastChild::OmitElement)
            .expect("removed");
        history.record_removal(&before_removal, &remap);

        let final_state = document.clone();
        assert_depths(&history, 6, 0);
        let errors: Vec<_> = validate(&document, None)
            .into_iter()
            .filter(|issue| issue.severity == Severity::Error)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");

        // Undo the whole session, one step at a time, holding a selection that
        // is remapped at every single one. Every intermediate state has to be a
        // document the editor could actually be showing.
        let mut held = upper;
        for expected in (0..6).rev() {
            let selection = held.min(document.len().saturating_sub(1));
            let step = history
                .undo(&document, Some(selection))
                .unwrap_or_else(|| panic!("step {expected} must be undoable"));
            if let Some(SelectionStep::Kept(index)) = step.selection() {
                held = index;
            }
            document = step.into_document();
            let errors: Vec<_> = validate(&document, None)
                .into_iter()
                .filter(|issue| issue.severity == Severity::Error)
                .collect();
            assert!(errors.is_empty(), "step {expected}: {errors:?}");
        }
        assert_eq!(document, original, "the session unwound completely");
        assert_eq!(
            held, 4,
            "the selection came back to the block it started on"
        );
        assert_depths(&history, 0, 6);
        assert!(history.undo(&document, Some(held)).is_none());

        // And forward again, to exactly the state the session ended in.
        for _ in 0..6 {
            let step = history.redo(&document, Some(held)).expect("redo");
            if let Some(SelectionStep::Kept(index)) = step.selection() {
                held = index;
            }
            document = step.into_document();
        }
        assert_eq!(document, final_state, "and replayed completely");
        assert_depths(&history, 6, 0);
        assert!(history.redo(&document, Some(held)).is_none());
    }

    /// The one obligation `record_removal` cannot express in its signature: the
    /// document and the remap must describe the same pre-removal state.
    ///
    /// `debug_assert` is the weight this crate gives an internal mismatch — the
    /// same choice `persist`'s encoder makes for an unbalanced container, and for
    /// the same reason: `#![forbid(unsafe_code)]`, hand-checked, and a mistake
    /// surfaces as a wrong document rather than as memory trouble. Without the
    /// assertion this test fails by not panicking, which is the whole point of
    /// writing it.
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "IndexRemap::len() is the pre-removal block count")]
    fn pairing_a_removal_with_a_different_document_is_caught() {
        let short = chain(3);
        let mut long = chain(5);
        let remap = long
            .remove_block(0, OnLastChild::OmitElement)
            .expect("removed");
        let mut history = CreatorHistory::default();
        // `short` was never the document `remap` came from.
        history.record_removal(&short, &remap);
    }
}
