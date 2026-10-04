//! Every raster record in the real corpus, in two languages, over both content
//! packages.
//!
//! **Every test here is `#[ignore]`d and every one skips cleanly when the game is
//! not installed.** They read `SPORE/Data/*.package` and `SPORE/DataEP1/*.package`
//! from the operator's disk; they never copy, embed or commit a Spore byte.
//! Without `SPORE/` they print a `SKIP:` line and return.
//!
//! ```text
//! cargo test -p spore-differential --release --test luminance_corpus -- --ignored --nocapture
//! ```
//!
//! # What this measures that `luminance_oracle.rs` cannot
//!
//! `luminance_oracle.rs` proves the decoder agrees with an independently written
//! identity **on records this repository built**. That leaves the load-bearing
//! question open: does the identity describe Spore's bytes? Only the corpus
//! answers that, and it answers it over **every** raster record rather than a
//! sample.
//!
//! Four things are checked per record, and they are different checks:
//!
//! 1. the framing identity holds **exactly** — the record is as long as the
//!    identity says, with nothing unconsumed and nothing padded. This is what
//!    makes the format finding a finding rather than a plausible guess.
//! 2. `spore_texture::decode_raster` accepts the record, and one chain comes
//!    back with one level per declared level. For the `0x15` family every level
//!    is additionally byte-identical to the window the framing gives it, because
//!    that family's codec is a copy.
//! 3. the driver's `rasterframe` walk — a second transcription of the framing in
//!    Python, sharing no constant with the crate — agrees on every offset, every
//!    length and every digest. See `spore_differential::oracle`'s module
//!    documentation for why that is a cross-*language* check and not an oracle.
//! 4. `field_18` states the base level's byte count. It is the one header word
//!    whose meaning the corpus settled, in all three formats.
//!
//! # What changed when the corpus refuted the layer model
//!
//! This file used to assert the two-layer reading: `DXT1` refused by name in all
//! 306 records, and *"a corpus where every DXT5 record decodes would mean the
//! division stopped refusing"* — a sentence that made "every record decodes" a
//! **failure**. It is now the measurement. `docs/RASTER-FRAMING.md` §2 has the
//! byte count that refuted it: the old rule charged 8 bytes per 4x4 block where
//! the records hold 16, and divided the payload by `16 + chain`; a 16-byte block
//! read at 8 bytes per block is two half-pictures and two of them make one
//! picture, so the two errors cancelled on 1374 records and not on the other 373.
//!
//! # What is deliberately not measured
//!
//! The block **codec**. `spore-texture` grades BC3 and BC1 `Inferred` — no decode
//! of a real record has been compared against the game's own output — and
//! `tools/spore/raster/raster.py` + `dxt5.py`, the only other decoder, implement
//! the refuted 8-byte block, so a comparison here would measure the disagreement
//! and not the truth. That disagreement is reported by
//! `tests/real_corpus::raster_envelope_and_base_mip` instead, which is where a
//! refuted oracle belongs: visible, classified, and not silently averaged away.
//!
//! Lane content of the `0x15` family: `spore-material`'s measurement, over the
//! same records. Repeating it here would be a second source of truth for a fact
//! that already has one.

use std::collections::BTreeMap;
use std::path::PathBuf;

use spore_assets::Package;
use spore_core::record::type_id;
use spore_differential::{Corpus, Json, OracleConfig, OracleSession};

/// The packages every number in this file is measured over, with their
/// raster-record census. `(package file name, total records, raster records)`.
///
/// `Spore_EP1_Content_01.package` is in `SPORE/DataEP1/`, which is why this goes
/// through [`Corpus::discover`] rather than naming one path: the EP1 half of the
/// measurement was missing while this file only looked at one directory.
const PACKAGES: [(&str, Option<usize>, usize); 2] = [
    ("Spore_Content.package", Some(17_119), 2_954),
    ("Spore_EP1_Content_01.package", None, 1_432),
];

/// Raster records per `fourcc` per package, measured 2026-10-04 and re-measured
/// by `every_raster_record_decodes_under_the_format_it_declares` below.
const CENSUS: [(&str, [(u32, usize); 3]); 2] = [
    (
        "Spore_Content.package",
        [
            (0x3554_5844, 1_747), // DXT5
            (0x0000_0015, 901),   // the uncompressed four-lane family
            (0x3154_5844, 306),   // DXT1
        ],
    ),
    (
        "Spore_EP1_Content_01.package",
        [(0x3554_5844, 731), (0x0000_0015, 373), (0x3154_5844, 328)],
    ),
];

