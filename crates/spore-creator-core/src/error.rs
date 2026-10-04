//! Failures this crate can report.
//!
//! Every variant names *what could not be read*, and the byte offset or element
//! where it went wrong. A creator document that cannot be decoded is a finding
//! about a record, not a reason to substitute a plausible default.

use thiserror::Error;

/// The result of a creator-core operation.
pub type Result<T> = core::result::Result<T, CreatorError>;

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
