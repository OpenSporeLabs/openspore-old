//! The `0x15` family: the uncompressed four-lane raster, decoded from its bytes.
//!
//! # This is the family `docs/MATERIALS-DESIGN.md` §1 called "luminance"
//!
//! It is **not** luminance, and the corpus says so plainly. The name survives
//! only because the vocabulary of this repository already carries it
//! ([`crate::claims::LUMINANCE_FOURCC`]); everything below is what the records
//! actually are. The full derivation is in `docs/LUMINANCE-RASTER.md`; the short
//! version:
//!
//! * The family is **uncompressed**. There is no block codec, no endpoint pair,
//!   no index stream. Every texel is four bytes on disk.
//! * The four bytes are a **tangent-space normal map plus a fourth lane**:
//!   `Z, X, Y, mask` in file order. Seven of the 901 records in
//!   `Spore_Content.package` have lanes 0-2 *constant* at `(255, 128, 128)`,
//!   which is exactly the encoding of the flat normal `(0, 0, 1)`; and 901 of
//!   901 records have lane 0's mean above 230 with lanes 1 and 2 centred on
//!   127.5.
//! * Its framing differs from the block formats in exactly two measurable ways,
//!   and both are now in the shared table in [`crate::framing`]: the header is
//!   **28** bytes rather than 32, and the length prefix starts at **level 1**
//!   rather than level 2.
//!
//! # Why there is no `decode_luminance_block`
//!
//! There is no block. [`decode_luminance_mip`] copies `width * height * 4`
//! bytes and validates that it has them; that is the whole codec. Writing a
//! block decoder here would be inventing a structure the corpus does not have,
//! so the module has no `BLOCK_BYTES` and no palette, and says so.

use crate::error::TextureError;
use crate::framing::{level_dimension, level_size, LUMINANCE_FRAME};
use crate::raster::MipImage;

/// Format word at envelope offset `0x14` for this family: `0x00000015`.
///
/// Stored little-endian, so the byte on disk is `0x15` followed by three zero
/// bytes. It is deliberately **not** a FOURCC: `'DXT5'` is four bytes of
/// printable tag, this is one byte of format enum plus padding, and that
/// difference is part of why the family needs its own frame.
pub const LUMINANCE_FOURCC: u32 = 0x0000_0015;

/// Bytes in the `0x15` header: seven little-endian `u32` words.
///
/// It is **four bytes shorter** than [`crate::ENVELOPE_SIZE`] because the word
/// at `0x1c` is not a field of this format at all: it is the first four bytes
/// of the base-level payload. `RasterEnvelope::field_1c` still *reports* those
/// bytes, and this module documents that reading them as a field is a category
/// error — see [`crate::claims::ENVELOPE_FIELD_1C_MEANING`].
pub const LUMINANCE_HEADER_SIZE: usize = 28;

/// Bytes per texel. Four, uncompressed: no block, no stride, no padding.
pub const LUMINANCE_BYTES_PER_TEXEL: usize = 4;

/// Bytes in the little-endian `u32` that precedes every mip past the base
/// level, stating that mip's own payload length.
pub const LUMINANCE_PREFIX_SIZE: usize = crate::framing::LEVEL_PREFIX_SIZE;

/// A compile-time agreement between the two ways this module says "four bytes
/// per texel". They are the same fact written twice, which is exactly the kind
/// of duplication that rots, so the compiler checks it instead of a comment.
const _: () = assert!(LUMINANCE_BYTES_PER_TEXEL == 4);

/// Dimension of one axis at one mip level: `max(1, base >> mip)`.
///
/// The same rule and the same [`MIP_FLOOR`](crate::MIP_FLOOR) cut as the block
/// walk, for the same reason: the chain identity only closes if every level's
/// dimension is derived the one way every implementation derives it.
pub fn luminance_dimension(base: u32, mip: u32) -> u32 {
    level_dimension(base, mip)
}

/// Payload bytes of one mip level, saturating.
///
/// `mip 0` is the base level. `max(1, w>>m) * max(1, h>>m) * 4`.
pub fn luminance_mip_size(width: u32, height: u32, mip: u32) -> usize {
    level_size(LUMINANCE_FRAME, width, height, mip)
}

/// Payload bytes of the whole mip chain, **excluding** the header and
/// **including** the four length prefixes that precede every mip past the base
/// level.
///
/// Saturating, like [`luminance_mip_size`].
pub fn luminance_chain_size(width: u32, height: u32, mip_count: u32) -> usize {
    crate::framing::chain_size(LUMINANCE_FRAME, width, height, mip_count)
}

/// The one record size an envelope of this family can legitimately have.
///
/// Exposed because the identity *is* the format finding: a caller can check a
/// record's framing without decoding a single texel, and the test suite pins it
/// against the three shapes measured in the corpus.
pub fn luminance_record_size(width: u32, height: u32, mip_count: u32) -> usize {
    crate::framing::record_size(LUMINANCE_FRAME, width, height, mip_count)
}

