//! Failures of the composition step itself.
//!
//! Note what is *not* here: an unresolvable block. 233 of 1549 blocks do not
//! resolve, and that is reported as a [`crate::UnresolvedPart`], not as an error.
//! The errors below are for a request that could not be attempted at all.

use thiserror::Error;

use spore_rigblock::RigblockError;

/// The result of a composition operation.
pub type Result<T> = core::result::Result<T, MorphologyError>;

/// Why a composition could not be attempted.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MorphologyError {
    /// The document has no blocks, so there is nothing to compose.
    #[error("the document has no blocks: there is no creature to resolve")]
    NoBlocksToResolve,
}

/// A part-layer decode failure, surfaced when a caller asks for the low-level
/// decode rather than for the resolved part.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PartDecodeError {
    /// A rigblock record would not decode.
    #[error(transparent)]
    Rigblock(#[from] RigblockError),
}