/// Every raster record measured, in both packages.
const RASTER_RECORDS_TOTAL: usize = 4_386;

/// The format word that selects the uncompressed family.
const LUMINANCE_FOURCC: u32 = 0x0000_0015;
const DXT5_FOURCC: u32 = 0x3554_5844;
const DXT1_FOURCC: u32 = 0x3154_5844;

// ------------------------------------------------ the framing, re-derived here

/// One measured frame, written out here rather than taken from the crate or from
/// the driver.
///
/// Three implementations of this table now exist -- this one, `spore-texture`'s
/// `framing` module, and the driver's `FRAMES` -- and the point of a third is
/// that it was written from the measured table in `docs/RASTER-FRAMING.md` §2
/// without reading either of the others. Where they disagree, one of them is a
/// transcription error, and that is worth finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Frame {
    /// Bytes of header before the base level's payload.
    header: usize,
    /// Bytes per 4x4 block, or `None` for the family that has no blocks.
    block: Option<usize>,
    /// First level preceded by its own `u32` byte count.
    first_prefixed: u32,
}

const DXT5_FRAME: Frame = Frame {
    header: 32,
    block: Some(16),
    first_prefixed: 2,
};
const DXT1_FRAME: Frame = Frame {
    header: 32,
    block: Some(8),
    first_prefixed: 2,
};
const LUMINANCE_FRAME: Frame = Frame {
    header: 28,
    block: None,
    first_prefixed: 1,
};

fn frame_for(fourcc: u32) -> Option<Frame> {
    match fourcc {
        DXT5_FOURCC => Some(DXT5_FRAME),
        DXT1_FOURCC => Some(DXT1_FRAME),
        LUMINANCE_FOURCC => Some(LUMINANCE_FRAME),
        _ => None,
    }
}

fn dim(base: u32, mip: u32) -> u32 {
    if mip >= 32 {
        1
    } else {
        (base >> mip).max(1)
    }
}

fn ceil4(n: u32) -> u32 {
    n / 4 + if n % 4 != 0 { 1 } else { 0 }
}

/// Payload bytes of one level under `frame`.
fn level_bytes(frame: Frame, width: u32, height: u32, mip: u32) -> usize {
    match frame.block {
        None => dim(width, mip) as usize * dim(height, mip) as usize * 4,
        Some(block) => ceil4(dim(width, mip)) as usize * ceil4(dim(height, mip)) as usize * block,
    }
}

/// The one record size this framing allows.
fn identity(frame: Frame, width: u32, height: u32, mip_count: u32) -> usize {
    let mut total = frame.header;
    let bounded = mip_count.min(32);
    for mip in 0..bounded {
        if mip >= frame.first_prefixed {
            total += 4;
        }
        total += level_bytes(frame, width, height, mip);
    }
    total
        + (mip_count.saturating_sub(bounded) as usize) * (4 + level_bytes(frame, width, height, 32))
}

/// `(offset, length)` of every level's payload.
fn windows(frame: Frame, width: u32, height: u32, mip_count: u32) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut at = frame.header;
    for mip in 0..mip_count {
        if mip >= frame.first_prefixed {
            at += 4;
        }
        let len = level_bytes(frame, width, height, mip);
        out.push((at, len));
        at += len;
    }
    out
}

fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

// ------------------------------------------------------------------ the corpus

/// The located packages this file measures, or an empty vector with a printed
/// skip line.
fn located() -> Vec<(String, PathBuf)> {
    let corpus = Corpus::discover();
    let mut out = Vec::new();
    for (name, _, _) in PACKAGES {
        match corpus.packages().iter().find(|p| p.name == name) {
            Some(package) => out.push((package.name.clone(), package.path.clone())),
            None => println!("SKIP: {name} is not installed"),
        }
    }
    if out.is_empty() {
        println!("SKIP: no Spore content package is installed; nothing to measure");
    }
    out
}

/// `(package row, entry)` for every raster record, in package order.
fn raster_rows(package: &Package) -> Vec<(usize, spore_dbpf::DbpfEntry)> {
    package
        .index()
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.type_id == type_id::RASTER)
        .map(|(row, entry)| (row, *entry))
        .collect()
}

