//! Envelope parsing, the framing identity, and the whole-record decode.
//!
//! Everything here is synthetic: the real raster records live only in the
//! git-ignored `SPORE/` tree. Each record is built field by field in
//! [`common`], so every byte the walk reads is visible in the test that reads it.
//!
//! # What this file asserts that the previous one could not
//!
//! The previous version asserted a two-layer split with a 16-byte header in
//! front of each layer, and that `layer_count_for` derived 2 from a synthetic
//! record built to fit that rule. **The corpus refutes both**: a raster record
//! has no layer count and no layer header, its levels carry their own `u32` byte
//! counts, and its block is 16 bytes rather than 8. The tests below are the
//! replacements, and they pin the *identity* — the record's length is exactly what
//! its own envelope implies — rather than a division that happened to work.

mod common;

use common::*;
use spore_texture::{
    decode_raster, dxt5_chain_size, dxt5_mip_size, layer_count_for, luminance_record_size,
    mip_dimension, parse_envelope, TextureError, DXT1_FOURCC, DXT5_FOURCC, ENVELOPE_SIZE,
    LUMINANCE_FOURCC,
};

/// The synthetic DXT5 record.
fn record() -> Vec<u8> {
    bc3_record()
}

/// The envelope the synthetic record declares, for direct comparison.
fn envelope() -> spore_texture::RasterEnvelope {
    parse_envelope(&record()).expect("the fixture is a whole record")
}

/// The envelope is eight little-endian `u32` words at their documented offsets.
#[test]
fn the_envelope_parses_field_by_field() {
    let env = envelope();
    assert_eq!(env.version, 1);
    assert_eq!(env.width, 8);
    assert_eq!(env.height, 8);
    assert_eq!(env.mip_count, 2);
    assert_eq!(env.field_10, 8);
    assert_eq!(env.fourcc, DXT5_FOURCC);
    assert_eq!(env.field_18, 64, "the 8x8 base level's four 16-byte blocks");
    assert_eq!(env.field_1c, 0x0000_FFFF);
    // The raw bytes, so a wrong offset cannot hide behind a correct value.
    assert_eq!(&record()[0x14..0x18], b"DXT5");
}

/// `parse_envelope` is pure structure: it reads a format this crate does not
/// decode without complaint, because inspecting an envelope and decoding one are
/// different jobs.
#[test]
fn parse_envelope_does_not_validate_the_format() {
    let mut bytes = record();
    bytes[0x14..0x18].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    let env = parse_envelope(&bytes).expect("structure is readable");
    assert_eq!(env.fourcc, 0xDEAD_BEEF);
    // And decoding it is where the format is named.
    assert!(matches!(
        decode_raster(&bytes),
        Err(TextureError::UnsupportedFourcc {
            fourcc: 0xDEAD_BEEF
        })
    ));
}

/// `field_10` is carried verbatim and never given a meaning.
///
/// The test asserts only that the word survives the round trip and that decoding
/// does not depend on it — which is now checkable, because the walk does read
/// `field_18`.
#[test]
fn field_10_is_carried_verbatim_and_never_decided() {
    let baseline = decode_raster(&record()).expect("decodes");
    for value in [0u32, 1, 8, 0xFFFF_FFFF] {
        let mut bytes = record();
        bytes[0x10..0x14].copy_from_slice(&value.to_le_bytes());
        let decoded = decode_raster(&bytes).expect("decodes whatever field_10 says");
        // The envelope is carried, so it differs by construction; the pixels are
        // the part that must not.
        assert_eq!(
            decoded.layers, baseline.layers,
            "field_10 = {value:#x} changed the decode"
        );
    }
}

/// `field_18` **is** read: a record whose header disagrees with its own geometry
/// is refused, by name, with both numbers.
#[test]
fn field_18_is_a_cross_check_and_a_disagreement_is_refused() {
    let mut bytes = record();
    bytes[0x18..0x1c].copy_from_slice(&65u32.to_le_bytes());
    assert_eq!(
        decode_raster(&bytes),
        Err(TextureError::Field18Mismatch {
            declared: 65,
            expected: 64,
            width: 8,
            height: 8,
            fourcc: DXT5_FOURCC
        })
    );
}

/// Every prefix shorter than the envelope is refused by name, and none panics.
#[test]
fn a_record_shorter_than_the_envelope_is_refused() {
    for length in 0..ENVELOPE_SIZE {
        let error = decode_raster(&vec![0u8; length]).unwrap_err();
        assert_eq!(
            error,
            TextureError::TruncatedEnvelope { available: length },
            "{length} bytes"
        );
        assert!(!error.to_string().is_empty());
    }
}

