//! Hand-computed **published-specification** expectations for the BC3 and BC1
//! block decoders, plus the addressing, padding and truncation properties that a
//! block walk can get wrong.
//!
//! # What changed, and why these tables are different
//!
//! This file used to pin the output of an **8-byte "Spore block"** against
//! `tests/test_textures.py::TestDxt5Synthetic`. Those two agreed with each other
//! perfectly and both were wrong: the corpus charges 16 bytes per 4x4 block, and
//! the tables below are the published BC3/BC1 specifications, written out by hand
//! so that a change to the decoder cannot change both sides at once.
//!
//! Nothing here is transcribed from a reference implementation. Every expected
//! RGBA value below is derived from the specification's own arithmetic, and the
//! derivation is in the comment above it. That is the only honest cross-check
//! available for a codec whose stride was measured rather than ported, and it is
//! why the codec claim is graded `Inferred` — see `spore_texture::claims`.

mod common;

use common::*;
use spore_texture::{
    decode_dxt1_mip, decode_dxt5_mip, dxt1_chain_size, dxt1_mip_size, dxt5_chain_size,
    dxt5_mip_size, TextureError, BC1_BLOCK_BYTES, BLOCK_BYTES, MIP_FLOOR,
};

/// Expands a 5-bit channel the way the specification says: bit replication.
const fn e5(v: u32) -> u8 {
    ((v << 3) | (v >> 2)) as u8
}

/// Expands a 6-bit channel the way the specification says: bit replication.
const fn e6(v: u32) -> u8 {
    ((v << 2) | (v >> 4)) as u8
}

/// The four palette entries of a `colour0 > colour1` BC1 half-block, written
/// straight from the specification: `c0`, `c1`, `(2*c0 + c1)/3` and
/// `(c0 + 2*c1)/3`.
fn four_colours(colour0: u16, colour1: u16) -> [[u8; 3]; 4] {
    let c0 = decode_rgb5g6b5(colour0);
    let c1 = decode_rgb5g6b5(colour1);
    let third = |a: [u8; 3], b: [u8; 3], n: u32| -> [u8; 3] {
        let mix = |x: u8, y: u8| ((u32::from(x) * n + u32::from(y) * (3 - n)) / 3) as u8;
        [mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])]
    };
    [c0, c1, third(c0, c1, 2), third(c0, c1, 1)]
}

/// Reads a 16-bit `R5G6B5` colour word as 8-bit RGB.
fn decode_rgb5g6b5(word: u16) -> [u8; 3] {
    let v = u32::from(word);
    [e5((v >> 11) & 0x1F), e6((v >> 5) & 0x3F), e5(v & 0x1F)]
}

// ---------------------------------------------------------------------------
// 1. Hand-computed BC3 alpha ramps
// ---------------------------------------------------------------------------

/// A BC3 block with `alpha0 > alpha1` gives the eight-level ramp
/// `((8-i)*a0 + (i-1)*a1) / 7`, truncated.
///
/// With endpoints `(255, 0)` that is `[255, 0, 218, 182, 145, 109, 72, 36]`: each
/// entry is `(7-i+1) * 255 / 7` and integer division truncates. Every texel reads
/// entry 0 here, so the whole 4x4 is opaque red (`colour0 = 0xF800` is pure red
/// and every colour index is 0).
#[test]
fn the_eight_level_alpha_ramp_is_hand_computed() {
    let block = bc3_block(255, 0, [0u8; 16], 0xF800, 0x0000, [0u8; 16]);
    let image = decode_dxt5_mip(&block, 4, 4).expect("one block covers 4x4");
    for pixel in image.pixels.chunks_exact(4) {
        assert_eq!(pixel, [255, 0, 0, 255], "index 0 is a0 = 255");
    }
    // The same block with every alpha index set to 7 reads the LAST ramp entry,
    // `(1*255 + 6*0) / 7 = 36`.
    let block = bc3_block(255, 0, [7u8; 16], 0xF800, 0x0000, [0u8; 16]);
    let image = decode_dxt5_mip(&block, 4, 4).expect("decodes");
    for pixel in image.pixels.chunks_exact(4) {
        assert_eq!(pixel, [255, 0, 0, 36], "index 7 is (255 + 0) / 7 = 36");
    }
}

