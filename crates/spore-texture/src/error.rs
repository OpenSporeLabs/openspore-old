//! Typed failures of the raster walk.
//!
//! One variant per failure mode, on purpose. "The texture did not load" throws
//! away the only thing a caller can act on: a record truncated mid-envelope, a
//! record whose `fourcc` is a format this crate deliberately does not decode, a
//! record whose length is not the one its own header implies, and a record whose
//! length prefix disagrees with that prefix's geometry are four different
//! problems with four different responses (skip the asset, extend the codec, fix
//! the framing, fix the writer). Each variant therefore names the section and
//! carries the numbers needed to identify the record in a report.
//!
//! Nothing here is ever inferred: no variant reports a value this build guessed.

use thiserror::Error;

use crate::{DXT1_FOURCC, DXT5_FOURCC, ENVELOPE_SIZE, LUMINANCE_FOURCC};

/// Every way decoding a Spore raster texture record can fail.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TextureError {
    // ---- envelope ------------------------------------------------------
    /// Fewer than [`ENVELOPE_SIZE`] bytes: not even the envelope fits.
    ///
    /// A raster record is never empty and is never shorter than its own
    /// header, so there is no "zero-length means default" reading to fall back
    /// to.
    #[error("texture: {available} bytes < the {ENVELOPE_SIZE}-byte raster envelope")]
    TruncatedEnvelope {
        /// Bytes actually available.
        available: usize,
    },

    /// The envelope's `fourcc` names a format this build has no framing for.
    ///
    /// **This build decodes three formats**: `0x35545844` `'DXT5'` as BC3,
    /// `0x31545844` `'DXT1'` as BC1, and `0x00000015`, the uncompressed
    /// four-lane normal-map family. The message names all three so a refusal
    /// says what was available. Note that a `fourcc` with no framing is an
    /// **absence**, not a verdict on the record.
    #[error(
        "texture: unsupported fourcc 0x{fourcc:08X} (this build decodes 0x{DXT5_FOURCC:08X} \
         = 'DXT5' as BC3, 0x{DXT1_FOURCC:08X} = 'DXT1' as BC1, and 0x{LUMINANCE_FOURCC:08X}, \
         the uncompressed normal-map family; this fourcc has no measured framing rule)"
    )]
    UnsupportedFourcc {
        /// The `fourcc` word as read from the envelope.
        fourcc: u32,
    },

    /// A known `fourcc` arrived with a block size this build has no codec for.
    ///
    /// **Unreachable**: [`crate::framing::FrameFormat`] has exactly three
    /// entries, with block sizes 16, 8 and 0. The arm exists so that adding a
    /// fourth format is a compile error in the walk rather than a silent decode
    /// with the wrong codec.
    #[error("texture: fourcc 0x{fourcc:08X} needs a {block_bytes}-byte block, which this build has no codec for")]
    UnsupportedBlockSize {
        /// The `fourcc` word as read from the envelope.
        fourcc: u32,
        /// Block size the frame asked for.
        block_bytes: usize,
    },

    /// `width` or `height` is zero.
    ///
    /// No observed record does this. The Python oracle does not check it either
    /// and would return an empty image for such a record; the C++ reference
    /// rejects it and so does this port, because an image with no texels has no
    /// meaning to propagate to a renderer.
    #[error("texture: zero-sized image (width={width}, height={height})")]
    ZeroDimension {
        /// Envelope width.
        width: u32,
        /// Envelope height.
        height: u32,
    },

    /// `mip_count` is zero, so the record has no base level.
    ///
    /// A chain of zero levels has a zero-length payload, which would make the
    /// framing identity vacuously true and quietly accept a record whose real
    /// content is something else.
    #[error("texture: mipCount is 0 (a raster record always has a base level)")]
    ZeroMipCount,

    /// The record handed to [`crate::layer_count_for`] is smaller than the
    /// envelope, so there is nothing to frame.
    #[error(
        "texture: record of {record_size} bytes is smaller than the {ENVELOPE_SIZE}-byte envelope"
    )]
    RecordShorterThanEnvelope {
        /// The record size that was offered.
        record_size: usize,
    },

    /// The mip chain of one record does not fit in an addressable `usize`.
    ///
    /// Portability note: with `u32` arguments this arm is **unreachable on a
    /// 64-bit target**, because the largest chain the sizing rules can produce
    /// is comfortably below `usize::MAX`; such an envelope is refused one line
    /// later by [`TextureError::RecordSizeMismatch`], naming the impossible size.
    /// On a 32-bit target the same call *does* saturate, because every one of
    /// those level terms is larger than a `u32`. The arm is kept for that target
    /// and as the bound on the saturating arithmetic, exactly as the C++
    /// reference keeps its own `SIZE_MAX` check.
    #[error(
        "texture: mip chain exceeds the addressable size \
         (width={width}, height={height}, mip_count={mip_count})"
    )]
    ChainSizeOverflow {
        /// Envelope width.
        width: u32,
        /// Envelope height.
        height: u32,
        /// Envelope mip count.
        mip_count: u32,
    },

    // ---- framing -------------------------------------------------------
    /// The record is not exactly as long as its own envelope implies.
    ///
    /// Unlike the old layer split — which divided a payload and refused the
    /// remainder — the whole chain length here is *determined*: header, base
    /// level, then a `u32` count and that many bytes for every remaining level.
    /// A record whose size differs is a record whose framing we do not
    /// understand, so it is refused with both numbers rather than decoded on a
    /// hunch.
    ///
    /// Establishes that the identity is exact, not approximate: over all 2954
    /// raster records of `Spore_Content.package` and all 1432 of
    /// `Spore_EP1_Content_01.package` the two agree to the byte.
    #[error(
        "texture: record is {record_size} bytes but fourcc 0x{fourcc:08X} at \
         {width}x{height} with {mip_count} level(s) implies {expected}"
    )]
    RecordSizeMismatch {
        /// Bytes actually present.
        record_size: usize,
        /// Bytes the envelope's own geometry requires.
        expected: usize,
        /// Envelope width.
        width: u32,
        /// Envelope height.
        height: u32,
        /// Envelope mip count.
        mip_count: u32,
        /// The envelope's format word.
        fourcc: u32,
    },

    /// `field_18` is not the base level's byte count.
    ///
    /// This is the header field the decoder *does* read, after a byte count over
    /// the corpus established what it means. A disagreement is a record whose
    /// header and geometry contradict each other, which is a refusal rather
    /// than a preference for either side.
    #[error(
        "texture: field_18 declares {declared} bytes but fourcc 0x{fourcc:08X} at {width}x{height} \
         has a {expected}-byte base level"
    )]
    Field18Mismatch {
        /// What the header word says.
        declared: u32,
        /// What the geometry requires.
        expected: usize,
        /// Envelope width.
        width: u32,
        /// Envelope height.
        height: u32,
        /// The envelope's format word.
        fourcc: u32,
    },

    /// A level's little-endian length prefix disagrees with that level's geometry.
    ///
    /// The prefix is the only per-level length the record carries, and it exists
    /// precisely so a reader does not have to derive it. A prefix that is not the
    /// level's own byte count means the record is not laid out the way
    /// [`crate::framing`] describes, and guessing past it would splice two
    /// different records together.
    #[error(
        "texture: level {mip} declares {declared} bytes at offset {offset} but its \
         {width}x{height} geometry needs {expected}"
    )]
    LevelLengthPrefix {
        /// Level the prefix belongs to.
        mip: u32,
        /// Byte count the record declares.
        declared: usize,
        /// Byte count the level's dimensions require.
        expected: usize,
        /// Offset the prefix was read from.
        offset: usize,
        /// Level width.
        width: u32,
        /// Level height.
        height: u32,
    },

    /// A level's length prefix, or its payload, runs past the end of the record.
    #[error(
        "texture: level {mip} needs {size} bytes at offset {offset}, \
         but the record is {record_size} bytes"
    )]
    LevelSlicePastRecord {
        /// Level the slice belongs to.
        mip: u32,
        /// Offset the slice starts at.
        offset: usize,
        /// Bytes the slice needs.
        size: usize,
        /// Bytes in the record.
        record_size: usize,
    },

    /// A payload smaller than the base level needs.
    ///
    /// `available` is `record_size - header`; `needed` is the base level's own
    /// byte count.
    #[error("texture: {available} payload bytes < {needed} for the base level")]
    PayloadTooSmall {
        /// Payload bytes present after the header.
        available: usize,
        /// Bytes the base level occupies.
        needed: usize,
    },

    /// The payload does not divide evenly by the old per-layer stride.
    ///
    /// **Refuted by the corpus, kept so a caller written against it still
    /// compiles.** There is no layer stride: the chain length is determined, not
    /// divided. This arm is unreachable from [`crate::decode_raster`] and
    /// [`crate::layer_count_for`], which use
    /// [`TextureError::RecordSizeMismatch`] instead. It is why 373 records used
    /// to be refused, so it is kept with that history attached rather than
    /// deleted.
    #[error("texture: {available} payload bytes is not a multiple of the {per}-byte layer stride (refuted framing; no record has been refused with this)")]
    PayloadNotMultiple {
        /// Payload bytes present after the header.
        available: usize,
        /// Bytes one layer used to occupy.
        per: usize,
    },

    /// A layer's mip slice runs past the end of the record.
    ///
    /// **Refuted by the corpus, kept for the same reason as
    /// [`TextureError::PayloadNotMultiple`].** There are no layers, so there is
    /// no layer slice; [`TextureError::LevelSlicePastRecord`] is the live arm.
    #[error("texture: layer {layer} mip {mip} needs {size} bytes at offset {offset}, but the record is {record_size} bytes (refuted framing)")]
    MipSlicePastRecord {
        /// Layer index the slice belongs to.
        layer: usize,
        /// Mip index within the layer.
        mip: u32,
        /// Byte offset the slice starts at.
        offset: usize,
        /// Bytes the slice needs.
        size: usize,
        /// Bytes in the record.
        record_size: usize,
    },

    // ---- luminance (0x15) framing --------------------------------------
    /// The record is not exactly as long as its own envelope implies.
    ///
    /// **Superseded by [`TextureError::RecordSizeMismatch`]**, which names the
    /// `fourcc` as well. Kept so a caller written against it still compiles, and
    /// so the `0x15` family's framing check stays individually addressable.
    #[error(
        "luminance: record is {record_size} bytes but the envelope implies {expected} \
         (width={width}, height={height}, mip_count={mip_count})"
    )]
    LuminanceRecordSize {
        /// Bytes actually present.
        record_size: usize,
        /// Bytes the envelope's own geometry requires.
        expected: usize,
        /// Envelope width.
        width: u32,
        /// Envelope height.
        height: u32,
        /// Envelope mip count.
        mip_count: u32,
    },

    /// A mip's little-endian length prefix disagrees with that mip's geometry.
    ///
    /// **Superseded by [`TextureError::LevelLengthPrefix`]**, which every format
    /// now shares. Kept for the same reason as
    /// [`TextureError::LuminanceRecordSize`].
    #[error(
        "luminance: mip {mip} declares {declared} bytes at offset {offset} but its \
         {width}x{height} geometry needs {expected}"
    )]
    LuminanceLengthPrefix {
        /// Mip index the prefix belongs to.
        mip: u32,
        /// Byte count the record declares.
        declared: usize,
        /// Byte count the mip's dimensions require.
        expected: usize,
        /// Offset the prefix was read from.
        offset: usize,
        /// Mip width.
        width: u32,
        /// Mip height.
        height: u32,
    },

    /// A mip's length prefix, or its payload, runs past the end of the record.
    ///
    /// **Superseded by [`TextureError::LevelSlicePastRecord`].**
    #[error(
        "luminance: mip {mip} needs {needed} bytes at offset {offset}, \
         but the record is {record_size} bytes"
    )]
    LuminanceSlicePastRecord {
        /// Mip index the slice belongs to.
        mip: u32,
        /// Offset the slice starts at.
        offset: usize,
        /// Bytes the slice needs.
        needed: usize,
        /// Bytes in the record.
        record_size: usize,
    },

    /// A mip's payload is shorter than `width * height * 4`.
    ///
    /// `0x15` is uncompressed, so unlike [`TextureError::ShortBlockChain`] there
    /// is no codec that could absorb the difference: a short payload means the
    /// record is truncated or mis-framed, and there is nothing to decode.
    #[error("luminance: mip payload of {available} bytes < {needed} for {width}x{height}")]
    ShortLuminanceMip {
        /// Payload bytes available.
        available: usize,
        /// Payload bytes the mip's dimensions require.
        needed: usize,
        /// Mip width.
        width: u32,
        /// Mip height.
        height: u32,
    },

    /// The two ways this module states "four bytes per texel" disagreed.
    ///
    /// **Unreachable through the public API**: [`crate::luminance_mip_size`] and
    /// the output allocation in [`crate::decode_luminance_mip`] are both
    /// `w * h * 4` from the same two arguments. The variant exists so that
    /// changing one of those formulas without the other becomes a typed refusal
    /// rather than a caller indexing past its own buffer.
    #[error(
        "luminance: internal size invariant failed for {width}x{height} \
         (mip payload {needed}, output {out_len})"
    )]
    LuminanceInvariant {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
        /// Payload bytes the mip's geometry requires.
        needed: usize,
        /// Output bytes the allocation produced.
        out_len: usize,
    },

    // ---- block codec ---------------------------------------------------
    /// The block chain is shorter than the level's own geometry requires.
    ///
    /// `needed` is `ceil4(w) * ceil4(h) * block_bytes` for the level's
    /// dimensions.
    #[error("dxt5: block chain of {available} bytes < {needed} for {width}x{height}")]
    ShortBlockChain {
        /// Block bytes available.
        available: usize,
        /// Block bytes the level's dimensions require.
        needed: usize,
        /// Level width.
        width: u32,
        /// Level height.
        height: u32,
    },

    /// The requested level is too large to address, let alone allocate.
    ///
    /// Two distinct bounds, reported as one variant because neither describes a
    /// real record: the `w * h * 4` product must not overflow, and the output
    /// buffer must fit inside `isize::MAX` bytes, which no `Vec` allocation can
    /// exceed on any supported target. The C++ reference checks only the overflow
    /// and would abort the process on a failed allocation; this port reports the
    /// second condition as a typed error instead.
    ///
    /// Reachability, stated plainly: on a 64-bit target **no** `u32` input
    /// reaches either arm — the level size saturates only where `usize` is 32
    /// bits, and a `w * h * 4` product above `isize::MAX` would first need a
    /// chain longer than `2^60` bytes to get past the length check. Such a call
    /// is refused one line earlier by [`TextureError::ShortBlockChain`]. This
    /// variant is the guard against a failed allocation on an adversarial
    /// envelope, written and kept so that widening the input types later cannot
    /// turn an abort into a crash.
    #[error("dxt5: {width}x{height} exceeds the addressable output size")]
    OutputTooLarge {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
    },

    /// The destination buffer was not the size the level's geometry requires.
    ///
    /// **Unreachable through the public API**: the decoder sizes its output as
    /// exactly `width * height * 4` from the same `width`/`height` it then
    /// indexes with, so the destination of every texel write is in bounds by
    /// construction. The variant exists so that the block writer has no path
    /// that can index out of bounds: an arithmetic slip becomes a typed error
    /// rather than a panic or, worse, a write into the wrong memory.
    #[error("dxt5: internal output-size invariant failed at texel ({texel_x}, {texel_y}) of {width}x{height}")]
    OutputInvariant {
        /// Requested width.
        width: u32,
        /// Requested height.
        height: u32,
        /// Texel column the write targeted.
        texel_x: u32,
        /// Texel row the write targeted.
        texel_y: u32,
    },
}
