//! The 32-byte raster envelope and the per-level walk every format shares.
//!
//! # The envelope
//!
//! ```text
//! off  field       observed value   meaning
//! 0x00 version     1                 format revision (only 1 seen)
//! 0x04 width                          texels
//! 0x08 height                         texels
//! 0x0c mip_count                      mip levels in the chain
//! 0x10 field_10    8                 UNRESOLVED — never read by any decoder
//! 0x14 fourcc      0x35545844        'DXT5' / 'DXT1' / 0x15, stored little-endian
//! 0x18 field_18    base level bytes  VERIFIED over 2954 of 2954 records
//! 0x1c field_1c    format-specific   payload for the 0x15 family
//! ```
//!
//! All eight words are little-endian `u32`.
//!
//! # The layer count is 1, and it is not derived
//!
//! The previous version **divided** the payload by a per-layer stride and
//! reported 2 for every DXT5 record. There is no layer count in a raster
//! record and there is nothing to derive one from: every raster record in
//! `Spore_Content.package` is **one chain of mip levels**, which
//! [`crate::framing`] walks exactly. The "two layers" were an artefact of
//! charging 8 bytes per 4x4 block where the records hold 16 — two half-sized
//! pictures read as one picture. The 32 bytes of "layer headers" the old layout
//! skipped were the first two blocks of the base level: they vary from record to
//! record, which is what block data does and is not what a header does.
//!
//! # `field_18` is a cross-check, not a layer count
//!
//! It is the base level's own byte count in **all three formats** — 2954 of 2954
//! records — so the walk refuses a record whose header disagrees with its own
//! geometry rather than trusting either side. That is the check the crate's
//! "derived, never read" rule wanted and could not have: before, the word read
//! as "format-specific" only because the block size was wrong.
//!
//! # Strictness
//!
//! * This port rejects `width == 0` / `height == 0`; the Python oracle does not
//!   and would return an empty image.
//! * This port rejects `mip_count == 0`.
//! * This port refuses a decode whose output would exceed `isize::MAX` bytes.
//! * Every level from [`crate::framing::FrameFormat::first_prefixed_level`] on
//!   carries its own `u32` byte count, and the walk **re-reads and checks** it
//!   rather than deriving past it. A disagreement is
//!   [`TextureError::LevelLengthPrefix`], by name.

use crate::dxt5::{decode_dxt1_mip, decode_dxt5_mip, BC1_BLOCK_BYTES, BLOCK_BYTES};
use crate::error::TextureError;
use crate::framing::{
    frame_for, level_dimension, level_offset, level_size, record_size, FrameFormat,
    LEVEL_PREFIX_SIZE,
};
use crate::luminance::decode_luminance_mip;
use crate::ENVELOPE_SIZE;

/// The 32-byte raster envelope.
///
/// Every field is reproduced verbatim. Nothing here has been given a meaning
/// this crate does not have evidence for: see the module documentation and
/// [`crate::claims`].
///
/// **The struct is shared by two header lengths.** DXT5 and DXT1 have a 32-byte
/// envelope whose last word (`field_1c`) is metadata. The `0x15` family has a
/// **28-byte** header, and `field_1c` in that format is *the first four bytes of
/// the base level's payload*, not a field — see
/// [`crate::claims::ENVELOPE_FIELD_1C_MEANING`]. The word is still reported
/// here because a caller reading an envelope should see the bytes that are there.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RasterEnvelope {
    /// Offset `0x00`. Observed `1` on every record. Never validated by a
    /// decoder: no record with a different value has been seen, so rejecting
    /// one would be a guess.
    pub version: u32,
    /// Offset `0x04`. Base-level width in texels.
    pub width: u32,
    /// Offset `0x08`. Base-level height in texels.
    pub height: u32,
    /// Offset `0x0c`. Mip levels present in the chain. Must be non-zero.
    pub mip_count: u32,
    /// Offset `0x10`. **Unresolved**: observed `8` in every sample. Parsed and
    /// carried; never read. Graded `UNKNOWN` in [`crate::claims`].
    pub field_10: u32,
    /// Offset `0x14`. Must be `DXT5_FOURCC`, [`crate::DXT1_FOURCC`] or
    /// `LUMINANCE_FOURCC` for this crate to decode.
    pub fourcc: u32,
    /// Offset `0x18`. **The base level's byte count**, verified over every
    /// raster record measured in `Spore_Content.package` and
    /// `Spore_EP1_Content_01.package` (2954 of 2954) and in all three formats.
    /// The walk checks it and refuses a disagreement; it never uses it to place
    /// anything. Graded in [`crate::claims`].
    pub field_18: u32,
    /// Offset `0x1c`. Reads `0x0000_FFFF` in DXT5 records. **In a `0x15` record
    /// it is not a header field at all** — those headers are 28 bytes long, so
    /// this word is the first texel of the base level. `docs/MATERIALS-DESIGN.md`
    /// §1 calls it "a float 1.0 in 0x15 records"; the bytes contradict that, and
    /// the bytes win. Carried; never used to select a decode.
    pub field_1c: u32,
}

