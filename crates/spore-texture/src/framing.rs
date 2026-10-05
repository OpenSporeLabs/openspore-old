//! How a raster record frames itself: header size, block size, and the level walk.
//!
//! # What was wrong before, in one number
//!
//! The previous rule charged **8 bytes per 4x4 block** and then divided the
//! payload by `16 + chain`, which is how it got "2 layers" on every 512² DXT5
//! record. Two errors cancelled: a 16-byte block read at 8 bytes per block is
//! two half-pictures, and two of them make one picture. The cancellation is
//! exact only when the geometry happens to line up — which is why 1374 of the
//! 1747 DXT5 records decoded and 373 did not.
//!
//! # The rule this module encodes, measured over 2954 of 2954 records
//!
//! ```text
//! off        field
//! 0x00       version  u32, always 1
//! 0x04       width    u32
//! 0x08       height   u32
//! 0x0c       mip_count u32
//! 0x10       field_10 u32, always 8            (UNRESOLVED)
//! 0x14       fourcc   u32
//! 0x18       field_18 u32 == base level bytes  (see claims)
//! 0x1c/0x20  level 0, NO length prefix
//!            level 1, no prefix (DXT5/DXT1 only; the 0x15 family prefixes it)
//!            level m >= first_prefixed_level: u32 LE == level m's own byte
//!                                          count, then that many payload bytes
//! ```
//!
//! and the record's length is **fully determined**:
//!
//! ```text
//! record_size == header + level0_bytes
//!                       + SUM over m in first_prefixed_level..mip_count of
//!                             (4 + level_m_bytes)
//! ```
//!
//! with nothing left over. The identity closes on every raster record in
//! `Spore_Content.package` and in `Spore_EP1_Content_01.package`, for all three
//! fourcc values — see `docs/RASTER-FRAMING.md` for the byte counts.
//!
//! # The per-format table
//!
//! | `fourcc` | header | bytes per 4x4 block | first prefixed level | records |
//! |---|---|---|---|---|
//! | `0x35545844` `DXT5` | 32 | **16** | 2 | 1 747 |
//! | `0x31545844` `DXT1` | 32 | **8** | 2 | 306 |
//! | `0x00000015` | **28** | 4 bytes per *texel* | **1** | 901 |
//!
//! Two of those three numbers are load-bearing discoveries and one is not:
//!
//! * **16 bytes per block for `DXT5`.** Forced by the records, not chosen. The
//!   base level of a 256² record is **65 536** bytes, which is 4096 blocks of
//!   16 bytes; at 8 bytes it would be 32 768. Every recorded length prefix in
//!   every DXT5 record is the 16-byte figure.
//! * **8 bytes per block for `DXT1`.** Forced the same way: the base level of a
//!   256² record is 32 768 bytes, which is 4096 blocks of 8.
//! * **The first prefixed level differs between the block formats and the `0x15`
//!   family.** The `0x15` family prefixes level 1; the block formats do not, and
//!   the bytes where such a prefix would sit hold block data, not a length. Why
//!   the two families differ is **UNKNOWN** — see
//!   [`crate::claims::FIRST_PREFIXED_LEVEL`].
//!
//! # Layer count
//!
//! There is **no layer count** and none is derived: every raster record in the
//! corpus is exactly one chain of mip levels. `field_18` is not a layer count
//! either — it is the base level's own byte count, which is why it read "two
//! base levels" for DXT5 under the old 8-byte block size.
//!
//! # Placing a level is where this module refuses
//!
//! [`level_offset`] is the only function here that tells a caller *where to read*,
//! and it is the only one that returns a [`crate::TextureError`]. Three input
//! classes have no answer and are refused by name rather than answered from the
//! geometry: a level the envelope's `mip_count` does not declare, a chain longer
//! than [`MAX_PLACEABLE_LEVELS`], and a zero dimension. The sizing functions
//! beside it ([`level_size`], [`chain_size`], [`record_size`]) are unchanged, are
//! cross-checked against the record's own length and against every prefixed
//! level's `u32` byte count, and keep their documented past-the-floor
//! approximation — which is graded as an approximation in
//! [`crate::claims::MIPS_BEYOND_32`] precisely because it is not used to place
//! anything.

use crate::{TextureError, DXT1_FOURCC, DXT5_FOURCC, LUMINANCE_FOURCC};

