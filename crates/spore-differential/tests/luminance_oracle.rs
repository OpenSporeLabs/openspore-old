//! An **independent** checker for the `0x15` raster family.
//!
//! # Why this file exists at all
//!
//! The `0x15` family has **no oracle**. `tools/spore/raster/raster.py::decode`
//! refuses any `fourcc` other than `DXT5`, and the crate in question now
//! *decodes* `0x15`. So the comparison in `compare::raster` has nothing to
//! compare against, and the family that grew by 901 real records this session
//! is the one family with no second implementation.
//!
//! That is a gap a port must not be left with, so this file builds the missing
//! second implementation **here**: `ref_*` below re-derive the framing identity
//! from the format description, in a file that never calls
//! `spore_texture::luminance_record_size`, `luminance_mip_size` or
//! `luminance_dimension`. The two are then compared over a sweep of shapes, and
//! the decoder is held to three properties that matter more than agreement:
//!
//! 1. **Any successful decode is consistent with the framing.** Every mip is the
//!    exact bytes of the window the independent layout gives it, the walk
//!    consumes the record to the byte, and the record is exactly as long as the
//!    identity says. A decoder that silently accepted a truncated or padded
//!    record fails here.
//! 2. **Refusal is total.** Empty input, every truncation offset, every
//!    extension offset, every single-byte flip in the 28-byte header, every
//!    corrupted length prefix, and absurd counts all return `Err` — never a
//!    panic, never a hang, never a partial image.
//! 3. **The two formats do not accept each other's bytes.** A DXT5 record with
//!    its format word set to `0x15` and vice versa must both be refused. That is
//!    the exact failure the format's own documentation warns about ("plausible
//!    garbage rather than an error"), and nothing else in the tree checks it.
//!
//! # What is deliberately *not* asserted here
//!
//! * What the four lanes **mean**. That is `spore-material`'s measurement, not
//!   this file's, and a framing test that asserted a lane order would be
//!   re-asserting the implementation.
//! * DXT5's texel values. `spore-texture`'s own golden tests compare against
//!   hand-computed values from `tests/dxt5_golden.rs`, and
//!   `compare::raster` checks the decoded base mip against `dxt5.py` over real
//!   records. Here DXT5 bytes are only ever used as *framing*.
//!
//! Hermetic: synthetic records only, no `SPORE/`, no Python, no network.

use spore_texture::{
    decode_raster, layer_count_for, luminance_record_size, parse_envelope, MipImage, RasterImage,
    TextureError, DXT5_FOURCC, LUMINANCE_FOURCC,
};

// ---------------------------------------------------------------- the oracle

/// Bytes in the `0x15` header, from the format description.
const HEADER: usize = 28;
/// Bytes in the length prefix before every mip past the base level.
const PREFIX: usize = 4;
/// Bytes per texel, uncompressed.
const TEXEL: usize = 4;
/// Bytes per output texel.
const OUT_TEXEL: usize = 4;
/// Bytes in the DXT5 envelope.
const DXT_HEADER: usize = 32;
/// Bytes in one 4x4 block.
const BLOCK: usize = 8;

/// The mip cut: every level at or beyond this index is 1x1.
const MIP_FLOOR: u32 = 32;

/// `max(1, base >> mip)`, with the 32-mip floor. Written out rather than shared
/// with the crate, which is the whole point of this file.
fn ref_dim(base: u32, mip: u32) -> u32 {
    if mip >= MIP_FLOOR {
        1
    } else {
        (base >> mip).max(1)
    }
}

/// Payload bytes of one mip, checked rather than saturating.
///
/// `None` means the geometry is not addressable on this machine, which is a
/// different answer from "a few bytes" and must not be confused with one.
fn ref_mip_bytes(width: u32, height: u32, mip: u32) -> Option<usize> {
    (ref_dim(width, mip) as usize)
        .checked_mul(ref_dim(height, mip) as usize)?
        .checked_mul(TEXEL)
}

/// The record size the format description implies.
///
/// ```text
/// record_size == 28 + width*height*4
///                   + SUM over mips 1..mip_count of (4 + max(1,w>>m)*max(1,h>>m)*4)
/// ```
fn ref_record_size(width: u32, height: u32, mip_count: u32) -> Option<usize> {
    let mut total = HEADER;
    if mip_count == 0 {
        return Some(total);
    }
    let bounded = mip_count.min(MIP_FLOOR);
    for mip in 0..bounded {
        if mip > 0 {
            total = total.checked_add(PREFIX)?;
        }
        total = total.checked_add(ref_mip_bytes(width, height, mip)?)?;
    }
    let extra = mip_count.saturating_sub(bounded) as usize;
    total.checked_add(extra.checked_mul(PREFIX + TEXEL)?)
}

