//! Failures this crate can report.
//!
//! Two error types, and the split is deliberate: [`CreatorError`] is about
//! **reading a record** and [`EditError`] is about **changing a document**. A
//! decode failure is a finding about bytes on disk; an edit refusal is a finding
//! about what the caller asked for. Folding them together would let a refusal
//! about "there is no block 99 of 3" wear the same `Display` as "this is not a
//! `<sporemodel>`", and the two are answered by completely different people.
//!
//! Every variant names *what could not be read*, and the byte offset or element
//! where it went wrong. A creator document that cannot be decoded is a finding
//! about a record, not a reason to substitute a plausible default.

use thiserror::Error;

use crate::document::BlockIndex;

/// The result of a creator-core decode operation.
pub type Result<T> = core::result::Result<T, CreatorError>;

/// The result of a creator-core authoring operation.
///
/// Separate from [`Result`] on purpose: an edit refusal is **not** a decode
/// failure and must never be reported as one.
pub type EditResult<T> = core::result::Result<T, EditError>;

/// Why a creator document, or a field in one, could not be read.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CreatorError {
    /// The bytes are not the XML subset this crate reads.
    #[error("xml: {reason} at byte {offset}")]
    Xml {
        /// What the parser expected.
        reason: String,
        /// Byte offset into the record.
        offset: usize,
    },

    /// The document is not a `<sporemodel>`.
    #[error("not a creator document: root element is <{found}>, expected <sporemodel>")]
    NotACreatorDocument {
        /// The root element name that was found.
        found: String,
    },

    /// A required element was missing.
    #[error("<blocks> is missing: a creator document has no parts to place")]
    MissingBlocks,

    /// A numeric field would not parse.
    #[error("<{element}> at block {block}: {value:?} is not a number: {reason}")]
    NotANumber {
        /// The element name.
        element: String,
        /// Which block it belongs to, when it is inside one.
        block: usize,
        /// The text that failed.
        value: String,
        /// Why it failed.
        reason: String,
    },

    /// A `<blockid>` did not have the shape `group, instance`.
    #[error("<blockid> at block {block}: expected `group, instance`, got {value:?}")]
    MalformedBlockId {
        /// Which block.
        block: usize,
        /// The text as written.
        value: String,
    },

    /// A tuple had the wrong number of components.
    #[error("<{element}> at block {block}: expected {expected} numbers, got {got}")]
    WrongArity {
        /// The element name.
        element: String,
        /// Which block.
        block: usize,
        /// How many components the corpus writes.
        expected: usize,
        /// How many were there.
        got: usize,
    },

    /// A `count` attribute disagreed with the number of children present.
    #[error("<{element}> at block {block}: count attribute says {declared}, but {actual} children are present")]
    CountMismatch {
        /// The element carrying the attribute.
        element: String,
        /// Which block, when applicable.
        block: usize,
        /// What the attribute said.
        declared: usize,
        /// What is actually there.
        actual: usize,
    },

    /// An element the grammar does not have.
    ///
    /// The decoder is **fail-closed**: an unknown element is an error, never a
    /// skipped child. A future corpus version adding a field must be a
    /// deliberate decision here, not a silent loss.
    #[error("unexpected element <{element}> at block {block}")]
    UnexpectedElement {
        /// The element name.
        element: String,
        /// Which block, when applicable.
        block: usize,
    },

    /// An attribute the grammar does not have.
    #[error("unexpected attribute {attribute} on <{element}> at block {block}")]
    UnexpectedAttribute {
        /// The element name.
        element: String,
        /// The attribute name.
        attribute: String,
        /// Which block, when applicable.
        block: usize,
    },
}

/// Why an authoring operation was refused and the document left untouched.
///
/// Every one of these is a **refusal, not a repair**: the document is exactly as
/// it was handed in. An editing API that quietly fixed a bad request would make
/// "what did the tool actually do" unanswerable, which is the one question an
/// undo stack and a corpus test both need answered.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum EditError {
    /// The named block does not exist.
    ///
    /// Carries the document's block count so a caller can tell "you are past the
    /// end" from "this document has no blocks", which a bare index cannot.
    #[error("no block {index}: the document places {len} block(s)")]
    NoSuchBlock {
        /// The index that was named.
        index: BlockIndex,
        /// How many blocks the document actually places.
        len: usize,
    },

    /// The block being added carries no `<blockid>`.
    ///
    /// 680 of 680 corpus blocks write one, so an unidentifiable block is a
    /// shape this format has never shown. It is refused rather than stored, so
    /// that "every block names a part" stays true of every document we produce.
    #[error("the block being added carries no <blockid>: an unplaced part cannot be identified")]
    MissingBlockIdentity,

    /// The block being added already carries a `<childlist>`.
    ///
    /// Its children were chosen *before* the append knew what index this block
    /// would get, so those numbers cannot be trusted to mean anything. The
    /// caller adds the subtree bottom-up through `add_block` instead.
    #[error("the block being added already lists {children} child(ren): add the subtree bottom-up so each <childid> is known when it is written")]
    NewBlockCarriesChildren {
        /// How many children the incoming block listed.
        children: usize,
    },

    /// An index-valued field on an incoming block names nothing.
    ///
    /// Only `symmetric` can do this at insertion, and only because the new block
    /// lands at the end: a partner index at or past the new index is the block
    /// itself or past the end. Reciprocity is **not** checked here — that is
    /// [`mod@crate::validate`]'s warning, and a second opinion would be a second
    /// source of truth.
    #[error("<symmetric> names block {partner}, but the document will have {len} block(s) when the new block lands at {len}")]
    DanglingSymmetry {
        /// The partner index that names nothing.
        partner: BlockIndex,
        /// How many blocks the document has, which is also the new block's index.
        len: usize,
    },

    /// The reparent would put a block inside its own subtree.
    ///
    /// 0 corpus cycles and max in-degree 1 mean the forest claim rests on
    /// refusing this, so it is refused rather than detected later.
    #[error("block {block} cannot take {would_be_parent} as a parent: that would make it its own ancestor")]
    Cycle {
        /// The block being moved.
        block: BlockIndex,
        /// The parent it was refused.
        would_be_parent: BlockIndex,
    },

    /// Ancestry could not be established because the document already has a
    /// cycle on the path being walked.
    ///
    /// The op refuses instead of guessing: an unprovable-safe reparent is a
    /// cycle we did not ask for. `validate` reports the pre-existing one.
    #[error("block {block}: the ancestor chain to the requested parent already cycles, so a safe reparent cannot be established")]
    CyclicAncestry {
        /// The block being moved.
        block: BlockIndex,
    },

    /// A caller-held target list still names the block that was removed.
    ///
    /// Returned by the index remap's target rewriter, which is **all-or-nothing**:
    /// a partially rewritten selection is a selection pointing at the wrong part.
    #[error("target block {index} was removed: re-read the remap, there is no index that names it any more")]
    RemovedTarget {
        /// The stale index, in the pre-removal index space.
        index: BlockIndex,
    },
}
