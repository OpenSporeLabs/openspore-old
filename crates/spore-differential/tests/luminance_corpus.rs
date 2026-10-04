//! The `0x15` raster family over the **real** corpus, in two languages.
//!
//! **Every test here is `#[ignore]`d and every one skips cleanly when the game is
//! not installed.** They read `SPORE/Data/Spore_Content.package` from the
//! operator's disk; they never copy, embed or commit a Spore byte. Without
//! `SPORE/` they print a `SKIP:` line and return.
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
//! answers that, and it answers it over every record of the family rather than a
//! sample.
//!
//! Three things are checked per record, and they are different checks:
//!
//! 1. the framing identity holds **exactly** -- the record is as long as the
//!    identity says, with nothing unconsumed and nothing padded. This is what
//!    makes the format finding a finding rather than a plausible guess.
//! 2. `spore_texture::decode_raster` accepts the record, and every decoded mip
//!    is byte-identical to the window the framing gives it. The family's
//!    "codec" is a copy, so this is exact and not a tolerance.
//! 3. the driver's `raster15` walk -- a second implementation of the same
//!    identity in Python -- agrees on every offset, every length and every
//!    digest. See `spore_differential::oracle`'s module documentation for why
//!    that is a cross-*language* check and not an oracle.
//!
//! # What is deliberately not measured
//!
//! Lane content. What the four bytes *mean* is `spore-material`'s measurement
//! over the same records; repeating it here would be a second source of truth
//! for a fact that already has one.

use std::collections::BTreeMap;
use std::path::PathBuf;

use spore_assets::Package;
use spore_core::record::type_id;
use spore_differential::{Json, OracleConfig, OracleSession};

/// The package every number in this file is measured over.
const PACKAGE: &str = "SPORE/Data/Spore_Content.package";

/// Records in `Spore_Content.package`, measured 2026-10-04.
const RECORDS: usize = 17_119;

/// Raster records in that package, measured 2026-10-04: 1 747 DXT5, 901 `0x15`,
/// 306 DXT1. See `spore-texture`'s crate documentation.
const RASTER_RECORDS: usize = 2_954;
const LUMINANCE_RECORDS: usize = 901;

/// The format word that selects the `0x15` walk.
const LUMINANCE_FOURCC: u32 = 0x0000_0015;
const DXT5_FOURCC: u32 = 0x3554_5844;