/// `(offset, length)` of every mip's payload, derived here.
fn ref_windows(width: u32, height: u32, mip_count: u32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = HEADER;
    for mip in 0..mip_count {
        if mip > 0 {
            at += PREFIX;
        }
        let len = ref_mip_bytes(width, height, mip).unwrap_or(0);
        out.push((at, len));
        at += len;
    }
    out
}

/// Little-endian `u32` at `at`, or `None` past the end. The test's own header
/// reader: the crate's `parse_envelope` is the thing under test.
fn ref_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The header as this file reads it.
fn ref_header(data: &[u8]) -> Option<(u32, u32, u32, u32)> {
    Some((
        ref_u32(data, 0x04)?, // width
        ref_u32(data, 0x08)?, // height
        ref_u32(data, 0x0C)?, // mip_count
        ref_u32(data, 0x14)?, // fourcc
    ))
}

// ------------------------------------------------------------- the builders

/// Pushes `count` bytes whose value is a function of the absolute offset, so
/// every window in the record is uniquely identifiable and a decoder that read
/// the wrong bytes cannot produce a matching image.
fn fill(out: &mut Vec<u8>, count: usize) {
    let start = out.len();
    for index in 0..count {
        out.push(((start + index) % 251) as u8);
    }
}

/// A well-formed `0x15` record.
fn build_luminance(width: u32, height: u32, mip_count: u32) -> Vec<u8> {
    let size = ref_record_size(width, height, mip_count).expect("addressable shape");
    let mut out = Vec::with_capacity(size);
    out.extend_from_slice(&1u32.to_le_bytes()); // version
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&mip_count.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes()); // field_10: unresolved
    out.extend_from_slice(&LUMINANCE_FOURCC.to_le_bytes());
    let base = ref_mip_bytes(width, height, 0).expect("addressable base") as u32;
    out.extend_from_slice(&base.to_le_bytes()); // field_18
    assert_eq!(out.len(), HEADER, "the 0x15 header is 28 bytes, not 32");
    for mip in 0..mip_count {
        if mip > 0 {
            let len = ref_mip_bytes(width, height, mip).expect("addressable mip") as u32;
            out.extend_from_slice(&len.to_le_bytes());
        }
        fill(
            &mut out,
            ref_mip_bytes(width, height, mip).expect("addressable mip"),
        );
    }
    assert_eq!(
        out.len(),
        size,
        "the builder must produce the identity's size"
    );
    out
}

/// Bytes in one DXT5 mip: `ceil(w/4) * ceil(h/4) * 8`.
fn ref_dxt5_mip(width: u32, height: u32, mip: u32) -> usize {
    let cw = ref_dim(width, mip) as usize;
    let ch = ref_dim(height, mip) as usize;
    cw.div_ceil(4) * ch.div_ceil(4) * BLOCK
}

fn ref_dxt5_chain(width: u32, height: u32, mip_count: u32) -> usize {
    let bounded = mip_count.min(MIP_FLOOR);
    let mut total = 0usize;
    for mip in 0..bounded {
        total += ref_dxt5_mip(width, height, mip);
    }
    total + mip_count.saturating_sub(bounded) as usize * BLOCK
}

/// A well-formed DXT5 record with `layers` layers of identical framing.
fn build_dxt5(width: u32, height: u32, mip_count: u32, layers: usize) -> Vec<u8> {
    let chain = ref_dxt5_chain(width, height, mip_count);
    let mut out = Vec::new();
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&width.to_le_bytes());
    out.extend_from_slice(&height.to_le_bytes());
    out.extend_from_slice(&mip_count.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&DXT5_FOURCC.to_le_bytes());
    out.extend_from_slice(&0x0004_0000u32.to_le_bytes());
    out.extend_from_slice(&0x0000_FFFFu32.to_le_bytes());
    assert_eq!(out.len(), DXT_HEADER);
    for layer in 0..layers {
        // A layer header that is *not* interpreted: four bytes of tag plus
        // three words, exactly as the corpus carries them.
        out.extend_from_slice(&0x3144_3965u32.to_le_bytes()); // "e9D1"
        out.extend_from_slice(&(layer as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(chain as u32).to_le_bytes());
    }
    for _ in 0..layers {
        fill(&mut out, chain);
    }
    out
}

// -------------------------------------------------- property 1: consistency

