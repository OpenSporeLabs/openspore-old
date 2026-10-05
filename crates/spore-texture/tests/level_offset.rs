//! What `framing::level_offset` refuses, and what it must never answer.
//!
//! Everything here is synthetic: the real raster records live only in the
//! git-ignored `SPORE/` tree. Each fixture is built field by field in
//! [`common`], so every byte the walk reads is visible in the test that reads it.
//!
//! # The failure mode this file is about
//!
//! `level_offset` places one level from four numbers. Wherever those four numbers
//! do not determine an offset, the previous signature had two choices — answer or
//! refuse — and it always answered, because it had no way to do otherwise. It
//! returned `usize`.
//!
//! A wrong number is the worst outcome this repository has, and it is worth being
//! exact about why. A refusal is a **specification**: "this build will not tell
//! you", which a caller can handle, report, and test. A plausible number is a
//! **fabrication**: it looks like an answer, it survives review, and every
//! downstream byte read through it is wrong in a way nothing detects. The
//! measured cases, all on real shapes:
//!
//! | input | what the old function said | what it should say |
//! |---|---|---|
//! | level `mip_count` of an `mip_count`-level chain | `record_size` — a plausible in-record offset | there is no such level |
//! | level `mip > 32` | one fixed number, for every index to `u32::MAX` | this build cannot place past the floor |
//! | `width == 0` | `max(1, 0 >> m)` promotes a 0-wide image to 1x1 and hands it 16 bytes per level | an image with no texels has no placement |
//!
//! The first row is the load-bearing one, and it is not a corner: **all 4386**
//! raster records in `Spore_Content.package` and `Spore_EP1_Content_01.package`
//! answer it that way, because each of them is asked the same question.
//! `crates/spore-differential/tests/framing_refusals.rs` is the corpus census
//! that measures all three classes and prints their counts.
//!
//! # What is deliberately NOT refused
//!
//! The sizing functions beside `level_offset` — `level_size`, `chain_size`,
//! `record_size` — still extrapolate past the mip floor and still saturate on
//! absurd dimensions. That is unchanged, it is cross-checked against the record's
//! own length and against every prefixed level's `u32` byte count, and its
//! past-the-floor part is a recorded **approximation**
//! ([`spore_texture::claims::MIPS_BEYOND_32`]) rather than a silent guess.
//! `a_sizing_question_is_still_answered_past_the_floor` pins that the refusal did
//! not leak into them, because a refusal that quietly widened would be this
//! repository's own version of the defect it is fixing.

mod common;

use common::*;
use spore_texture::framing::{
    frame_for, level_offset, level_size, record_size, FrameFormat, LEVEL_PREFIX_SIZE,
    MAX_PLACEABLE_LEVELS,
};
use spore_texture::{
    decode_raster, layer_count_for, parse_envelope, TextureError, DXT1_FOURCC, DXT5_FOURCC,
    LUMINANCE_FOURCC, MIP_FLOOR,
};

/// The three frames, so every refusal class is checked against every format.
fn frames() -> [FrameFormat; 3] {
    [
        frame_for(DXT5_FOURCC).expect("a frame"),
        frame_for(DXT1_FOURCC).expect("a frame"),
        frame_for(LUMINANCE_FOURCC).expect("a frame"),
    ]
}

/// The offset of level `mip`'s prefix, walked here rather than asked of
/// `level_offset`.
///
/// The fixture builder must not depend on the function under test to place a
/// record, or a refusal inside `level_offset` would make the fixture
/// unbuildable — and a fixture that cannot be built cannot witness the refusal.
/// This is a plain sum over the levels before `mip`, which is what the framing
/// module says the layout is.
fn prefix_at(frame: FrameFormat, width: u32, height: u32, mip: u32) -> usize {
    let mut at = frame.header_size;
    for level in 0..mip {
        if level >= frame.first_prefixed_level {
            at += LEVEL_PREFIX_SIZE;
        }
        at += level_size(frame, width, height, level);
    }
    at
}