/// One located package, or `None` with a printed skip line.
fn content_package() -> Option<(Package, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .ok()?;
    let path = root.join(PACKAGE);
    if !path.exists() {
        println!("SKIP: {PACKAGE} is not installed");
        return None;
    }
    let package = Package::open("Spore_Content", path.clone()).expect("package must open");
    println!(
        "corpus: {} ({} bytes)",
        path.display(),
        package.bytes().len()
    );
    Some((package, path))
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

fn read_u32(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The `0x15` framing identity, re-derived here rather than taken from the crate
/// or from the driver. Three implementations now exist -- this one, the crate's
/// and the driver's -- and the point of a third is that it was written from the
/// format description without reading either of the others.
fn identity(width: u32, height: u32, mip_count: u32) -> Option<usize> {
    let dim = |base: u32, mip: u32| {
        if mip >= 32 {
            1
        } else {
            (base >> mip).max(1)
        }
    };
    let mut total = 28usize;
    if mip_count == 0 {
        return Some(total);
    }
    let bounded = mip_count.min(32);
    for mip in 0..bounded {
        if mip > 0 {
            total = total.checked_add(4)?;
        }
        let w = dim(width, mip) as usize;
        let h = dim(height, mip) as usize;
        total = total.checked_add(w.checked_mul(h)?.checked_mul(4)?)?;
    }
    let extra = mip_count.saturating_sub(bounded) as usize;
    total.checked_add(extra.checked_mul(8)?)
}

/// Every `0x15` record frames exactly, decodes to its own bytes, and agrees with
/// a second implementation of the identity in another language.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn every_luminance_record_frames_exactly_in_rust_and_in_python() {
    let Some((package, path)) = content_package() else {
        return;
    };
    assert_eq!(
        package.record_count(),
        RECORDS,
        "Spore_Content.package is recorded as holding {RECORDS} records"
    );
    let image = package.bytes();
    let rows = raster_rows(&package);
    assert_eq!(
        rows.len(),
        RASTER_RECORDS,
        "the raster population of this package is a fact worth seeing change"
    );

    let config = OracleConfig::detect();
    assert!(
        config.is_available(),
        "SKIP: the oracle driver is unavailable: {}",
        config.unavailable_reason().unwrap_or_default()
    );
    let mut session = OracleSession::spawn(&config).expect("driver must start");
    session.open(&path).expect("driver must open the package");

    let mut luminance_rows = 0usize;
    let mut decoded = 0usize;
    let mut cross_checked = 0usize;
    let mut shapes: BTreeMap<(u32, u32, u32), usize> = BTreeMap::new();

    for (row, entry) in &rows {
        let bytes = spore_dbpf::extract_record(image, entry)
            .unwrap_or_else(|error| panic!("row {row}: extract failed: {error}"));
        let envelope = spore_texture::parse_envelope(&bytes)
            .unwrap_or_else(|error| panic!("row {row}: envelope refused: {error}"));
        if envelope.fourcc != LUMINANCE_FOURCC {
            continue;
        }
        luminance_rows += 1;
        let identity =
            identity(envelope.width, envelope.height, envelope.mip_count).expect("addressable");
        assert_eq!(
            bytes.len(),
            identity,
            "row {row}: the record must be exactly as long as the framing identity says, \
             with nothing unconsumed and nothing padded"
        );
        *shapes
            .entry((envelope.width, envelope.height, envelope.mip_count))
            .or_default() += 1;

        // --- the crate decodes it, and every mip is its own window.
        let picture = spore_texture::decode_raster(&bytes).unwrap_or_else(|error| {
            panic!("row {row}: the crate refused a well-framed record: {error}")
        });
        assert_eq!(picture.layers.len(), 1, "row {row}: one layer, always");
        decoded += 1;

        // --- the driver walks the same bytes with its own identity.
        let reply = session
            .request_luminance(*row)
            .unwrap_or_else(|error| panic!("row {row}: the driver failed: {error}"));
        assert_eq!(
            reply.get("ok").and_then(Json::as_bool),
            Some(true),
            "row {row}: the driver's walk refused a well-framed record: {}",
            reply
                .get("err")
                .and_then(Json::as_str)
                .unwrap_or("<no reason>")
        );
        assert_eq!(
            reply.get("identity").and_then(Json::as_u32),
            Some(identity as u32),
            "row {row}: the two implementations disagree about the record size"
        );
        assert_eq!(
            reply.get("consumed").and_then(Json::as_u32),
            Some(bytes.len() as u32),
            "row {row}: the driver's walk must end at the record end"
        );
        let mips = reply
            .get("mips")
            .and_then(Json::as_array)
            .unwrap_or_else(|| panic!("row {row}: the driver returned no mip list"));
        assert_eq!(
            mips.len(),
            envelope.mip_count as usize,
            "row {row}: one decoded mip per declared level"
        );
        for (mip, reported) in mips.iter().enumerate() {
            let decoded_mip = &picture.layers[0][mip];
            let at = reported
                .get("offset")
                .and_then(Json::as_u32)
                .expect("offset") as usize;
            let len = reported.get("geom").and_then(Json::as_u32).expect("geom") as usize;
            assert_eq!(
                decoded_mip.pixels.len(),
                len,
                "row {row} mip {mip}: length disagreement"
            );
            assert_eq!(
                &decoded_mip.pixels,
                &bytes[at..at + len],
                "row {row} mip {mip}: the crate's bytes are not the record's bytes at the \
                 offset the second implementation gives"
            );
            assert_eq!(
                decoded_mip.width,
                reported.get("width").and_then(Json::as_u32).expect("width"),
                "row {row} mip {mip}: width disagreement"
            );
            assert_eq!(
                decoded_mip.height,
                reported
                    .get("height")
                    .and_then(Json::as_u32)
                    .expect("height"),
                "row {row} mip {mip}: height disagreement"
            );
            if mip > 0 {
                assert_eq!(
                    read_u32(&bytes, at - 4),
                    Some(len as u32),
                    "row {row} mip {mip}: the record's own length prefix must state this level's \
                     size"
                );
                assert_eq!(
                    reported.get("declared").and_then(Json::as_u32),
                    Some(len as u32),
                    "row {row} mip {mip}: prefix disagreement"
                );
            }
        }
        cross_checked += 1;
    }

    println!("raster records: {}", rows.len());
    println!("luminance records: {luminance_rows}");
    println!("decoded by the crate: {decoded}");
    println!("cross-checked against the driver: {cross_checked}");
    for ((w, h, mips), count) in &shapes {
        println!(
            "  {w}x{h} mips={mips}: {count} record(s), {:?} bytes",
            identity(*w, *h, *mips)
        );
    }

    assert_eq!(
        luminance_rows, LUMINANCE_RECORDS,
        "the family is recorded as holding {LUMINANCE_RECORDS} records in this package"
    );
    assert_eq!(
        decoded, luminance_rows,
        "every record of the family decodes; a refusal here is a regression, not a ceiling"
    );
    assert_eq!(
        cross_checked, luminance_rows,
        "every record was cross-checked against the second implementation"
    );
    assert!(
        shapes.len() >= 3,
        "the family occurs in at least three shapes; measured {}: {shapes:?}",
        shapes.len()
    );
}

/// No raster record of any other format is accepted by the `0x15` walk, and
/// nothing in the package is accepted by a format it does not declare.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn no_raster_record_decodes_under_a_format_it_does_not_declare() {
    let Some((package, _)) = content_package() else {
        return;
    };
    let image = package.bytes();
    let rows = raster_rows(&package);

    let mut by_fourcc: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for (row, entry) in &rows {
        let bytes = spore_dbpf::extract_record(image, entry)
            .unwrap_or_else(|error| panic!("row {row}: extract failed: {error}"));
        let envelope = spore_texture::parse_envelope(&bytes)
            .unwrap_or_else(|error| panic!("row {row}: envelope refused: {error}"));
        let slot = by_fourcc.entry(envelope.fourcc).or_default();
        slot.0 += 1;
        // Whatever decoded must have declared its own format word: the crate
        // dispatches on it, so this is a check that the dispatch and the
        // accepted set cannot drift apart. A refusal is a refusal, and is
        // counted rather than asserted on individually.
        if let Ok(picture) = spore_texture::decode_raster(&bytes) {
            slot.1 += 1;
            assert!(
                envelope.fourcc == DXT5_FOURCC || envelope.fourcc == LUMINANCE_FOURCC,
                "row {row}: fourcc 0x{:08x} decoded, which this build refuses by name",
                envelope.fourcc
            );
            assert!(!picture.layers.is_empty());
        }
    }
    for (fourcc, (total, ok)) in &by_fourcc {
        println!(
            "fourcc 0x{fourcc:08x}: {total} record(s), {ok} decoded, {} refused",
            total - ok
        );
    }
    // The census itself. These are measurements, asserted exactly, so a new
    // record family appearing in a patch shows up instead of being absorbed.
    assert_eq!(
        by_fourcc.len(),
        3,
        "three format words, measured: {by_fourcc:?}"
    );
    assert_eq!(by_fourcc[&DXT5_FOURCC].0, 1_747);
    assert_eq!(by_fourcc[&LUMINANCE_FOURCC].0, LUMINANCE_RECORDS);
    assert_eq!(
        by_fourcc[&0x3154_5844].1, 0,
        "DXT1 is present in the corpus and is refused by name, in every record"
    );
    assert_eq!(
        by_fourcc[&LUMINANCE_FOURCC].1, LUMINANCE_RECORDS,
        "every record of the 0x15 family decodes"
    );
    // DXT5 is the family with a real ceiling: some records fall short of a whole
    // number of layers and are refused by the division. Asserted as a lower
    // bound with the count printed, because the exact figure moves with the
    // install while the property -- "no DXT5 record decodes to a partial image" --
    // does not.
    let dxt5_total = by_fourcc[&DXT5_FOURCC].0;
    let dxt5_ok = by_fourcc[&DXT5_FOURCC].1;
    assert!(
        dxt5_ok > 0 && dxt5_ok < dxt5_total,
        "DXT5 decodes {dxt5_ok} of {dxt5_total}; a corpus where every DXT5 record decodes \
         would mean the division stopped refusing"
    );
}