/// Holds a successful `0x15` decode to the independently derived framing.
///
/// This is the anti-silent-acceptance property, and it is deliberately written
/// against **this file's** offsets rather than the crate's: a decoder that read
/// a mip from the wrong place, or that returned a short image, or that left
/// bytes unconsumed, cannot satisfy it.
fn assert_decode_matches_the_independent_layout(record: &[u8], image: &RasterImage) {
    let (width, height, mip_count, fourcc) = ref_header(record).expect("a 28-byte header");
    assert_eq!(fourcc, LUMINANCE_FOURCC);

    assert_eq!(
        record.len(),
        ref_record_size(width, height, mip_count).expect("addressable"),
        "a decoded record must be exactly as long as the identity says"
    );
    assert_eq!(
        image.layers.len(),
        1,
        "the 0x15 family has no layer headers, so there is exactly one layer"
    );
    let windows = ref_windows(width, height, mip_count);
    assert_eq!(
        image.layers[0].len(),
        windows.len(),
        "one decoded mip per declared level"
    );
    assert_eq!(
        windows.last().map(|(at, len)| at + len),
        Some(record.len()),
        "the walk must end at the record end, with nothing unconsumed"
    );

    for (mip, (offset, len)) in windows.iter().enumerate() {
        let decoded: &MipImage = &image.layers[0][mip];
        assert_eq!(decoded.width, ref_dim(width, mip as u32), "mip {mip} width");
        assert_eq!(
            decoded.height,
            ref_dim(height, mip as u32),
            "mip {mip} height"
        );
        assert_eq!(decoded.pixels.len(), *len, "mip {mip} length");
        assert_eq!(
            &decoded.pixels,
            &record[*offset..*offset + *len],
            "mip {mip} must be the exact bytes at its own window; there is no codec"
        );
    }
}

// --------------------------------------------------- property 2: total refusal

/// Every one of these must be a refusal, and none may panic.
///
/// The sweep is what makes "refuses" a claim about the whole input space around
/// a valid record rather than about three hand-picked inputs.
fn assert_all_refused(cases: &[(String, Vec<u8>)]) {
    for (what, bytes) in cases {
        let outcome = decode_raster(bytes);
        assert!(
            outcome.is_err(),
            "{what}: a {}-byte record decoded as Ok, which is silent acceptance",
            bytes.len()
        );
    }
}

// ------------------------------------------------------------------ the tests

/// Every shape the family occurs in, plus shapes that break the naive
/// assumptions: non-square, a single texel, a mip count past the point where
/// `w >> mip` reaches zero, and a chain longer than the geometry needs.
fn shapes() -> Vec<(u32, u32, u32)> {
    let mut out = vec![
        // The three shapes measured in Spore_Content.package.
        (32, 32, 6),
        (256, 256, 9),
        (512, 512, 10),
    ];
    for (w, h) in [
        (1u32, 1u32),
        (1, 8),
        (8, 1),
        (2, 3),
        (3, 2),
        (4, 4),
        (5, 7),
        (7, 5),
    ] {
        for mips in [1u32, 2, 3, 4, 8, 12, 33] {
            out.push((w, h, mips));
        }
    }
    for mips in [1u32, 2, 5, 9, 10, 11, 40] {
        out.push((64, 64, mips));
        out.push((128, 64, mips));
        out.push((64, 128, mips));
    }
    out
}

#[test]
fn the_crates_record_size_equals_an_independently_written_identity() {
    for (w, h, mips) in shapes() {
        let mine = ref_record_size(w, h, mips).expect("addressable shape");
        assert_eq!(
            luminance_record_size(w, h, mips),
            mine,
            "{w}x{h} mips={mips}: the crate's derived record size must equal the identity \
             written out in this file"
        );
    }
}

#[test]
fn an_unaddressable_geometry_saturates_rather_than_wrapping_to_a_small_number() {
    // The failure this guards is a wrapped multiply returning "a few bytes",
    // which a caller cannot distinguish from a small texture. Every one of these
    // geometries overflows `usize` on a 64-bit target.
    // (u32::MAX, 1) is deliberately NOT here: 4294967295 * 4 fits in a 64-bit
    // `usize`, so it is addressable and belongs to the case above, not here.
    for (w, h, mips) in [
        (u32::MAX, u32::MAX, 1u32),
        (u32::MAX, u32::MAX, 40),
        (0x8000_0000, 0x8000_0000, 2),
        (0x8000_0000, 0x8000_0000, 1),
    ] {
        assert_eq!(
            ref_record_size(w, h, mips),
            None,
            "{w}x{h} mips={mips} must be unaddressable here, or this test proves nothing"
        );
        assert_eq!(
            luminance_record_size(w, h, mips),
            usize::MAX,
            "{w}x{h} mips={mips}: the crate must saturate, not wrap"
        );
    }
    // And a geometry that *is* addressable must not be mistaken for one that is
    // not: the boundary case is real data, not an error.
    assert!(ref_record_size(4096, 4096, 1).is_some());
}

