//! Failures this crate can report.
//!
//! Three error types, and the split is deliberate: [`CreatorError`] is about
//! **reading a record**, [`EditError`] is about **changing a document**, and
//! [`PersistError`] is about **reading or writing a canonical save** of one. A
//! decode failure is a finding about bytes on disk; an edit refusal is a finding
//! about what the caller asked for; a persistence failure is a finding about a
//! file this format claims to have written. Folding them together would let a
//! refusal about "there is no block 99 of 3" wear the same `Display` as "this is
//! not a `<sporemodel>`", and the two are answered by completely different
//! people.
//!
//! Every variant names *what could not be read*, and the byte offset or element
//! where it went wrong. A creator document that cannot be decoded is a finding
//! about a record, not a reason to substitute a plausible default.
//!
//! [`PersistError`] is the same discipline pointed at a different target: **every
//! variant carries the 1-based `line` it failed on**, because a JSON Lines file
//! is read one line at a time and "the file is malformed" is not an answer a
//! caller can act on.

use core::fmt;

use thiserror::Error;

use crate::document::BlockIndex;

/// The result of a creator-core decode operation.
pub type Result<T> = core::result::Result<T, CreatorError>;

/// The result of a creator-core authoring operation.
///
/// Separate from [`Result`] on purpose: an edit refusal is **not** a decode
/// failure and must never be reported as one.
pub type EditResult<T> = core::result::Result<T, EditError>;

/// The result of a creator-core canonical-save read or write.
///
/// Separate from both other result types for the same reason: a persistence
/// refusal is a finding about a *file*, not about a record or a request.
pub type PersistResult<T> = core::result::Result<T, PersistError>;

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

/// Where in a canonical save something went wrong.
///
/// One type rather than a `line` on every variant, because it has to appear in
/// every message and a caller reporting a bad file wants to print it as one
/// thing. `line` is **1-based**, so it is the number an editor shows; `block` is
/// the index the offending line declares and is `None` for the header and for
/// document-level fields.
///
/// [`fmt::Display`] renders `None` as nothing at all: a document-level failure
/// reads `line 1: ...`, and a block-level one reads `line 41, block 7: ...`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistSite {
    /// 1-based line number in the file.
    pub line: usize,
    /// The block index the offending line declares, when it is inside one.
    pub block: Option<BlockIndex>,
}

impl PersistSite {
    /// A site on the header, or on any document-level field of any line.
    pub const fn document(line: usize) -> Self {
        Self { line, block: None }
    }

    /// A site inside the block line at `line`, which declares block `block`.
    pub const fn block(line: usize, block: BlockIndex) -> Self {
        Self {
            line,
            block: Some(block),
        }
    }
}

impl fmt::Display for PersistSite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}", self.line)?;
        if let Some(block) = self.block {
            write!(f, ", block {block}")?;
        }
        Ok(())
    }
}