/// With `alpha0 <= alpha1` the ramp has six entries and divides by five:
/// `(4*a0 + a1)/5, (3*a0 + 2*a1)/5, (2*a0 + 3*a1)/5, (a0 + 4*a1)/5`.
///
/// Endpoints `(0, 255)` give `[0, 255, 51, 102, 153, 204]`, and the two indices
/// the specification leaves undefined clamp to `alpha0 = 0`.
#[test]
fn the_six_level_alpha_ramp_is_hand_computed_and_clamps() {
    for (index, expected) in [
        (0u8, 0u8),
        (1, 255),
        (2, 51),
        (3, 102),
        (4, 153),
        (5, 204),
        (6, 0),
        (7, 0),
    ] {
        let block = bc3_block(0, 255, [index; 16], 0xF800, 0x0000, [0u8; 16]);
        let image = decode_dxt5_mip(&block, 4, 4).expect("decodes");
        assert_eq!(image.pixels[3], expected, "alpha index {index}");
    }
}

/// `alpha0 == alpha1` takes the six-level branch, where every entry is that same
/// value — so a degenerate block is **not** made constant across the 0 and 255
/// sentinels the old codec produced. Every index reads `alpha0`.
#[test]
fn equal_alpha_endpoints_have_no_special_case() {
    for index in 0..8u8 {
        let block = bc3_block(7, 7, [index; 16], 0xF800, 0x0000, [0u8; 16]);
        let image = decode_dxt5_mip(&block, 4, 4).expect("decodes");
        assert_eq!(image.pixels[3], 7, "alpha index {index} must all read 7");
    }
}

// ---------------------------------------------------------------------------
// 2. Hand-computed BC1 colour palettes
// ---------------------------------------------------------------------------

/// `colour0 > colour1`: four colours, and entries 2 and 3 are the 2/3 and 1/3
/// blends.
///
/// `0xF800` is `(31,0,0)5` -> `(255,0,0)` and `0x0000` is black, so the palette is
/// red, black, `(2*255)/3 = 170` and `(1*255)/3 = 85`.
#[test]
fn the_four_colour_palette_is_hand_computed() {
    let expected = [[255, 0, 0], [0, 0, 0], [170, 0, 0], [85, 0, 0]];
    for index in 0..4u8 {
        let block = bc3_block(255, 255, [0u8; 16], 0xF800, 0x0000, [index; 16]);
        let image = decode_dxt5_mip(&block, 4, 4).expect("decodes");
        assert_eq!(
            &image.pixels[..3],
            &expected[index as usize],
            "colour index {index}"
        );
    }
    // `four_colours` is the same arithmetic written independently of the decoder,
    // so the two must agree on all four.
    assert_eq!(four_colours(0xF800, 0x0000), expected);
}

/// `colour0 <= colour1`: still four entries, blended with the one-fifth weights
/// instead of the one-third ones.
///
/// Entries 2 and 3 are `(3*c0 + 2*c1)/5` and `(2*c0 + 3*c1)/5`. With `c0` black
/// and `c1` red those are 102 and 153. There are **no entries 4 and 5**: a BC1
/// colour block carries a 2-bit index, so the punch-through entries are
/// unreachable and no decoder can choose between them.
#[test]
fn the_unordered_colour_palette_is_hand_computed() {
    let expected = [[0, 0, 0], [255, 0, 0], [102, 0, 0], [153, 0, 0]];
    for index in 0..4u8 {
        let decode =
            decode_dxt1_mip(&bc1_block(0x0000, 0xF800, [index; 16]), 4, 4).expect("decodes");
        assert_eq!(
            &decode.pixels[..3],
            &expected[index as usize],
            "colour index {index}"
        );
        assert_eq!(decode.pixels[3], 255, "BC1 alpha is opaque");
    }
}