#[test]
fn a_well_formed_record_decodes_to_exactly_the_bytes_that_were_written() {
    for (w, h, mips) in shapes() {
        let record = build_luminance(w, h, mips);
        let image = decode_raster(&record)
            .unwrap_or_else(|error| panic!("{w}x{h} mips={mips} must decode: {error}"));
        assert_eq!(image.envelope.mip_count, mips, "{w}x{h}");
        assert_eq!(image.envelope.width, w);
        assert_eq!(image.envelope.height, h);
        assert_eq!(image.envelope.fourcc, LUMINANCE_FOURCC);
        assert_eq!(image.envelope.version, 1);
        assert_eq!(image.envelope.field_10, 8);
        assert_eq!(
            image.envelope.field_18,
            ref_mip_bytes(w, h, 0).expect("addressable") as u32,
            "field_18 restates the base level's size; it is carried, not read"
        );
        assert_eq!(image.envelope.mip_count as usize, mips as usize);
        assert_eq!(layer_count_for(record.len(), &image.envelope), Ok(1));
        assert_decode_matches_the_independent_layout(&record, &image);
    }
}

#[test]
fn the_first_four_bytes_after_the_header_are_the_first_texel_not_a_field() {
    // The 28-byte header is the load-bearing structural difference from DXT5,
    // and `RasterEnvelope::field_1c` deliberately reports bytes that are *not* a
    // field of this format. If a future edit made the header 32 bytes, the
    // envelope would start reporting a padding word and this would still pass —
    // so the assertion is on what those four bytes ARE.
    let record = build_luminance(4, 4, 2);
    let image = decode_raster(&record).expect("a valid record decodes");
    let envelope_word = image.envelope.field_1c;
    assert_eq!(
        envelope_word,
        u32::from_le_bytes([record[0x1C], record[0x1D], record[0x1E], record[0x1F]]),
        "field_1c reports the bytes that are there"
    );
    let first_texel = &image.layers[0][0].pixels[..OUT_TEXEL];
    assert_eq!(
        first_texel,
        &record[HEADER..HEADER + OUT_TEXEL],
        "and those bytes are the first texel of the base level, not a header word"
    );
    // The first texel is a distinguishable pattern, so this is not a comparison
    // of two zeros.
    assert_ne!(first_texel, &[0, 0, 0, 0]);
}

#[test]
fn truncation_at_every_offset_is_refused_and_never_partially_accepted() {
    // 8x8 / 4 mips: 28 + 256 + (4+64) + (4+16) + (4+4) = 380 bytes.
    let record = build_luminance(8, 8, 4);
    assert_eq!(record.len(), 380);
    assert!(decode_raster(&record).is_ok());

    let cases: Vec<(String, Vec<u8>)> = (0..record.len())
        .map(|n| (format!("truncated to {n}"), record[..n].to_vec()))
        .collect();
    assert_all_refused(&cases);
}

#[test]
fn padding_a_record_is_refused_because_nothing_unconsumed_is_allowed() {
    // The mirror of the truncation sweep and the more dangerous half: a decoder
    // that only checked "enough bytes" would accept every one of these and hand
    // the caller an image built from bytes the record does not claim.
    let record = build_luminance(8, 8, 4);
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for extra in 1..=64usize {
        let mut padded = record.clone();
        padded.extend(std::iter::repeat_n(0u8, extra));
        cases.push((format!("padded by {extra}"), padded));
    }
    // And padding that looks like another mip's prefix, which is the shape a
    // decoder that derived its own offsets would accept.
    for extra in [4usize, 8, 68, 72] {
        let mut padded = record.clone();
        padded.extend_from_slice(&256u32.to_le_bytes());
        padded.resize(record.len() + extra, 0);
        cases.push((format!("padded by a plausible {extra}"), padded));
    }
    assert_all_refused(&cases);
}

