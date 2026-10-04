//! Synthetic fixtures shared by the integration tests.
//!
//! **No committed binary fixture exists**: the real raster records live only in
//! the git-ignored `SPORE/` tree, so every byte here is built in the test.
//!
//! ## The framing these fixtures build
//!
//! ```text
//! off        field
//! 0x00       envelope, 32 bytes (28 for the 0x15 family)
//! header     level 0 payload, NO length prefix
//!            level 1 payload, no prefix (DXT5 / DXT1)
//!            level m >= 2: u32 LE == that level's byte count, then the bytes
//! ```
//!
//! The byte counts the builders below produce are the same ones every raster
//! record in the measured corpus has; `docs/RASTER-FRAMING.md` carries the table
//! and `crate::framing`'s own tests pin the corpus record sizes directly.
//!
//! ## The block codec
//!
//! `DXT5` is BC3 (16 bytes: BC4 alpha + BC1 colour) and `DXT1` is BC1 (8 bytes,
//! alpha opaque) — the **published** specifications, reached because the record
//! lengths fix the stride. There is no hand-transcribed oracle table in this
//! module any more: the previous one recorded what an 8-byte "Spore block"
//! decoded to, and the corpus refutes that block. What the fixtures pin instead
//! are the *published* rules, written out by hand so a change to the decoder
//! cannot change both sides at once.

// Shared by several test binaries; not every one of them uses every helper.
#![allow(dead_code)]

/// Bytes in a BC3 block.
pub const BC3_BLOCK: usize = 16;

/// Bytes in a BC1 block.
pub const BC1_BLOCK: usize = 8;

/// The two alpha endpoints of a BC3 block.
pub const ALPHA0: u8 = 255;
/// The second alpha endpoint of a BC3 block; `ALPHA0 > ALPHA1` selects the
/// eight-level ramp.
pub const ALPHA1: u8 = 0;

/// Packs 16 three-bit BC3 alpha indices into the six bytes that follow the two
/// endpoints.
///
/// `indices[t]` is texel `t`'s index. Texel order is row-major within the block:
/// index `t` lives in row `t / 4`, column `t % 4`.
pub fn alpha_indices(indices: [u8; 16]) -> [u8; 6] {
    let mut words = 0u64;
    for (t, value) in indices.into_iter().enumerate() {
        let value = u64::from(value & 0x7);
        words |= value << (3 * t);
    }
    let bytes = words.to_le_bytes();
    [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]]
}

/// Packs 16 two-bit BC1 colour indices into the four bytes that follow the two
/// colour words.
pub fn colour_indices(indices: [u8; 16]) -> [u8; 4] {
    let mut words = 0u32;
    for (t, value) in indices.into_iter().enumerate() {
        let value = u32::from(value & 0x3);
        words |= value << (2 * t);
    }
    words.to_le_bytes()
}

/// Builds one BC3 block: two alpha endpoints, 16 alpha indices, two `R5G6B5`
/// colour words, 16 colour indices.
pub fn bc3_block(
    alpha0: u8,
    alpha1: u8,
    alpha: [u8; 16],
    colour0: u16,
    colour1: u16,
    colour: [u8; 16],
) -> [u8; 16] {
    let mut block = [0u8; 16];
    block[0] = alpha0;
    block[1] = alpha1;
    block[2..8].copy_from_slice(&alpha_indices(alpha));
    block[8..10].copy_from_slice(&colour0.to_le_bytes());
    block[10..12].copy_from_slice(&colour1.to_le_bytes());
    block[12..16].copy_from_slice(&colour_indices(colour));
    block
}

/// Builds one BC1 block: two `R5G6B5` colour words and 16 colour indices.
pub fn bc1_block(colour0: u16, colour1: u16, colour: [u8; 16]) -> [u8; 8] {
    let mut block = [0u8; 8];
    block[0..2].copy_from_slice(&colour0.to_le_bytes());
    block[2..4].copy_from_slice(&colour1.to_le_bytes());
    block[4..8].copy_from_slice(&colour_indices(colour));
    block
}

/// A BC3 block with **opaque alpha** in every texel and a single colour.
///
/// Endpoints `(255, 255)` take the six-level ramp, whose entry 0 is 255, and
/// every alpha index is 0 — so all sixteen texels read 255. `colour0 > colour1`
/// keeps the colour palette at four entries, and every colour index is 0, so
/// every texel takes `palette[0]`.
pub const OPAQUE_SOLID: u16 = 0xF800;