/// `R5G6B5`: green gets six bits, red and blue five.
#[test]
fn rgb5g6b5_is_five_six_five() {
    assert_eq!(decode_rgb5g6b5(0xF800), [255, 0, 0]);
    assert_eq!(decode_rgb5g6b5(0x07E0), [0, 255, 0]);
    assert_eq!(decode_rgb5g6b5(0x001F), [0, 0, 255]);
    assert_eq!(decode_rgb5g6b5(0xFFFF), [255, 255, 255]);
    // Green is the six-bit channel, and it is expanded by REPLICATION, so
    // 32/63 reads 130 and not the 129 a scale would give. That one-LSB class of
    // difference is why the expansion is tested against the formula rather than
    // against a table.
    assert_eq!(decode_rgb5g6b5(0x0400)[1], 130);
}

// ---------------------------------------------------------------------------
// 3. Geometry
// ---------------------------------------------------------------------------

/// Level sizes follow `ceil4(w) * ceil4(h) * 16`, and a chain is exactly the sum
/// of its levels.
///
/// The 512x512 figure is the corpus's: `dxt5_chain_size(512, 512, 10)` is 349552,
/// and the record is 349616 — the extra 64 being the 32-byte envelope and eight
/// level prefixes of 4 bytes each.
#[test]
fn level_sizes_follow_the_block_geometry_and_a_chain_is_their_sum() {
    for (width, height, mips) in [
        (512u32, 512u32, 10u32),
        (256, 256, 9),
        (64, 64, 7),
        (8, 8, 2),
    ] {
        let chain = dxt5_chain_size(width, height, mips);
        let sum: usize = (0..mips).map(|mip| dxt5_mip_size(width, height, mip)).sum();
        assert_eq!(chain, sum, "{width}x{height} mips={mips}");
        // The corpus shapes, byte for byte.
        if (width, height, mips) == (512, 512, 10) {
            assert_eq!(chain, 349_552);
        }
        if (width, height, mips) == (256, 256, 9) {
            assert_eq!(chain, 87_408);
        }
        if (width, height, mips) == (64, 64, 7) {
            assert_eq!(chain, 5_488);
        }
    }
    // The BC1 stride is exactly half, which is what makes a 256x256 DXT1 record
    // 43764 bytes against a DXT5 record's 87468.
    assert_eq!(dxt1_mip_size(256, 256, 0), 32_768);
    assert_eq!(dxt1_chain_size(256, 256, 9), 43_704);
    assert_eq!(dxt5_mip_size(256, 256, 0), 2 * dxt1_mip_size(256, 256, 0));
}

/// A 4x4 level is one block; an 8x8 level is four, laid out two wide.
#[test]
fn a_4x4_level_is_one_block_and_an_8x8_level_is_four() {
    assert_eq!(dxt5_mip_size(4, 4, 0), 16);
    assert_eq!(dxt5_mip_size(8, 8, 0), 64);
    let image = decode_dxt5_mip(&opaque_solid_block(), 4, 4).expect("decodes");
    assert_eq!(image.pixels.len(), 4 * 4 * 4);
}

/// The row stride is in **blocks**, never in texels.
#[test]
fn an_8x8_level_addresses_four_blocks_in_block_row_major_order() {
    let image = decode_dxt5_mip(
        &chain(&[
            addressing_bc3(0),
            addressing_bc3(1),
            addressing_bc3(2),
            addressing_bc3(3),
        ]),
        8,
        8,
    )
    .expect("four blocks cover 8x8");
    assert_eq!(image.pixels, expected_addressing_bc3_8x8());
}

/// The padding texels of a level whose size is not a multiple of four must never
/// be written: a 5x3 level occupies two blocks and the second contributes one
/// column of three texels.
#[test]
fn a_5x3_level_skips_the_padding_texels_of_its_second_block() {
    let mut blocks = vec![addressing_bc3(0), addressing_bc3(1)];
    blocks[0][..16].copy_from_slice(&opaque_solid_block());
    let image = decode_dxt5_mip(&chain(&blocks), 5, 3).expect("two blocks cover 5x3");
    assert_eq!(image.width, 5);
    assert_eq!(image.height, 3);
    assert_eq!(image.pixels.len(), 5 * 3 * 4);
    // Texels 0..3 of row 0 come from the first block, which is solid; texel 4
    // comes from the second, so only the first four may be solid.
    for pixel in image.pixels.chunks_exact(4).take(4) {
        assert_eq!(pixel, [255, 0, 0, 255]);
    }
}