#[test]
fn every_single_byte_flip_in_the_header_is_either_refused_or_changes_nothing_read() {
    // 8x8 / 4 mips, so every one of the seven header words carries a value that
    // a flip changes. The classification per word is the assertion:
    //
    // | word        | flips to            | because                                   |
    // |-------------|---------------------|-------------------------------------------|
    // | `fourcc`    | refused by name     | it selects the format                     |
    // | width/height/mip_count | refused  | the record then fails its own exact-size check |
    // | `version`   | decodes, no texel moves | nothing reads it                       |
    // | `field_10`  | decodes, no texel moves | nothing reads it                       |
    // | `field_18`  | decodes, no texel moves | the crate's rule is "derived, never read" |
    //
    // The `field_18` row is the one that only this test can catch: if the
    // decoder ever started reading that word instead of deriving the base
    // level's size from the geometry, the pixels would stay right on every
    // valid record and a corrupted header would start being trusted.
    let (w, h, mips) = (8u32, 8u32, 4u32);
    let record = build_luminance(w, h, mips);
    let baseline = decode_raster(&record).expect("the unflipped record decodes");
    let base_pixels = baseline.layers[0][0].pixels.clone();

    // 0x00 version, 0x04 width, 0x08 height, 0x0c mip_count, 0x10 field_10,
    // 0x14 fourcc, 0x18 field_18. Seven words in 28 bytes.
    const NEVER_READ: [usize; 3] = [0, 4, 6];
    const GEOMETRY: [usize; 3] = [1, 2, 3];
    const FORMAT_WORD: usize = 5;

    let mut flips_per_word = [0usize; 7];
    for byte in 0..HEADER {
        for pattern in [0x01u8, 0x80, 0xFF] {
            let mut flipped = record.clone();
            flipped[byte] ^= pattern;
            let word = byte / 4;
            flips_per_word[word] += 1;
            match decode_raster(&flipped) {
                Err(error) => {
                    if word == FORMAT_WORD {
                        assert!(
                            matches!(error, TextureError::UnsupportedFourcc { .. }),
                            "a flipped format word must be refused by name, got {error}"
                        );
                    } else {
                        assert!(
                            GEOMETRY.contains(&word),
                            "header word {word} (offset 0x{:02x}) must never be the reason a \
                             record is refused, got {error}",
                            word * 4
                        );
                        assert!(
                            matches!(error, TextureError::LuminanceRecordSize { .. }),
                            "a flipped geometry word must fail the exact-size check, got {error}"
                        );
                    }
                }
                Ok(image) => {
                    assert!(
                        NEVER_READ.contains(&word),
                        "header word {word} (offset 0x{:02x}) decoded, but only the three \
                         never-read words may: it selects the framing",
                        word * 4
                    );
                    assert_decode_matches_the_independent_layout(&flipped, &image);
                    assert_eq!(
                        image.layers[0][0].pixels,
                        base_pixels,
                        "offset 0x{:02x} is never read, so a flip there must not move a texel",
                        word * 4
                    );
                }
            }
        }
    }
    for (word, count) in flips_per_word.iter().enumerate() {
        assert!(*count > 0, "header word {word} was never flipped");
    }

    // The four bytes just past the header are the first texel, so a flip there
    // MUST move pixels. Without this the "28-byte header" claim has no test.
    for byte in HEADER..HEADER + 4 {
        let mut flipped = record.clone();
        flipped[byte] ^= 0xFF;
        let image = decode_raster(&flipped).expect("a texel flip keeps the framing valid");
        assert_decode_matches_the_independent_layout(&flipped, &image);
        assert_ne!(
            image.layers[0][0].pixels, base_pixels,
            "offset 0x{byte:02x} is inside the base level's payload, so a flip must move pixels"
        );
    }
}

#[test]
fn every_length_prefix_must_state_its_own_mips_size() {
    // The prefix is the record's only statement about its own framing past the
    // base level, so a decoder that ignored it would be discarding the only
    // check available. Each prefix is corrupted on its own, and each must be
    // refused *by name* at its own mip.
    let (w, h, mips) = (8u32, 8u32, 5u32);
    let record = build_luminance(w, h, mips);
    let windows = ref_windows(w, h, mips);
    assert_eq!(windows.len(), 5);

    for (mip, &(payload_at, _)) in windows.iter().enumerate().take(mips as usize).skip(1) {
        let prefix_at = payload_at - PREFIX;
        let correct = ref_mip_bytes(w, h, mip as u32).expect("addressable") as u32;
        for declared in [0u32, 1, correct.wrapping_sub(1), correct + 4, u32::MAX] {
            let mut broken = record.clone();
            broken[prefix_at..prefix_at + PREFIX].copy_from_slice(&declared.to_le_bytes());
            match decode_raster(&broken) {
                Ok(image) => panic!(
                    "mip {mip} prefix {declared} (correct {correct}) decoded as Ok; \
                     first mip {} bytes",
                    image.layers[0][0].pixels.len()
                ),
                Err(TextureError::LuminanceLengthPrefix { mip: reported, .. }) => assert_eq!(
                    reported, mip as u32,
                    "the refusal must name the mip whose prefix disagrees"
                ),
                Err(other) => panic!(
                    "mip {mip} prefix {declared}: expected LuminanceLengthPrefix, got {other}"
                ),
            }
        }
    }
}

