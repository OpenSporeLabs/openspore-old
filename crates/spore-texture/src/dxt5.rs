//! The 16-byte BC3 block codec, and the 8-byte BC1 block codec.
//!
//! # This replaces an 8-byte "Spore block" that the corpus refutes
//!
//! The previous decoder read **8 bytes per 4x4 block**, drove a 2-bit index
//! stream from bytes 4..8, read the "colour" from bytes 2..3 as a 6-bit word
//! expanded R5G5B5, and inverted the palette-size test. That is not what the
//! records contain, and the records say so three separate ways:
//!
//! 1. **The bytes do not divide.** A 256² DXT5 base level occupies 65 536 bytes
//!    in the record — 16 bytes per 4x4 block. At 8 bytes it would be 32 768, and
//!    the recorded level prefixes are the 16-byte figures in 1747 of 1747
//!    records. See [`crate::framing`].
//! 2. **The old codec decodes noise.** Horizontal total variation of the
//!    decoded alpha, mean over 25 sampled records per shape: BC3 gives
//!    1.95 (512²), 8.72 (256²) and 2.20 (64²); the 8-byte rule gives 60.49,
//!    106.06 and 50.25. A factor of 20–50 on every shape.
//! 3. **"DXT5 records carry no colour" was a tautology.** Green and blue read as
//!    0 under R5G5B5 on a 6-bit word *by construction*, because a 6-bit value
//!    cannot set bits 10..19. Read as BC3 the same records carry 32 distinct
//!    greens and 32 distinct blues. The old claim measured its own codec, not
//!    the data.
//!
//! What is graded where is in [`crate::claims`]: the **framing** is `Verified`
//! (an exact identity over 2954 of 2954 records) and the **codec** is
//! `Inferred` — BC3 is the published reading of a record that declares `DXT5`,
//! is charged 16 bytes per block by the records themselves, and beats the
//! alternative by an order of magnitude on every shape, but no decode of a real
//! Spore DXT5 record has been compared against the game's own output.
//!
//! # BC3 block layout (16 bytes)
//!
//! ```text
//! b[0]      alpha0    u8
//! b[1]      alpha1    u8
//! b[2..8]   16 three-bit texel indices, texel i at bits 3*i of the little-endian
//!                      48-bit word b[2..8]
//! b[8..10]  colour0   u16, R5G6B5 little-endian
//! b[10..12] colour1   u16
//! b[12..16] 16 two-bit colour indices, texel i at bits 2*i of the little-endian
//!                      32-bit word b[12..16]
//! ```
//!
//! **Alpha** (BC4). `alpha0 > alpha1` gives eight levels, `alpha0 <= alpha1`
//! gives six; the published spec leaves indices 6 and 7 *undefined* in the
//! six-level case, and this decoder **clamps them to `alpha0`** rather than
//! inventing an interpolation. That clamp is the one place a real record could
//! disagree with this decoder, so it is a claim — see
//! [`crate::claims::BC3_ALPHA_SIX_LEVEL_CLAMP`].
//!
//! **Colour** (BC1). `R5G6B5` expanded 5→8 by replication (`(v << 3) | (v >> 2)`
//! for the alpha half and `(v << 2) | (v >> 4)` for the six-bit colour half).
//! `colour0 > colour1` gives four colours; `colour0 <= colour1` gives four plus
//! two "half" entries, which is the reading this decoder uses (the alternative
//! is to make entries 4 and 5 transparent, which is DXT1's punch-through mode).
//! That choice is a claim: [`crate::claims::BC3_COLOUR_SIX_LEVEL_ENTRIES`].
//!
//! **Alpha and colour are independent.** There is one index stream per half and
//! they are *not* the same index, which is the single structural difference
//! from the codec this module replaces (that one drove both channels from one
//! 2-bit stream).

use crate::error::TextureError;
use crate::raster::MipImage;

/// Bytes in one 4x4 **BC3** block.
pub const BLOCK_BYTES: usize = 16;

/// Bytes in one 4x4 **BC1** block.
pub const BC1_BLOCK_BYTES: usize = 8;

/// Mip levels at or above this index are charged a single block.
///
/// The shift `w >> mip` is only defined for `mip < 32` in Rust, so every mip at
/// or beyond 32 is treated as 1x1 — one block. The reference implementations
/// make the same cut; see [`crate::claims::MIPS_BEYOND_32`].
pub const MIP_FLOOR: u32 = 32;