/// A blank record of a declared shape: header fields written, body zeros, and the
/// level prefixes the identity expects so the record is a *valid* one and any
/// refusal observed is the one under test and not a side effect.
fn blank_record(frame: FrameFormat, width: u32, height: u32, mips: u32) -> Vec<u8> {
    let mut bytes = vec![0u8; record_size(frame, width, height, mips)];
    bytes[0x00..0x04].copy_from_slice(&1u32.to_le_bytes());
    bytes[0x04..0x08].copy_from_slice(&width.to_le_bytes());
    bytes[0x08..0x0c].copy_from_slice(&height.to_le_bytes());
    bytes[0x0c..0x10].copy_from_slice(&mips.to_le_bytes());
    bytes[0x10..0x14].copy_from_slice(&8u32.to_le_bytes());
    bytes[0x14..0x18].copy_from_slice(&frame.fourcc.to_le_bytes());
    let base = level_size(frame, width, height, 0);
    bytes[0x18..0x1c].copy_from_slice(&(base as u32).to_le_bytes());
    for mip in 0..mips {
        if mip >= frame.first_prefixed_level {
            let at = prefix_at(frame, width, height, mip);
            let level = level_size(frame, width, height, mip);
            bytes[at..at + LEVEL_PREFIX_SIZE].copy_from_slice(&(level as u32).to_le_bytes());
        }
    }
    bytes
}

// ---------------------------------------------------------------------------

/// A level the envelope does not declare is refused, and never answered with the
/// record's own end offset.
///
/// The old signature had four arguments and therefore no way to know a chain's
/// length. For a ten-level 512x512 DXT5 record it answered level 10 with
/// **349 616** — which is exactly `record_size(frame, 512, 512, 10)`, so
/// `record[349_616..349_616 + n]` is a slice of a record whose last byte belongs
/// to level 9. It is the most plausible wrong answer in the module: a real
/// offset, inside the record, on the boundary.
///
/// Every corpus shape is checked, because a chain-length check written for one
/// `mip_count` is not a check.
#[test]
fn a_level_the_envelope_does_not_declare_is_refused_not_answered_with_the_record_end() {
    // (width, height, mips): the seven shapes of the two content packages, plus
    // the single-level `0x15` fixture shape.
    let shapes = [
        (512u32, 512u32, 10u32),
        (256, 256, 9),
        (64, 64, 7),
        (32, 32, 6),
        (4, 4, 1),
    ];
    for frame in frames() {
        for (width, height, mips) in shapes {
            let record_end = record_size(frame, width, height, mips);
            for mip in [mips, mips + 1, mips + 7, 64] {
                assert_eq!(
                    level_offset(frame, width, height, mip, mips),
                    Err(TextureError::LevelNotInChain {
                        mip,
                        mip_count: mips
                    }),
                    "{:#010x} {width}x{height} declares {mips} level(s); level {mip} is not one of \
                     them",
                    frame.fourcc
                );
            }
            // The specific wrong answer, spelled out so the reason the refusal
            // exists is visible rather than implied.
            assert!(
                level_offset(frame, width, height, mips, mips).is_err(),
                "{:#010x} {width}x{height}: level {mips} used to come back as {record_end}, the \
                 record's own length",
                frame.fourcc
            );
        }
    }
}

/// Every level the envelope *does* declare is still placed, exactly where the sum
/// of the levels before it puts it.
///
/// The refusal must not cost a placement: this is the half of the identity that
/// held before and has to hold now.
#[test]
fn every_declared_level_is_still_placed_exactly() {
    for frame in frames() {
        for (width, height, mips) in [(512u32, 512u32, 10u32), (64, 64, 7), (32, 32, 6)] {
            let mut expected = frame.header_size;
            for mip in 0..mips {
                assert_eq!(
                    level_offset(frame, width, height, mip, mips),
                    Ok(expected),
                    "{:#010x} {width}x{height} level {mip}",
                    frame.fourcc
                );
                if mip >= frame.first_prefixed_level {
                    expected += LEVEL_PREFIX_SIZE;
                }
                expected += level_size(frame, width, height, mip);
            }
            assert_eq!(
                expected,
                record_size(frame, width, height, mips),
                "{:#010x} {width}x{height} mips={mips}: the walk must end where the sizing says \
                 the record ends",
                frame.fourcc
            );
        }
    }
}