/// Degenerate level sizes still address correctly rather than dividing by zero.
#[test]
fn degenerate_level_dimensions_still_address_correctly() {
    for (width, height) in [(1u32, 1u32), (1, 4), (4, 1), (3, 3)] {
        let size = dxt5_mip_size(width, height, 0);
        let image = decode_dxt5_mip(&vec![0u8; size], width, height).expect("decodes");
        assert_eq!(image.pixels.len(), width as usize * height as usize * 4);
    }
}

/// Trailing bytes after a level are ignored, which is what lets the walk hand
/// over a slice of the whole chain.
#[test]
fn trailing_bytes_after_a_level_are_ignored() {
    let mut chain_bytes = opaque_solid_block().to_vec();
    chain_bytes.extend_from_slice(&[0xAB; 64]);
    let image = decode_dxt5_mip(&chain_bytes, 4, 4).expect("decodes");
    assert_eq!(image.pixels.len(), 64);
    for pixel in image.pixels.chunks_exact(4) {
        assert_eq!(pixel, [255, 0, 0, 255]);
    }
}

// ---------------------------------------------------------------------------
// 4. Refusals
// ---------------------------------------------------------------------------

/// Every prefix length of one block is `Ok` or `Err` and never panics.
#[test]
fn every_prefix_length_of_one_block_is_ok_or_err_and_never_panics() {
    let full = opaque_solid_block();
    for length in 0..=full.len() {
        let result = decode_dxt5_mip(&full[..length], 4, 4);
        if length < BLOCK_BYTES {
            assert!(result.is_err(), "{length} bytes must be refused");
        } else {
            assert!(result.is_ok(), "{length} bytes must decode");
        }
    }
}

/// Zero dimensions are refused before any allocation.
#[test]
fn zero_dimensions_are_refused() {
    assert!(matches!(
        decode_dxt5_mip(&[], 0, 4),
        Err(TextureError::ZeroDimension { .. })
    ));
    assert!(matches!(
        decode_dxt1_mip(&[], 4, 0),
        Err(TextureError::ZeroDimension { .. })
    ));
}

/// A short chain reports the geometry it needed, with both numbers.
#[test]
fn a_short_chain_reports_the_geometry_it_needed() {
    let error = decode_dxt5_mip(&[0u8; 8], 64, 64).unwrap_err();
    assert_eq!(
        error,
        TextureError::ShortBlockChain {
            available: 8,
            needed: 16 * 16 * BLOCK_BYTES,
            width: 64,
            height: 64
        }
    );
    // The BC1 arm names its own stride.
    let error = decode_dxt1_mip(&[0u8; 8], 64, 64).unwrap_err();
    assert_eq!(
        error,
        TextureError::ShortBlockChain {
            available: 8,
            needed: 16 * 16 * BC1_BLOCK_BYTES,
            width: 64,
            height: 64
        }
    );
}

/// Chain size never wraps to a small number.
#[test]
fn chain_size_never_wraps_to_a_small_number() {
    assert!(dxt5_chain_size(u32::MAX, u32::MAX, 1) > u32::MAX as usize);
    assert!(dxt1_chain_size(u32::MAX, u32::MAX, 1) > u32::MAX as usize);
    assert_eq!(dxt5_chain_size(4, 4, 0), 0);
    // Past the floor, one block each — an approximation no record exercises.
    assert_eq!(
        dxt5_chain_size(4, 4, 33),
        dxt5_chain_size(4, 4, 32) + BLOCK_BYTES
    );
}