/// Texels along one block edge.
const BLOCK_EDGE: u32 = 4;

/// Texels in one block.
const TEXELS_PER_BLOCK: usize = (BLOCK_EDGE * BLOCK_EDGE) as usize;

/// Bytes per output texel: RGBA8.
const RGBA_BYTES: usize = 4;

/// Bits of an alpha index.
const ALPHA_INDEX_BITS: u32 = 3;

/// Mask of an alpha index.
const ALPHA_INDEX_MASK: u64 = 0x7;

/// Bits of a colour index.
const COLOUR_INDEX_BITS: u32 = 2;

/// Mask of a colour index.
const COLOUR_INDEX_MASK: u32 = 0x3;

/// Mask of one 5-bit channel.
const FIVE_BIT_MASK: u32 = 0x1F;

/// Mask of one 6-bit channel.
const SIX_BIT_MASK: u32 = 0x3F;

/// Shift of the red channel in a 16-bit `R5G6B5` colour word.
const R5_SHIFT: u32 = 11;

/// Shift of the green channel in a 16-bit `R5G6B5` colour word.
const G5_SHIFT: u32 = 5;

/// Divisor of the two-thirds colour blend.
const COLOUR_BLEND_THIRDS: u32 = 3;

/// Divisor of the three-fifths colour blend, the two "half" palette entries.
const COLOUR_BLEND_FIFTHS: u32 = 5;

/// Divisor of the seven-way alpha ramp.
const ALPHA_DIVISOR: u32 = 7;

/// Divisor of the five-way alpha ramp.
const ALPHA_DIVISOR_FIVE: u32 = 5;

/// `ceil(n / 4)`, the block count along one axis.
fn ceil_div4(n: u32) -> u32 {
    n / BLOCK_EDGE + if n % BLOCK_EDGE != 0 { 1 } else { 0 }
}

/// Raw bytes of the mip chain for one mip level of a base size, at `block_bytes`
/// per block.
///
/// `mip 0` is the base level itself. Every mip at or beyond [`MIP_FLOOR`] is
/// charged a single block. All arithmetic saturates: an absurd `width * height`
/// yields `usize::MAX` rather than a wrapped small number, which is what a caller
/// must be able to tell apart from "small texture".
pub fn block_mip_size(width: u32, height: u32, mip: u32, block_bytes: usize) -> usize {
    if mip >= MIP_FLOOR {
        return block_bytes;
    }
    let w = (width >> mip).max(1);
    let h = (height >> mip).max(1);
    let blocks = (ceil_div4(w) as usize).saturating_mul(ceil_div4(h) as usize);
    blocks.saturating_mul(block_bytes)
}

/// Payload bytes of one mip level of a **DXT5** chain.
pub fn dxt5_mip_size(width: u32, height: u32, mip: u32) -> usize {
    block_mip_size(width, height, mip, BLOCK_BYTES)
}

/// Payload bytes of the whole chain of one **DXT5** record.
///
/// The sum is taken over `min(mip_count, 32)` mips and then charges
/// [`BLOCK_BYTES`] for **each mip beyond 32**. That beyond-32 rule is a
/// documented approximation reproduced from the references; it is not a claim
/// about how Spore writes a 33rd mip, because no record has one.
pub fn dxt5_chain_size(width: u32, height: u32, mip_count: u32) -> usize {
    if mip_count == 0 {
        return 0;
    }
    let mut total = 0usize;
    for mip in 0..mip_count.min(MIP_FLOOR) {
        total = total.saturating_add(dxt5_mip_size(width, height, mip));
    }
    if mip_count > MIP_FLOOR {
        let extra = (mip_count - MIP_FLOOR) as usize;
        total = total.saturating_add(extra.saturating_mul(BLOCK_BYTES));
    }
    total
}

/// Payload bytes of one mip level of a **DXT1** chain.
pub fn dxt1_mip_size(width: u32, height: u32, mip: u32) -> usize {
    block_mip_size(width, height, mip, BC1_BLOCK_BYTES)
}

/// Payload bytes of the whole chain of one **DXT1** record.
pub fn dxt1_chain_size(width: u32, height: u32, mip_count: u32) -> usize {
    if mip_count == 0 {
        return 0;
    }
    let mut total = 0usize;
    for mip in 0..mip_count.min(MIP_FLOOR) {
        total = total.saturating_add(dxt1_mip_size(width, height, mip));
    }
    if mip_count > MIP_FLOOR {
        let extra = (mip_count - MIP_FLOOR) as usize;
        total = total.saturating_add(extra.saturating_mul(BC1_BLOCK_BYTES));
    }
    total
}

