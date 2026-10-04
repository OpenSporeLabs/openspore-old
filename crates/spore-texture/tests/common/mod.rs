//! Synthetic fixtures shared by the integration tests.
//!
//! **No committed binary fixture exists**: the real raster records live only in
//! the git-ignored `SPORE/` tree, so every byte here is built in the test.
//!
//! The block constants and the expected RGBA values are transcribed from
//! `tests/test_textures.py::TestDxt5Synthetic`, whose expected values are
//! hand-computed there and cross-checked there against
//! `tools/spore/dxt5/dxt5.py`. This module only *builds inputs*; the expected
//! outputs live in the test files next to the assertions they justify.

// Shared by several test binaries; not every one of them uses every helper.
#![allow(dead_code)]

/// `tests/test_textures.py::test_block_two_color_full_alpha`.
///
/// `a0=255 > a1=0`, so the ramp is reversed; `c0=0x20` (bit 5 set, so
/// `rgb5 = (8,0,0)`) and `c1=0x00` (`(0,0,0)`), and `c0 > c1` selects the
/// two-colour palette. All index bits are 0, so every texel takes
/// `palette[0]` and the reversed ramp's `round(6*255/8) = 191`.
pub const TWO_COLOUR_FULL_ALPHA: [u8; 8] = [255, 0, 0x20, 0x00, 0, 0, 0, 0];

/// `tests/test_textures.py::test_block_four_color_alpha_ramp`.
///
/// `a0=0 < a1=255`, unreversed, and each index byte is `0xE4`, which reads as
/// codes 0,1,2,3 across the byte. `c0=0x00 < c1=0x20` selects the four-colour
/// palette `(0,0,0)/(8,0,0)/(0,0,0)/(0,0,0)`.
pub const FOUR_COLOUR_ALPHA_RAMP: [u8; 8] = [0, 255, 0x00, 0x08, 0xE4, 0xE4, 0xE4, 0xE4];

/// `tests/test_textures.py::test_tail_clamp`.
///
/// `c0=0x3F` (`(8,0,0)`), `c1=0x0F<<2 = 60` (`(8,0,0)`), `c0 > c1` so two
/// colours. Used four times over for a 6x6 image, where the padding texels of
/// the edge blocks must never be written.
pub const TAIL_CLAMP: [u8; 8] = [255, 0, 0x3F, 0x0F, 0, 0, 0, 0];

/// Builds one 8-byte block from explicit texel codes.
///
/// `codes` is in texel order: index 0 is the top-left texel, and the packing is
/// the format's own — four codes per byte, `code n` at bit `2 * (n % 4)` of byte
/// `4 + n / 4`, low bit first.
pub fn block(alpha0: u8, alpha1: u8, colour_lo: u8, colour_hi: u8, codes: [u8; 16]) -> [u8; 8] {
    let mut bytes = [0u8; 8];
    bytes[0] = alpha0;
    bytes[1] = alpha1;
    bytes[2] = colour_lo;
    bytes[3] = colour_hi;
    for (texel, code) in codes.into_iter().enumerate() {
        let byte = 4 + texel / 4;
        let shift = 2 * (texel % 4);
        bytes[byte] |= (code & 0x3) << shift;
    }
    bytes
}

/// A block whose texel `t` carries code `(k >> t) & 1`.
///
/// Endpoints `a0=0 < a1=255` (unreversed) and `c0=0x20 > c1=0x00`, so the
/// two-colour palette is `[(8,0,0), (0,0,0)]`: code 0 gives `(8,0,0)` with
/// alpha `round(2*255/8) = 64`, code 1 gives `(0,0,0)` with alpha 0.
///
/// Sixteen distinct blocks, each a different bit pattern, which is what makes
/// this usable as an addressing probe: a wrong block offset puts visibly wrong
/// texels on screen rather than plausible ones.
pub fn addressing_block(k: u8) -> [u8; 8] {
    let codes: [u8; 16] = core::array::from_fn(|t| (((u32::from(k)) >> t) & 1) as u8);
    block(0, 255, 0x20, 0x00, codes)
}

/// Concatenates blocks into a raw mip chain.
pub fn chain(blocks: &[[u8; 8]]) -> Vec<u8> {
    blocks.iter().flat_map(|b| b.iter().copied()).collect()
}

/// Appends a little-endian `u32`.
pub fn push_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