/// The colour every texel of [`OPAQUE_SOLID`] decodes to: pure red.
pub const OPAQUE_SOLID_RGB: [u8; 3] = [255, 0, 0];

/// One BC3 block whose every texel is opaque red.
///
/// Used as the base-level filler so a walk's output is a known constant rather
/// than whatever the zero bytes happen to decode to.
pub fn opaque_solid_block() -> [u8; 16] {
    bc3_block(255, 255, [0u8; 16], OPAQUE_SOLID, 0x0000, [0u8; 16])
}

/// One BC1 block whose every texel is opaque red.
pub fn opaque_solid_bc1_block() -> [u8; 8] {
    bc1_block(OPAQUE_SOLID, 0x0000, [0u8; 16])
}

/// An **addressing** BC3 block: texel `t` carries alpha index `(k >> t) & 1`.
///
/// Sixteen distinct blocks, each a different bit pattern, which is what makes
/// this usable as an addressing probe: a wrong block offset puts visibly wrong
/// texels on screen rather than plausible ones. Alpha endpoints `(0, 255)` take
/// the six-level ramp, whose entry 1 is 255 and entry 0 is 0, so index 0 reads 0
/// and index 1 reads 255 — the two most different values the ramp offers.
pub fn addressing_bc3(k: u8) -> [u8; 16] {
    let indices: [u8; 16] = core::array::from_fn(|t| (u32::from(k) >> t) as u8 & 1);
    bc3_block(0, 255, indices, 0x0000, 0xF800, [0u8; 16])
}

/// An **addressing** BC1 block: colour index `(k >> t) & 1` on texel `t`.
pub fn addressing_bc1(k: u8) -> [u8; 8] {
    let indices: [u8; 16] = core::array::from_fn(|t| (u32::from(k) >> t) as u8 & 1);
    bc1_block(0x0000, 0xF800, indices)
}

/// Concatenates blocks into a raw level chain.
pub fn chain(blocks: &[[u8; 16]]) -> Vec<u8> {
    blocks.iter().flat_map(|b| b.iter().copied()).collect()
}

/// Concatenates BC1 blocks into a raw level chain.
pub fn chain1(blocks: &[[u8; 8]]) -> Vec<u8> {
    blocks.iter().flat_map(|b| b.iter().copied()).collect()
}

/// Appends a little-endian `u32`.
pub fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

// ---------------------------------------------------------------------------
// The DXT5 (BC3) family
// ---------------------------------------------------------------------------

/// Width and height of the synthetic DXT5 record: 8x8, so the base level is four
/// blocks and the block grid is two wide — the smallest size that exercises both
/// the row stride and the padding rule.
pub const BC3_WIDTH: u32 = 8;
/// Height of the synthetic DXT5 record (equal to [`BC3_WIDTH`]).
pub const BC3_HEIGHT: u32 = 8;
/// Mip count: 8x8 then 4x4, so level 1 is bare and level 2 does not exist. The
/// prefix path is exercised by the three-level fixture below.
pub const BC3_MIPS: u32 = 2;

/// `32 + 4 * 16 + 16` — the header, the base level's four blocks and the 4x4
/// level's single block.
pub const BC3_RECORD_SIZE: usize = 32 + 64 + 16;

/// Builds the synthetic DXT5 record.
///
/// ```text
/// 0x00  envelope 8x8, mip_count = 2, field_18 = 64   (32 bytes)
/// 0x20  level 0: four addressing blocks, 8x8           (64 bytes)
/// 0x60  level 1: one solid block, 4x4                 (16 bytes)
/// ```
///
/// 112 bytes. `field_18` is 64, the base level's own byte count, which is the
/// cross-check the walk enforces.
pub fn bc3_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1); // version
    push_u32(&mut record, BC3_WIDTH);
    push_u32(&mut record, BC3_HEIGHT);
    push_u32(&mut record, BC3_MIPS);
    push_u32(&mut record, 8); // field_10: observed constant, never read
    push_u32(&mut record, 0x3554_5844); // fourcc 'DXT5'
    push_u32(&mut record, 64); // field_18 == the 8x8 base level's 64 bytes
    push_u32(&mut record, 0x0000_FFFF); // field_1c

    record.extend_from_slice(&chain(&[
        addressing_bc3(0),
        addressing_bc3(1),
        addressing_bc3(2),
        addressing_bc3(3),
    ]));
    record.extend_from_slice(&opaque_solid_block());
    record
}

