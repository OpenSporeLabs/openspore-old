//! The `0x15` family, end to end, from synthetic bytes only.
//!
//! **No Spore asset bytes are committed here.** The real records live in the
//! git-ignored `SPORE/` tree; this file transcribes the *measurements* that
//! established the format (the three shapes the family occurs in, their record
//! sizes, the length prefix values found near the end of a real record) and
//! builds every byte under test from those numbers. The derivation is
//! `docs/LUMINANCE-RASTER.md`; the constants are `tests/common/mod.rs`.
//!
//! What these tests protect, in order of how expensive a regression would be:
//!
//! 1. **The framing identity.** `record_size == 28 + base + sum(4 + level)` for
//!    every mip past the base, holding on the three measured corpus shapes. A
//!    decoder with the wrong header length, the wrong prefix placement or a
//!    missing prefix still "decodes" — it just returns the wrong pixels — so
//!    this is the assertion that matters most.
//! 2. **Refusals by name.** A length prefix that disagrees with its mip's
//!    geometry, a record whose size does not close, an unknown `fourcc`, a
//!    zero mip count: four different problems, four different errors.
//! 3. **No panics over any input shape.** Every prefix length of the record, and
//!    every record length against a valid envelope.
//! 4. **The byte-order claim.** The four stored bytes come out as R, G, B, A in
//!    *file* order, which is the whole of `claims::luminance_0x15_channel_\
//!    swizzle`'s observation.

mod common;

use common::{
    luminance_record, luminance_single_mip_record, FLAT_NORMAL_TEXEL, LUM_HEIGHT, LUM_MIPS,
    LUM_WIDTH, TILTED_NORMAL_TEXEL,
};
use spore_texture::{
    claims, decode_luminance_mip, decode_raster, layer_count_for, luminance_chain_size,
    luminance_dimension, luminance_mip_size, luminance_record_size, parse_envelope, TextureError,
    LUMINANCE_BYTES_PER_TEXEL, LUMINANCE_FOURCC, LUMINANCE_HEADER_SIZE, LUMINANCE_PREFIX_SIZE,
};

// ---------------------------------------------------------------------------
// The framing identity
// ---------------------------------------------------------------------------

/// The identity, on the three shapes `0x15` occurs in across the corpus.
///
/// These four `(width, height, mip_count, record_size)` tuples are measurements,
/// not constructions: every one of the 901 records of the family in
/// `Spore_Content.package` is one of them, and each was verified to have its
/// prefix values equal to their own mip's `w * h * 4` and to consume exactly.
#[test]
fn the_three_corpus_shapes_close_exactly() {
    for (width, height, mips, size) in [
        (32u32, 32u32, 6u32, 5_508usize),
        (256, 256, 9, 349_584),
        (512, 512, 10, 1_398_164),
    ] {
        assert_eq!(
            luminance_record_size(width, height, mips),
            size,
            "{width}x{height} mips={mips}"
        );
        // And the identity written out, so the table above cannot be satisfied
        // by a formula that happens to be right for the wrong reason.
        let mut manual = LUMINANCE_HEADER_SIZE + luminance_mip_size(width, height, 0);
        for mip in 1..mips {
            manual += LUMINANCE_PREFIX_SIZE + luminance_mip_size(width, height, mip);
        }
        assert_eq!(manual, size, "{width}x{height} mips={mips}");
        assert_eq!(luminance_chain_size(width, height, mips), size - 28);
    }
}

/// `field_18` equals `width * height * 4` in every measured record.
///
/// It is 0x1000 for the 32² shape, 0x40000 for 256² and 0x100000 for 512², in
/// 901 of 901 records. The decoder **does not read it** — it derives the same
/// number from the geometry — so this test is here to pin the *claim*, and to
/// make it visible that the claim is about a word nothing depends on.
#[test]
fn field_18_is_the_base_level_byte_count_and_nothing_reads_it() {
    for (width, height, mips, size) in [
        (32u32, 32u32, 6u32, 5_508usize),
        (256u32, 256u32, 9u32, 349_584usize),
        (512u32, 512u32, 10u32, 1_398_164usize),
    ] {
        // `0x18` is the sixth word: version, width, height, mips, field_10, fourcc.
        let declared = width * height * 4;
        assert_eq!(declared as usize, luminance_mip_size(width, height, 0));
        assert_ne!(
            declared as usize, size,
            "field_18 is the base level, not the record"
        );
        // A record whose field_18 disagrees still decodes: the geometry wins and
        // the disagreeing word is carried but unused. That is the "derived, never
        // read" rule, and it is why no test may make a decode depend on it.
        let mut bytes = luminance_record();
        bytes[0x18..0x1C].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        let image = decode_raster(&bytes).expect("field_18 is not load-bearing");
        assert_eq!(image.envelope.field_18, 0xDEAD_BEEF);
        assert_eq!(image.envelope.fourcc, LUMINANCE_FOURCC);
        assert_eq!(luminance_record_size(width, height, mips), size);
    }
}