/// The synthetic raster record every raster test decodes.
///
/// Layout, matching the format's own rule that **all** layer headers precede
/// **all** payloads:
///
/// ```text
/// 0x00  envelope  8x8 DXT5, mip_count = 2       (32 bytes)
/// 0x20  layer 0 header: "e9D1" + three words    (16 bytes)
/// 0x30  layer 1 header: 0 + three words         (16 bytes)
/// 0x40  layer 0 payload: mip0 8x8 (4 blocks) + mip1 4x4 (1 block)   (40 bytes)
/// 0x68  layer 1 payload: mip0 8x8 (4 blocks) + mip1 4x4 (1 block)   (40 bytes)
/// ```
///
/// 144 bytes total. `chain_size(8, 8, 2)` is 40, so the layer stride is 56 and
/// `layer_count_for` must derive 2 from it. The record was also built and
/// decoded by `tools/spore/raster/raster.py`, which derived the same two layers
/// and the same mip dimensions; `the_builder_reproduces_the_oracle_record`
/// pins the exact bytes.
pub const SYNTHETIC_RECORD_HEX: &str = concat!(
    "01000000080000000800000002000000080000004458543500000400ffff0000",
    "65394431111111112222222233333333",
    "00000000444444445555555566666666",
    "00ff20000000000000ff20000100000000ff20000400000000ff20000500000000ff0008e4e4e4e4",
    "ff00200000000000ff00200000000000ff00200000000000ff00200000000000ff003f0f00000000",
);

/// Width and height of the synthetic record.
pub const SYNTHETIC_WIDTH: u32 = 8;

/// Height of the synthetic record (equal to [`SYNTHETIC_WIDTH`]).
pub const SYNTHETIC_HEIGHT: u32 = 8;

/// Mip count of the synthetic record.
pub const SYNTHETIC_MIPS: u32 = 2;

/// Builds the synthetic raster record from its parts.
///
/// The point of building it rather than embedding a blob is that every field is
/// visible; [`SYNTHETIC_RECORD_HEX`] and a test assert that the two agree, so a
/// silent edit to either is caught.
pub fn synthetic_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1); // version
    push_u32(&mut record, SYNTHETIC_WIDTH);
    push_u32(&mut record, SYNTHETIC_HEIGHT);
    push_u32(&mut record, SYNTHETIC_MIPS);
    push_u32(&mut record, 8); // field_10: observed constant, never read
    push_u32(&mut record, 0x3554_5844); // fourcc 'DXT5'
    push_u32(&mut record, 0x0004_0000); // field_18: observed constant, never read
    push_u32(&mut record, 0x0000_FFFF); // field_1c

    // Layer headers, both of which precede both payloads. Their contents are
    // unresolved and are never interpreted; the distinct words here exist only
    // so a decoder that mistakenly reads them cannot look plausible.
    record.extend_from_slice(b"e9D1");
    push_u32(&mut record, 0x1111_1111);
    push_u32(&mut record, 0x2222_2222);
    push_u32(&mut record, 0x3333_3333);
    record.extend_from_slice(&[0, 0, 0, 0]);
    push_u32(&mut record, 0x4444_4444);
    push_u32(&mut record, 0x5555_5555);
    push_u32(&mut record, 0x6666_6666);

    // Layer 0: four addressing blocks for the 8x8 base, then the 4x4 mip.
    record.extend_from_slice(&chain(&[
        addressing_block(0),
        addressing_block(1),
        addressing_block(2),
        addressing_block(3),
    ]));
    record.extend_from_slice(&FOUR_COLOUR_ALPHA_RAMP);

    // Layer 1: a flat field of one texel value, then a distinct block.
    record.extend_from_slice(&chain(&[
        TWO_COLOUR_FULL_ALPHA,
        TWO_COLOUR_FULL_ALPHA,
        TWO_COLOUR_FULL_ALPHA,
        TWO_COLOUR_FULL_ALPHA,
    ]));
    record.extend_from_slice(&TAIL_CLAMP);

    record
}

/// Builds the same raster with a **single** layer, 88 bytes long.
///
/// `layer_count_for` must derive 1 from it, which is what makes the derived
/// count a fact about the record rather than a constant: the envelope is
/// byte-identical to [`synthetic_record`]'s and only the payload differs.
pub fn single_layer_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1);
    push_u32(&mut record, SYNTHETIC_WIDTH);
    push_u32(&mut record, SYNTHETIC_HEIGHT);
    push_u32(&mut record, SYNTHETIC_MIPS);
    push_u32(&mut record, 8);
    push_u32(&mut record, 0x3554_5844);
    push_u32(&mut record, 0x0004_0000);
    push_u32(&mut record, 0x0000_FFFF);
    record.extend_from_slice(b"e9D1");
    push_u32(&mut record, 0x1111_1111);
    push_u32(&mut record, 0x2222_2222);
    push_u32(&mut record, 0x3333_3333);
    record.extend_from_slice(&chain(&[
        addressing_block(0),
        addressing_block(1),
        addressing_block(2),
        addressing_block(3),
    ]));
    record.extend_from_slice(&FOUR_COLOUR_ALPHA_RAMP);
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

// ---------------------------------------------------------------------------
// The 0x15 family (docs/LUMINANCE-RASTER.md)
// ---------------------------------------------------------------------------

/// Bytes in the `0x15` header: seven words, so four bytes shorter than DXT5's.
pub const LUM_HEADER_SIZE: usize = 28;

/// Width and height of the synthetic `0x15` record: 4x4, the smallest shape for
/// which "mip 0 has no prefix and every later mip does" is actually exercised
/// (a 1-mip record would have no prefix at all).
pub const LUM_WIDTH: u32 = 4;
/// Height of the synthetic `0x15` record (equal to [`LUM_WIDTH`]).
pub const LUM_HEIGHT: u32 = 4;

