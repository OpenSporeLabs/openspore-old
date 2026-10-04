//! Failures this crate can report.
//!
//! Every variant says what could not be read and what the corpus says should
//! have been there. A body graph that decodes into an impossible shape is worse
//! than one that refuses to decode, because the first produces a creature and
//! the second produces a finding.

use thiserror::Error;

/// The result of a rigblock-layer operation.
pub type Result<T> = core::result::Result<T, RigblockError>;

/// Why a part-layer record could not be read.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RigblockError {
    /// A record is shorter than the structure it claims to hold.
    #[error("rigblock: need at least {need} bytes, have {have}")]
    Truncated {
        /// Bytes needed.
        need: usize,
        /// Bytes available.
        have: usize,
    },

    /// The record size is not `36 + 12·4^k`.
    #[error("rigblock: size {size} matches no `36 + 12*4^k` for k in 0..=7")]
    BadSize {
        /// The observed size.
        size: usize,
    },

    /// The per-level resolution bytes are not `[4]*k ++ [0]*(8-k)`.
    #[error("rigblock: level byte {index} is {found}, expected {expected}")]
    BadLevels {
        /// Which of the eight bytes.
        index: usize,
        /// What was there.
        found: u8,
        /// What the level count requires.
        expected: u8,
    },

    /// A skeleton payload ended before its declared bone count was readable.
    #[error("skeleton: need at least {need} bytes for its bones, have {have}")]
    SkeletonTruncated {
        /// Bytes needed.
        need: usize,
        /// Bytes available.
        have: usize,
    },

    /// The two bone-count words in a skeleton header disagree.
    #[error("skeleton: header declares {declared} bones but the count word says {bone_count}")]
    SkeletonHeaderMismatch {
        /// `N` from `d+0x0C`.
        bone_count: usize,
        /// The word at `d+0x14`.
        declared: u32,
    },

    /// A skeleton claims zero bones.
    ///
    /// Refused rather than returned as an empty graph: an empty skeleton and a
    /// decoded-but-empty one are different claims, and only the first is in the
    /// corpus.
    #[error("skeleton: declares zero bones")]
    SkeletonEmpty,

    /// A self-relative array pointer does not address its array.
    #[error("skeleton: the pointer to array {array} is {recorded:#x}, expected {expected:#x}")]
    SkeletonPointerMismatch {
        /// Which array, 1-based.
        array: usize,
        /// What the header said.
        recorded: usize,
        /// Where the array actually is.
        expected: usize,
    },

    /// A parent index is outside `[-1, N-1]`.
    #[error("skeleton: bone {bone} names parent {parent}, outside a graph of {bone_count}")]
    ParentOutOfRange {
        /// The bone carrying the bad index.
        bone: usize,
        /// The value found.
        parent: i32,
        /// How many bones there are.
        bone_count: usize,
    },

    /// A skeleton does not have exactly one root.
    #[error("skeleton: expected exactly one root, found {found}")]
    RootCount {
        /// How many bones claimed to be roots.
        found: usize,
    },

    /// A parent chain does not terminate.
    #[error("skeleton: the parent chain through bone {bone} does not reach a root")]
    CycleThrough {
        /// A bone on the non-terminating chain.
        bone: usize,
    },

    /// A vocabulary lookup was asked for something that cannot be looked up.
    #[error("vocabulary: {0}")]
    Vocabulary(String),
}