/// Every raster record of both packages frames exactly, decodes to its own
/// bytes, and agrees with a second transcription of the framing in another
/// language.
///
/// This is the test the whole framing rests on. The numbers it prints are the
/// measurement; the assertions are those numbers, so a new record family in a
/// patch shows up instead of being absorbed.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1.5 GB)"]
fn every_raster_record_frames_exactly_in_rust_and_in_python() {
    let packages = located();
    if packages.is_empty() {
        return;
    }
    let config = OracleConfig::detect();
    assert!(
        config.is_available(),
        "SKIP: the oracle driver is unavailable: {}",
        config.unavailable_reason().unwrap_or_default()
    );

    let mut total_raster = 0usize;
    let mut total_decoded = 0usize;
    let mut total_cross_checked = 0usize;
    let mut total_field_18 = 0usize;
    let mut shapes: BTreeMap<(u32, u32, u32, u32), usize> = BTreeMap::new();
    let mut by_package: Vec<(String, usize, usize, usize)> = Vec::new();

    for (name, path) in &packages {
        let package = Package::open(name.clone(), path.clone()).expect("package must open");
        println!(
            "corpus: {} ({} bytes)",
            path.display(),
            package.bytes().len()
        );
        let image = package.bytes();
        let rows = raster_rows(&package);
        let expected_raster = PACKAGES
            .iter()
            .find(|(expected, _, _)| *expected == name.as_str())
            .map(|(_, _, count)| *count)
            .expect("a measured package");
        assert_eq!(
            rows.len(),
            expected_raster,
            "{name}: the raster population of this package is a fact worth seeing change"
        );
        if let Some(expected_records) = PACKAGES
            .iter()
            .find(|(expected, _, _)| *expected == name.as_str())
            .and_then(|(_, records, _)| *records)
        {
            assert_eq!(
                package.record_count(),
                expected_records,
                "{name} is recorded as holding {expected_records} records"
            );
        }

        let mut session = OracleSession::spawn(&config).expect("driver must start");
        session.open(path).expect("driver must open the package");

        let mut raster = 0usize;
        let mut decoded = 0usize;
        let mut cross_checked = 0usize;
        let mut field_18 = 0usize;

        for (row, entry) in &rows {
            raster += 1;
            let bytes = spore_dbpf::extract_record(image, entry)
                .unwrap_or_else(|error| panic!("{name} row {row}: extract failed: {error}"));
            let envelope = spore_texture::parse_envelope(&bytes)
                .unwrap_or_else(|error| panic!("{name} row {row}: envelope refused: {error}"));
            let frame = frame_for(envelope.fourcc).unwrap_or_else(|| {
                panic!(
                    "{name} row {row}: fourcc 0x{:08x} has no measured framing, so this corpus \
                     holds a format family this file does not measure",
                    envelope.fourcc
                )
            });
            let size = identity(frame, envelope.width, envelope.height, envelope.mip_count);
            assert_eq!(
                bytes.len(),
                size,
                "{name} row {row}: the record must be exactly as long as the framing identity \
                 says, with nothing unconsumed and nothing padded"
            );
            *shapes
                .entry((
                    envelope.fourcc,
                    envelope.width,
                    envelope.height,
                    envelope.mip_count,
                ))
                .or_default() += 1;

            // --- the crate decodes it, into exactly one chain.
            let picture = spore_texture::decode_raster(&bytes).unwrap_or_else(|error| {
                panic!("{name} row {row}: the crate refused a well-framed record: {error}")
            });
            assert_eq!(
                picture.layers.len(),
                1,
                "{name} row {row}: one chain, always"
            );
            assert_eq!(
                picture.layers[0].len(),
                envelope.mip_count as usize,
                "{name} row {row}: one decoded level per declared level"
            );
            decoded += 1;

            // --- field_18 is the base level's byte count, in every format.
            let base = level_bytes(frame, envelope.width, envelope.height, 0);
            assert_eq!(
                envelope.field_18 as usize, base,
                "{name} row {row}: field_18 restates the base level's payload size"
            );
            field_18 += 1;

            // --- and every level is the bytes at its own window. For the block
            //     formats a codec intervenes, so only geometry is comparable;
            //     for the uncompressed family the pixels ARE the bytes, and that
            //     is checked exactly.
            let windows = windows(frame, envelope.width, envelope.height, envelope.mip_count);
            assert_eq!(
                windows.last().map(|(at, len)| at + len),
                Some(bytes.len()),
                "{name} row {row}: the identity's own walk must end at the record end"
            );
            if frame.block.is_none() {
                for (mip, (at, len)) in windows.iter().enumerate() {
                    assert_eq!(
                        &picture.layers[0][mip].pixels,
                        &bytes[*at..*at + *len],
                        "{name} row {row} mip {mip}: the family's codec is a copy, so a level must \
                         be the exact bytes at its own window"
                    );
                }
            }

            // --- the driver walks the same bytes with its own transcription.
            let reply = session
                .request_raster_frame(*row)
                .unwrap_or_else(|error| panic!("{name} row {row}: the driver failed: {error}"));
            assert_eq!(
                reply.get("ok").and_then(Json::as_bool),
                Some(true),
                "{name} row {row}: the driver's walk refused a well-framed record: {}",
                reply
                    .get("err")
                    .and_then(Json::as_str)
                    .unwrap_or("<no reason>")
            );
            assert_eq!(
                reply.get("identity").and_then(Json::as_u32),
                Some(size as u32),
                "{name} row {row}: the two transcriptions disagree about the record size"
            );
            assert_eq!(
                reply.get("consumed").and_then(Json::as_u32),
                Some(bytes.len() as u32),
                "{name} row {row}: the driver's walk must end at the record end"
            );
            assert_eq!(
                reply.get("block_bytes").and_then(Json::as_u32),
                frame.block.map(|b| b as u32),
                "{name} row {row}: the two transcriptions disagree about the block stride"
            );
            assert_eq!(
                reply.get("first_prefixed_level").and_then(Json::as_u32),
                Some(frame.first_prefixed),
                "{name} row {row}: the two transcriptions disagree about the first prefixed level"
            );
            assert_eq!(
                reply.get("field_18_is_base_geom").and_then(Json::as_bool),
                Some(true),
                "{name} row {row}: the driver's own reading of field_18 disagrees with its geometry"
            );
            let mips = reply
                .get("mips")
                .and_then(Json::as_array)
                .unwrap_or_else(|| panic!("{name} row {row}: the driver returned no level list"));
            assert_eq!(
                mips.len(),
                envelope.mip_count as usize,
                "{name} row {row}: one level per declared level"
            );
            for (mip, reported) in mips.iter().enumerate() {
                let decoded_mip = &picture.layers[0][mip];
                let at = reported
                    .get("offset")
                    .and_then(Json::as_u32)
                    .expect("offset") as usize;
                let len = reported.get("geom").and_then(Json::as_u32).expect("geom") as usize;
                assert_eq!(
                    at, windows[mip].0,
                    "{name} row {row} mip {mip}: the two transcriptions disagree about the offset"
                );
                assert_eq!(
                    len, windows[mip].1,
                    "{name} row {row} mip {mip}: length disagreement"
                );
                assert_eq!(
                    decoded_mip.pixels.len(),
                    if frame.block.is_none() {
                        len
                    } else {
                        decoded_mip.width as usize * decoded_mip.height as usize * 4
                    },
                    "{name} row {row} mip {mip}: length disagreement"
                );
                assert_eq!(
                    decoded_mip.width,
                    reported.get("width").and_then(Json::as_u32).expect("width"),
                    "{name} row {row} mip {mip}: width disagreement"
                );
                assert_eq!(
                    decoded_mip.height,
                    reported
                        .get("height")
                        .and_then(Json::as_u32)
                        .expect("height"),
                    "{name} row {row} mip {mip}: height disagreement"
                );
                if (mip as u32) >= frame.first_prefixed {
                    // The record's own length prefix, at the offset the identity
                    // derives. This is the strongest evidence the framing has: the
                    // record is stating its own level lengths here.
                    assert_eq!(
                        read_u32(&bytes, at - 4),
                        Some(len as u32),
                        "{name} row {row} mip {mip}: the record's own length prefix must state this \
                         level's size"
                    );
                    assert_eq!(
                        reported.get("declared").and_then(Json::as_u32),
                        Some(len as u32),
                        "{name} row {row} mip {mip}: prefix disagreement"
                    );
                } else {
                    // The driver states `null` for a level with no prefix, which
                    // is JSON's own "no value" -- an absent prefix and a
                    // missing key are the same answer here.
                    assert!(
                        matches!(reported.get("declared"), None | Some(Json::Null)),
                        "{name} row {row} mip {mip}: a level below the first prefixed one carries \
                         no prefix, got {:?}",
                        reported.get("declared")
                    );
                }
            }
            cross_checked += 1;
        }

        println!(
            "{name}: {raster} raster record(s), {decoded} decoded, {cross_checked} cross-checked"
        );
        total_raster += raster;
        total_decoded += decoded;
        total_cross_checked += cross_checked;
        total_field_18 += field_18;
        by_package.push((name.clone(), raster, decoded, cross_checked));
    }

    println!("raster records: {total_raster}");
    println!("decoded by the crate: {total_decoded}");
    println!("cross-checked against the driver: {total_cross_checked}");
    println!("field_18 read as the base level's size: {total_field_18}");
    for ((fourcc, w, h, mips), count) in &shapes {
        println!(
            "  0x{fourcc:08x} {w}x{h} mips={mips}: {count} record(s), {} bytes",
            identity(
                frame_for(*fourcc).expect("a measured fourcc"),
                *w,
                *h,
                *mips
            )
        );
    }

    assert_eq!(
        total_raster, RASTER_RECORDS_TOTAL,
        "every raster record of both content packages is a fact worth seeing change"
    );
    assert_eq!(
        total_decoded, total_raster,
        "every raster record decodes; a refusal here is a regression, not a ceiling"
    );
    assert_eq!(
        total_cross_checked, total_raster,
        "every raster record was cross-checked against the second transcription"
    );
    assert_eq!(
        total_field_18, total_raster,
        "field_18 states the base level's byte count in every raster record of every format"
    );
    assert_eq!(
        by_package.len(),
        PACKAGES.len(),
        "both content packages were measured; the EP1 half of the census must not go missing"
    );
}