/// One decoded mip level: RGBA8, row-major, exactly `width * height * 4` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MipImage {
    /// This mip's own width. Not the base level's.
    pub width: u32,
    /// This mip's own height.
    pub height: u32,
    /// `width * height * 4` bytes of RGBA8 in row-major order.
    pub pixels: Vec<u8>,
}

/// A whole raster record: its envelope and its mip chain.
///
/// `layers[m][mip]` is level `mip` of chain `m`. **There is exactly one chain**:
/// [`crate::framing`] measures every raster record in the corpus as one, and
/// nothing in a record says otherwise. The field keeps its plural name and its
/// nesting so that a caller written against the old two-layer reading compiles
/// and keeps working; `layers.len()` is 1, always, and that is the finding
/// rather than an accident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterImage {
    /// The envelope the levels were decoded under.
    pub envelope: RasterEnvelope,
    /// Mip chains, in record order. One element.
    pub layers: Vec<Vec<MipImage>>,
}

/// Reads the 32-byte envelope.
///
/// Pure structure: this does **not** validate `fourcc`, `mip_count` or the
/// dimensions, so a caller can inspect an envelope of a format this crate does
/// not decode. [`decode_raster`] is the validating entry point.
pub fn parse_envelope(data: &[u8]) -> Result<RasterEnvelope, TextureError> {
    if data.len() < ENVELOPE_SIZE {
        return Err(TextureError::TruncatedEnvelope {
            available: data.len(),
        });
    }
    Ok(RasterEnvelope {
        version: read_u32(data, 0x00)?,
        width: read_u32(data, 0x04)?,
        height: read_u32(data, 0x08)?,
        mip_count: read_u32(data, 0x0C)?,
        field_10: read_u32(data, 0x10)?,
        fourcc: read_u32(data, 0x14)?,
        field_18: read_u32(data, 0x18)?,
        field_1c: read_u32(data, 0x1C)?,
    })
}

/// Dimension of one axis at one mip level.
///
/// `max(1, base >> mip)`, with every mip at or beyond the mip floor clamped to
/// 1 — the rule the framing identity closes under.
pub fn mip_dimension(base: u32, mip: u32) -> u32 {
    level_dimension(base, mip)
}

/// How many mip chains a record of `record_size` bytes holds.
///
/// **Always 1**, for a format this build has a frame for, and it is *not*
/// derived by division any more: the chain's own byte count is determined by
/// [`crate::framing::record_size`], so the record's length either matches that
/// exactly or is refused. [`TextureError::UnsupportedFourcc`] when the `fourcc`
/// has no frame, which is not a failure but an absence — the crate cannot say
/// how such a record is framed.
pub fn layer_count_for(record_size: usize, env: &RasterEnvelope) -> Result<usize, TextureError> {
    if record_size < ENVELOPE_SIZE {
        return Err(TextureError::RecordShorterThanEnvelope { record_size });
    }
    check_envelope_shape(record_size, env)?;
    Ok(1)
}

/// The geometry checks every entry point shares: non-zero dimensions, non-zero
/// mip count, a known format, and a length that is exactly the one the envelope
/// implies.
fn check_envelope_shape(present: usize, env: &RasterEnvelope) -> Result<FrameFormat, TextureError> {
    if env.width == 0 || env.height == 0 {
        return Err(TextureError::ZeroDimension {
            width: env.width,
            height: env.height,
        });
    }
    if env.mip_count == 0 {
        return Err(TextureError::ZeroMipCount);
    }
    let frame =
        frame_for(env.fourcc).ok_or(TextureError::UnsupportedFourcc { fourcc: env.fourcc })?;
    let expected = record_size(frame, env.width, env.height, env.mip_count);
    if present != expected {
        return Err(TextureError::RecordSizeMismatch {
            record_size: present,
            expected,
            width: env.width,
            height: env.height,
            mip_count: env.mip_count,
            fourcc: env.fourcc,
        });
    }
    Ok(frame)
}

