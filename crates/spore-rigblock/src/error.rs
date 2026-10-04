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

    /// A `gmsh` recipe declares a zero-length block chain.
    ///
    /// Every observed recipe declares 3 to 35 blocks, so zero is not a shape this
    /// format has.
    #[error("gmsh: recipe {recipe:#010x} declares no attribute blocks")]
    GmshNoBlocks {
        /// The recipe word.
        recipe: u32,
    },

    /// A `gmsh` header's two constant bytes at `0x16`/`0x17` are not `0x00`/`0x0C`.
    ///
    /// `0x14` is not one integer: read little-endian it looks like an opaque
    /// `0x0c00xxxx`, and read as `u16` big-endian plus those two bytes it is the
    /// vertex count. A record where the bytes differ is a different shape.
    #[error("gmsh: header bytes 0x16/0x17 are {byte_16:#04x}/{byte_17:#04x}, expected 0x00/0x0c")]
    GmshHeaderShape {
        /// Byte at `0x16`.
        byte_16: u8,
        /// Byte at `0x17`.
        byte_17: u8,
    },

    /// A `gmsh` block header would read past the record.
    #[error("gmsh: the block starting at {at} overruns the record")]
    GmshBlockOverrun {
        /// Byte offset of the block header.
        at: usize,
    },

    /// A `gmsh` block declared an element size of zero.
    #[error("gmsh: the block starting at {at} declares a zero element size")]
    GmshZeroElementSize {
        /// Byte offset of the block header.
        at: usize,
    },

    /// A `gmsh` record did not hold exactly one index block.
    #[error("gmsh: expected exactly one triangle index block, found {found}")]
    GmshIndexBlockCount {
        /// How many were found.
        found: usize,
    },

    /// The `gmsh` index block's element size was not 2.
    #[error("gmsh: the index block's element size is {found}, expected 2")]
    GmshIndexElementSize {
        /// What was found.
        found: u16,
    },

    /// The `gmsh` index count is not a multiple of three.
    #[error("gmsh: {count} indices is not a whole number of triangles")]
    GmshIndexCountNotTriangles {
        /// The index count.
        count: usize,
    },

    /// A `gmsh` index pointed at or past the vertex count.
    #[error("gmsh: index {index} is outside the {vertex_count} vertices")]
    GmshIndexOutOfRange {
        /// The offending index.
        index: u16,
        /// How many vertices there are.
        vertex_count: usize,
    },

    /// The `gmsh` bone-range table does not line up with the index count.
    #[error("gmsh: {ranges} bone ranges for {bones} bones")]
    GmshBoneRangeCount {
        /// Ranges found.
        ranges: usize,
        /// Bones declared.
        bones: u32,
    },

    /// A `gmsh` bone range did not start where the previous one ended.
    ///
    /// A gap means a triangle no bone owns, which would be drawn by nothing.
    #[error("gmsh: a bone range starts at {got} but the previous one ended at {expected}")]
    GmshBoneRangeGap {
        /// Where the tiling should have continued.
        expected: u32,
        /// Where it actually started.
        got: u32,
    },

    /// A `gmsh` bone range ran backwards.
    #[error("gmsh: bone {bone}'s range runs backwards, start {start} end {end}")]
    GmshBoneRangeInverted {
        /// Which bone.
        bone: u32,
        /// Its start.
        start: u32,
        /// Its end.
        end: u32,
    },

    /// A `gmsh` bone-range entry's `bone` word is not the entry's own index.
    #[error("gmsh: bone-range entry {entry} carries bone {bone}")]
    GmshBoneOrdinal {
        /// The entry position.
        entry: usize,
        /// The bone word it carried.
        bone: u32,
    },

    /// The `gmsh` bone ranges did not cover the whole index buffer.
    #[error("gmsh: bone ranges cover {covered} of {total} indices")]
    GmshBoneRangeShortfall {
        /// Indices covered.
        covered: u32,
        /// Indices there are.
        total: u32,
    },

    /// A `gmsh` bone-range entry had a non-zero pad word.
    #[error("gmsh: the bone-range entry for bone {bone} has a non-zero pad")]
    GmshBoneRangePad {
        /// Which bone.
        bone: usize,
    },

    /// The `gmsh` tail's vertex count disagreed with the header's.
    #[error("gmsh: the tail says {declared} vertices, the header says {actual}")]
    GmshTailVertexMismatch {
        /// What the tail said.
        declared: usize,
        /// What the header said.
        actual: usize,
    },

    /// A `gmsh` tail byte was not the constant the measured layout requires.
    #[error("gmsh: tail byte {at} is {found:#04x}, which the layout does not allow")]
    GmshTailShape {
        /// The byte offset within the tail.
        at: usize,
        /// What was there.
        found: u8,
    },

    /// A `gmsh` tail ordinal entry was not the measured shape.
    #[error("gmsh: the tail ordinal entry {index} at byte {at} does not match `u8 index, u16 FFFF, u8 0`")]
    GmshTailOrdinalShape {
        /// Which ordinal.
        index: usize,
        /// The byte offset within the tail.
        at: usize,
    },
}