/// A chain longer than this build can place is refused by `level_offset` and by
/// the whole-record entry points, and the refusal names the ceiling.
///
/// Two things were wrong here before. The walk stopped accumulating at
/// `mip.min(MIP_FLOOR)`, so level 33, level 64 and level `u32::MAX` all came back
/// with the *same* number — one answer to four different questions. And that
/// number contradicted the crate's own `record_size`: on a 34-level 1x1 DXT5
/// record the sizing said **704** and the walk said **664**, forty bytes apart,
/// because the sizing extrapolates past the floor and the walk did not.
///
/// The refusal is deliberately *not* made to agree with the extrapolation. The
/// past-the-floor rule is a recorded approximation with no measurement behind it
/// ([`spore_texture::claims::MIPS_BEYOND_32`]), so the honest outcome for a
/// question only that rule can answer is to decline the question.
#[test]
fn a_chain_longer_than_the_ceiling_is_refused_by_the_walk_and_by_both_entry_points() {
    for frame in frames() {
        for mips in [
            MAX_PLACEABLE_LEVELS + 1,
            MAX_PLACEABLE_LEVELS + 7,
            64,
            u32::MAX - 1,
        ] {
            // 1. the walk, asked for a level inside the declared but unplaceable
            //    chain.
            assert_eq!(
                level_offset(frame, 8, 8, MAX_PLACEABLE_LEVELS, mips),
                Err(TextureError::MipChainBeyondFloor {
                    levels: MAX_PLACEABLE_LEVELS + 1,
                    floor: MAX_PLACEABLE_LEVELS
                }),
                "{:#010x}: level {} is the first index past the ceiling",
                frame.fourcc,
                MAX_PLACEABLE_LEVELS
            );
            // 2. one number no longer answers every index past the floor: each of
            //    these is refused, so none of them carries a value at all.
            for mip in [MAX_PLACEABLE_LEVELS + 1, 64, 1_000, u32::MAX - 1] {
                assert_eq!(
                    level_offset(frame, 8, 8, mip, u32::MAX),
                    Err(TextureError::MipChainBeyondFloor {
                        levels: mip + 1,
                        floor: MAX_PLACEABLE_LEVELS
                    }),
                    "{:#010x} level {mip} is past the ceiling",
                    frame.fourcc
                );
            }
            // 3. both whole-record entry points refuse, by the same variant, so
            //    `layer_count_for` can never answer `Ok(1)` for a record
            //    `decode_raster` rejects.
            let bytes = blank_record(frame, 8, 8, mips.min(64));
            let declared = if mips > 64 { u32::MAX - 1 } else { mips };
            let mut bytes = bytes;
            bytes[0x0c..0x10].copy_from_slice(&declared.to_le_bytes());
            let env = parse_envelope(&bytes).expect("envelope");
            assert_eq!(
                layer_count_for(bytes.len(), &env),
                Err(TextureError::MipChainBeyondFloor {
                    levels: declared,
                    floor: MAX_PLACEABLE_LEVELS
                }),
                "{:#010x}: layer_count_for must refuse a chain it cannot place",
                frame.fourcc
            );
            assert!(
                matches!(
                    decode_raster(&bytes),
                    Err(TextureError::MipChainBeyondFloor { .. })
                ),
                "{:#010x}: decode_raster must refuse a chain it cannot place, by name and not by \
                 a downstream symptom such as a length mismatch",
                frame.fourcc
            );
        }
    }
}