/// Width and height of the three-level DXT5 record: 4x4, the smallest shape for
/// which "levels 0 and 1 are bare and level 2 onwards is prefixed" is exercised.
pub const BC3_MIPS_WIDTH: u32 = 4;
/// Height of the three-level DXT5 record (equal to [`BC3_MIPS_WIDTH`]).
pub const BC3_MIPS_HEIGHT: u32 = 4;
/// Levels: 4x4, 2x2, 1x1.
pub const BC3_THREE_MIPS: u32 = 3;

/// `32 + 16 + 16 + 4 + 16` — header, two bare levels, then a prefixed 1x1.
pub const BC3_THREE_RECORD_SIZE: usize = 32 + 16 + 16 + 4 + 16;

/// Builds the three-level DXT5 record, whose level 2 carries a length prefix.
///
/// Level 2 is deliberately a *different* block from level 1, so a walk that lost
/// its cursor cannot pass by returning the same bytes twice.
pub fn bc3_three_level_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1);
    push_u32(&mut record, BC3_MIPS_WIDTH);
    push_u32(&mut record, BC3_MIPS_HEIGHT);
    push_u32(&mut record, BC3_THREE_MIPS);
    push_u32(&mut record, 8);
    push_u32(&mut record, 0x3554_5844);
    push_u32(&mut record, 16); // field_18 == one 4x4 BC3 block
    push_u32(&mut record, 0x0000_FFFF);

    record.extend_from_slice(&opaque_solid_block());
    record.extend_from_slice(&opaque_solid_block());
    push_u32(&mut record, 16);
    record.extend_from_slice(&opaque_solid_block());
    record
}

// ---------------------------------------------------------------------------
// The DXT1 (BC1) family
// ---------------------------------------------------------------------------

/// Width and height of the synthetic DXT1 record: 8x8, four BC1 blocks.
pub const BC1_WIDTH: u32 = 8;
/// Height of the synthetic DXT1 record (equal to [`BC1_WIDTH`]).
pub const BC1_HEIGHT: u32 = 8;
/// Levels: 8x8 then 4x4.
pub const BC1_MIPS: u32 = 2;

/// `32 + 4 * 8 + 8` — header, base level's four blocks, the 4x4 level's one.
pub const BC1_RECORD_SIZE: usize = 32 + 32 + 8;

/// Builds the synthetic DXT1 record, laid out exactly like the DXT5 one.
pub fn bc1_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1);
    push_u32(&mut record, BC1_WIDTH);
    push_u32(&mut record, BC1_HEIGHT);
    push_u32(&mut record, BC1_MIPS);
    push_u32(&mut record, 8);
    push_u32(&mut record, 0x3154_5844); // fourcc 'DXT1'
    push_u32(&mut record, 32); // field_18 == the 8x8 base level's 32 bytes
    push_u32(&mut record, 0x0000_0000);

    record.extend_from_slice(&chain1(&[
        addressing_bc1(0),
        addressing_bc1(1),
        addressing_bc1(2),
        addressing_bc1(3),
    ]));
    record.extend_from_slice(&opaque_solid_bc1_block());
    record
}

// ---------------------------------------------------------------------------
// The 0x15 family (docs/LUMINANCE-RASTER.md)
// ---------------------------------------------------------------------------

/// Bytes in the `0x15` header: seven words, so four bytes shorter than DXT5's.
pub const LUM_HEADER_SIZE: usize = 28;

/// Width and height of the synthetic `0x15` record: 4x4, the smallest shape for
/// which "level 0 has no prefix and every later level does" is actually
/// exercised (a 1-level record would have no prefix at all).
pub const LUM_WIDTH: u32 = 4;
/// Height of the synthetic `0x15` record (equal to [`LUM_WIDTH`]).
pub const LUM_HEIGHT: u32 = 4;

/// Levels: 4x4, 2x2, 1x1.
pub const LUM_MIPS: u32 = 3;

/// The texel `(255, 128, 128, mask)` repeated over a `w * h` base level.
///
/// This is the **flat tangent normal**: lane 0 is Z at 1.0, lanes 1 and 2 are X
/// and Y at 0.0, encoded 0..255 with 128 as zero. Seven real records in
/// `Spore_Content.package` are *exactly* this constant over their whole base
/// level (`40652902:04bafc1e`, `40642902:0fbd1396`, `40642902:19ed05ad`,
/// `40632902:0675f7af`, `40632902:06804c10`, `40632902:06804c15`,
/// `40632902:06804c1a`), which is the single strongest piece of evidence that
/// lanes 0-2 are a normal map with Z first. No Spore bytes are embedded here;
/// the constant is transcribed from the measurement.
pub const FLAT_NORMAL_TEXEL: [u8; 4] = [0xFF, 0x80, 0x80, 0xFF];