/// The base level's payload is four bytes per texel with **no** codec, so the
/// four bytes at `0x1c` are the first texel rather than a header field. Measured
/// on real records, because that is where the 32-versus-28-byte confusion came
/// from.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn the_word_at_0x1c_is_the_first_texel_of_every_record_of_the_family() {
    let Some((package, _)) = content_package() else {
        return;
    };
    let image = package.bytes();
    let mut checked = 0usize;
    let mut with_field_18_equal_to_the_base_size = 0usize;
    for (row, entry) in raster_rows(&package) {
        let bytes = spore_dbpf::extract_record(image, &entry)
            .unwrap_or_else(|error| panic!("row {row}: extract failed: {error}"));
        let envelope = spore_texture::parse_envelope(&bytes)
            .unwrap_or_else(|error| panic!("row {row}: envelope refused: {error}"));
        if envelope.fourcc != LUMINANCE_FOURCC {
            continue;
        }
        let base = (envelope.width as usize)
            .checked_mul(envelope.height as usize)
            .and_then(|n| n.checked_mul(4))
            .expect("addressable");
        assert_eq!(
            envelope.field_18 as usize, base,
            "row {row}: field_18 restates the base level's payload size in every record"
        );
        with_field_18_equal_to_the_base_size += 1;

        let picture = spore_texture::decode_raster(&bytes).expect("decodes");
        assert_eq!(
            &picture.layers[0][0].pixels[..4],
            &bytes[0x1C..0x20],
            "row {row}: the four bytes the 32-byte reader reports as `field_1c` are the first \
             texel of the base level, so this family's header is 28 bytes and not 32"
        );
        // And it is a real texel, not padding: a normal map's Z lane leads.
        assert!(
            picture.layers[0][0].pixels[0] > picture.layers[0][0].pixels[1],
            "row {row}: first texel lane 0 should lead lane 1 (Z-first); got {:?}",
            &picture.layers[0][0].pixels[..4]
        );
        checked += 1;
    }
    assert_eq!(checked, LUMINANCE_RECORDS);
    assert_eq!(with_field_18_equal_to_the_base_size, LUMINANCE_RECORDS);
    println!("first-texel check: {checked} record(s)");
}