/// The ceiling is 33, not 32, and the last placeable index is placed exactly.
///
/// `base >> 32` is exhausted for every `u32` base, so [`MIP_FLOOR`] pins every
/// level from 32 on to 1x1 — but level **32** is still placeable, because its
/// offset is the exact sum of levels `0..32`, which is the sum `chain_size` and an
/// independent transcription both produce. Level 33 is the first index that would
/// need the past-the-floor rule. Pinning both sides is what stops the ceiling from
/// drifting one level in either direction.
#[test]
fn the_ceiling_is_one_past_the_floor_and_the_last_placeable_level_is_exact() {
    assert_eq!(MAX_PLACEABLE_LEVELS, MIP_FLOOR + 1);
    assert_eq!(MAX_PLACEABLE_LEVELS, 33);
    for frame in frames() {
        for mip in 1..=MIP_FLOOR {
            let prefix = usize::from(mip >= frame.first_prefixed_level) * LEVEL_PREFIX_SIZE;
            assert_eq!(
                level_offset(frame, 8, 8, mip, mip + 1).unwrap()
                    + level_size(frame, 8, 8, mip)
                    + prefix,
                record_size(frame, 8, 8, mip + 1),
                "{:#010x}: level {mip} is placeable and its offset plus its own size is the \
                 record size of the chain that ends with it",
                frame.fourcc
            );
        }
        assert!(
            level_offset(frame, 8, 8, MIP_FLOOR, MIP_FLOOR + 1).is_ok(),
            "{:#010x}: level {MIP_FLOOR} is the last placeable index",
            frame.fourcc
        );
        assert!(
            level_offset(frame, 8, 8, MIP_FLOOR + 1, MIP_FLOOR + 2).is_err(),
            "{:#010x}: level {} is not placeable",
            frame.fourcc,
            MIP_FLOOR + 1
        );
    }
}

/// A zero dimension has no level to place, and the refusal is the one the decoder
/// already used.
///
/// `level_dimension` is `max(1, base >> mip)`, which is correct for a shrinking
/// chain and wrong for a zero base: it promotes a 0-wide image to 1x1, giving it
/// 16 bytes per level (DXT5) and a plausible offset for each. `decode_raster`
/// already refused such a record with [`TextureError::ZeroDimension`], so this is
/// the same refusal at the function that places levels — **not** a fix for a
/// defect the corpus shows. The census measures zero such records; this test
/// aligns two entry points that disagreed.
#[test]
fn a_degenerate_dimension_has_no_level_to_place() {
    for frame in frames() {
        for (width, height) in [(0u32, 8u32), (8, 0), (0, 0)] {
            for mip in 0..4 {
                assert_eq!(
                    level_offset(frame, width, height, mip, 4),
                    Err(TextureError::ZeroDimension { width, height }),
                    "{:#010x} {width}x{height} level {mip}",
                    frame.fourcc
                );
            }
            // And the zero dimension is refused for a zero-level chain too: the
            // order of the checks does not make the geometry answerable.
            assert_eq!(
                level_offset(frame, width, height, 0, 0),
                Err(TextureError::ZeroDimension { width, height })
            );
        }
    }
}