/// Expands a 5-bit channel to 8 bits by bit replication: `(v << 3) | (v >> 2)`.
///
/// Replication rather than scaling (`v * 255 / 31`) is the published rule, and
/// the two are not the same function: they differ at exactly four of the 32
/// inputs (`3`, `7`, `24`, `28`), pinned by `expand5_replicates_rather_than_scales`.
const fn expand5(v: u32) -> u8 {
    ((v << 3) | (v >> 2)) as u8
}

/// Expands a 6-bit channel to 8 bits by bit replication: `(v << 2) | (v >> 4)`.
const fn expand6(v: u32) -> u8 {
    ((v << 2) | (v >> 4)) as u8
}

/// The alpha palette of one BC4 half-block: eight `u8` values.
///
/// Six-level palettes repeat `alpha0` in the last two slots, which is the
/// documented clamp for the indices the specification leaves undefined.
fn alpha_palette(alpha0: u8, alpha1: u8) -> [u8; 8] {
    let a0 = u32::from(alpha0);
    let a1 = u32::from(alpha1);
    if alpha0 > alpha1 {
        [
            alpha0,
            alpha1,
            ramp(a0, 6, a1, 1, ALPHA_DIVISOR),
            ramp(a0, 5, a1, 2, ALPHA_DIVISOR),
            ramp(a0, 4, a1, 3, ALPHA_DIVISOR),
            ramp(a0, 3, a1, 4, ALPHA_DIVISOR),
            ramp(a0, 2, a1, 5, ALPHA_DIVISOR),
            ramp(a0, 1, a1, 6, ALPHA_DIVISOR),
        ]
    } else {
        [
            alpha0,
            alpha1,
            ramp(a0, 4, a1, 1, ALPHA_DIVISOR_FIVE),
            ramp(a0, 3, a1, 2, ALPHA_DIVISOR_FIVE),
            ramp(a0, 2, a1, 3, ALPHA_DIVISOR_FIVE),
            ramp(a0, 1, a1, 4, ALPHA_DIVISOR_FIVE),
            alpha0,
            alpha0,
        ]
    }
}

/// `(a0 * n + a1 * d) / divisor`, clamped to a `u8`.
///
/// `a0` is at most 255 and every weight at most 6, so the numerator is at most
/// 2040 and the quotient at most 255; the clamp is belt and braces so a change
/// to the weights cannot wrap.
fn ramp(a0: u32, n: u32, a1: u32, d: u32, divisor: u32) -> u8 {
    let numerator = a0.saturating_mul(n).saturating_add(a1.saturating_mul(d));
    (numerator / divisor).min(u32::from(u8::MAX)) as u8
}

/// Reads one `R5G6B5` colour word as 8-bit RGB.
fn rgb5g6b5(word: u16) -> [u8; 3] {
    let v = u32::from(word);
    [
        expand5((v >> R5_SHIFT) & FIVE_BIT_MASK),
        expand6((v >> G5_SHIFT) & SIX_BIT_MASK),
        expand5(v & FIVE_BIT_MASK),
    ]
}

