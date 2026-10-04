//! The 32-byte raster envelope, the derived layer split, and the per-layer mip
//! walk.
//!
//! # The envelope
//!
//! ```text
//! off  field       observed value   meaning
//! 0x00 version     1                 format revision (only 1 seen)
//! 0x04 width                          texels
//! 0x08 height                         texels
//! 0x0c mip_count                      mip levels in every layer
//! 0x10 field_10    8                 UNRESOLVED — never read by any decoder
//! 0x14 fourcc      0x35545844        'DXT5', stored little-endian
//! 0x18 field_18    0x00040000        UNRESOLVED — never read by any decoder
//! 0x1c field_1c    0x0000FFFF        format-specific (float 1.0 in 0x15 records)
//! ```
//!
//! All eight words are little-endian `u32`, byte-pinned in
//! `docs/MATERIALS-DESIGN.md` §1 against the 512² DXT5 records `0x40662900/01`,
//! the `raster_ap_5552`/`raster_0` family (same layout) and the 256²
//! `raster_6948` (format `0x15`).
//!
//! **Only `fourcc` drives decoding.** `field_10` and `field_18` hold the same
//! value in every sampled record and nobody knows what they are; `field_1c` is
//! described by the oracle as format-specific. They are parsed and carried
//! verbatim so a caller can see them, and they are given no semantics here —
//! see [`crate::claims`], where each is graded `UNKNOWN` rather than guessed.
//!
//! Any `fourcc` other than `0x35545844` (`DXT5`) and `0x00000015` is rejected by
//! name. Those two are **different formats**, not two spellings of one: DXT5
//! stores 4x4 BC3-shaped blocks with 16-byte layer headers, while the `0x15`
//! family is uncompressed, has no layer headers, and prefixes every mip past
//! the base level with its own byte count. Feeding either to the other's codec
//! would produce plausible garbage rather than an error, so the walk dispatches
//! on `fourcc` and each branch is held to its own framing identity — see
//! [`crate::luminance`] and `docs/LUMINANCE-RASTER.md`.
//!
//! # The layer split is derived, never read
//!
//! Spore's raster records store no layer count. It is computed, and **the rule
//! is per-format** because the two formats frame themselves differently.
//!
//! **DXT5** (`0x35545844`), 32-byte envelope:
//!
//! ```text
//! per = 16 + chain_size(width, height, mip_count)
//! n   = (record_size - 32) / per          // must divide EXACTLY
//! ```
//!
//! and a remainder is a hard error, because rounding it would invent a layer
//! the record does not have. On every real 512² DXT5 record this yields
//! `n = 2`, verified in `docs/MATERIALS-DESIGN.md` §1.
//!
//! **The layout assumption, which is load-bearing**: all `n` 16-byte layer
//! headers come first, then the layer payloads, so layer `i`'s data starts at
//! `32 + n * 16 + i * chain_size`. Verified against real records (`payloadOff`
//! `= 0x40` for `n = 2`, and the first block at that offset is a valid DXT5
//! block). The 16 header bytes are **not interpreted**: they read as a 4-byte
//! name (`"e9D1"` in one sample) plus three words, and nobody knows what the
//! words are. They are skipped, which is the only thing the crate can honestly
//! do with them.
//!
//! **`0x15`** (`0x00000015`), 28-byte header and **no layer headers at all**: the
//! count is always 1, and the check is stronger than a division — the record
//! size must equal `28 + luminance_chain_size(...)` exactly, with no remainder
//! and nothing unconsumed. See [`crate::luminance`].
//!
//! # Strictness differences against the references
//!
//! * This port rejects `width == 0` / `height == 0`; the Python oracle does not
//!   and would return an empty image. The C++ reference rejects it.
//! * This port rejects `mip_count == 0`; both references reject it in
//!   `decode`, but the Python `layer_count` alone would accept it and derive a
//!   16-byte stride.
//! * This port refuses a decode whose output would exceed `isize::MAX` bytes;
//!   the C++ reference would fail the allocation instead.

use crate::dxt5::{decode_dxt5_mip, dxt5_chain_size, dxt5_mip_size, MIP_FLOOR};
use crate::error::TextureError;
use crate::luminance::{
    decode_luminance_mip, luminance_chain_size, luminance_dimension, luminance_mip_size,
    LUMINANCE_HEADER_SIZE,
};
use crate::{DXT5_FOURCC, ENVELOPE_SIZE, LAYER_HEADER_SIZE, LUMINANCE_FOURCC};