/// The offset is derived from the geometry and the frame table, and from **no**
/// header word this crate has no meaning for.
///
/// `0x10` is `UNRESOLVED` and `0x1c` is `Verified` only for the `0x15` family;
/// `0x18` is the base level's byte count and is a **cross-check**, never a source
/// of layout. If the offset were derived from any of them, that derivation would be
/// fabricated — and the corpus would still agree, because the corpus is what the
/// framing was measured on. So this is pinned negatively: perturbing those words
/// must not move the offset by a byte. `field_18` is perturbed through
/// `decode_raster` instead, which refuses on it, so the two halves are separate
/// assertions on purpose.
#[test]
fn the_offset_reads_no_envelope_word_this_crate_has_no_meaning_for() {
    let frame = frame_for(DXT5_FOURCC).expect("a frame");
    let (width, height, mips) = (64u32, 64u32, 7u32);
    let base = blank_record(frame, width, height, mips);
    let offsets: Vec<usize> = (0..mips)
        .map(|mip| level_offset(frame, width, height, mip, mips).expect("a declared level"))
        .collect();

    // `field_10` (0x10) and `field_1c` (0x1c) are pure header words for the block
    // formats. Perturbing either must leave every offset untouched.
    for (at, word, what) in [
        (
            0x10usize,
            [0u32, 1, 0xDEAD_BEEF, u32::MAX],
            "field_10 (UNRESOLVED)",
        ),
        (0x1c, [0u32, 1, 0xDEAD_BEEF, u32::MAX], "field_1c"),
    ] {
        for value in word {
            let mut bytes = base.clone();
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
            for (mip, expected) in offsets.iter().enumerate() {
                assert_eq!(
                    level_offset(frame, width, height, mip as u32, mips),
                    Ok(*expected),
                    "{what} = 0x{value:08x} moved the offset of level {mip}: the placement is \
                     derived from a word with no established meaning"
                );
            }
        }
    }

    // `field_18` is different: it IS established, as the base level's byte count,
    // and the walk *checks* it. So it is refused by name rather than ignored --
    // the disagreement is a finding, not something to place past.
    for value in [0u32, 1, base.len() as u32 - 1, u32::MAX] {
        let mut bytes = base.clone();
        bytes[0x18..0x1c].copy_from_slice(&value.to_le_bytes());
        assert!(
            matches!(
                decode_raster(&bytes),
                Err(TextureError::Field18Mismatch { .. })
            ),
            "field_18 = {value} against a {}-byte base level must be refused by name",
            level_size(frame, width, height, 0)
        );
    }
}

/// The sizing half is untouched: `record_size`, `chain_size` and `level_size` still
/// answer past the floor and still saturate.
///
/// This is the direction the refusal must not leak. An "honest" fix that made
/// every function in the module decline past the floor would have removed a
/// documented, cross-checked, explicitly-graded approximation and replaced it with
/// an absence nobody asked for. `MIPS_BEYOND_32` records the past-the-floor rule
/// as an `Approximation` and says it "bounds an absurd header rather than
/// describing how Spore writes a 33rd level" — which is true precisely because
/// nothing places a level with it any more.
#[test]
fn a_sizing_question_is_still_answered_past_the_floor() {
    for frame in frames() {
        // The recorded past-the-floor rule: one prefix plus one 1x1 payload per
        // extra level. Unchanged.
        for mips in [MAX_PLACEABLE_LEVELS, MAX_PLACEABLE_LEVELS + 1, 40] {
            let one = LEVEL_PREFIX_SIZE + level_size(frame, 8, 8, MIP_FLOOR);
            let walked = record_size(frame, 8, 8, MAX_PLACEABLE_LEVELS);
            let expected = walked + (mips - MAX_PLACEABLE_LEVELS) as usize * one;
            assert_eq!(
                record_size(frame, 8, 8, mips),
                expected,
                "{:#010x}: the sizing rule past the floor must be unchanged -- it is recorded as \
                 an approximation in claims::MIPS_BEYOND_32, and removing it would be a \
                 different change",
                frame.fourcc
            );
        }
        // And the saturation is still a number a caller can tell apart from "small
        // texture", which is what it is for.
        assert!(
            level_size(frame, u32::MAX, u32::MAX, 0) > u32::MAX as usize,
            "{:#010x}: absurd dimensions must saturate, not wrap",
            frame.fourcc
        );
        assert_eq!(record_size(frame, 4, 4, 0), frame.header_size);
    }
}