/// The layer count is **1**, and it comes from the framing identity rather than
/// from a division.
#[test]
fn the_layer_count_is_one_and_the_identity_is_exact() {
    let bytes = record();
    assert_eq!(bytes.len(), BC3_RECORD_SIZE);
    assert_eq!(layer_count_for(bytes.len(), &envelope()), Ok(1));
    // The identity, spelled out: 32 + 64 (bare base) + 16 (bare 4x4).
    assert_eq!(32 + 64 + 16, BC3_RECORD_SIZE);
}

/// Every extra byte, and every missing one, is refused.
///
/// This is the test that replaces "the payload divides by the layer stride". A
/// division has a remainder that could be rounded; an identity has none.
#[test]
fn a_length_that_is_not_the_identity_is_refused() {
    for delta in [-2i64, -1, 1, 2, 16] {
        let length = (BC3_RECORD_SIZE as i64 + delta) as usize;
        let mut bytes = record();
        bytes.resize(length, 0);
        let error = decode_raster(&bytes).unwrap_err();
        match error {
            TextureError::RecordSizeMismatch {
                record_size,
                expected,
                fourcc,
                ..
            } => {
                assert_eq!(record_size, length);
                assert_eq!(expected, BC3_RECORD_SIZE);
                assert_eq!(fourcc, DXT5_FOURCC);
            }
            other => panic!("{length} bytes was refused as {other:?}, not by the identity"),
        }
    }
}

/// The whole DXT5 record decodes: one chain, two levels, the exact texels the
/// fixtures say the blocks hold.
#[test]
fn a_synthetic_dxt5_record_decodes_one_chain_of_two_levels() {
    let image = decode_raster(&record()).expect("decodes");
    assert_eq!(image.layers.len(), 1, "one chain, always");
    let chain = &image.layers[0];
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].width, 8);
    assert_eq!(chain[0].height, 8);
    assert_eq!(chain[1].width, 4);
    assert_eq!(chain[1].height, 4);
    assert_eq!(chain[0].pixels, expected_addressing_bc3_8x8());
    // Level 1 is the solid block: opaque red, all sixteen texels.
    for pixel in chain[1].pixels.chunks_exact(4) {
        assert_eq!(pixel, [255, 0, 0, 255]);
    }
}

/// Level 2 onwards is preceded by a `u32` the walk **reads**, and a prefix that
/// disagrees with the level's geometry is refused by name.
#[test]
fn a_level_length_prefix_is_read_and_checked() {
    let bytes = bc3_three_level_record();
    assert_eq!(bytes.len(), BC3_THREE_RECORD_SIZE);
    let image = decode_raster(&bytes).expect("decodes");
    assert_eq!(image.layers[0].len(), 3);
    // Now lie about the prefix. The prefix of level 2 sits at
    // 32 + 16 + 16 = 64.
    let mut lying = bytes.clone();
    lying[64..68].copy_from_slice(&17u32.to_le_bytes());
    assert_eq!(
        decode_raster(&lying),
        Err(TextureError::LevelLengthPrefix {
            mip: 2,
            declared: 17,
            expected: 16,
            offset: 64,
            width: 1,
            height: 1
        })
    );
}

/// Every level image's buffer is exactly `width * height * 4` bytes.
#[test]
fn every_level_buffer_is_exactly_width_times_height_times_four() {
    for bytes in [
        record(),
        bc3_three_level_record(),
        bc1_record(),
        luminance_record(),
    ] {
        let image = decode_raster(&bytes).expect("decodes");
        for chain in &image.layers {
            for mip in chain {
                assert_eq!(
                    mip.pixels.len(),
                    mip.width as usize * mip.height as usize * 4,
                    "{}x{}",
                    mip.width,
                    mip.height
                );
            }
        }
    }
}

/// The DXT1 record decodes on the same framing, one chain, two levels.
#[test]
fn a_synthetic_dxt1_record_decodes_on_the_same_framing() {
    let bytes = bc1_record();
    assert_eq!(bytes.len(), BC1_RECORD_SIZE);
    let image = decode_raster(&bytes).expect("decodes");
    assert_eq!(image.layers.len(), 1);
    let chain = &image.layers[0];
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].pixels.len(), 8 * 8 * 4);
    // BC1 carries no alpha, so every texel is opaque; the colour of each texel is
    // black or red according to its addressing bit.
    for (index, pixel) in chain[0].pixels.chunks_exact(4).enumerate() {
        let x = (index % 8) as u32;
        let y = (index / 8) as u32;
        let block = (y / 4) * 2 + (x / 4);
        let texel = (y % 4) * 4 + (x % 4);
        // `colour0 = 0x0000` is black and `colour1 = 0xF800` is red, and
        // `colour0 <= colour1` puts them at slots 0 and 1 of the six-entry
        // palette, so index 0 reads black and index 1 red.
        let colour = ((block >> texel) as u8) & 1;
        assert_eq!(pixel[3], 255, "BC1 alpha is opaque");
        assert_eq!(
            &pixel[..3],
            if colour == 1 {
                &[255, 0, 0]
            } else {
                &[0, 0, 0]
            }
        );
    }
}