/// The 32-byte raster envelope.
///
/// Every field is reproduced verbatim. Nothing here has been given a meaning
/// this crate does not have evidence for: see the module documentation and
/// [`crate::claims`].
///
/// **The struct is shared by two formats with different header lengths.** DXT5
/// has a 32-byte envelope whose last word (`field_1c`) reads `0x0000FFFF`. The
/// `0x15` family has a **28-byte** header, and `field_1c` in that format is
/// *the first four bytes of the base-level payload*, not a field — see
/// [`crate::claims::ENVELOPE_FIELD_1C_MEANING`]. The word is still reported
/// here because a caller reading an envelope should see the bytes that are
/// there, and the field's documentation says which they are.
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
    /// Offset `0x0c`. Mip levels present in every layer. Must be non-zero.
    pub mip_count: u32,
    /// Offset `0x10`. **Unresolved**: observed `8` in every sample. Parsed and
    /// carried; never read. Graded `UNKNOWN` in [`crate::claims`].
    pub field_10: u32,
    /// Offset `0x14`. Must be [`DXT5_FOURCC`] or [`LUMINANCE_FOURCC`] for this
    /// crate to decode.
    pub fourcc: u32,
    /// Offset `0x18`. **Format-specific.** Reads `0x0004_0000` in the DXT5
    /// records sampled so far, and in every `0x15` record it is exactly
    /// `width * height * 4` — the base level's payload size (901 of 901 records
    /// in `Spore_Content.package`). Carried; never read, because the decoder
    /// derives the same quantity from the geometry and the crate's rule is
    /// "derived, never read". Graded in [`crate::claims`].
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

/// A whole raster record: its envelope and every layer's mip chain.
///
/// `layers[layer][mip]` is the `mip`-th level of the `layer`-th layer, with
/// `mip == 0` the base level. The layer count is **derived** from the record
/// size (see the module documentation), so it can legitimately differ between
/// two records that share an envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RasterImage {
    /// The envelope the layers were decoded under.
    pub envelope: RasterEnvelope,
    /// Layer mip chains, in record order.
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
/// `max(1, base >> mip)`, with every mip at or beyond [`MIP_FLOOR`] clamped to
/// 1 — the same rule both reference implementations use, and the reason
/// [`dxt5_mip_size`] and the decoded [`MipImage`] dimensions always agree.
pub fn mip_dimension(base: u32, mip: u32) -> u32 {
    if mip >= MIP_FLOOR {
        return 1;
    }
    (base >> mip).max(1)
}

/// Derives how many layers a record of `record_size` bytes holds.
///
/// The count is computed from the record size and the envelope, never read from
/// a header field, because the record stores no such field. **The rule is
/// per-format**, because the two formats frame themselves differently:
///
/// * `DXT5`: `n = (record_size - 32) / (16 + dxt5_chain_size(...))`, which must
///   divide **exactly**.
/// * `LUMINANCE_FOURCC` (`0x15`): there are no layer headers and no layer count
///   to divide for, so the answer is always **1** and the check is the stronger
///   one — `record_size` must equal
///   `LUMINANCE_HEADER_SIZE + luminance_chain_size(...)` to the byte.
///
/// Checked in this order: record smaller than the envelope, unsupported
/// `fourcc`, zero dimension, zero mip count, an unaddressable chain, payload
/// smaller than one layer, payload not a multiple of the layer stride.
pub fn layer_count_for(record_size: usize, env: &RasterEnvelope) -> Result<usize, TextureError> {
    if record_size < ENVELOPE_SIZE {
        return Err(TextureError::RecordShorterThanEnvelope { record_size });
    }
    if env.width == 0 || env.height == 0 {
        return Err(TextureError::ZeroDimension {
            width: env.width,
            height: env.height,
        });
    }
    if env.mip_count == 0 {
        return Err(TextureError::ZeroMipCount);
    }

    // The `0x15` branch first, because it has no layer stride to divide by and
    // because its header is 4 bytes shorter than `ENVELOPE_SIZE`.
    if env.fourcc == LUMINANCE_FOURCC {
        let expected = luminance_record_size_of(env);
        if record_size != expected {
            return Err(TextureError::LuminanceRecordSize {
                record_size,
                expected,
                width: env.width,
                height: env.height,
                mip_count: env.mip_count,
            });
        }
        return Ok(1);
    }

    if env.fourcc != DXT5_FOURCC {
        return Err(TextureError::UnsupportedFourcc { fourcc: env.fourcc });
    }
    let chain = dxt5_chain_size(env.width, env.height, env.mip_count);
    if chain == usize::MAX || chain > usize::MAX - LAYER_HEADER_SIZE {
        return Err(TextureError::ChainSizeOverflow {
            width: env.width,
            height: env.height,
            mip_count: env.mip_count,
        });
    }
    let per_layer = LAYER_HEADER_SIZE + chain;
    let remaining = record_size - ENVELOPE_SIZE;
    if remaining < per_layer {
        return Err(TextureError::PayloadTooSmall {
            available: remaining,
            needed: per_layer,
        });
    }
    if remaining % per_layer != 0 {
        return Err(TextureError::PayloadNotMultiple {
            available: remaining,
            per: per_layer,
        });
    }
    Ok(remaining / per_layer)
}