/// Absurd dimensions are refused by name rather than attempted.
#[test]
fn absurd_dimensions_are_refused_by_name() {
    assert!(matches!(
        decode_dxt5_mip(&[], u32::MAX, u32::MAX),
        Err(TextureError::ShortBlockChain { .. }) | Err(TextureError::OutputTooLarge { .. })
    ));
    assert!(dxt5_mip_size(u32::MAX, u32::MAX, MIP_FLOOR) == BLOCK_BYTES);
}

/// Error messages name their section and the formats that were available, so a
/// log line is enough to act on.
#[test]
fn every_error_message_names_its_section() {
    let cases: Vec<(TextureError, &str)> = vec![
        (
            TextureError::TruncatedEnvelope { available: 4 },
            "4 bytes < the 32-byte",
        ),
        (
            TextureError::UnsupportedFourcc {
                fourcc: 0x1500_0000,
            },
            "unsupported fourcc 0x15000000",
        ),
        (
            TextureError::UnsupportedBlockSize {
                fourcc: 0x3554_5844,
                block_bytes: 12,
            },
            "needs a 12-byte block",
        ),
        (
            TextureError::ZeroDimension {
                width: 0,
                height: 4,
            },
            "zero-sized image",
        ),
        (TextureError::ZeroMipCount, "mipCount is 0"),
        (
            TextureError::RecordShorterThanEnvelope { record_size: 8 },
            "smaller than the 32-byte",
        ),
        (
            TextureError::ChainSizeOverflow {
                width: u32::MAX,
                height: u32::MAX,
                mip_count: 1,
            },
            "exceeds the addressable size",
        ),
        (
            TextureError::RecordSizeMismatch {
                record_size: 87468,
                expected: 87436,
                width: 256,
                height: 256,
                mip_count: 9,
                fourcc: 0x3554_5844,
            },
            "record is 87468 bytes but fourcc 0x35545844",
        ),
        (
            TextureError::Field18Mismatch {
                declared: 131072,
                expected: 262144,
                width: 512,
                height: 512,
                fourcc: 0x3554_5844,
            },
            "field_18 declares 131072 bytes",
        ),
        (
            TextureError::LevelLengthPrefix {
                mip: 2,
                declared: 2048,
                expected: 4096,
                offset: 86052,
                width: 64,
                height: 64,
            },
            "level 2 declares 2048 bytes at offset 86052",
        ),
        (
            TextureError::LevelSlicePastRecord {
                mip: 2,
                offset: 28,
                size: 8,
                record_size: 32,
            },
            "level 2 needs 8 bytes at offset 28",
        ),
        (
            TextureError::PayloadTooSmall {
                available: 3,
                needed: 16,
            },
            "3 payload bytes < 16",
        ),
        (
            TextureError::ShortBlockChain {
                available: 7,
                needed: 16,
                width: 4,
                height: 4,
            },
            "7 bytes < 16 for 4x4",
        ),
        (
            TextureError::OutputTooLarge {
                width: 1,
                height: 2,
            },
            "exceeds the addressable output",
        ),
        (
            TextureError::OutputInvariant {
                width: 4,
                height: 4,
                texel_x: 5,
                texel_y: 6,
            },
            "internal output-size invariant failed at texel (5, 6)",
        ),
    ];
    for (error, needle) in &cases {
        let text = error.to_string();
        assert!(
            text.contains(needle),
            "message {text:?} does not mention {needle:?}"
        );
        assert!(!text.is_empty());
    }
    // A new variant needs a message case here, or its message is unasserted.
    assert_eq!(
        cases.len(),
        15,
        "a new TextureError variant needs a message case here"
    );
}

/// The refuted framing errors still say they are refuted, so a log line from an
/// older build is recognisable and a caller reading one learns why.
#[test]
fn the_refuted_errors_say_so_in_their_own_message() {
    let text = TextureError::PayloadNotMultiple {
        available: 87436,
        per: 43720,
    }
    .to_string();
    assert!(text.contains("refuted framing"), "{text}");
    let text = TextureError::MipSlicePastRecord {
        layer: 1,
        mip: 2,
        offset: 28,
        size: 8,
        record_size: 32,
    }
    .to_string();
    assert!(text.contains("refuted framing"), "{text}");
}

