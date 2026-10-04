//! Envelope parsing, the derived layer split, and the whole-record decode.
//!
//! Everything here is synthetic: the real raster records live only in the
//! git-ignored `SPORE/` tree. The one non-synthetic input is the *record bytes*
//! in [`common::SYNTHETIC_RECORD_HEX`], which is not from a game installation
//! either — it is a record this crate's tests build field by field, and which
//! was also assembled and decoded by `tools/spore/raster/raster.py` so that the
//! layer split, the mip dimensions and every texel have an independent oracle.

mod common;

use common::*;
use spore_texture::{
    decode_raster, layer_count_for, parse_envelope, RasterEnvelope, TextureError, DXT5_FOURCC,
    ENVELOPE_SIZE, LAYER_HEADER_SIZE, RASTER_TYPE,
};

/// The synthetic record, so the byte counts below are checkable.
fn record() -> Vec<u8> {
    synthetic_record()
}

/// The envelope the synthetic record declares, for direct comparison.
fn envelope() -> RasterEnvelope {
    RasterEnvelope {
        version: 1,
        width: SYNTHETIC_WIDTH,
        height: SYNTHETIC_HEIGHT,
        mip_count: SYNTHETIC_MIPS,
        field_10: 8,
        fourcc: DXT5_FOURCC,
        field_18: 0x0004_0000,
        field_1c: 0x0000_FFFF,
    }
}

// ---------------------------------------------------------------------------
// The envelope
// ---------------------------------------------------------------------------

/// The envelope is eight little-endian `u32` words at their documented offsets.
#[test]
fn the_envelope_parses_field_by_field() {
    assert_eq!(
        parse_envelope(&record()).expect("a full record has an envelope"),
        envelope()
    );
}

/// `parse_envelope` is pure structure: it reads a format this crate does not
/// decode without complaint, because inspecting an envelope and decoding one are
/// different jobs.
#[test]
fn parse_envelope_does_not_validate_the_format() {
    let mut bytes = record();
    bytes[0x14..0x18].copy_from_slice(&0x1500_0000u32.to_le_bytes());
    bytes[0x0C..0x10].copy_from_slice(&0u32.to_le_bytes());
    let parsed = parse_envelope(&bytes).expect("structure only");
    assert_eq!(
        parsed.fourcc, 0x1500_0000,
        "the luminance family is readable, not decodable"
    );
    assert_eq!(parsed.mip_count, 0);
    assert_eq!(parsed.field_10, 8);
    assert_eq!(parsed.field_18, 0x0004_0000);
    assert_eq!(parsed.field_1c, 0x0000_FFFF);
}

/// The unresolved words are carried verbatim and are never given a meaning.
///
/// `field_10` and `field_18` hold the same value in every sampled record and
/// nobody knows what they are; this test asserts only that they survive the
/// round trip and that decoding does not depend on them.
#[test]
fn the_unresolved_words_are_carried_verbatim_and_never_decided() {
    let mut bytes = record();
    // Zero and non-zero values of the unresolved words must change nothing.
    for filler in [0u32, 0xDEAD_BEEF] {
        bytes[0x10..0x14].copy_from_slice(&filler.to_le_bytes());
        bytes[0x18..0x1C].copy_from_slice(&filler.to_le_bytes());
        let image = decode_raster(&bytes).expect("the unresolved words drive nothing");
        assert_eq!(image.envelope.field_10, filler);
        assert_eq!(image.envelope.field_18, filler);
        assert_eq!(
            image.layers.len(),
            2,
            "the split cannot depend on an unresolved word"
        );
        assert_eq!(image.layers[0][0].pixels, expected_addressing_8x8());
    }
}

/// Every prefix shorter than the envelope is refused by name, and none panics.
#[test]
fn a_record_shorter_than_the_envelope_is_refused() {
    for length in 0..ENVELOPE_SIZE {
        let bytes = vec![0u8; length];
        assert_eq!(
            parse_envelope(&bytes).expect_err("a short record has no envelope"),
            TextureError::TruncatedEnvelope { available: length },
            "length {length}"
        );
        assert_eq!(
            decode_raster(&bytes).expect_err("a short record decodes to nothing"),
            TextureError::TruncatedEnvelope { available: length },
            "length {length}"
        );
    }
    assert_eq!(ENVELOPE_SIZE, 32);
    assert!(
        parse_envelope(&[0u8; ENVELOPE_SIZE]).is_ok(),
        "32 bytes is an envelope"
    );
}