#[test]
fn a_prefix_that_agrees_cannot_rescue_a_record_that_is_too_short() {
    // The two checks are independent and both must hold. A decoder that trusted
    // the prefix over the geometry would decode this; one that trusted the
    // geometry over the prefix would decode the previous test's case.
    let (w, h, mips) = (8u32, 8u32, 4u32);
    let record = build_luminance(w, h, mips);
    let windows = ref_windows(w, h, mips);
    let last = windows[mips as usize - 1];
    let mut short = record[..last.0 + last.1 - 1].to_vec();
    // Make the last prefix agree with what is left, so only the size check can
    // catch it.
    let prefix_at = last.0 - PREFIX;
    short[prefix_at..prefix_at + PREFIX].copy_from_slice(&3u32.to_le_bytes());
    assert!(matches!(
        decode_raster(&short),
        Err(TextureError::LuminanceRecordSize { .. })
    ));
}

#[test]
fn absurd_counts_are_refused_before_anything_is_allocated_or_walked() {
    // `mip_count = 0xFFFFFFFF` is the dangerous one: the walk is
    // `for mip in 0..mip_count`, so a decoder that reached the walk before
    // checking the framing would spin for four billion iterations. The exact-size
    // check is what makes this cheap, and this test is the only thing that says
    // so. If it ever hangs here, that check moved.
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for (w, h, mips) in [
        (u32::MAX, u32::MAX, 1u32),
        (u32::MAX, u32::MAX, u32::MAX),
        (1, 1, u32::MAX),
        (8, 8, u32::MAX),
        (0x8000_0000, 8, u32::MAX),
    ] {
        let mut record = build_luminance(8, 8, 1);
        record[0x04..0x08].copy_from_slice(&w.to_le_bytes());
        record[0x08..0x0C].copy_from_slice(&h.to_le_bytes());
        record[0x0C..0x10].copy_from_slice(&mips.to_le_bytes());
        cases.push((format!("{w}x{h} mips={mips}"), record));
    }
    // Zero dimensions and a zero mip count, each refused by name.
    for (what, patch) in [
        ("width 0", (0x04usize, 0u32)),
        ("height 0", (0x08, 0)),
        ("mips 0", (0x0C, 0)),
    ] {
        let mut record = build_luminance(8, 8, 4);
        record[patch.0..patch.0 + 4].copy_from_slice(&patch.1.to_le_bytes());
        cases.push((what.to_owned(), record));
    }
    assert_all_refused(&cases);

    // And the refusals that must be *named*, so a caller can tell a malformed
    // header from an absent decoder.
    let mut zero_width = build_luminance(8, 8, 4);
    zero_width[0x04..0x08].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(
        decode_raster(&zero_width),
        Err(TextureError::ZeroDimension {
            width: 0,
            height: 8
        })
    ));
    let mut zero_mips = build_luminance(8, 8, 4);
    zero_mips[0x0C..0x10].copy_from_slice(&0u32.to_le_bytes());
    assert!(matches!(
        decode_raster(&zero_mips),
        Err(TextureError::ZeroMipCount)
    ));
}