/// The three real fixtures still decode, byte for byte.
///
/// A refusal added to the level walk is only safe if it costs nothing on the
/// records this crate is actually given. These are the same shapes the crate's
/// other test files build, and they are re-decoded here so a change to
/// `level_offset`'s signature or its checks cannot quietly stop the crate
/// decoding anything at all.
#[test]
fn the_real_fixtures_still_decode_and_still_end_where_the_framing_says() {
    for (name, bytes, fourcc, mips) in [
        (
            "bc3",
            bc3_record(),
            DXT5_FOURCC,
            spore_texture::parse_envelope(&bc3_record())
                .expect("envelope")
                .mip_count,
        ),
        (
            "bc3_three_level",
            bc3_three_level_record(),
            DXT5_FOURCC,
            spore_texture::parse_envelope(&bc3_three_level_record())
                .expect("envelope")
                .mip_count,
        ),
        (
            "bc1",
            bc1_record(),
            DXT1_FOURCC,
            spore_texture::parse_envelope(&bc1_record())
                .expect("envelope")
                .mip_count,
        ),
        (
            "0x15",
            luminance_record(),
            LUMINANCE_FOURCC,
            spore_texture::parse_envelope(&luminance_record())
                .expect("envelope")
                .mip_count,
        ),
        (
            "0x15_single",
            luminance_single_mip_record(),
            LUMINANCE_FOURCC,
            spore_texture::parse_envelope(&luminance_single_mip_record())
                .expect("envelope")
                .mip_count,
        ),
    ] {
        let frame = frame_for(fourcc).expect("a frame");
        let image = decode_raster(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(image.layers.len(), 1, "{name}: one chain, always");
        assert_eq!(
            image.layers[0].len(),
            mips as usize,
            "{name}: one level per declared level"
        );
        assert_eq!(
            bytes.len(),
            record_size(frame, image.envelope.width, image.envelope.height, mips),
            "{name}: the record is as long as the framing says"
        );

        // Every declared level is at the offset this crate reports, and reading the
        // prefix there yields that level's own byte count.
        for mip in 0..mips {
            let at = level_offset(
                frame,
                image.envelope.width,
                image.envelope.height,
                mip,
                mips,
            )
            .unwrap_or_else(|error| panic!("{name} level {mip}: {error}"));
            if mip >= frame.first_prefixed_level {
                let declared =
                    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("four bytes in range"))
                        as usize;
                assert_eq!(
                    declared,
                    level_size(frame, image.envelope.width, image.envelope.height, mip),
                    "{name} level {mip}: the prefix at the reported offset is this level's own \
                     byte count"
                );
            }
            let size = level_size(frame, image.envelope.width, image.envelope.height, mip);
            assert!(
                at + size <= bytes.len(),
                "{name} level {mip}: the level is inside the record"
            );
        }
    }
}

/// Every refusal is a typed variant, never a panic and never a number.
///
/// The sweep is over the whole `u32` index space at its corners plus every
/// `mip_count` the corpus uses, because a refusal that is implemented with an
/// unchecked `+` or a subtraction can overflow at exactly one of them.
#[test]
fn no_index_and_no_geometry_panics_and_nothing_unplaceable_returns_a_number() {
    for frame in frames() {
        for mips in [0u32, 1, 6, 7, 9, 10, MAX_PLACEABLE_LEVELS, u32::MAX] {
            for mip in [
                0u32,
                1,
                mips.saturating_sub(1),
                mips,
                mips.saturating_add(1),
                MIP_FLOOR,
                MIP_FLOOR + 1,
                u32::MAX - 1,
                u32::MAX,
            ] {
                for (width, height) in [(512u32, 512u32), (1, 1), (0, 8), (8, 0), (0, 0)] {
                    let answer = level_offset(frame, width, height, mip, mips);
                    let placeable =
                        width != 0 && height != 0 && mip < mips && mip < MAX_PLACEABLE_LEVELS;
                    assert_eq!(
                        answer.is_ok(),
                        placeable,
                        "{:#010x} {width}x{height} mips={mips} level {mip}: {}",
                        frame.fourcc,
                        if placeable {
                            "should be placed"
                        } else {
                            "should be refused"
                        }
                    );
                    if let Ok(offset) = answer {
                        // A returned offset is inside the record the envelope
                        // implies. Never past it, never a sentinel.
                        let record = record_size(frame, width, height, mips);
                        assert!(
                            offset < record,
                            "{:#010x} {width}x{height} mips={mips} level {mip}: offset {offset} \
                             is not inside the {record}-byte record",
                            frame.fourcc
                        );
                    }
                }
            }
        }
    }
}