/// Bytes per output texel: RGBA8.
pub(crate) const RGBA_BYTES: usize = 4;

/// Texels along one block edge.
pub(crate) const BLOCK_EDGE: u32 = 4;

/// Bytes in the little-endian `u32` that precedes a prefixed level.
pub const LEVEL_PREFIX_SIZE: usize = 4;

/// How one raster `fourcc` frames itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameFormat {
    /// The `fourcc` word this describes.
    pub fourcc: u32,
    /// Bytes of header before the base level's payload.
    pub header_size: usize,
    /// Bytes per 4x4 block. the frame format's bytes-per-texel marks the one
    /// format that is uncompressed and has no blocks at all.
    pub block_bytes: usize,
    /// The first mip level preceded by a `u32` byte count. Levels below it are
    /// written bare.
    pub first_prefixed_level: u32,
}

/// The sentinel [`FrameFormat::block_bytes`] carries for a format that has no
/// blocks at all: the `0x15` family, whose bytes are per *texel*.
pub const BYTES_PER_TEXEL: usize = 0;

/// `DXT5`: 32-byte header, 16 bytes per block, level 2 onwards prefixed.
pub const DXT5_FRAME: FrameFormat = FrameFormat {
    fourcc: DXT5_FOURCC,
    header_size: 32,
    block_bytes: 16,
    first_prefixed_level: 2,
    // `BYTES_PER_TEXEL` is 0, i.e. this is a blocked format.
};

/// `DXT1`: 32-byte header, 8 bytes per block, level 2 onwards prefixed.
pub const DXT1_FRAME: FrameFormat = FrameFormat {
    fourcc: DXT1_FOURCC,
    header_size: 32,
    block_bytes: 8,
    first_prefixed_level: 2,
};

/// The `0x15` family: 28-byte header, four bytes per texel, level 1 onwards
/// prefixed.
pub const LUMINANCE_FRAME: FrameFormat = FrameFormat {
    fourcc: LUMINANCE_FOURCC,
    header_size: 28,
    block_bytes: BYTES_PER_TEXEL,
    first_prefixed_level: 1,
};

/// The frame of the `fourcc`, or `None` when this build has no rule for it.
///
/// Absence is not a failure: a caller can ask what a record *would* be without
/// this build knowing how to read it.
pub fn frame_for(fourcc: u32) -> Option<FrameFormat> {
    match fourcc {
        DXT5_FOURCC => Some(DXT5_FRAME),
        DXT1_FOURCC => Some(DXT1_FRAME),
        LUMINANCE_FOURCC => Some(LUMINANCE_FRAME),
        _ => None,
    }
}

/// `ceil(n / 4)`, the block count along one axis.
pub(crate) fn ceil_div4(n: u32) -> u32 {
    n / BLOCK_EDGE + if n % BLOCK_EDGE != 0 { 1 } else { 0 }
}

/// Dimension of one axis at one mip level: `max(1, base >> mip)`.
pub fn level_dimension(base: u32, mip: u32) -> u32 {
    if mip >= crate::MIP_FLOOR {
        return 1;
    }
    (base >> mip).max(1)
}

/// Payload bytes of one mip level, saturating.
///
/// `mip 0` is the base level. A blocked format charges
/// `ceil4(w >> mip) * ceil4(h >> mip) * block_bytes`; the uncompressed family
/// charges `w * mip` `h * mip` `4`.
pub fn level_size(frame: FrameFormat, width: u32, height: u32, mip: u32) -> usize {
    let w = level_dimension(width, mip);
    let h = level_dimension(height, mip);
    if frame.block_bytes == BYTES_PER_TEXEL {
        return (w as usize)
            .saturating_mul(h as usize)
            .saturating_mul(RGBA_BYTES);
    }
    let blocks = (ceil_div4(w) as usize).saturating_mul(ceil_div4(h) as usize);
    blocks.saturating_mul(frame.block_bytes)
}