/// The census, and the claim it used to deny: **every** raster record decodes.
///
/// The previous version of this file asserted `by_fourcc[&0x3154_5844].1 == 0`
/// ("DXT1 is present in the corpus and is refused by name, in every record") and,
/// for DXT5, `dxt5_ok > 0 && dxt5_ok < dxt5_total` with the comment *"a corpus
/// where every DXT5 record decodes would mean the division stopped refusing"*.
/// Both arms encoded the refuted layer model. The measurement is the opposite,
/// and the count of refusals is now asserted to be **zero** so a future refusal
/// cannot appear without this test failing.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1.5 GB)"]
fn every_raster_record_decodes_under_the_format_it_declares() {
    let packages = located();
    if packages.is_empty() {
        return;
    }
    let mut grand: BTreeMap<u32, (usize, usize)> = BTreeMap::new();

    for (name, path) in &packages {
        let package = Package::open(name.clone(), path.clone()).expect("package must open");
        let image = package.bytes();
        let rows = raster_rows(&package);
        let mut by_fourcc: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
        let mut refusals: Vec<String> = Vec::new();

        for (row, entry) in &rows {
            let bytes = spore_dbpf::extract_record(image, entry)
                .unwrap_or_else(|error| panic!("{name} row {row}: extract failed: {error}"));
            let envelope = spore_texture::parse_envelope(&bytes)
                .unwrap_or_else(|error| panic!("{name} row {row}: envelope refused: {error}"));
            let slot = by_fourcc.entry(envelope.fourcc).or_default();
            slot.0 += 1;
            match spore_texture::decode_raster(&bytes) {
                Ok(picture) => {
                    slot.1 += 1;
                    // Whatever decoded must have declared its own format word: the
                    // crate dispatches on it, so this is a check that the dispatch
                    // and the accepted set cannot drift apart.
                    assert!(
                        frame_for(envelope.fourcc).is_some(),
                        "{name} row {row}: fourcc 0x{:08x} decoded, which has no measured framing",
                        envelope.fourcc
                    );
                    assert_eq!(picture.layers.len(), 1, "{name} row {row}: one chain");
                    assert!(!picture.layers[0].is_empty(), "{name} row {row}: no levels");
                }
                Err(error) => {
                    if refusals.len() < 10 {
                        refusals.push(format!("{name} row {row}: {error}"));
                    }
                }
            }
        }

        println!("{name}:");
        for (fourcc, (total, ok)) in &by_fourcc {
            println!(
                "  fourcc 0x{fourcc:08x}: {total} record(s), {ok} decoded, {} refused",
                total - ok
            );
        }
        let measured = CENSUS
            .iter()
            .find(|(expected, _)| *expected == name.as_str())
            .map(|(_, rows)| *rows)
            .expect("a measured package");
        assert_eq!(
            by_fourcc.len(),
            3,
            "{name}: three format words, measured: {by_fourcc:?}"
        );
        for (fourcc, count) in measured {
            assert_eq!(
                by_fourcc[&fourcc].0, count,
                "{name}: fourcc 0x{fourcc:08x} population is a fact worth seeing change"
            );
            assert_eq!(
                by_fourcc[&fourcc].1,
                count,
                "{name}: fourcc 0x{fourcc:08x} decodes in EVERY record. This arm used to assert the \
                 opposite -- 0 decoded for DXT1, and a partial decode for DXT5 -- because the \
                 record was held to a division the corpus refuted."
            );
        }
        assert!(
            refusals.is_empty(),
            "{name}: TextureError is unreachable from decode_raster over the measured corpus, and \
             these records reached it: {refusals:?}"
        );
        for (fourcc, counts) in &by_fourcc {
            let entry = grand.entry(*fourcc).or_default();
            entry.0 += counts.0;
            entry.1 += counts.1;
        }
    }

    println!("both packages:");
    for (fourcc, (total, ok)) in &grand {
        println!(
            "  fourcc 0x{fourcc:08x}: {total} record(s), {ok} decoded, {} refused",
            total - ok
        );
    }
    let total: usize = grand.values().map(|(total, _)| total).sum();
    let ok: usize = grand.values().map(|(_, ok)| ok).sum();
    assert_eq!(
        total, RASTER_RECORDS_TOTAL,
        "every raster record of both content packages"
    );
    assert_eq!(
        ok, total,
        "2954 of 2954 and 1432 of 1432: there is no raster refusal left in either package"
    );
}