/// A one-mip record has no prefix at all, which is the whole reason the base
/// level is unprefixed rather than merely lucky.
#[test]
fn a_single_mip_record_has_no_prefix() {
    let bytes = luminance_single_mip_record();
    assert_eq!(bytes.len(), 28 + 16 * 4);
    let envelope = parse_envelope(&bytes).expect("structure only");
    assert_eq!(envelope.mip_count, 1);
    assert_eq!(
        layer_count_for(bytes.len(), &envelope).expect("one layer"),
        1
    );
    let image = decode_raster(&bytes).expect("decodes");
    assert_eq!(image.layers.len(), 1);
    assert_eq!(image.layers[0].len(), 1);
    assert_eq!(image.layers[0][0].width, LUM_WIDTH);
    // The whole record is the base level, so its first texel is at 0x1c.
    assert_eq!(&bytes[28..32], &TILTED_NORMAL_TEXEL);
}

// ---------------------------------------------------------------------------
// Decoding
// ---------------------------------------------------------------------------

/// The record decodes to one layer, three mips, at the right dimensions and
/// with the right bytes in every one.
#[test]
fn a_synthetic_record_decodes_one_layer_of_three_mips() {
    let image = decode_raster(&luminance_record()).expect("decodes");
    assert_eq!(image.envelope.fourcc, LUMINANCE_FOURCC);
    assert_eq!(image.envelope.version, 1);
    assert_eq!(image.envelope.field_10, 8);
    assert_eq!(
        image.layers.len(),
        1,
        "0x15 has no layer headers, so one layer"
    );
    let mips = &image.layers[0];
    assert_eq!(mips.len(), LUM_MIPS as usize);
    assert_eq!(
        mips.iter().map(|m| (m.width, m.height)).collect::<Vec<_>>(),
        vec![(4, 4), (2, 2), (1, 1)]
    );
    for mip in mips {
        assert_eq!(
            mip.pixels.len(),
            mip.width as usize * mip.height as usize * LUMINANCE_BYTES_PER_TEXEL
        );
    }
    let flat16: Vec<u8> = FLAT_NORMAL_TEXEL
        .iter()
        .copied()
        .cycle()
        .take(16 * 4)
        .collect();
    let flat4: Vec<u8> = FLAT_NORMAL_TEXEL
        .iter()
        .copied()
        .cycle()
        .take(4 * 4)
        .collect();
    assert_eq!(mips[0].pixels, flat16);
    assert_eq!(mips[1].pixels, flat4);
    // The last mip is a different texel, so a walk that forgot to advance its
    // cursor would return the flat normal three times instead.
    assert_eq!(mips[2].pixels, TILTED_NORMAL_TEXEL.to_vec());
}

/// The bytes come out in **file order**, unmodified.
///
/// This is the observation `claims::luminance_0x15_channel_swizzle` records and
/// the reason it is a *question* rather than a fact: lane 0 is the normal map's
/// Z, so a consumer wanting the conventional R=X, G=Y, B=Z order must re-index.
/// The decoder does not, because nothing in the workspace establishes which
/// order the renderer wants. Pinning the pass-through here means changing that
/// decision has to change this test on purpose.
#[test]
fn the_four_stored_bytes_become_rgba_in_file_order() {
    let image = decode_raster(&luminance_record()).expect("decodes");
    let first = &image.layers[0][0].pixels[0..4];
    assert_eq!(
        first, &FLAT_NORMAL_TEXEL,
        "R, G, B, A are the stored bytes 0..4, in that order"
    );
    // Spelled out, so the mapping is a statement in this file and not only in a
    // module doc: R = Z, G = X, B = Y, A = the fourth lane.
    let (r, g, b, a) = (first[0], first[1], first[2], first[3]);
    assert_eq!((r, g, b), (0xFF, 0x80, 0x80), "the flat normal, unswizzled");
    assert_eq!(a, 0xFF);
}