// ---------------------------------------------------------------------------
// The derived layer split
// ---------------------------------------------------------------------------

/// `layer_count_for` derives 2 from the record size, and agrees with the
/// envelope's own geometry: `chain_size(8, 8, 2)` is 40, so one layer is
/// `16 + 40 = 56` bytes and `(144 - 32) / 56 = 2`.
#[test]
fn the_layer_count_is_derived_from_the_record_size() {
    let bytes = record();
    let parsed = parse_envelope(&bytes).expect("a full record has an envelope");
    assert_eq!(bytes.len(), 144);
    assert_eq!(layer_count_for(bytes.len(), &parsed), Ok(2));
    assert_eq!((bytes.len() - ENVELOPE_SIZE) % (LAYER_HEADER_SIZE + 40), 0);
}

/// The two layer headers sit together at the front of the payload region, so the
/// first payload byte is `32 + 2 * 16 = 0x40`.
///
/// This is the load-bearing layout assumption: if the headers were interleaved
/// with their payloads, layer 0's first block would be read from `0x30` and the
/// decode would still succeed — with different pixels. The exact-pixel
/// assertions in [`a_synthetic_record_decodes_two_layers_of_two_mips`] are what
/// catch that, and they pass only for the grouped layout.
#[test]
fn all_layer_headers_precede_all_payloads() {
    let bytes = record();
    assert_eq!(
        &bytes[ENVELOPE_SIZE..ENVELOPE_SIZE + 4],
        b"e9D1",
        "layer 0 header name"
    );
    assert_eq!(
        &bytes[ENVELOPE_SIZE + LAYER_HEADER_SIZE..ENVELOPE_SIZE + LAYER_HEADER_SIZE + 4],
        &[0, 0, 0, 0]
    );
    let first_payload = ENVELOPE_SIZE + 2 * LAYER_HEADER_SIZE;
    assert_eq!(first_payload, 0x40);
    assert_eq!(
        &bytes[first_payload..first_payload + 8],
        &[0x00, 0xFF, 0x20, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
}

/// The whole record decodes: two layers, two mips each, with the exact pixels the
/// oracle produces for the same bytes.
///
/// Layer 0's mip 0 is the four addressing blocks; its mip 1 is the
/// four-colour ramp block. Layer 1 is a flat field of `(8,0,0,191)`.
#[test]
fn a_synthetic_record_decodes_two_layers_of_two_mips() {
    let image = decode_raster(&record()).expect("the synthetic record decodes");
    assert_eq!(image.envelope, envelope());
    assert_eq!(
        image.layers.len(),
        2,
        "two layers are derived from the record size"
    );
    for layer in &image.layers {
        assert_eq!(layer.len(), 2, "mip_count is 2");
        assert_eq!((layer[0].width, layer[0].height), (8, 8));
        assert_eq!((layer[1].width, layer[1].height), (4, 4));
    }

    assert_eq!(image.layers[0][0].pixels.len(), 8 * 8 * 4);
    assert_eq!(image.layers[0][0].pixels, expected_addressing_8x8());
    assert_eq!(image.layers[0][1].pixels.len(), 4 * 4 * 4);
    assert_eq!(image.layers[0][1].pixels, expected_four_colour_ramp());
    assert_eq!(
        image.layers[1][0].pixels,
        vec![[8u8, 0, 0, 191]; 64].concat()
    );
    assert_eq!(image.layers[1][1].pixels, [[8u8, 0, 0, 191]; 16].concat());
}

/// Every mip image's buffer is exactly `width * height * 4` bytes, for every
/// layer and every mip.
#[test]
fn every_mip_buffer_is_exactly_width_times_height_times_four() {
    let image = decode_raster(&record()).expect("the synthetic record decodes");
    for (layer_index, layer) in image.layers.iter().enumerate() {
        for (mip_index, mip) in layer.iter().enumerate() {
            assert_eq!(
                mip.pixels.len(),
                mip.width as usize * mip.height as usize * 4,
                "layer {layer_index} mip {mip_index}"
            );
        }
    }
}

/// The builder's bytes are the bytes the oracle was run against.
#[test]
fn the_builder_reproduces_the_oracle_record_byte_for_byte() {
    assert_eq!(record(), unhex(SYNTHETIC_RECORD_HEX));
    assert_eq!(record().len(), 144);
}

// ---------------------------------------------------------------------------
// Rejections
// ---------------------------------------------------------------------------

/// A `fourcc` this build does not decode is refused by name.
///
/// The set shrank when the `0x15` family stopped being refused: it is decoded
/// now. What remains is genuinely unsupported — the `DXT1` records that are
/// present in the corpus and whose framing was *measured not to fit* either
/// rule, plus anything unrecognised. The refusal names both decoded formats so
/// the message says what was available.
#[test]
fn an_unsupported_fourcc_is_refused_by_name() {
    for fourcc in [
        0x3154_5844u32, // 'DXT1': 306 records in Spore_Content.package
        0x1500_0000,    // the byte-swapped spelling of 0x15, which is not it
        0x3554_5843,    // 'DXT3'
        0,
    ] {
        let mut bytes = record();
        bytes[0x14..0x18].copy_from_slice(&fourcc.to_le_bytes());
        assert_eq!(
            decode_raster(&bytes).expect_err("only DXT5 and 0x15 are decoded"),
            TextureError::UnsupportedFourcc { fourcc },
            "fourcc 0x{fourcc:08X}"
        );
    }
}

/// `0x15` is no longer a refusal, so the old "refuse the luminance family" test
/// has become "the DXT5 body's bytes are not a `0x15` record": the same input
/// now fails with the framing error instead of the format error, and it is worth
/// pinning that the two errors are genuinely different rather than the same
/// refusal under a new name.
#[test]
fn the_dxt5_body_is_not_a_luminance_record_and_says_so() {
    let mut bytes = record();
    bytes[0x14..0x18].copy_from_slice(&0x0000_0015u32.to_le_bytes());
    // 8x8 with 2 mips would be 28 + 256 + 4 + 64 = 352 bytes; the record is 144.
    assert_eq!(
        decode_raster(&bytes).expect_err("the framing does not close"),
        TextureError::LuminanceRecordSize {
            record_size: 144,
            expected: 352,
            width: SYNTHETIC_WIDTH,
            height: SYNTHETIC_HEIGHT,
            mip_count: SYNTHETIC_MIPS,
        }
    );
}

/// A zero dimension is refused before any allocation, and named.
#[test]
fn a_zero_dimension_is_refused() {
    for (width, height) in [(0u32, SYNTHETIC_HEIGHT), (SYNTHETIC_WIDTH, 0), (0, 0)] {
        let mut bytes = record();
        bytes[0x04..0x08].copy_from_slice(&width.to_le_bytes());
        bytes[0x08..0x0C].copy_from_slice(&height.to_le_bytes());
        assert_eq!(
            decode_raster(&bytes).expect_err("a zero-sized image is refused"),
            TextureError::ZeroDimension { width, height }
        );
        let parsed = parse_envelope(&bytes).expect("structure only");
        assert_eq!(
            layer_count_for(bytes.len(), &parsed),
            Err(TextureError::ZeroDimension { width, height })
        );
    }
}

/// `mip_count == 0` is refused: a zero-length chain would make the layer stride
/// 16 bytes and quietly accept a record whose real content is something else.
#[test]
fn a_zero_mip_count_is_refused() {
    let mut bytes = record();
    bytes[0x0C..0x10].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        decode_raster(&bytes).expect_err("no base level"),
        TextureError::ZeroMipCount
    );
    let parsed = parse_envelope(&bytes).expect("structure only");
    assert_eq!(
        layer_count_for(bytes.len(), &parsed),
        Err(TextureError::ZeroMipCount)
    );
}

/// A payload smaller than one layer is refused, with both numbers.
#[test]
fn a_payload_smaller_than_one_layer_is_refused() {
    let bytes = record();
    let parsed = parse_envelope(&bytes).expect("a full record has an envelope");
    let per_layer = LAYER_HEADER_SIZE + 40;
    for remaining in [0usize, 1, per_layer / 2, per_layer - 1] {
        let size = ENVELOPE_SIZE + remaining;
        assert_eq!(
            layer_count_for(size, &parsed),
            Err(TextureError::PayloadTooSmall {
                available: remaining,
                needed: per_layer
            }),
            "payload {remaining}"
        );
    }
}

/// A payload that does not divide by the layer stride is refused. This is the
/// check that catches a wrong split rule, so it is tested from both sides.
#[test]
fn a_payload_that_does_not_divide_is_refused() {
    let bytes = record();
    let parsed = parse_envelope(&bytes).expect("a full record has an envelope");
    let per_layer = LAYER_HEADER_SIZE + 40;
    for extra in [1usize, 7, 16, per_layer - 1] {
        let remaining = per_layer + extra;
        assert_eq!(
            layer_count_for(ENVELOPE_SIZE + remaining, &parsed),
            Err(TextureError::PayloadNotMultiple {
                available: remaining,
                per: per_layer
            }),
            "payload {remaining}"
        );
    }
    // A record with a trailing byte appended is exactly this case.
    let mut padded = bytes.clone();
    padded.push(0);
    assert_eq!(
        decode_raster(&padded).expect_err("a stray trailing byte breaks the split"),
        TextureError::PayloadNotMultiple {
            available: 113,
            per: 56
        }
    );
}

/// A record smaller than the envelope cannot be split at all.
#[test]
fn layer_count_for_refuses_a_record_smaller_than_the_envelope() {
    for record_size in 0..ENVELOPE_SIZE {
        assert_eq!(
            layer_count_for(record_size, &envelope()),
            Err(TextureError::RecordShorterThanEnvelope { record_size }),
            "record size {record_size}"
        );
    }
}

/// An absurd envelope is refused, and the refusal names the geometry rather than
/// going on to slice.
///
/// A `u32::MAX` square needs a `2^63`-byte chain, so the payload of any realistic
/// record size is smaller than one layer and `PayloadTooSmall` reports the
/// impossible stride. On a 64-bit target `ChainSizeOverflow` sits behind that arm
/// and is unreachable from `u32` arguments (the largest representable chain is
/// `12_297_829_382_473_034_416`, below `usize::MAX`); it is the live arm on a
/// 32-bit target, where the same call saturates. Its message is covered by
/// `every_error_message_names_its_section`.
#[test]
fn an_absurd_envelope_is_refused_before_any_slicing() {
    let huge = RasterEnvelope {
        width: u32::MAX,
        height: u32::MAX,
        ..envelope()
    };
    let per_layer = (1usize << 63) + (1usize << 61) + 16;
    assert_eq!(
        layer_count_for(ENVELOPE_SIZE + 64, &huge),
        Err(TextureError::PayloadTooSmall {
            available: 64,
            needed: per_layer
        })
    );
}

/// The per-mip slice bound is unreachable through `decode_raster` because the
/// derived split gives each layer exactly its chain; the bound exists as the
/// limit on that assumption, and the branch is exercised directly in the
/// crate's own unit tests. What this test can assert is the property that makes
/// it unreachable: every mip slice of a valid record lies inside the record.
#[test]
fn every_mip_slice_of_a_valid_record_lies_inside_the_record() {
    let bytes = record();
    let image = decode_raster(&bytes).expect("the synthetic record decodes");
    let chain = 40usize;
    for (layer_index, _) in image.layers.iter().enumerate() {
        let mut cursor = ENVELOPE_SIZE + 2 * LAYER_HEADER_SIZE + layer_index * chain;
        for mip in 0..SYNTHETIC_MIPS {
            let size = match mip {
                0 => 4 * 8,
                _ => 8,
            };
            assert!(
                cursor + size <= bytes.len(),
                "layer {layer_index} mip {mip}"
            );
            cursor += size;
        }
        assert_eq!(
            cursor,
            ENVELOPE_SIZE + 2 * LAYER_HEADER_SIZE + (layer_index + 1) * chain,
            "layer {layer_index}'s slices end at the next layer's start"
        );
    }
    // The last layer's slices end exactly at the record end, which is what makes
    // the out-of-record mip bound unreachable through this path.
    for (layer_index, _) in image.layers.iter().enumerate() {
        let mut cursor = ENVELOPE_SIZE + 2 * LAYER_HEADER_SIZE + layer_index * chain;
        for mip in 0..SYNTHETIC_MIPS {
            cursor += if mip == 0 { 4 * 8 } else { 8 };
        }
        if layer_index + 1 == image.layers.len() {
            assert_eq!(cursor, bytes.len(), "the last layer ends at the record end");
        }
    }
}

/// The same envelope with one layer instead of two decodes to one layer: the
/// count is derived from the record size, so it is a fact about the record and
/// not a constant.
///
/// This is the case that a "read the layer count from the header" implementation
/// cannot get right, because there is no such field.
#[test]
fn the_same_envelope_with_one_layer_derives_one_layer() {
    let bytes = single_layer_record();
    assert_eq!(bytes.len(), 88);
    let parsed = parse_envelope(&bytes).expect("a full record has an envelope");
    assert_eq!(
        parsed,
        envelope(),
        "the envelope is byte-identical to the two-layer record"
    );
    assert_eq!(layer_count_for(bytes.len(), &parsed), Ok(1));

    let image = decode_raster(&bytes).expect("one layer divides evenly");
    assert_eq!(image.layers.len(), 1);
    assert_eq!(image.layers[0].len(), 2);
    assert_eq!(image.layers[0][0].pixels, expected_addressing_8x8());
    assert_eq!(image.layers[0][1].pixels, expected_four_colour_ramp());
}

// ---------------------------------------------------------------------------
// Truncation sweep
// ---------------------------------------------------------------------------

/// Every prefix of the synthetic record: `Ok` or `Err`, never a panic, and never
/// an `Ok` that claims more layers or more mips than its bytes can hold.
#[test]
fn every_prefix_length_of_the_record_is_ok_or_err_and_never_panics() {
    let bytes = record();
    let mut ok_count = 0usize;
    let mut err_count = 0usize;
    for length in 0..=bytes.len() {
        let prefix = &bytes[..length];
        match decode_raster(prefix) {
            Ok(image) => {
                ok_count += 1;
                assert!(
                    !image.layers.is_empty(),
                    "length {length}: a record with no layer"
                );
                for (layer_index, layer) in image.layers.iter().enumerate() {
                    assert_eq!(layer.len() as u32, SYNTHETIC_MIPS, "length {length}");
                    for mip in layer {
                        assert_eq!(
                            mip.pixels.len(),
                            mip.width as usize * mip.height as usize * 4,
                            "length {length}, layer {layer_index}"
                        );
                    }
                }
                // An `Ok` must mean the split consumed the record exactly.
                assert_eq!(
                    layer_count_for(length, &image.envelope),
                    Ok(image.layers.len()),
                    "length {length}"
                );
            }
            Err(_) => err_count += 1,
        }
    }
    assert!(
        ok_count > 0 && err_count > 0,
        "the sweep must see both outcomes"
    );
    assert_eq!(ok_count + err_count, bytes.len() + 1);
}

/// Every prefix of the envelope, fed to `layer_count_for` alongside a valid
/// envelope: the same no-panic sweep for the derived-split entry point.
#[test]
fn every_record_size_against_a_valid_envelope_is_ok_or_err_and_never_panics() {
    let env = envelope();
    for size in 0..=256usize {
        let _ = layer_count_for(size, &env);
    }
}

/// Every prefix of the first layer's payload, decoded as an mip: the block
/// codec's own sweep, for every prefix length of a real block chain.
#[test]
fn every_prefix_of_a_real_block_chain_is_ok_or_err_and_never_panics() {
    let bytes = record();
    let chain_start = ENVELOPE_SIZE + 2 * LAYER_HEADER_SIZE;
    let chain = &bytes[chain_start..chain_start + 32];
    for length in 0..=chain.len() {
        let _ = spore_texture::decode_dxt5_mip(&chain[..length], 8, 8);
    }
}

/// The record type id is the raster type, and the fourcc is the DXT5 one: the
/// two ids must not be confused for each other.
#[test]
fn the_record_type_and_the_fourcc_are_different_identities() {
    assert_eq!(RASTER_TYPE, 0x2F4E_681C);
    assert_eq!(DXT5_FOURCC, 0x3554_5844);
    assert_ne!(RASTER_TYPE, DXT5_FOURCC);
}