/// The one record size an envelope's geometry implies, saturating.
///
/// A saturating `usize::MAX` can never equal a `record_size`, so an
/// unaddressable envelope takes the [`TextureError::LuminanceRecordSize`] arm
/// with a number no real record can match — the same refusal the DXT5 branch
/// gives through [`TextureError::ChainSizeOverflow`].
fn luminance_record_size_of(env: &RasterEnvelope) -> usize {
    LUMINANCE_HEADER_SIZE.saturating_add(luminance_chain_size(env.width, env.height, env.mip_count))
}

/// Decodes a whole raster record: envelope, derived layer split, every mip of
/// every layer.
///
/// Rejects a record shorter than the envelope, a zero dimension, a `fourcc`
/// this build does not decode (`DXT1` is present in the corpus and is not one of
/// them), `mip_count == 0`, a payload that does not match its format's framing
/// exactly, a length prefix that disagrees with its mip's geometry, and a mip
/// slice that would run past the record. Nothing is returned on any error: there
/// is no partially decoded image.
///
/// The walk dispatches on `fourcc`. DXT5 takes the grouped-header path documented
/// in this module; `0x15` takes [`crate::luminance`]'s path, which has one layer
/// and a per-mip length prefix.
pub fn decode_raster(data: &[u8]) -> Result<RasterImage, TextureError> {
    let envelope = parse_envelope(data)?;
    if envelope.width == 0 || envelope.height == 0 {
        return Err(TextureError::ZeroDimension {
            width: envelope.width,
            height: envelope.height,
        });
    }
    if envelope.fourcc != DXT5_FOURCC && envelope.fourcc != LUMINANCE_FOURCC {
        return Err(TextureError::UnsupportedFourcc {
            fourcc: envelope.fourcc,
        });
    }
    if envelope.mip_count == 0 {
        return Err(TextureError::ZeroMipCount);
    }

    let layer_count = layer_count_for(data.len(), &envelope)?;
    if envelope.fourcc == LUMINANCE_FOURCC {
        return decode_luminance_layers(data, envelope);
    }

    let chain_size = dxt5_chain_size(envelope.width, envelope.height, envelope.mip_count);
    // Every one of the `n` layer headers precedes every payload: this is the
    // load-bearing layout assumption, and `header_bytes` is its only use.
    let header_bytes =
        layer_count
            .checked_mul(LAYER_HEADER_SIZE)
            .ok_or(TextureError::ChainSizeOverflow {
                width: envelope.width,
                height: envelope.height,
                mip_count: envelope.mip_count,
            })?;
    let mut layers = Vec::with_capacity(layer_count);
    for layer in 0..layer_count {
        // Layer i's payload: past the envelope, past *all* the headers, then i
        // whole mip chains along. The stride is the chain, not chain+16: the
        // headers are all at the front, which is exactly the assumption this
        // offset encodes.
        let payload_offset = ENVELOPE_SIZE
            .checked_add(header_bytes)
            .and_then(|base| base.checked_add(layer.checked_mul(chain_size)?))
            .ok_or(TextureError::ChainSizeOverflow {
                width: envelope.width,
                height: envelope.height,
                mip_count: envelope.mip_count,
            })?;

        let mut mips = Vec::with_capacity(envelope.mip_count as usize);
        let mut cursor = payload_offset;
        for mip in 0..envelope.mip_count {
            let size = dxt5_mip_size(envelope.width, envelope.height, mip);
            let bytes = slice_at(data, cursor, size, layer, mip)?;
            let width = mip_dimension(envelope.width, mip);
            let height = mip_dimension(envelope.height, mip);
            mips.push(decode_dxt5_mip(bytes, width, height)?);
            cursor = cursor.saturating_add(size);
        }
        layers.push(mips);
    }

    Ok(RasterImage { envelope, layers })
}