/// Decodes one mip level's payload into row-major RGBA8.
///
/// There is no codec. The payload is four bytes per texel in file order, so
/// this validates the length and copies. `width`/`height` are the **mip's own**
/// dimensions, not the base level's, and the output is exactly
/// `width * height * 4` bytes.
///
/// A shortfall is [`TextureError::ShortLuminanceMip`], never a partial image:
/// the repository's rule is that a refused decode returns nothing.
pub fn decode_luminance_mip(img: &[u8], width: u32, height: u32) -> Result<MipImage, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::ZeroDimension { width, height });
    }
    let needed = luminance_mip_size(width, height, 0);
    if needed == usize::MAX {
        return Err(TextureError::OutputTooLarge { width, height });
    }
    if img.len() < needed {
        return Err(TextureError::ShortLuminanceMip {
            available: img.len(),
            needed,
            width,
            height,
        });
    }
    let out_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|count| count.checked_mul(LUMINANCE_BYTES_PER_TEXEL));
    let out_len = match out_len {
        Some(len) if len <= isize::MAX as usize => len,
        _ => return Err(TextureError::OutputTooLarge { width, height }),
    };
    // `needed == out_len` by construction: both are `w * h * 4` from the same
    // two arguments. The bound is written out anyway so that changing either
    // formula without the other fails here instead of in a caller.
    if needed != out_len {
        return Err(TextureError::LuminanceInvariant {
            width,
            height,
            needed,
            out_len,
        });
    }
    let pixels = img
        .get(..out_len)
        .ok_or(TextureError::ShortLuminanceMip {
            available: img.len(),
            needed,
            width,
            height,
        })?
        .to_vec();
    Ok(MipImage {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_corpus_shapes_consume_exactly() {
        // (width, height, mip_count, measured record_size) for every shape the
        // 0x15 family occurs in, measured over 901 records of
        // Spore_Content.package. This is the identity the whole format rests
        // on; if it stops holding, the framing is wrong, not the arithmetic.
        for (w, h, mips, size) in [
            (32u32, 32u32, 6u32, 5_508usize),
            (256, 256, 9, 349_584),
            (512, 512, 10, 1_398_164),
        ] {
            assert_eq!(
                luminance_record_size(w, h, mips),
                size,
                "{w}x{h} mips={mips}"
            );
        }
    }

    #[test]
    fn the_base_level_needs_no_prefix_and_the_rest_do() {
        // The base level's size is already in the header (field_18); every
        // other level carries its own count. Chain = base + sum(prefix + level).
        let w = 256u32;
        let h = 256u32;
        let mips = 9u32;
        let base = luminance_mip_size(w, h, 0);
        let mut manual = base;
        for mip in 1..mips {
            manual += LUMINANCE_PREFIX_SIZE + luminance_mip_size(w, h, mip);
        }
        assert_eq!(luminance_chain_size(w, h, mips), manual);
        // 256*256*4 = 262144 is field_18 of every 256^2 record in the corpus.
        assert_eq!(base, 262_144);
    }

    #[test]
    fn the_chain_climbs_by_exactly_four_over_twenty_six() {
        // 349_584 - 262_144 - 32 = 87_408 for the 128^2 level alone; the point
        // of this test is that no level is free and none is charged twice.
        assert_eq!(luminance_mip_size(256, 256, 1), 65_536);
        assert_eq!(luminance_chain_size(256, 256, 1), 262_144);
        assert_eq!(luminance_chain_size(256, 256, 2), 262_144 + 4 + 65_536);
    }

    #[test]
    fn dimensions_use_the_same_floor_as_the_block_walk() {
        assert_eq!(luminance_dimension(512, 0), 512);
        assert_eq!(luminance_dimension(512, 9), 1);
        assert_eq!(luminance_dimension(512, 32), 1);
        assert_eq!(luminance_dimension(512, 99), 1);
        assert_eq!(luminance_dimension(1, 31), 1);
    }

    #[test]
    fn beyond_32_mips_are_each_charged_a_prefix_plus_one_texel() {
        // 33 mips: mips 0..31 are walked, mip 32 is 1x1 with its prefix. The
        // rule is an approximation reproduced from the shared floor, exactly as
        // the block walk's is, and it exists to keep the shift defined.
        let walked = luminance_chain_size(4, 4, 32);
        let beyond = luminance_chain_size(4, 4, 33);
        assert_eq!(beyond, walked + LUMINANCE_PREFIX_SIZE + 4);
    }

    #[test]
    fn a_short_payload_is_refused_and_never_truncated() {
        let payload = [0u8; 3 * 2 * 4];
        let image = decode_luminance_mip(&payload, 2, 2).unwrap();
        assert_eq!(image.pixels.len(), 16);
        let error = decode_luminance_mip(&payload[..15], 2, 2).unwrap_err();
        assert_eq!(
            error,
            TextureError::ShortLuminanceMip {
                available: 15,
                needed: 16,
                width: 2,
                height: 2
            }
        );
    }

    #[test]
    fn zero_dimensions_are_refused() {
        assert!(matches!(
            decode_luminance_mip(&[], 0, 4),
            Err(TextureError::ZeroDimension { .. })
        ));
    }

    #[test]
    fn absurd_dimensions_saturate_rather_than_wrap() {
        // The point of saturating: a caller must be able to tell "unaddressable"
        // from "small". A wrapping multiply would return a few bytes here, which
        // is the failure mode the saturating form exists to prevent.
        assert!(luminance_mip_size(u32::MAX, u32::MAX, 0) > u32::MAX as usize);
        assert_eq!(
            luminance_chain_size(4, 4, 0),
            0,
            "a zero mip count is no chain"
        );
    }
}