/// The `0x15` family still decodes on the same walk, with its own two framing
/// differences: a 28-byte header and a prefix from level 1.
#[test]
fn the_luminance_family_decodes_on_the_shared_walk() {
    let bytes = luminance_record();
    assert_eq!(bytes.len(), 120);
    let image = decode_raster(&bytes).expect("decodes");
    assert_eq!(image.layers.len(), 1);
    let chain = &image.layers[0];
    assert_eq!(chain.len(), 3);
    assert_eq!(chain[0].pixels.len(), 4 * 4 * 4);
    // Level 1's prefix is at 28 + 64 = 92, and a lie there is caught.
    let mut lying = bytes.clone();
    lying[92..96].copy_from_slice(&17u32.to_le_bytes());
    assert!(matches!(
        decode_raster(&lying),
        Err(TextureError::LevelLengthPrefix { mip: 1, .. })
    ));
}

/// A record whose size is not the identity is refused for the `0x15` family too.
#[test]
fn a_luminance_record_of_the_wrong_length_is_refused() {
    let mut bytes = luminance_record();
    bytes.push(0);
    assert!(matches!(
        decode_raster(&bytes),
        Err(TextureError::RecordSizeMismatch {
            record_size: 121,
            expected: 120,
            fourcc: LUMINANCE_FOURCC,
            ..
        })
    ));
}

/// A `fourcc` this build has no measured framing for is refused by name.
///
/// The set is now the formats nobody has measured, which after the 2026-10-04
/// re-measurement is **none of the corpus**. That is the point: the refusal names
/// what *is* decoded, so a log line says what was available.
#[test]
fn an_unsupported_fourcc_is_refused_by_name() {
    let mut bytes = record();
    bytes[0x14..0x18].copy_from_slice(&0x1234_5678u32.to_le_bytes());
    let error = decode_raster(&bytes).unwrap_err();
    let text = error.to_string();
    assert!(text.contains("0x12345678"), "{text}");
    assert!(text.contains("0x35545844"), "{text}: DXT5 must be named");
    assert!(text.contains("0x31545844"), "{text}: DXT1 must be named");
    assert!(
        text.contains("0x00000015"),
        "{text}: the 0x15 family must be named"
    );
}

/// A zero dimension is refused before any allocation, and named.
#[test]
fn a_zero_dimension_is_refused() {
    for (width, height) in [(0u32, 4u32), (4, 0), (0, 0)] {
        let mut bytes = record();
        bytes[0x04..0x08].copy_from_slice(&width.to_le_bytes());
        bytes[0x08..0x0c].copy_from_slice(&height.to_le_bytes());
        assert_eq!(
            decode_raster(&bytes),
            Err(TextureError::ZeroDimension { width, height })
        );
    }
}

/// `mip_count == 0` is refused: a zero-length chain would make the framing
/// identity vacuously true and accept a record whose content is something else.
#[test]
fn a_zero_mip_count_is_refused() {
    let mut bytes = record();
    bytes[0x0c..0x10].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(decode_raster(&bytes), Err(TextureError::ZeroMipCount));
    assert_eq!(layer_count_for(bytes.len(), &envelope()), Ok(1));
    assert_eq!(
        layer_count_for(bytes.len(), &parse_envelope(&bytes).unwrap()),
        Err(TextureError::ZeroMipCount)
    );
}

/// A record shorter than the envelope is refused by `layer_count_for` too.
#[test]
fn layer_count_refuses_a_record_shorter_than_the_envelope() {
    let env = envelope();
    assert_eq!(
        layer_count_for(8, &env),
        Err(TextureError::RecordShorterThanEnvelope { record_size: 8 })
    );
}