/// The base level's payload is four bytes per texel with **no** codec, so the
/// four bytes at `0x1c` are the first texel rather than a header field. Measured
/// on real records, because that is where the 32-versus-28-byte confusion came
/// from.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1.5 GB)"]
fn the_word_at_0x1c_is_the_first_texel_of_every_record_of_the_family() {
    let packages = located();
    if packages.is_empty() {
        return;
    }
    let mut checked = 0usize;
    let mut with_field_18_equal_to_the_base_size = 0usize;

    for (name, path) in &packages {
        let package = Package::open(name.clone(), path.clone()).expect("package must open");
        let image = package.bytes();
        for (row, entry) in raster_rows(&package) {
            let bytes = spore_dbpf::extract_record(image, &entry)
                .unwrap_or_else(|error| panic!("{name} row {row}: extract failed: {error}"));
            let envelope = spore_texture::parse_envelope(&bytes)
                .unwrap_or_else(|error| panic!("{name} row {row}: envelope refused: {error}"));
            if envelope.fourcc != LUMINANCE_FOURCC {
                continue;
            }
            let base = level_bytes(LUMINANCE_FRAME, envelope.width, envelope.height, 0);
            assert_eq!(
                envelope.field_18 as usize, base,
                "{name} row {row}: field_18 restates the base level's payload size in every record"
            );
            with_field_18_equal_to_the_base_size += 1;

            let picture = spore_texture::decode_raster(&bytes).expect("decodes");
            assert_eq!(
                &picture.layers[0][0].pixels[..4],
                &bytes[0x1C..0x20],
                "{name} row {row}: the four bytes the 32-byte reader reports as `field_1c` are the \
                 first texel of the base level, so this family's header is 28 bytes and not 32"
            );
            // And it is a real texel, not padding: a normal map's Z lane leads.
            assert!(
                picture.layers[0][0].pixels[0] > picture.layers[0][0].pixels[1],
                "{name} row {row}: first texel lane 0 should lead lane 1 (Z-first); got {:?}",
                &picture.layers[0][0].pixels[..4]
            );
            checked += 1;
        }
    }
    let expected: usize = CENSUS
        .iter()
        .filter(|(name, _)| packages.iter().any(|(located, _)| located == name))
        .map(|(_, rows)| {
            rows.iter()
                .find(|(fourcc, _)| *fourcc == LUMINANCE_FOURCC)
                .map_or(0, |(_, count)| *count)
        })
        .sum();
    assert!(
        checked > 0,
        "no record of the family was checked; the SKIP lines above say why"
    );
    assert_eq!(
        checked, expected,
        "every record of the family in every measured package was checked"
    );
    assert_eq!(with_field_18_equal_to_the_base_size, expected);
    println!("first-texel check: {checked} record(s)");
}

/// The record counts themselves, so a missing package is a failure with a reason
/// rather than a silently smaller measurement.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1.5 GB)"]
fn both_content_packages_are_the_ones_the_census_names() {
    let packages = located();
    assert_eq!(
        packages.len(),
        PACKAGES.len(),
        "SKIP: the measurement covers {} package(s); a missing one changes every number above",
        packages.len()
    );
    for (name, _) in &packages {
        println!("measured: {name}");
    }
}