/// Every mip's byte count is exactly `width * height * 4`, and the record
/// contains nothing else: the walk's cursor ends on the last byte.
#[test]
fn the_walk_consumes_the_record_exactly() {
    let bytes = luminance_record();
    let image = decode_raster(&bytes).expect("decodes");
    let mut cursor = LUMINANCE_HEADER_SIZE;
    for (index, mip) in image.layers[0].iter().enumerate() {
        if index > 0 {
            // The prefix the walk read must equal the payload it then took.
            let prefix =
                u32::from_le_bytes(bytes[cursor..cursor + 4].try_into().expect("4 bytes")) as usize;
            cursor += LUMINANCE_PREFIX_SIZE;
            assert_eq!(prefix, mip.pixels.len(), "mip {index} prefix");
        }
        assert_eq!(&bytes[cursor..cursor + mip.pixels.len()], &mip.pixels[..]);
        cursor += mip.pixels.len();
    }
    assert_eq!(cursor, bytes.len(), "nothing left over and nothing skipped");
}

// ---------------------------------------------------------------------------
// Refusals
// ---------------------------------------------------------------------------

/// A length prefix that disagrees with its mip's geometry is refused by name.
///
/// This is the arm that matters: a decoder that derived the level size and
/// ignored the prefix would sail past a corrupt record and hand back pixels from
/// the wrong offset, which looks like a successful decode.
#[test]
fn a_length_prefix_that_disagrees_is_refused() {
    let bytes = luminance_record();
    // Mip 1's prefix sits at 28 + 64.
    let prefix_offset = LUMINANCE_HEADER_SIZE + 16 * 4;
    for declared in [0u32, 4, 15, 17, 64, 0xFFFF_FFFF] {
        let mut corrupt = bytes.clone();
        corrupt[prefix_offset..prefix_offset + 4].copy_from_slice(&declared.to_le_bytes());
        assert_eq!(
            decode_raster(&corrupt).expect_err("the prefix is the record's own statement"),
            TextureError::LuminanceLengthPrefix {
                mip: 1,
                declared: declared as usize,
                expected: 16,
                offset: prefix_offset,
                width: 2,
                height: 2,
            },
            "prefix {declared}"
        );
    }
}

/// A record whose length does not close is refused, and the error names both
/// numbers.
#[test]
fn a_record_whose_size_does_not_close_is_refused() {
    let base = luminance_record();
    let expected = luminance_record_size(LUM_WIDTH, LUM_HEIGHT, LUM_MIPS);
    let mut one_too_long = base.clone();
    one_too_long.push(0);
    // One byte short, one byte long, and a stub shorter than any chain: three
    // lengths that are not records.
    for corrupt in [
        base[..base.len() - 1].to_vec(),
        one_too_long,
        base[..32].to_vec(),
    ] {
        let record_size = corrupt.len();
        assert_ne!(record_size, expected, "the case must not be a valid record");
        assert_eq!(
            decode_raster(&corrupt).expect_err("the framing must close exactly"),
            TextureError::LuminanceRecordSize {
                record_size,
                expected,
                width: LUM_WIDTH,
                height: LUM_HEIGHT,
                mip_count: LUM_MIPS,
            },
            "size {record_size}"
        );
    }
}

/// Every prefix length of the record is `Ok` or `Err`, and never a panic.
///
/// The truncation sweep is the crate's standing guard and this format gets it
/// too: the walk slices, so a wrong cursor would be an index-out-of-bounds
/// rather than a wrong pixel.
#[test]
fn every_prefix_length_of_the_record_is_ok_or_err_and_never_panics() {
    let bytes = luminance_record();
    for length in 0..=bytes.len() {
        let truncated = &bytes[..length];
        if let Ok(image) = decode_raster(truncated) {
            assert_eq!(
                image.layers[0][0].pixels.len(),
                16 * 4,
                "a decode that succeeded returned the wrong base level"
            );
        }
    }
}

/// Every record length against a valid envelope is `Ok` or `Err`, never a panic.
///
/// This is the arm that reaches code the successful path never does: an
/// adversarial `record_size` with a real envelope. `layer_count_for` is called
/// directly so the check happens before any slicing.
#[test]
fn every_record_size_against_a_valid_envelope_is_ok_or_err_and_never_panics() {
    let envelope = parse_envelope(&luminance_record()).expect("structure only");
    for record_size in 0..600usize {
        let _ = layer_count_for(record_size, &envelope);
    }
}