/// Payload bytes of the whole chain, **including** the level prefixes and
/// excluding the header.
///
/// Saturating: an absurd `width * height` yields `usize::MAX` rather than a
/// wrapped small number, which is what a caller must be able to tell apart from
/// "small texture".
pub fn chain_size(frame: FrameFormat, width: u32, height: u32, mip_count: u32) -> usize {
    if mip_count == 0 {
        return 0;
    }
    let mut total = level_size(frame, width, height, 0);
    // Mips at or beyond MIP_FLOOR are all 1x1, so each still costs one level and
    // (from its index on) one prefix. This is the same approximation
    // `MIP_FLOOR` documents: no record has more than 10 levels.
    let bounded = mip_count.min(crate::MIP_FLOOR);
    for mip in 1..bounded {
        if mip >= frame.first_prefixed_level {
            total = total.saturating_add(LEVEL_PREFIX_SIZE);
        }
        total = total.saturating_add(level_size(frame, width, height, mip));
    }
    if mip_count > bounded {
        let each =
            LEVEL_PREFIX_SIZE.saturating_add(level_size(frame, width, height, crate::MIP_FLOOR));
        total = total.saturating_add((mip_count - bounded) as usize * each);
    }
    total
}

/// The one record size a record of this shape and format can have.
///
/// This identity **is** the framing finding, so it is exposed: a caller can
/// check a record's framing without decoding a texel.
pub fn record_size(frame: FrameFormat, width: u32, height: u32, mip_count: u32) -> usize {
    frame
        .header_size
        .saturating_add(chain_size(frame, width, height, mip_count))
}

/// The number of mip levels this build can place, and therefore the level count
/// above which it refuses to answer.
///
/// [`crate::MIP_FLOOR`] is 32, and `base >> 32` is exhausted for every `u32`
/// base, so [`level_dimension`] pins every level at or past it to 1x1. Level 32
/// is still *placeable* — its offset is the exact sum of levels `0..32`, which is
/// the same sum [`chain_size`] and the independent transcription in
/// `crates/spore-differential/tests/luminance_corpus.rs` both produce — so the
/// ceiling is **33**, not 32. Level 33 is the first index whose offset would have
/// to come from the past-the-floor rule instead of the measured geometry.
///
/// Measured over the real corpus this is not a ceiling at all: the largest chain
/// in 4386 raster records is **10** levels, and
/// `crates/spore-differential/tests/framing_refusals.rs` re-measures that. This
/// constant exists because a decoder that extrapolates silently past the end of
/// its evidence is worse than one that stops.
pub const MAX_PLACEABLE_LEVELS: u32 = crate::MIP_FLOOR + 1;