/// Decodes a whole raster record: envelope, framing check, every mip level.
///
/// Rejects a record shorter than the envelope, a zero dimension, a `fourcc` this
/// build has no frame for, `mip_count == 0`, a length that is not exactly the
/// one its own envelope implies, a `field_18` that is not the base level's byte
/// count, a length prefix that disagrees with its level's geometry, and a level
/// slice that would run past the record. Nothing is returned on any error: there
/// is no partially decoded image.
pub fn decode_raster(data: &[u8]) -> Result<RasterImage, TextureError> {
    let envelope = parse_envelope(data)?;
    let frame = check_envelope_shape(data.len(), &envelope)?;

    // The base level's own byte count is in the header. Checking it here is the
    // cross-check the "derived, never read" rule wanted: the walk still places
    // every level from the geometry, so a header that disagrees is refused
    // rather than believed. A base level too large to compare is skipped rather
    // than refused — an unaddressable envelope is already refused above.
    let base = level_size(frame, envelope.width, envelope.height, 0);
    if let Ok(declared) = u32::try_from(base) {
        if declared != envelope.field_18 {
            return Err(TextureError::Field18Mismatch {
                declared: envelope.field_18,
                expected: base,
                width: envelope.width,
                height: envelope.height,
                fourcc: envelope.fourcc,
            });
        }
    }

    let mut mips = Vec::with_capacity(envelope.mip_count as usize);
    for mip in 0..envelope.mip_count {
        let width = mip_dimension(envelope.width, mip);
        let height = mip_dimension(envelope.height, mip);
        let size = level_size(frame, envelope.width, envelope.height, mip);
        let mut cursor = level_offset(frame, envelope.width, envelope.height, mip);
        if mip >= frame.first_prefixed_level {
            let declared = read_u32(data, cursor)?;
            cursor += LEVEL_PREFIX_SIZE;
            if declared as usize != size {
                return Err(TextureError::LevelLengthPrefix {
                    mip,
                    declared: declared as usize,
                    expected: size,
                    offset: cursor - LEVEL_PREFIX_SIZE,
                    width,
                    height,
                });
            }
        }
        let bytes = slice_at(data, cursor, size, mip)?;
        let image = match frame.block_bytes {
            0 => decode_luminance_mip(bytes, width, height)?,
            BLOCK_BYTES => decode_dxt5_mip(bytes, width, height)?,
            BC1_BLOCK_BYTES => decode_dxt1_mip(bytes, width, height)?,
            // Unreachable: `FrameFormat` has exactly three entries and their
            // block sizes are 16, 8 and 0. The arm exists so that adding a
            // fourth format is a compile error here rather than a silent decode
            // with the wrong codec.
            other => {
                return Err(TextureError::UnsupportedBlockSize {
                    fourcc: envelope.fourcc,
                    block_bytes: other,
                })
            }
        };
        mips.push(image);
    }

    Ok(RasterImage {
        envelope,
        layers: vec![mips],
    })
}

/// Bounds one level's slice, refusing to read past the record.
///
/// Currently unreachable through [`decode_raster`]: the framing identity has
/// already matched the record's length exactly, and each level's bytes are the
/// sum of what follows it, so the last level ends exactly at the record end. It
/// is kept as the bound on that argument.
fn slice_at(data: &[u8], offset: usize, size: usize, mip: u32) -> Result<&[u8], TextureError> {
    let past_end = || TextureError::LevelSlicePastRecord {
        mip,
        offset,
        size,
        record_size: data.len(),
    };
    let end = offset.checked_add(size).ok_or_else(past_end)?;
    data.get(offset..end).ok_or_else(past_end)
}

/// Reads one little-endian `u32` at `offset`.
fn read_u32(data: &[u8], offset: usize) -> Result<u32, TextureError> {
    let bytes: [u8; 4] = data
        .get(offset..offset + 4)
        .and_then(|window| window.try_into().ok())
        .ok_or(TextureError::TruncatedEnvelope {
            available: data.len(),
        })?;
    Ok(u32::from_le_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_dimension_never_reaches_zero() {
        assert_eq!(mip_dimension(512, 0), 512);
        assert_eq!(mip_dimension(512, 1), 256);
        assert_eq!(mip_dimension(512, 9), 1);
        assert_eq!(mip_dimension(512, 10), 1);
        assert_eq!(mip_dimension(512, 31), 1);
        assert_eq!(mip_dimension(512, 32), 1);
        assert_eq!(mip_dimension(1, 0), 1);
        assert_eq!(mip_dimension(1, 31), 1);
        assert_eq!(mip_dimension(7, 1), 3);
    }

    #[test]
    fn the_level_slice_bounds_are_a_typed_error_not_a_panic() {
        // The arm `decode_raster` cannot currently reach, exercised directly: a
        // slice that would run past the record must be refused by name.
        let data = [0u8; 32];
        assert_eq!(slice_at(&data, 24, 8, 0), Ok(&data[24..32]));
        let error = slice_at(&data, 28, 8, 2).unwrap_err();
        assert_eq!(
            error,
            TextureError::LevelSlicePastRecord {
                mip: 2,
                offset: 28,
                size: 8,
                record_size: 32
            }
        );
        // Offset + size overflowing a usize is the same refusal, not a wrap.
        let error = slice_at(&data, usize::MAX, 8, 0).unwrap_err();
        assert!(matches!(error, TextureError::LevelSlicePastRecord { .. }));
        let error = slice_at(&data, usize::MAX - 4, 8, 0).unwrap_err();
        assert!(matches!(error, TextureError::LevelSlicePastRecord { .. }));
    }
}