/// Mip count of the synthetic `0x15` record: 4x4, 2x2, 1x1.
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

/// A texel that is *not* the flat normal, used to tell mip levels and layers
/// apart: `Z=0xFF`, `X=0x00`, `Y=0xFF`, mask `0x7F`.
pub const TILTED_NORMAL_TEXEL: [u8; 4] = [0xFF, 0x00, 0xFF, 0x7F];

/// The synthetic `0x15` record every luminance test decodes.
///
/// Layout, matching the family's own framing — no layer headers, and a
/// little-endian byte count in front of every mip past the base level:
///
/// ```text
/// 0x00  envelope  4x4, mip_count = 3                 (28 bytes)
/// 0x1c  mip 0 payload: 16 texels, flat normal       (64 bytes)
/// 0x5c  mip 1 byte count: 16                        ( 4 bytes)
/// 0x60  mip 1 payload: 4 texels, flat normal        (16 bytes)
/// 0x70  mip 2 byte count: 4                         ( 4 bytes)
/// 0x74  mip 2 payload: 1 texel,  tilted normal       ( 4 bytes)
/// ```
///
/// 120 bytes total. `luminance_record_size(4, 4, 3)` is `28 + 64 + (4+16) + (4+4)`
/// = 120, so `layer_count_for` must derive **1** layer from it and the walk must
/// consume it exactly.
///
/// The payload start at `0x1c` is the load-bearing detail: it is where
/// `RasterEnvelope::field_1c` reports the first texel (`0xFF8080FF`), which is
/// why the header of this family is 28 bytes and not 32.
pub fn luminance_record() -> Vec<u8> {
    let mut record = Vec::new();
    push_u32(&mut record, 1); // version
    push_u32(&mut record, LUM_WIDTH);
    push_u32(&mut record, LUM_HEIGHT);
    push_u32(&mut record, LUM_MIPS);
    push_u32(&mut record, 8); // field_10: observed constant, never read
    push_u32(&mut record, 0x0000_0015); // fourcc 0x15
    push_u32(&mut record, LUM_WIDTH * LUM_HEIGHT * 4); // field_18 = w*h*4

    // Mip 0: no length prefix. Its length is already in the header.
    for _ in 0..(LUM_WIDTH * LUM_HEIGHT) {
        record.extend_from_slice(&FLAT_NORMAL_TEXEL);
    }
    // Mip 1: 2x2, prefixed.
    push_u32(&mut record, 2 * 2 * 4);
    for _ in 0..4 {
        record.extend_from_slice(&FLAT_NORMAL_TEXEL);
    }
    // Mip 2: 1x1, prefixed, and deliberately a *different* texel so a walk that
    // lost its cursor cannot pass by returning the same bytes twice.
    push_u32(&mut record, 4);
    record.extend_from_slice(&TILTED_NORMAL_TEXEL);
    record
}

/// The synthetic `0x15` record with a single mip, 92 bytes long.
///
/// `28 + 16 * 4` = 92. It exists for the same reason
/// [`single_layer_record`] does for DXT5: a record with a *different* layer
/// count and a byte-identical envelope, which is what makes the derived count a
/// fact about the record rather than a constant read from a field.
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

/// The 4x4 decode of [`FOUR_COLOUR_ALPHA_RAMP`], byte for byte, as produced by
/// `tools/spore/dxt5/dxt5.py`.
///
/// Row-major RGBA8: the index bytes `0xE4` give codes 0,1,2,3 per row, so each
/// row is `(0,0,0,64)`, `(8,0,0,0)`, `(0,0,0,255)`, `(0,0,0,64)` and repeats.
pub fn expected_four_colour_ramp() -> Vec<u8> {
    unhex(concat!(
        "00000040", "08000000", "000000ff", "00000040", "00000040", "08000000", "000000ff",
        "00000040", "00000040", "08000000", "000000ff", "00000040", "00000040", "08000000",
        "000000ff", "00000040",
    ))
}

/// The 8x8 decode of four [`addressing_block`]s, byte for byte, as produced by
/// `tools/spore/dxt5/dxt5.py` for the same four blocks in this order.
///
/// Block `k` is at byte offset `k * 8`, and the block grid of an 8x8 image is
/// two wide, so this table fails loudly if the row stride is ever taken in
/// texels instead of blocks.
pub fn expected_addressing_8x8() -> Vec<u8> {
    unhex(concat!(
        // Rows 0-3 come from blocks 0 and 1: block 0 has every code 0, block 1
        // has only texel 0 set, so only (x=4, y=0) is dark.
        "08000040080000400800004008000040",
        "00000000080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        // Rows 4-7 come from blocks 2 and 3: block 2 has only texel 2 set,
        // block 3 has texels 0 and 2.
        "08000040000000000800004008000040",
        "00000000000000000800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
        "08000040080000400800004008000040",
    ))
}