// ---------------------------------------------------------------------------
// 5. Regression pins over a deterministic corpus
// ---------------------------------------------------------------------------

/// A deterministic pseudo-random block chain, so this test needs no fixture file
/// and no randomness.
///
/// An LCG of the kind C standard libraries ship, taking bits 16..23 of each
/// state as one byte. **This is a regression pin, not an oracle cross-check**:
/// the expected values were produced by this crate's own decoder, because the
/// independent implementations this crate used to agree with were measuring an
/// 8-byte block the corpus refutes. Its value is that any change to the BC3
/// arithmetic — a ramp weight, a channel width, an index width — moves the hash,
/// and a failure names the size that broke.
fn lcg_bytes(count: usize, seed: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(count);
    let mut state = seed;
    for _ in 0..count {
        state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        out.push(((state >> 16) & 0xFF) as u8);
    }
    out
}

/// FNV-1a over 64 bits. A checksum rather than 4096 hard-coded bytes: it pins the
/// whole output, and a failure points at the level that broke.
fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[test]
fn a_pseudo_random_32x32_corpus_has_a_pinned_hash() {
    let chain = lcg_bytes(64 * BLOCK_BYTES, 123_456_789);
    assert_eq!(chain.len(), 64 * BLOCK_BYTES);
    let image = decode_dxt5_mip(&chain, 32, 32).expect("64 blocks cover a 32x32 level");
    assert_eq!(image.pixels.len(), 32 * 32 * 4);
    assert_eq!(fnv1a64(&image.pixels), PINNED_BC3_32);
}

#[test]
fn a_pseudo_random_corpus_has_pinned_hashes_at_awkward_sizes() {
    for (width, height, blocks, expected) in [
        (5u32, 3u32, 2usize, PINNED_BC3_5X3),
        (13, 7, 8, PINNED_BC3_13X7),
        (1, 1, 1, PINNED_BC3_1X1),
        (64, 64, 256, PINNED_BC3_64X64),
    ] {
        let chain = lcg_bytes(blocks * BLOCK_BYTES, 999);
        let image = decode_dxt5_mip(&chain, width, height).expect("the chain covers the level");
        assert_eq!(
            image.pixels.len(),
            width as usize * height as usize * 4,
            "{width}x{height}"
        );
        assert_eq!(fnv1a64(&image.pixels), expected, "{width}x{height}");
        assert_eq!(
            chain.len(),
            dxt5_mip_size(width, height, 0),
            "{width}x{height}"
        );
    }
}

/// Filled in by [`print_the_pinned_bc3_hashes`], which is how a deliberate
/// decoder change updates them.
const PINNED_BC3_32: u64 = 0xa212_0bd4_aef2_97e1;
const PINNED_BC3_5X3: u64 = 0x8583_5bb5_ba62_59a7;
const PINNED_BC3_13X7: u64 = 0xd7bf_1a07_de3f_f95b;
const PINNED_BC3_1X1: u64 = 0x3e2a_5602_9e00_969d;
const PINNED_BC3_64X64: u64 = 0x9ba4_c952_e7ee_ce21;

/// Prints the pinned hashes, so a deliberate decoder change can update them.
///
/// This is `#[ignore]`d because it is a maintenance tool, not a test: it passes
/// whatever the decoder currently does, which is exactly why it must not run in
/// a normal `cargo test`.
#[test]
#[ignore = "prints the BC3 regression pins; run with --ignored --nocapture"]
fn print_the_pinned_bc3_hashes() {
    let mut out = String::new();
    for (width, height, blocks, seed) in [
        (32u32, 32u32, 64usize, 123_456_789u32),
        (5, 3, 2, 999),
        (13, 7, 8, 999),
        (1, 1, 1, 999),
        (64, 64, 256, 999),
    ] {
        let chain = lcg_bytes(blocks * BLOCK_BYTES, seed);
        let image = decode_dxt5_mip(&chain, width, height).expect("decodes");
        out.push_str(&format!(
            "{width}x{height}: {:#018x}\n",
            fnv1a64(&image.pixels)
        ));
    }
    println!("{out}");
}