/// The colour palette of one BC1 half-block: four `R5G6B5` entries.
///
/// A BC3 block's colour half is a BC1 **colour** block: a 2-bit index, so exactly
/// four entries. `colour0 > colour1` and `colour0 <= colour1` both give four, and
/// the two blend weights differ between them (one third against one fifth).
///
/// The six-entry form of DXT1's punch-through mode is **not reachable here**: it
/// needs a 3-bit colour index, and no block in either format carries one. That is
/// worth stating because a decoder which "supports punch-through" on these bytes
/// is reading an index width the format does not have — and it is why the claim
/// about those two entries records them as unobservable rather than undecided.
fn colour_palette(colour0: u16, colour1: u16) -> [[u8; 3]; 4] {
    let c0 = rgb5g6b5(colour0);
    let c1 = rgb5g6b5(colour1);
    let blend = |a: [u8; 3], b: [u8; 3], n: u32, divisor: u32| -> [u8; 3] {
        let mix = |x: u8, y: u8| -> u8 {
            ((u32::from(x) * n + u32::from(y) * (divisor - n)) / divisor).min(u32::from(u8::MAX))
                as u8
        };
        [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
    };
    if colour0 > colour1 {
        [
            c0,
            c1,
            blend(c0, c1, 2, COLOUR_BLEND_THIRDS),
            blend(c0, c1, 1, COLOUR_BLEND_THIRDS),
        ]
    } else {
        [
            c0,
            c1,
            blend(c0, c1, 3, COLOUR_BLEND_FIFTHS),
            blend(c0, c1, 2, COLOUR_BLEND_FIFTHS),
        ]
    }
}

/// Decodes one mip's BC3 block chain into row-major RGBA8.
///
/// `img` must hold at least `dxt5_mip_size(width, height, 0)` bytes; extra
/// trailing bytes are ignored. `width`/`height` are the **mip's own** dimensions.
///
/// Output is exactly `width * height * 4` bytes. Texels of a block outside those
/// bounds are **skipped, not an error**: a 5x3 mip occupies two blocks and the
/// padding texels simply do not exist.
pub fn decode_dxt5_mip(img: &[u8], width: u32, height: u32) -> Result<MipImage, TextureError> {
    decode_block_mip(img, width, height, BLOCK_BYTES, true)
}

/// Decodes one mip's BC1 block chain into row-major RGBA8, alpha opaque.
///
/// BC1 carries no alpha, so every texel gets `255`. Everything else is the BC1
/// half of [`decode_dxt5_mip`].
pub fn decode_dxt1_mip(img: &[u8], width: u32, height: u32) -> Result<MipImage, TextureError> {
    decode_block_mip(img, width, height, BC1_BLOCK_BYTES, false)
}

/// The shared block walk for both codecs.
fn decode_block_mip(
    img: &[u8],
    width: u32,
    height: u32,
    block_bytes: usize,
    with_alpha: bool,
) -> Result<MipImage, TextureError> {
    if width == 0 || height == 0 {
        return Err(TextureError::ZeroDimension { width, height });
    }

    let needed = block_mip_size(width, height, 0, block_bytes);
    if needed == usize::MAX {
        return Err(TextureError::OutputTooLarge { width, height });
    }
    if img.len() < needed {
        return Err(TextureError::ShortBlockChain {
            available: img.len(),
            needed,
            width,
            height,
        });
    }

    let pixel_count = (width as usize).checked_mul(height as usize);
    let out_len = pixel_count.and_then(|count| count.checked_mul(RGBA_BYTES));
    let out_len = match out_len {
        Some(len) if len <= isize::MAX as usize => len,
        _ => return Err(TextureError::OutputTooLarge { width, height }),
    };

    let mut pixels = vec![0u8; out_len];
    let blocks_wide = ceil_div4(width);

    for block_row in 0..ceil_div4(height) {
        let row_top = block_row * BLOCK_EDGE;
        for block_col in 0..blocks_wide {
            let col_left = block_col * BLOCK_EDGE;
            let offset = ((block_row as usize) * (blocks_wide as usize) + (block_col as usize))
                * block_bytes;
            let bytes = img.get(offset..offset + block_bytes);
            write_block(
                &mut pixels,
                width,
                height,
                col_left,
                row_top,
                bytes,
                block_bytes,
                with_alpha,
            )?;
        }
    }

    Ok(MipImage {
        width,
        height,
        pixels,
    })
}

/// Decodes one block and writes its texels that fall inside the image.
///
/// `bytes` is `Option<&[u8]>` rather than `&[u8]` so the caller can slice the
/// chain without indexing it first; `None` yields
/// [`TextureError::ShortBlockChain`] instead of a panic. The size check in
/// [`decode_block_mip`] makes that path unreachable today, and it is kept so the
/// block reader has no way to read out of bounds.
#[allow(clippy::too_many_arguments)]
fn write_block(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    col_left: u32,
    row_top: u32,
    bytes: Option<&[u8]>,
    block_bytes: usize,
    with_alpha: bool,
) -> Result<(), TextureError> {
    let short_chain = || TextureError::ShortBlockChain {
        available: bytes.map_or(0, <[u8]>::len),
        needed: block_bytes,
        width,
        height,
    };
    let bytes = bytes.ok_or_else(short_chain)?;

    // The colour word lives at 8..10 for BC3 and at 0..2 for BC1; the alpha
    // endpoints at 0..2 for BC3 only. Both are read through `get` so a
    // short slice is a typed refusal rather than a panic.
    let (alpha, colour) = if with_alpha {
        let (Some(&a0), Some(&a1)) = (bytes.first(), bytes.get(1)) else {
            return Err(short_chain());
        };
        let indices = u64::from_le_bytes(pad6(bytes.get(2..8).unwrap_or(&[])));
        let palette = alpha_palette(a0, a1);
        let Some(colour) = bytes.get(8..16) else {
            return Err(short_chain());
        };
        let colour0 = read_u16(&colour[0..2]);
        let colour1 = read_u16(&colour[2..4]);
        let colour_indices = read_u32(&colour[4..8]);
        (
            Some((palette, indices)),
            (colour_palette(colour0, colour1), colour_indices),
        )
    } else {
        let Some(colour) = bytes.get(0..8) else {
            return Err(short_chain());
        };
        let colour0 = read_u16(&colour[0..2]);
        let colour1 = read_u16(&colour[2..4]);
        (
            None,
            (colour_palette(colour0, colour1), read_u32(&colour[4..8])),
        )
    };

    // Walk the block's own texels in image order, skipping anything past the
    // right or bottom edge: a 5x3 level's second block contributes one column of
    // three, not sixteen.
    for texel in 0..TEXELS_PER_BLOCK {
        let texel_x = col_left + (texel % 4) as u32;
        let texel_y = row_top + (texel / 4) as u32;
        if texel_y < height && texel_x < width {
            // The colour index is two bits, so the slot is always in range; the
            // `min` is belt and braces against a future width change.
            let colour_slot =
                ((colour.1 >> (COLOUR_INDEX_BITS * texel as u32)) & COLOUR_INDEX_MASK) as usize;
            let rgb = colour.0[colour_slot.min(colour.0.len() - 1)];
            let a = match alpha {
                Some((palette, indices)) => {
                    let slot = ((indices >> (ALPHA_INDEX_BITS * texel as u32)) & ALPHA_INDEX_MASK)
                        as usize;
                    palette[slot.min(palette.len() - 1)]
                }
                None => 255,
            };
            // `decode_block_mip` sized the buffer as exactly
            // `width * height * RGBA_BYTES` from these same three values, so the
            // destination is in bounds by construction; the `None` arm is a
            // typed refusal, never a panic, so no arithmetic slip here can
            // become an out-of-bounds write.
            let pixel = (texel_y as usize) * (width as usize) + (texel_x as usize);
            let destination = pixels
                .get_mut(pixel * RGBA_BYTES..pixel * RGBA_BYTES + RGBA_BYTES)
                .ok_or(TextureError::OutputInvariant {
                    width,
                    height,
                    texel_x,
                    texel_y,
                })?;
            for (slot, value) in destination.iter_mut().zip([rgb[0], rgb[1], rgb[2], a]) {
                *slot = value;
            }
        }
    }

    Ok(())
}

/// Zero-extends a slice of at most six bytes into the 48-bit little-endian word
/// the alpha index stream is read from.
fn pad6(bytes: &[u8]) -> [u8; 8] {
    let mut word = [0u8; 8];
    for (slot, byte) in word.iter_mut().zip(bytes) {
        *slot = *byte;
    }
    word
}

/// Reads a little-endian `u16` from a two-byte slice, or 0 if it is shorter.
fn read_u16(bytes: &[u8]) -> u16 {
    u16::from_le_bytes([bytes[0], bytes[1]])
}

/// Reads a little-endian `u32` from a four-byte slice, or 0 if it is shorter.
fn read_u32(bytes: &[u8]) -> u32 {
    u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand5_replicates_rather_than_scales() {
        // (v << 3) | (v >> 2) is the published expansion. It is NOT the same
        // function as the more obvious `v * 255 / 31` rounding: the two differ at
        // exactly four of the 32 inputs, and the spec's formula is the contract.
        assert_eq!(expand5(0), 0);
        assert_eq!(expand5(1), 8);
        assert_eq!(expand5(31), 255);
        let differing: Vec<u32> = (0..32)
            .filter(|&v| u32::from(expand5(v)) != ((v * 255) + 15) / 31)
            .collect();
        assert_eq!(differing, [3, 7, 24, 28]);
        assert_eq!(expand5(3), 24, "a scaling expansion would give 25");
        assert_eq!(expand5(7), 57, "a scaling expansion would give 58");
        assert_eq!(expand5(24), 198, "a scaling expansion would give 197");
        assert_eq!(expand5(28), 231, "a scaling expansion would give 230");
    }

    #[test]
    fn expand6_replicates_rather_than_scales() {
        assert_eq!(expand6(0), 0);
        assert_eq!(expand6(1), 4);
        assert_eq!(expand6(63), 255);
        // Replication differs from the obvious scaling at some inputs; the count
        // is pinned so a change to either function that moves a boundary shows.
        let differing: Vec<u32> = (0..64)
            .filter(|&v| u32::from(expand6(v)) != ((v * 255) + 31) / 63)
            .collect();
        assert!(
            !differing.is_empty(),
            "replication and scaling cannot agree on every 6-bit input"
        );
        for v in 0..64u32 {
            assert!(
                u32::from(expand6(v)) == ((v << 2) | (v >> 4)),
                "expand6({v}) must be the replication, not a scale"
            );
        }
    }

    #[test]
    fn the_alpha_palette_has_eight_levels_when_the_endpoints_are_ordered() {
        // The published BC4 ramp, checked at both endpoints and in the middle.
        // The published BC4 ramp with truncating integer division: v[i] is
        // ((8-i)*a0 + (i-1)*a1) / 7 for i in 2..=7. The rounding of that division
        // is itself a claim -- see crate::claims::BC4_RAMP_ROUNDING.
        assert_eq!(alpha_palette(255, 0), [255, 0, 218, 182, 145, 109, 72, 36]);
        // The six-level ramp divides by five instead, and leaves the last two
        // slots clamped to alpha0 because the specification does not define them.
        assert_eq!(alpha_palette(0, 255), [0, 255, 51, 102, 153, 204, 0, 0]);
        // `alpha0 == alpha1` takes the six-level branch, as the spec says.
        assert_eq!(alpha_palette(7, 7), [7, 7, 7, 7, 7, 7, 7, 7]);
    }

    #[test]
    fn both_colour_branches_offer_four_entries_at_different_blend_weights() {
        // colour0 > colour1: entries 2 and 3 are the two-thirds and one-third
        // blends of the ordered endpoints.
        assert_eq!(
            colour_palette(0xF800, 0x0000),
            [[255, 0, 0], [0, 0, 0], [170, 0, 0], [85, 0, 0]]
        );
        // colour0 <= colour1: entries 2 and 3 are the three-fifths and
        // two-fifths blends of the same two colours.
        assert_eq!(
            colour_palette(0x0000, 0xF800),
            [[0, 0, 0], [255, 0, 0], [102, 0, 0], [153, 0, 0]]
        );
        // And there are only four: the punch-through entries need a 3-bit index.
        assert_eq!(colour_palette(0x0000, 0xF800).len(), 4);
    }

    #[test]
    fn rgb5g6b5_reads_five_six_five() {
        assert_eq!(rgb5g6b5(0xF800), [255, 0, 0]);
        assert_eq!(rgb5g6b5(0x07E0), [0, 255, 0]);
        assert_eq!(rgb5g6b5(0x001F), [0, 0, 255]);
    }

    #[test]
    fn ceil_div4_rounds_up() {
        assert_eq!(ceil_div4(0), 0);
        assert_eq!(ceil_div4(1), 1);
        assert_eq!(ceil_div4(4), 1);
        assert_eq!(ceil_div4(5), 2);
        assert_eq!(ceil_div4(u32::MAX), 1 << 30);
    }

    #[test]
    fn the_two_block_sizes_differ_by_exactly_a_factor_of_two() {
        assert_eq!(dxt5_mip_size(256, 256, 0), 65_536);
        assert_eq!(dxt1_mip_size(256, 256, 0), 32_768);
        // And field_18 is the first of those, which is why it read as "two base
        // levels" for as long as the crate charged 8 bytes per block.
        assert_eq!(dxt5_mip_size(512, 512, 0), 262_144);
        assert_eq!(dxt5_mip_size(64, 64, 0), 4_096);
    }

    #[test]
    fn a_missing_block_is_an_error_and_not_a_panic() {
        let mut pixels = [0u8; 4 * 4 * 4];
        let result = write_block(&mut pixels, 4, 4, 0, 0, None, BLOCK_BYTES, true);
        assert!(matches!(
            result,
            Err(TextureError::ShortBlockChain { needed: 16, .. })
        ));
        let short = [0u8; 15];
        let result = write_block(&mut pixels, 4, 4, 0, 0, Some(&short), BLOCK_BYTES, true);
        assert!(matches!(
            result,
            Err(TextureError::ShortBlockChain { needed: 16, .. })
        ));
    }
}