/// A texel that is *not* the flat normal, used to tell mip levels apart:
/// `Z=0xFF`, `X=0x00`, `Y=0xFF`, mask `0x7F`.
pub const TILTED_NORMAL_TEXEL: [u8; 4] = [0xFF, 0x00, 0xFF, 0x7F];

/// Builds the synthetic `0x15` record every luminance test decodes.
///
/// ```text
/// 0x00  envelope 4x4, mip_count = 3                  (28 bytes)
/// 0x1c  level 0: 16 texels, flat normal              (64 bytes)
/// 0x5c  level 1 byte count: 16                        ( 4 bytes)
/// 0x60  level 1: 4 texels, flat normal               (16 bytes)
/// 0x70  level 2 byte count: 4                         ( 4 bytes)
/// 0x74  level 2: 1 texel,  tilted normal              ( 4 bytes)
/// ```
///
/// 120 bytes. The payload start at `0x1c` is the load-bearing detail: it is
/// where `RasterEnvelope::field_1c` reports the first texel (`0xFF8080FF`), which
/// is why the header of this family is 28 bytes and not 32.
pub fn luminance_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1);
    push_u32(&mut record, LUM_WIDTH);
    push_u32(&mut record, LUM_HEIGHT);
    push_u32(&mut record, LUM_MIPS);
    push_u32(&mut record, 8); // field_10: observed constant, never read
    push_u32(&mut record, 0x0000_0015); // fourcc 0x15
    push_u32(&mut record, LUM_WIDTH * LUM_HEIGHT * 4); // field_18 = w*h*4

    for _ in 0..(LUM_WIDTH * LUM_HEIGHT) {
        record.extend_from_slice(&FLAT_NORMAL_TEXEL);
    }
    push_u32(&mut record, 2 * 2 * 4);
    for _ in 0..4 {
        record.extend_from_slice(&FLAT_NORMAL_TEXEL);
    }
    push_u32(&mut record, 4);
    record.extend_from_slice(&TILTED_NORMAL_TEXEL);
    record
}

/// The synthetic `0x15` record with a single level, 92 bytes long.
///
/// It exists because its envelope differs from [`luminance_record`]'s in
/// `mip_count` alone, which is what makes the record-size identity a fact about
/// the record rather than a constant.
pub fn luminance_single_mip_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1);
    push_u32(&mut record, LUM_WIDTH);
    push_u32(&mut record, LUM_HEIGHT);
    push_u32(&mut record, 1);
    push_u32(&mut record, 8);
    push_u32(&mut record, 0x0000_0015);
    push_u32(&mut record, LUM_WIDTH * LUM_HEIGHT * 4);
    for _ in 0..(LUM_WIDTH * LUM_HEIGHT) {
        record.extend_from_slice(&TILTED_NORMAL_TEXEL);
    }
    record
}

/// Decodes a hex string into bytes. Panics on malformed input, which is a
/// test-authoring bug rather than a condition under test.
pub fn unhex(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0, "hex string must have even length");
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("valid hex literal"))
        .collect()
}

/// The 8x8 decode of four [`addressing_bc3`] blocks, byte for byte.
///
/// Block `k` is at byte offset `k * 16`, and the block grid of an 8x8 image is
/// two wide, so this table fails loudly if the row stride is ever taken in texels
/// instead of blocks. Alpha endpoints `(0, 255)` put every texel's alpha at
/// exactly 0 or 255; the colour is `colour1`'s red because every colour index is
/// 0 and `colour0 = 0x0000` is black.
pub fn expected_addressing_bc3_8x8() -> Vec<u8> {
    let mut out = Vec::with_capacity(8 * 8 * 4);
    for row in 0..8u32 {
        for x in 0..8u32 {
            let block = (row / 4) * 2 + (x / 4);
            let texel = (row % 4) * 4 + (x % 4);
            let alpha = ((block >> texel) as u8) & 1;
            out.extend_from_slice(&[0, 0, 0, alpha * 255]);
        }
    }
    out
}