#[test]
fn a_mutation_sweep_never_produces_an_inconsistent_success() {
    // The generalisation of the three sweeps above: 3 000 deterministic
    // mutations of a valid record, and the single rule that every one of them
    // must obey. Mutations that leave the record decodable must decode to the
    // independently derived windows; mutations that do not must be refused.
    // There is no third possibility, and no assertion about *which* mutations
    // land where, because that is not a property worth pinning.
    let record = build_luminance(16, 16, 5);
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        // xorshift64*, so the sweep is identical on every machine and run.
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };

    let mut successes = 0usize;
    let mut refusals = 0usize;
    for _ in 0..3_000 {
        let mut mutated = record.clone();
        match next() % 4 {
            0 => {
                // Truncate.
                let cut = (next() as usize) % mutated.len();
                mutated.truncate(cut);
            }
            1 => {
                // Extend.
                let extra = (next() as usize) % 17;
                mutated.extend(std::iter::repeat_n(0u8, extra));
            }
            2 => {
                // Flip a byte anywhere, header included.
                let at = (next() as usize) % mutated.len();
                mutated[at] ^= (next() % 255 + 1) as u8;
            }
            _ => {
                // Flip between one and four bytes, which is what a
                // decompressor that lost sync actually produces.
                let count = 1 + (next() % 4) as usize;
                for _ in 0..count {
                    let at = (next() as usize) % mutated.len();
                    mutated[at] ^= (next() % 255 + 1) as u8;
                }
            }
        }
        match decode_raster(&mutated) {
            Ok(image) => {
                successes += 1;
                let (_, _, _, fourcc) = ref_header(&mutated)
                    .expect("a record that decoded must carry a readable 28-byte header");
                assert_eq!(fourcc, LUMINANCE_FOURCC, "a non-0x15 record decoded");
                assert_decode_matches_the_independent_layout(&mutated, &image);
            }
            Err(_) => refusals += 1,
        }
    }
    // The sweep is only meaningful if it exercised both arms.
    assert!(
        successes > 0,
        "no mutation decoded; the sweep proved nothing"
    );
    assert!(
        refusals > 0,
        "no mutation was refused; the sweep proved nothing"
    );
    println!("mutation sweep: {successes} decoded, {refusals} refused, 0 inconsistent");
}

#[test]
fn the_two_formats_do_not_accept_each_others_bytes() {
    // The failure the format documentation names explicitly: DXT5 and `0x15`
    // frame themselves differently (32-byte envelope, 16-byte layer headers,
    // compressed blocks versus a 28-byte header, no layer headers, four bytes
    // per texel), so feeding either to the other's codec would produce
    // *plausible garbage* rather than an error. Only a test that hands one
    // format's bytes to the other's decoder can tell the difference.
    for (w, h, mips) in [(8u32, 8u32, 4u32), (64, 64, 7), (256, 256, 9)] {
        let dxt5 = build_dxt5(w, h, mips, 2);
        assert!(
            decode_raster(&dxt5).is_ok(),
            "the DXT5 builder must produce a record its own decoder accepts"
        );

        let mut as_luminance = dxt5.clone();
        as_luminance[0x14..0x18].copy_from_slice(&LUMINANCE_FOURCC.to_le_bytes());
        match decode_raster(&as_luminance) {
            Ok(_) => panic!("{w}x{h}: a DXT5 record with the 0x15 format word decoded"),
            Err(error) => assert!(
                matches!(error, TextureError::LuminanceRecordSize { .. }),
                "{w}x{h}: expected the 0x15 exact-size refusal, got {error}"
            ),
        }

        let luminance = build_luminance(w, h, mips);
        let mut as_dxt5 = luminance.clone();
        as_dxt5[0x14..0x18].copy_from_slice(&DXT5_FOURCC.to_le_bytes());
        assert!(
            decode_raster(&as_dxt5).is_err(),
            "{w}x{h}: a 0x15 record with the DXT5 format word decoded as DXT5"
        );
    }
}

#[test]
fn dxt1_and_every_other_format_word_are_refused_by_name() {
    // `0x31545844` is present in the corpus (306 records) and is deliberately
    // not decoded. A decoder that accepted "any fourcc it did not recognise"
    // would silently mis-frame them.
    for fourcc in [0x3154_5844u32, 0, 1, 0xDEAD_BEEF, 0x1500, 0x15FF] {
        let mut record = build_luminance(8, 8, 4);
        record[0x14..0x18].copy_from_slice(&fourcc.to_le_bytes());
        match decode_raster(&record) {
            Ok(_) => panic!("fourcc 0x{fourcc:08x} decoded"),
            Err(error) => assert!(
                matches!(error, TextureError::UnsupportedFourcc { .. }),
                "fourcc 0x{fourcc:08x}: expected UnsupportedFourcc, got {error}"
            ),
        }
        // And the envelope still parses, because reading a header is not
        // decoding: the refusal must be about the format, not about the bytes.
        let envelope = parse_envelope(&record).expect("28 bytes is a readable header");
        assert_eq!(envelope.fourcc, fourcc);
    }
}