/// Byte offset of level `mip`'s payload, headers and prefixes included, or the
/// refusal that says why there is none.
///
/// # Why this refuses
///
/// Four arguments are not enough to place a level, and every refusal below names
/// a piece of evidence this function was handed and did not have:
///
/// * **`mip_count`** — a raster record's header says how many levels it has, and
///   it is the only statement of the chain's length in the record. Placing a
///   level the envelope does not declare is a question about a level that does
///   not exist. Before `mip_count` was a parameter this function answered one
///   anyway: for every one of the 4386 raster records in the two content
///   packages, `level_offset(_, _, _, mip_count)` returned the record's own
///   length, which a caller would have sliced as though a level 10 existed in a
///   ten-level record.
/// * **`mip >= MAX_PLACEABLE_LEVELS`** — the past-the-floor rule is recorded as an
///   [`crate::claims::MIPS_BEYOND_32`] approximation, not a measurement, and the
///   largest chain in the corpus is ten levels. The previous loop bound was
///   `0..mip.min(MIP_FLOOR)`, which quietly stopped accumulating: level 33, level
///   64 and level `u32::MAX` all came back with the *same* plausible offset, and
///   the result disagreed with [`record_size`] on the very record it pointed into
///   (664 against 704 on a 34-level 1x1 DXT5 record).
/// * **a zero dimension** — [`level_dimension`] pins `max(1, base >> mip)`, so a
///   zero base is silently promoted to 1x1 and a 0-wide image acquires 16 bytes
///   per level and an offset for each. `decode_raster` already refuses such a
///   record with [`TextureError::ZeroDimension`]; this is the same refusal at the
///   function that places levels. The corpus holds none (measured), so this
///   aligns the two rather than fixing a seen defect.
///
/// [`level_size`], [`chain_size`] and [`record_size`] are **unchanged**. They are
/// sizing arithmetic with documented saturating behaviour, they are cross-checked
/// against the record's own length and against every prefixed level's `u32` byte
/// count, and the past-the-floor part of them is a recorded approximation rather
/// than a silent guess. This function is the one that tells a caller where to
/// read, so it is the one that refuses.
pub fn level_offset(
    frame: FrameFormat,
    width: u32,
    height: u32,
    mip: u32,
    mip_count: u32,
) -> Result<usize, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::ZeroDimension { width, height });
    }
    if mip >= mip_count {
        return Err(TextureError::LevelNotInChain { mip, mip_count });
    }
    if mip >= MAX_PLACEABLE_LEVELS {
        return Err(TextureError::MipChainBeyondFloor {
            levels: mip + 1,
            floor: MAX_PLACEABLE_LEVELS,
        });
    }
    // No clamp: `mip < MAX_PLACEABLE_LEVELS`, so every level walked here is one
    // the measured geometry accounts for and `record_size` agrees on.
    let mut off = frame.header_size;
    for level in 0..mip {
        if level >= frame.first_prefixed_level {
            off = off.saturating_add(LEVEL_PREFIX_SIZE);
        }
        off = off.saturating_add(level_size(frame, width, height, level));
    }
    Ok(off)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_table_is_the_measured_one() {
        assert_eq!(DXT5_FRAME.block_bytes, 16);
        assert_eq!(DXT5_FRAME.first_prefixed_level, 2);
        assert_eq!(DXT5_FRAME.header_size, 32);
        assert_eq!(DXT1_FRAME.block_bytes, 8);
        assert_eq!(DXT1_FRAME.first_prefixed_level, 2);
        assert_eq!(LUMINANCE_FRAME.header_size, 28);
        assert_eq!(LUMINANCE_FRAME.block_bytes, 0);
        assert_eq!(LUMINANCE_FRAME.first_prefixed_level, 1);
        assert_eq!(frame_for(0xDEAD_BEEF), None);
        assert_eq!(frame_for(DXT5_FOURCC), Some(DXT5_FRAME));
    }

    #[test]
    fn every_corpus_shape_has_the_record_size_the_corpus_has() {
        // (fourcc, w, h, mips, measured record size) for every raster shape in
        // Spore_Content.package and Spore_EP1_Content_01.package. These are the
        // sizes the identity must reproduce; a change that breaks one of them
        // changes the framing, not the arithmetic.
        for (fourcc, w, h, mips, size) in [
            (DXT5_FOURCC, 512u32, 512u32, 10u32, 349_616usize),
            (DXT5_FOURCC, 256, 256, 9, 87_468),
            (DXT5_FOURCC, 64, 64, 7, 5_540),
            (DXT1_FOURCC, 256, 256, 9, 43_764),
            (LUMINANCE_FOURCC, 512, 512, 10, 1_398_164),
            (LUMINANCE_FOURCC, 256, 256, 9, 349_584),
            (LUMINANCE_FOURCC, 32, 32, 6, 5_508),
        ] {
            let frame = frame_for(fourcc).expect("a frame for every corpus fourcc");
            assert_eq!(
                record_size(frame, w, h, mips),
                size,
                "{fourcc:#010x} {w}x{h} mips={mips}"
            );
        }
    }

    #[test]
    fn a_level_offset_is_the_sum_of_the_levels_before_it() {
        for fourcc in [DXT5_FOURCC, DXT1_FOURCC, LUMINANCE_FOURCC] {
            let frame = frame_for(fourcc).unwrap();
            let mut off = frame.header_size;
            for mip in 0..8u32 {
                assert_eq!(
                    level_offset(frame, 64, 64, mip, 8),
                    Ok(off),
                    "{fourcc:#010x} mip {mip}"
                );
                if mip >= frame.first_prefixed_level {
                    off += LEVEL_PREFIX_SIZE;
                }
                off += level_size(frame, 64, 64, mip);
            }
        }
    }

    #[test]
    fn a_level_offset_is_the_record_size_of_the_chain_before_it() {
        // The identity the previous version contradicted. `level_offset` addresses
        // a prefixed level's **prefix** (which is why `decode_raster` adds
        // `LEVEL_PREFIX_SIZE` after reading it), so a level past
        // `first_prefixed_level` contributes its prefix twice — once inside the
        // truncated chain's size and once ahead of its payload. Level 32, the last
        // placeable index, is included because `chain_size` and this function
        // agree about it; level 33 is not placeable at all, and is refused below.
        for fourcc in [DXT5_FOURCC, DXT1_FOURCC, LUMINANCE_FOURCC] {
            let frame = frame_for(fourcc).unwrap();
            for mip in 1..=crate::MIP_FLOOR {
                let prefix = usize::from(mip >= frame.first_prefixed_level) * LEVEL_PREFIX_SIZE;
                assert_eq!(
                    level_offset(frame, 8, 8, mip, mip + 1).unwrap()
                        + level_size(frame, 8, 8, mip)
                        + prefix,
                    record_size(frame, 8, 8, mip + 1),
                    "{fourcc:#010x} chain of {} levels",
                    mip + 1
                );
            }
        }
    }

    #[test]
    fn the_past_the_floor_placement_is_refused_not_extrapolated() {
        for fourcc in [DXT5_FOURCC, DXT1_FOURCC, LUMINANCE_FOURCC] {
            let frame = frame_for(fourcc).unwrap();
            for mip in [crate::MIP_FLOOR + 1, crate::MIP_FLOOR + 2, 64, u32::MAX - 1] {
                assert_eq!(
                    level_offset(frame, 8, 8, mip, u32::MAX),
                    Err(TextureError::MipChainBeyondFloor {
                        levels: mip + 1,
                        floor: MAX_PLACEABLE_LEVELS
                    }),
                    "{fourcc:#010x} level {mip} is past the ceiling"
                );
            }
        }
    }

    #[test]
    fn a_level_the_envelope_does_not_declare_has_no_offset() {
        for fourcc in [DXT5_FOURCC, DXT1_FOURCC, LUMINANCE_FOURCC] {
            let frame = frame_for(fourcc).unwrap();
            // Every corpus shape: the declared chain's own length is refused, and
            // so is the level after it. The pre-fix function answered the first of
            // these with `record_size`, i.e. with a plausible in-record offset.
            for (mips, mip) in [(10u32, 10u32), (9, 9), (7, 7), (6, 6), (10, 11), (1, 1)] {
                assert_eq!(
                    level_offset(frame, 512, 512, mip, mips),
                    Err(TextureError::LevelNotInChain {
                        mip,
                        mip_count: mips
                    }),
                    "{fourcc:#010x} a {mips}-level chain has no level {mip}"
                );
            }
            // And a zero-level chain has no levels at all, which is the same
            // refusal rather than a special case: `mip >= mip_count` covers it.
            assert_eq!(
                level_offset(frame, 512, 512, 0, 0),
                Err(TextureError::LevelNotInChain {
                    mip: 0,
                    mip_count: 0
                })
            );
        }
    }

    #[test]
    fn a_degenerate_dimension_has_no_level_to_place() {
        for frame in [DXT5_FRAME, DXT1_FRAME, LUMINANCE_FRAME] {
            for (width, height) in [(0u32, 8u32), (8, 0), (0, 0)] {
                assert_eq!(
                    level_offset(frame, width, height, 0, 1),
                    Err(TextureError::ZeroDimension { width, height }),
                    "{:?} {width}x{height}",
                    frame.fourcc
                );
            }
        }
    }

    #[test]
    fn a_zero_mip_count_is_no_chain_and_saturates_rather_than_wrapping() {
        assert_eq!(chain_size(DXT5_FRAME, 4, 4, 0), 0);
        assert!(level_size(DXT5_FRAME, u32::MAX, u32::MAX, 0) > u32::MAX as usize);
        assert!(level_size(LUMINANCE_FRAME, u32::MAX, u32::MAX, 0) > u32::MAX as usize);
    }

    #[test]
    fn dimensions_use_the_shared_floor() {
        assert_eq!(level_dimension(512, 0), 512);
        assert_eq!(level_dimension(512, 9), 1);
        assert_eq!(level_dimension(512, 32), 1);
        assert_eq!(level_dimension(1, 31), 1);
    }

    #[test]
    fn beyond_32_levels_cost_a_prefix_plus_one_block() {
        let walked = chain_size(DXT5_FRAME, 4, 4, 32);
        let beyond = chain_size(DXT5_FRAME, 4, 4, 33);
        assert_eq!(beyond, walked + LEVEL_PREFIX_SIZE + 16);
        let lum_walked = chain_size(LUMINANCE_FRAME, 4, 4, 32);
        let lum_beyond = chain_size(LUMINANCE_FRAME, 4, 4, 33);
        assert_eq!(lum_beyond, lum_walked + LEVEL_PREFIX_SIZE + 4);
    }
}