/// A short mip payload is refused rather than padded, because there is no codec
/// that could absorb the difference: `0x15` is uncompressed.
#[test]
fn a_short_mip_payload_is_refused_not_padded() {
    let mut short = vec![0u8; 15];
    assert_eq!(
        decode_luminance_mip(&short, 2, 2).expect_err("16 bytes are needed"),
        TextureError::ShortLuminanceMip {
            available: 15,
            needed: 16,
            width: 2,
            height: 2,
        }
    );
    short.push(0);
    let image = decode_luminance_mip(&short, 2, 2).expect("exactly enough");
    assert_eq!(image.pixels.len(), 16);
}

/// The two formats' envelopes are told apart by their length, and a DXT5 record
/// is never accepted as `0x15` or the reverse.
#[test]
fn the_two_formats_do_not_accept_each_other() {
    // DXT5 bytes with fourcc 0x15: refused by the framing, not decoded.
    let dxt5 = common::synthetic_record();
    let mut mislabelled = dxt5.clone();
    mislabelled[0x14..0x18].copy_from_slice(&LUMINANCE_FOURCC.to_le_bytes());
    assert!(matches!(
        decode_raster(&mislabelled),
        Err(TextureError::LuminanceRecordSize { .. })
    ));

    // 0x15 bytes with fourcc 'DXT5': refused by the layer split, not decoded.
    let mut mislabelled = luminance_record();
    mislabelled[0x14..0x18].copy_from_slice(&spore_texture::DXT5_FOURCC.to_le_bytes());
    assert!(matches!(
        decode_raster(&mislabelled),
        Err(TextureError::PayloadNotMultiple { .. })
    ));

    // And each still decodes under its own label.
    assert!(decode_raster(&dxt5).is_ok());
    assert!(decode_raster(&luminance_record()).is_ok());
}

/// The DXT5 path is untouched by the `0x15` dispatch: the same record still
/// derives two layers and the same texels.
#[test]
fn the_dxt5_path_still_works_after_the_dispatch() {
    let image = decode_raster(&common::synthetic_record()).expect("decodes");
    assert_eq!(image.envelope.fourcc, spore_texture::DXT5_FOURCC);
    assert_eq!(image.layers.len(), 2);
    assert_eq!(image.layers[0][0].pixels, common::expected_addressing_8x8());
}

// ---------------------------------------------------------------------------
// The claim, queried the way a consumer would
// ---------------------------------------------------------------------------

/// The claims module reports what the decoder established, with the grades the
/// derivation supports. A test on the *values* would be worthless; these are on
/// the grades and on the fact that the swizzle question stayed a non-finding.
#[test]
fn the_claims_report_the_family_as_established_but_the_swizzle_as_unknown() {
    use spore_core::evidence::EvidenceLevel;
    assert_eq!(
        spore_texture::claims::grade_of(claims::LUMINANCE_FOURCC),
        EvidenceLevel::Verified
    );
    assert_eq!(
        spore_texture::claims::grade_of(claims::LUMINANCE_FRAMING),
        EvidenceLevel::Verified
    );
    assert_eq!(
        spore_texture::claims::grade_of(claims::LUMINANCE_TEXEL_LAYOUT),
        EvidenceLevel::Verified
    );
    assert_eq!(
        spore_texture::claims::grade_of(claims::LUMINANCE_FOURTH_LANE),
        EvidenceLevel::Inferred
    );
    let swizzle = claims::claim(claims::LUMINANCE_CHANNEL_SWIZZLE).expect("recorded");
    assert!(
        swizzle.meaning.is_unavailable(),
        "what the decoder SHOULD do is not a finding this build established"
    );
    assert_eq!(
        swizzle.meaning.level(),
        EvidenceLevel::Unknown,
        "and it must not be upgraded because the decoder needed an answer"
    );
}

/// The dimensions the walk uses are the ones the framing identity assumes.
#[test]
fn mip_dimensions_follow_the_shared_floor() {
    assert_eq!(luminance_dimension(512, 0), 512);
    assert_eq!(luminance_dimension(512, 8), 2);
    assert_eq!(luminance_dimension(512, 9), 1);
    assert_eq!(luminance_dimension(512, 32), 1);
    assert_eq!(luminance_dimension(512, 1_000), 1);
}