/// Walks a `0x15` record: one layer, no layer headers, a length prefix in front
/// of every mip past the base level.
///
/// The exactness check has already run in [`layer_count_for`], so this walk
/// re-reads each prefix rather than deriving past it. That is deliberate and it
/// is the point of the format: the prefix is the record's own statement of how
/// long the level is, so a decoder that ignored it would be discarding the only
/// thing the record says about its own framing. Disagreement is
/// [`TextureError::LuminanceLengthPrefix`], by name.
fn decode_luminance_layers(
    data: &[u8],
    envelope: RasterEnvelope,
) -> Result<RasterImage, TextureError> {
    let past_end =
        |mip: u32, offset: usize, needed: usize| TextureError::LuminanceSlicePastRecord {
            mip,
            offset,
            needed,
            record_size: data.len(),
        };

    let mut mips = Vec::with_capacity(envelope.mip_count as usize);
    let mut cursor = LUMINANCE_HEADER_SIZE;
    for mip in 0..envelope.mip_count {
        let width = luminance_dimension(envelope.width, mip);
        let height = luminance_dimension(envelope.height, mip);
        let size = luminance_mip_size(envelope.width, envelope.height, mip);
        if mip > 0 {
            let prefix = data
                .get(cursor..cursor + 4)
                .and_then(|window| <[u8; 4]>::try_from(window).ok())
                .map(u32::from_le_bytes)
                .ok_or_else(|| past_end(mip, cursor, 4))?;
            cursor += 4;
            if prefix as usize != size {
                return Err(TextureError::LuminanceLengthPrefix {
                    mip,
                    declared: prefix as usize,
                    expected: size,
                    offset: cursor - 4,
                    width,
                    height,
                });
            }
        }
        let bytes = data
            .get(cursor..cursor.saturating_add(size))
            .ok_or_else(|| past_end(mip, cursor, size))?;
        mips.push(decode_luminance_mip(bytes, width, height)?);
        cursor += size;
    }
    // `layer_count_for` already required the record to be exactly this long, so
    // `cursor == data.len()` is guaranteed; the assertion-free way to keep that
    // honest is to *use* the fact rather than to claim it.
    debug_assert_eq!(
        cursor,
        data.len(),
        "layer_count_for accepted a 0x15 record whose walk does not consume it exactly"
    );

    Ok(RasterImage {
        envelope,
        layers: vec![mips],
    })
}

/// Bounds one mip's slice, refusing to read past the record.
///
/// Currently unreachable through [`decode_raster`]'s DXT5 path: the derived
/// split gives each layer exactly `chain_size` bytes and `chain_size` is the sum
/// of that layer's mip sizes, so the last mip ends exactly at the record end. It
/// is kept as the bound on that assumption — see
/// [`TextureError::MipSlicePastRecord`].
fn slice_at(
    data: &[u8],
    offset: usize,
    size: usize,
    layer: usize,
    mip: u32,
) -> Result<&[u8], TextureError> {
    let past_end = || TextureError::MipSlicePastRecord {
        layer,
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
    fn the_mip_slice_bounds_are_a_typed_error_not_a_panic() {
        // The arm `decode_raster` cannot currently reach, exercised directly:
        // a slice that would run past the record must be refused by name.
        let data = [0u8; 32];
        assert_eq!(slice_at(&data, 24, 8, 0, 0), Ok(&data[24..32]));
        let error = slice_at(&data, 28, 8, 1, 2).unwrap_err();
        assert_eq!(
            error,
            TextureError::MipSlicePastRecord {
                layer: 1,
                mip: 2,
                offset: 28,
                size: 8,
                record_size: 32
            }
        );
        // Offset + size overflowing a usize is the same refusal, not a wrap.
        let error = slice_at(&data, usize::MAX, 8, 0, 0).unwrap_err();
        assert!(matches!(error, TextureError::MipSlicePastRecord { .. }));
        let error = slice_at(&data, usize::MAX - 4, 8, 0, 0).unwrap_err();
        assert!(matches!(error, TextureError::MipSlicePastRecord { .. }));
    }
}