/// The corpus's own record sizes, restated as an assertion on the public API.
///
/// These seven numbers are the whole finding: each is a real raster record's
/// length from `Spore_Content.package`, and `decode_raster`'s identity reproduces
/// every one. They are duplicated from `framing`'s unit tests on purpose — those
/// test the arithmetic, these test that the *decoder* agrees with it.
#[test]
fn the_corpus_record_sizes_are_reproduced_by_the_public_identity() {
    for (fourcc, w, h, mips, size) in [
        (DXT5_FOURCC, 512u32, 512u32, 10u32, 349_616usize),
        (DXT5_FOURCC, 256, 256, 9, 87_468),
        (DXT5_FOURCC, 64, 64, 7, 5_540),
        (DXT1_FOURCC, 256, 256, 9, 43_764),
        (LUMINANCE_FOURCC, 512, 512, 10, 1_398_164),
        (LUMINANCE_FOURCC, 256, 256, 9, 349_584),
        (LUMINANCE_FOURCC, 32, 32, 6, 5_508),
    ] {
        // An all-zero body of the right length: the identity is about framing,
        // and this test is about the identity, so the pixels are irrelevant.
        let mut bytes = vec![0u8; size];
        bytes[0x00..0x04].copy_from_slice(&1u32.to_le_bytes());
        bytes[0x04..0x08].copy_from_slice(&w.to_le_bytes());
        bytes[0x08..0x0c].copy_from_slice(&h.to_le_bytes());
        bytes[0x0c..0x10].copy_from_slice(&mips.to_le_bytes());
        bytes[0x10..0x14].copy_from_slice(&8u32.to_le_bytes());
        bytes[0x14..0x18].copy_from_slice(&fourcc.to_le_bytes());
        let base: usize = if fourcc == LUMINANCE_FOURCC {
            (w * h * 4) as usize
        } else if fourcc == DXT1_FOURCC {
            spore_texture::dxt1_mip_size(w, h, 0)
        } else {
            dxt5_mip_size(w, h, 0)
        };
        bytes[0x18..0x1c].copy_from_slice(&(base as u32).to_le_bytes());
        // Write the level prefixes the identity expects, so the body is a real
        // (blank) record rather than one that the prefix check must refuse.
        let frame = spore_texture::frame_for(fourcc).expect("a frame");
        for mip in 0..mips {
            if mip >= frame.first_prefixed_level {
                let at = spore_texture::framing::level_offset(frame, w, h, mip, mips)
                    .expect("a level the envelope declares");
                let level = spore_texture::framing::level_size(frame, w, h, mip);
                bytes[at..at + 4].copy_from_slice(&(level as u32).to_le_bytes());
            }
        }
        let env = parse_envelope(&bytes).expect("envelope");
        assert_eq!(
            layer_count_for(size, &env),
            Ok(1),
            "{fourcc:#010x} {w}x{h} mips={mips} size={size}"
        );
        // And the whole record decodes: a blank body is a valid (if empty) image.
        let decoded = decode_raster(&bytes).expect("a blank body still decodes");
        assert_eq!(decoded.layers.len(), 1);
        assert_eq!(decoded.layers[0].len(), mips as usize);
    }
}

/// The DXT5 chain and level sizes are the 16-byte ones, and the field is the base
/// level's byte count.
#[test]
fn the_block_stride_is_sixteen_bytes_and_field_18_is_the_base_level() {
    // 512x512: 16384 blocks of 16 = 262144, which is field_18 on every real
    // 512x512 DXT5 record. The old 8-byte reading gave 131072 and the crate
    // therefore read "two base levels".
    assert_eq!(dxt5_mip_size(512, 512, 0), 262_144);
    assert_eq!(dxt5_chain_size(512, 512, 10), 349_552);
    // 256x256: 4096 blocks of 16 = 65536.
    assert_eq!(dxt5_mip_size(256, 256, 0), 65_536);
    assert_eq!(dxt5_chain_size(256, 256, 9), 87_408);
    // 64x64: 256 blocks of 16 = 4096.
    assert_eq!(dxt5_mip_size(64, 64, 0), 4_096);
    assert_eq!(dxt5_chain_size(64, 64, 7), 5_488);
    // And the 0x15 family's identity is its own.
    assert_eq!(luminance_record_size(32, 32, 6), 5_508);
    assert_eq!(luminance_record_size(256, 256, 9), 349_584);
    assert_eq!(luminance_record_size(512, 512, 10), 1_398_164);
}

/// `mip_dimension` is the shared rule every format derives levels with.
#[test]
fn the_level_dimension_is_the_shared_rule() {
    assert_eq!(mip_dimension(256, 0), 256);
    assert_eq!(mip_dimension(256, 8), 1);
    assert_eq!(mip_dimension(256, 9), 1);
    assert_eq!(mip_dimension(3, 1), 1);
}