/// Why a canonical save could not be written, or could not be read back.
///
/// **Every variant carries the site** — line, and block where applicable — because
/// a JSON Lines file is read one line at a time and is meant to be diffed, so
/// "line 41, block 7 has a field this build does not know" is an answer and "the
/// file is malformed" is not.
///
/// The split from [`CreatorError`] is deliberate. A `<sporemodel>` record that
/// will not parse is a finding about Spore's bytes; a `spore-creator-doc/1` file
/// that will not parse is a finding about **our own output**, and the two are
/// answered by different people. Reporting them as one type would let a
/// hand-corrupted save file be filed as a corpus problem.
///
/// # None of these is a repair
///
/// Every variant is a refusal. Nothing here fills an absent field, drops a
/// block, clamps an index or defaults a float: a document that cannot be read
/// back unchanged is a document this format cannot vouch for, and substituting a
/// plausible one would make `load(save(doc)) == doc` hold for the wrong reason.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PersistError {
    /// The line is not one well-formed JSON value, or has content after it.
    #[error("{site}: not well-formed JSON: {reason}")]
    Syntax {
        /// Where the line went wrong.
        site: PersistSite,
        /// What the grammar expected.
        reason: String,
    },

    /// Line 1 is not a `spore-creator-doc/1` metadata record.
    ///
    /// Also the error for a header whose `record`, `schema` or evidence grade is
    /// not what this build writes. A file that calls itself a different schema is
    /// a different format; one that calls itself this schema with a *stronger*
    /// grade than [`crate::document::DOCUMENT_EVIDENCE`] is making a claim the
    /// corpus does not support.
    #[error("{site}: not a spore-creator-doc/1 metadata record: {reason}")]
    Schema {
        /// Where the header went wrong.
        site: PersistSite,
        /// What the header said.
        reason: String,
    },

    /// The header declares a float encoding this build does not read.
    ///
    /// The encoding is named in the file precisely so a future revision cannot be
    /// misread by an older reader: the safe answer to an unknown encoding is a
    /// refusal, never a best guess at the digits.
    #[error("{site}: this file declares float encoding {found:?}, which this build does not read")]
    UnsupportedFloatEncoding {
        /// Where the header went wrong.
        site: PersistSite,
        /// The encoding the header named.
        found: String,
    },

    /// A field the grammar does not have.
    ///
    /// **Fail-closed**, exactly like [`CreatorError::UnexpectedElement`]: a field
    /// added by a future revision has to be adopted deliberately, in a diff a
    /// reader can see. Dropping it would make a save file that loses data look
    /// like a save file that works.
    #[error("{site}: field {field:?} is not part of spore-creator-doc/1")]
    UnexpectedField {
        /// Where the field was found.
        site: PersistSite,
        /// The field name.
        field: String,
    },

    /// A field the grammar requires is absent.
    ///
    /// "Absent is not zero" cuts both ways: a field that every save writes and
    /// that is missing from this line is a truncated or hand-edited file, and
    /// reading it as a default would invent a value.
    #[error("{site}: field {field:?} is required by spore-creator-doc/1")]
    MissingField {
        /// Where the field should have been.
        site: PersistSite,
        /// The field name.
        field: String,
    },

    /// A value is not of the type its field requires.
    #[error("{site}: field {field:?} is a {found}, expected {expected}")]
    WrongType {
        /// Where the value was found.
        site: PersistSite,
        /// The dotted field path, e.g. `transform.orientation.row1`.
        field: String,
        /// What the field requires.
        expected: String,
        /// What the value actually is.
        found: String,
    },

    /// A tuple-valued field — a vector, a matrix row — has the wrong length.
    #[error("{site}: field {field:?} holds {got} component(s), expected exactly {expected}")]
    WrongArity {
        /// Where the value was found.
        site: PersistSite,
        /// The dotted field path.
        field: String,
        /// How many components the type has.
        expected: usize,
        /// How many were written.
        got: usize,
    },

    /// A `u32` word is not a 32-bit id in either radix.
    #[error("{site}: field {field:?}: {value:?} is not a 32-bit id")]
    NotAnId {
        /// Where the value was found.
        site: PersistSite,
        /// The dotted field path.
        field: String,
        /// The text that failed.
        value: String,
    },

    /// A positional index is not a decimal `usize`.
    ///
    /// Its own variant because a [`BlockIndex`] is a **position**, not an id: it is
    /// written in decimal, and reading a hexadecimal number here would be a
    /// silent re-addressing of a document.
    #[error("{site}: field {field:?}: {value:?} is not a decimal block index")]
    NotAnIndex {
        /// Where the value was found.
        site: PersistSite,
        /// The dotted field path.
        field: String,
        /// The text that failed.
        value: String,
    },

    /// A float does not read back as the `f32` it was written from.
    ///
    /// The load-bearing case is `"1e40"`: RFC 8259 says that is a number, and
    /// `"1e40".parse::<f32>()` is `Ok(f32::INFINITY)`. A loader that trusted the
    /// parse would hand back a document with an infinite transform and no error.
    /// Every writer in this crate emits a shortest-round-trip decimal, so this can
    /// only fire on a hand-edited or foreign file — which is exactly when it must.
    #[error("{site}: field {field:?}: {value:?} does not read back as the same f32")]
    FloatNotExact {
        /// Where the value was found.
        site: PersistSite,
        /// The dotted field path.
        field: String,
        /// The literal as written.
        value: String,
    },

    /// A colour component is outside `0.0 ..= 1.0`.
    ///
    /// Its own variant because it is a **different finding** from a float that does
    /// not read back: the value read back exactly and is still not a colour this
    /// build will draw. [`crate::color::UnitRgb::new`] refuses rather than clamps,
    /// for the reason the corpus grammar refuses too — a component outside the unit
    /// range is data this corpus has never contained, and clamping it would turn
    /// that into a colour.
    ///
    /// Reported rather than corrected because the alternative is worse: a save file
    /// that read back with a clamped colour would load, and
    /// `UnitRgb::to_corpus_text` would then emit a spelling that `UnitRgb::parse`
    /// refuses. The two directions of this crate's own grammar have to agree.
    #[error("{site}: field {field:?} is {value:?}, which is outside 0.0 ..= 1.0")]
    ColourOutOfRange {
        /// Where the value was found.
        site: PersistSite,
        /// The dotted field path.
        field: String,
        /// The triple as written.
        value: String,
    },

    /// The header's own integrity counts disagree with the body they head.
    #[error("{site}: the header says {field} is {declared}, but the file has {found}")]
    HeaderMismatch {
        /// Where the header went wrong.
        site: PersistSite,
        /// Which integrity fact disagreed.
        field: String,
        /// What the header claimed.
        declared: usize,
        /// What the body actually holds.
        found: usize,
    },

    /// A block line's `index` is not the position that line occupies.
    ///
    /// Reordering is not a valid edit (`docs/analysis/creator/05-serialization.md`
    /// F14), and `index` is written so this is checkable rather than assumed: a
    /// reordered file is still valid JSON, so nothing else would notice.
    #[error("{site}: the block declares index {declared}, but it is content line {found}")]
    BlockOrder {
        /// Where the line went wrong.
        site: PersistSite,
        /// What the block's own `index` field said.
        declared: BlockIndex,
        /// Which content line it actually is, counted from 0.
        found: BlockIndex,
    },

    /// `content_sha256` does not cover these content lines.
    ///
    /// Checked **before** the body is parsed, so a corrupted file reports the
    /// corruption instead of whichever field the corruption happened to land in.
    #[error("{site}: content_sha256 is {declared:?}, but the content lines hash to {computed:?}")]
    Checksum {
        /// Where the header went wrong.
        site: PersistSite,
        /// What the header declared.
        declared: String,
        /// What the content lines actually hash to.
        computed: String,
    },
}