#[test]
fn an_empty_or_tiny_input_is_refused_rather_than_read_past() {
    for length in 0..HEADER {
        assert!(
            decode_raster(&vec![0u8; length]).is_err(),
            "{length} bytes must be refused"
        );
        assert!(matches!(
            parse_envelope(&vec![0u8; length]),
            Err(TextureError::TruncatedEnvelope { .. })
        ));
    }
    // A header with a valid-looking geometry but no payload at all. Note which
    // refusal this is: `parse_envelope` reads eight words and therefore needs 32
    // bytes, so a 28-byte record is refused as a **truncated envelope** even
    // though this format's header is 28. The consequence is that the smallest
    // decodable `0x15` record is 32 bytes -- a 1x1 base level with one mip --
    // and that is a fact about the reader, not about the format.
    let mut header_only = build_luminance(8, 8, 4);
    header_only.truncate(HEADER);
    assert!(matches!(
        decode_raster(&header_only),
        Err(TextureError::TruncatedEnvelope { available: 28 })
    ));
    assert_eq!(
        build_luminance(1, 1, 1).len(),
        32,
        "a 1x1 record with one mip is exactly the 32 bytes the envelope reader needs"
    );
    assert!(decode_raster(&build_luminance(1, 1, 1)).is_ok());
    // And 31 bytes is one short of it, refused the same way.
    let mut one_short = build_luminance(1, 1, 1);
    one_short.truncate(31);
    assert!(matches!(
        decode_raster(&one_short),
        Err(TextureError::TruncatedEnvelope { available: 31 })
    ));
}

#[test]
fn one_layer_is_derived_and_never_two() {
    // A DXT5 record of the same geometry derives two layers. The two formats
    // must not share a layer-count rule, because the count is what positions
    // every payload byte.
    for (w, h, mips) in [(8u32, 8u32, 4u32), (64, 64, 7)] {
        let luminance = build_luminance(w, h, mips);
        let dxt5 = build_dxt5(w, h, mips, 2);
        assert_eq!(
            layer_count_for(luminance.len(), &parse_envelope(&luminance).unwrap()),
            Ok(1)
        );
        assert_eq!(
            layer_count_for(dxt5.len(), &parse_envelope(&dxt5).unwrap()),
            Ok(2)
        );
        assert_eq!(
            decode_raster(&luminance).unwrap().layers.len(),
            1,
            "{w}x{h}: the 0x15 family has no layer headers, so there is one layer"
        );
        assert_eq!(decode_raster(&dxt5).unwrap().layers.len(), 2);
    }
    // A one-layer DXT5 record is legal and must stay legal: the count is
    // derived, and refusing small counts would be inventing a rule.
    let single = build_dxt5(8, 8, 4, 1);
    assert_eq!(decode_raster(&single).unwrap().layers.len(), 1);
}

#[test]
fn a_layer_count_that_does_not_divide_is_refused_rather_than_rounded() {
    // DXT5 only: `0x15` has no stride to divide. Rounding here would invent a
    // layer the record does not have, which is the documented refusal.
    let record = build_dxt5(8, 8, 4, 2);
    let envelope = parse_envelope(&record).unwrap();
    for extra in 1..=8usize {
        let mut longer = record.clone();
        longer.extend(std::iter::repeat_n(0u8, extra));
        match layer_count_for(longer.len(), &envelope) {
            Err(error) => assert!(
                matches!(
                    error,
                    TextureError::PayloadNotMultiple { .. } | TextureError::PayloadTooSmall { .. }
                ),
                "{extra} extra byte(s) gave {error}"
            ),
            Ok(count) => panic!("{extra} extra byte(s) derived {count} layers"),
        }
    }
    // And a DXT5 record with a *bigger* payload than two layers must derive
    // three, because the count is arithmetic and not a constant.
    let three = build_dxt5(8, 8, 4, 3);
    assert_eq!(decode_raster(&three).unwrap().layers.len(), 3);
}

#[test]
fn the_builder_matches_the_reference_oracle_it_is_checked_against() {
    // A builder that did not agree with the identity would make every test in
    // this file vacuous, so the agreement is asserted directly rather than left
    // implicit: the three corpus shapes, with the sizes the repository records.
    for (w, h, mips, size) in [
        (32u32, 32u32, 6u32, 5_508usize),
        (256, 256, 9, 349_584),
        (512, 512, 10, 1_398_164),
    ] {
        assert_eq!(
            ref_record_size(w, h, mips),
            Some(size),
            "{w}x{h} mips={mips}"
        );
        assert_eq!(build_luminance(w, h, mips).len(), size);
    }
    // The DXT5 oracle's own arithmetic, re-derived: ceil(w/4)*ceil(h/4)*8.
    assert_eq!(ref_dxt5_mip(512, 512, 0), 128 * 128 * 8);
    assert_eq!(ref_dxt5_mip(8, 8, 0), 2 * 2 * 8);
    assert_eq!(
        ref_dxt5_mip(1, 1, 0),
        8,
        "a 1x1 level is still one 4x4 block"
    );
    assert_eq!(ref_dxt5_mip(5, 3, 0), 2 * BLOCK);
}
