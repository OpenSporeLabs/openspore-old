//! The manifest's decoder classification must never disagree with the loader.
//!
//! Hermetic, except for the corpus test at the end, which is `#[ignore]`d and
//! skips cleanly. No Spore asset bytes are committed; the hermetic half builds a
//! synthetic package in memory.
//!
//! ```text
//! cargo test -p spore-assets --test manifest_vs_loader
//! cargo test -p spore-assets --release --test manifest_vs_loader -- --ignored --nocapture
//! ```
//!
//! # The invariant
//!
//! `DecodeStatus::Ok` means exactly one thing: **this workspace has a per-record
//! decoder for this type id**. It is a type-level claim, made by a table, and a
//! table is exactly the kind of thing that rots: a decoder gets added, or a
//! format turns out to be something else, and the table keeps answering the
//! question it was asked a year ago.
//!
//! The existing tests check the classifier against itself -- they call it with
//! ids and assert what it returns. That proves the table says what it says. It
//! does not prove the table is *right*, and the two ways it can be wrong are
//! different bugs with different symptoms:
//!
//! * **too narrow** -- a decoder exists but the manifest says `Undecoded`. The
//!   record is skipped by `osptool verify`, never rendered, and nothing warns.
//! * **too wide** -- the manifest advertises decodability the workspace does not
//!   have. This is the one that already happened twice: `png` and `jpeg` were
//!   listed as decodable and are raw PNG and raw JPEG.
//!
//! So the hermetic half asserts the two-way agreement against the **loader
//! functions themselves**, over a table of ids that includes every id this
//! workspace has a decoder for and several near misses. The corpus half then
//! closes the loop over real records: for every gmdl record the cheap header
//! probe and the real walk must reach compatible verdicts, and for every raster
//! and rw4 record the classification must be `Ok` with the loader accepting or
//! naming its refusal.

use std::path::PathBuf;

use spore_assets::manifest::{probe_gmdl, DecodeStatus, ManifestBuilder};
use spore_assets::{ContentStore, Package};
use spore_core::record::type_id;
use spore_core::ResourceKey;

const HEADER_SIZE: usize = 96;
const ROW_SIZE: usize = 28;

/// Every type id this workspace can decode, and the loader that does it.
const DECODABLE: &[(u32, &str)] = &[
    (type_id::GMDL, "spore_gmdl::parse"),
    (type_id::RW4, "spore_rw4::parse"),
    (type_id::RASTER, "spore_texture::decode_raster"),
];

/// Type ids that must NOT be classified decodable, and why. Each of these is a
/// record family that exists in the corpus and that something here has already
/// got wrong once.
const NOT_DECODABLE: &[(u32, &str)] = &[
    (
        type_id::PNG,
        "raw PNG: 10 487 of 10 487 carry the PNG magic",
    ),
    // Near misses, one bit away from a decodable id. `RW4 + 1` would be a poor
    // choice: RW4 is 0x2f4e681b and RASTER is 0x2f4e681c, so "neighbour" is not
    // a concept here -- adjacency in the number line means nothing.
    (type_id::GMDL ^ 1, "one bit from a decodable id"),
    (type_id::RW4 ^ 1, "one bit from a decodable id"),
    (type_id::RASTER ^ 1, "one bit from a decodable id"),
    (0x0119_89B7, "plt: a palette, and nothing here decodes one"),
    (
        0x01C1_35DA,
        "gmsh: a RenderWare mesh section, not a gmdl record",
    ),
    (0x2F7D_0002, "jpeg: raw JPEG"),
    (0, "zero is not a type id"),
    (u32::MAX, "the largest u32 is not a type id"),
];

/// Builds a synthetic DBPF v3 image holding one record per type id, so the
/// classifier can be read back out of a real manifest rather than called
/// directly. A private function that a test cannot reach is not the thing under
/// test.
fn manifest_status_of(type_id: u32) -> DecodeStatus {
    let index_offset = HEADER_SIZE;
    let arena_offset = index_offset + 4 + ROW_SIZE;
    let key = ResourceKey::new(type_id, 0, 1);

    let mut index = Vec::with_capacity(4 + ROW_SIZE);
    index.extend_from_slice(&0u32.to_le_bytes());
    index.extend_from_slice(&key.type_id.to_le_bytes());
    index.extend_from_slice(&key.group_id.to_le_bytes());
    index.extend_from_slice(&key.instance_id.to_le_bytes());
    index.extend_from_slice(&(arena_offset as u32).to_le_bytes());
    index.extend_from_slice(&1u32.to_le_bytes());
    index.extend_from_slice(&1u32.to_le_bytes());
    index.extend_from_slice(&0u16.to_le_bytes());
    index.push(0);
    index.push(0);

    let mut out = vec![0u8; HEADER_SIZE];
    out[0..4].copy_from_slice(b"DBPF");
    out[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
    out[0x28..0x2c].copy_from_slice(&((4 + ROW_SIZE) as u32).to_le_bytes());
    out[0x40..0x44].copy_from_slice(&(index_offset as u32).to_le_bytes());
    out.extend_from_slice(&index);
    out.push(0xAB);

    let mut store = ContentStore::new();
    store.push(Package::from_vec("synthetic", out).expect("the synthetic package must open"));
    let manifest = ManifestBuilder::from_store(&store);
    let status = manifest
        .rows()
        .next()
        .expect("one record, one row")
        .decode_status;
    status
}

#[test]
fn every_decoder_this_workspace_has_is_classified_decodable() {
    for (id, loader) in DECODABLE {
        assert_eq!(
            manifest_status_of(*id),
            DecodeStatus::Ok,
            "0x{id:08x} has a decoder ({loader}) so the manifest must say `Ok`; classifying it \\
             `Undecoded` makes the record invisible to `verify` and to any renderer that trusts \\
             the manifest"
        );
    }
}

#[test]
fn nothing_without_a_decoder_is_classified_decodable() {
    for (id, why) in NOT_DECODABLE {
        let status = manifest_status_of(*id);
        assert_ne!(
            status,
            DecodeStatus::Ok,
            "0x{id:08x} is classified decodable but nothing here decodes it ({why})"
        );
    }
}

#[test]
fn a_container_is_not_decodable_and_not_undecoded_either() {
    // Three states, not two: `Undecoded` and `ContainerUndecoded` are different
    // answers and collapsing them would lose the fact that a container is a
    // deliberate, understood shape rather than a gap.
    assert_eq!(
        manifest_status_of(type_id::PROP),
        DecodeStatus::ContainerUndecoded
    );
    assert_eq!(
        manifest_status_of(0x2399_BE55),
        DecodeStatus::ContainerUndecoded
    );
    let seen: std::collections::BTreeSet<&'static str> = DECODABLE
        .iter()
        .chain(NOT_DECODABLE.iter())
        .map(|(id, _)| manifest_status_of(*id).as_str())
        .collect();
    assert!(
        seen.contains(DecodeStatus::Ok.as_str()),
        "the table must exercise the `Ok` arm: {seen:?}"
    );
    assert!(
        seen.contains(DecodeStatus::Undecoded.as_str()),
        "the table must exercise the `Undecoded` arm: {seen:?}"
    );
}

#[test]
fn the_three_classifications_spell_themselves_and_none_of_them_is_a_default() {
    // A status that renders as the same string as another one cannot be told
    // apart in a report, which is where these numbers are read.
    let labels: Vec<&str> = [
        DecodeStatus::Ok.as_str(),
        DecodeStatus::Undecoded.as_str(),
        DecodeStatus::ContainerUndecoded.as_str(),
        DecodeStatus::WalkFail.as_str(),
    ]
    .to_vec();
    let unique: std::collections::BTreeSet<&&str> = labels.iter().collect();
    assert_eq!(
        unique.len(),
        labels.len(),
        "two statuses share a label: {labels:?}"
    );
    assert!(!labels.iter().any(|label| label.is_empty()));
}

/// The corpus half: the cheap header probe and the real walk must reach
/// compatible verdicts on real records.
///
/// `probe_gmdl` exists so `osptool manifest` can classify 4 209 gmdl records
/// without decompressing and walking all of them. A probe that disagrees with
/// the walk is worse than no probe: the manifest is a report a reader trusts.
/// **`#[ignore]`d; skips cleanly.**
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn the_header_probe_and_the_real_walk_never_disagree_on_the_corpus() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize();
    let Ok(root) = root else {
        println!("SKIP: the workspace root could not be resolved");
        return;
    };
    let path = root.join("SPORE/Data/Spore_Content.package");
    if !path.exists() {
        println!("SKIP: SPORE/Data/Spore_Content.package is not installed");
        return;
    }
    let mut store = ContentStore::new();
    store.push(Package::open("Spore_Content", path).expect("package must open"));
    let package = &store.packages()[0];
    let image = package.bytes();

    // --- the manifest's own verdict, per type id, must match the decoder list.
    let manifest = ManifestBuilder::from_store(&store);
    let mut decoded_types = std::collections::BTreeSet::new();
    for row in manifest.rows() {
        if row.decode_status == DecodeStatus::Ok {
            decoded_types.insert(row.key.type_id);
        }
    }
    let expected: std::collections::BTreeSet<u32> = DECODABLE.iter().map(|(id, _)| *id).collect();
    assert_eq!(
        decoded_types, expected,
        "over the real corpus the manifest calls exactly these type ids decodable"
    );

    // --- gmdl: probe vs walk.
    let mut probe_ok = 0usize;
    let mut probe_walk_fail = 0usize;
    let mut probe_ok_but_unsupported_version = 0usize;
    let mut probe_walk_fail_but_parsed = 0usize;
    for entry in package.index().entries() {
        if entry.type_id != type_id::GMDL {
            continue;
        }
        let bytes = spore_dbpf::extract_record(image, entry).expect("every gmdl record extracts");
        let verdict = probe_gmdl(&bytes);
        match verdict {
            DecodeStatus::Ok => probe_ok += 1,
            DecodeStatus::WalkFail => probe_walk_fail += 1,
            other => panic!("a gmdl record probed as {}", other.as_str()),
        }
        // `parse` is allowed to fail on a record the probe accepted -- that is
        // the anim-data and 0x218 ceiling -- but it must not fail on the header,
        // which is the only thing the probe looks at.
        if let Err(spore_gmdl::GmdlError::UnsupportedVersion { .. }) = spore_gmdl::parse(&bytes) {
            if verdict == DecodeStatus::Ok {
                probe_ok_but_unsupported_version += 1;
            }
        }
        if verdict == DecodeStatus::WalkFail && spore_gmdl::parse(&bytes).is_ok() {
            probe_walk_fail_but_parsed += 1;
        }
    }
    println!("gmdl records the probe accepted: {probe_ok}");
    println!("gmdl records the probe refused: {probe_walk_fail}");
    println!(
        "probe accepted, walk rejected on the version word: {probe_ok_but_unsupported_version}"
    );
    println!("probe refused, walk accepted: {probe_walk_fail_but_parsed}");
    assert_eq!(
        probe_ok_but_unsupported_version, 0,
        "the probe reads the version word, so accepting a record the walk rejects on that same \\
         word is a contradiction"
    );
    assert_eq!(
        probe_walk_fail_but_parsed, 0,
        "a record the walk decodes must never be classified `WalkFail` by the manifest: that is \\
         the direction that loses a record"
    );

    // --- raster: classified decodable, and the loader either decodes or names
    //     its refusal. Never a silent success on a record it should not accept.
    let mut raster_ok = 0usize;
    let mut raster_refused = 0usize;
    let mut raster_unexpected = 0usize;
    for entry in package.index().entries() {
        if entry.type_id != type_id::RASTER {
            continue;
        }
        let bytes = spore_dbpf::extract_record(image, entry).expect("every raster record extracts");
        match spore_texture::decode_raster(&bytes) {
            Ok(_) => raster_ok += 1,
            Err(error) => {
                raster_refused += 1;
                // A refusal must be a *named* condition. An unnamed one would
                // mean the decoder met something it has no arm for.
                let text = error.to_string();
                assert!(
                    !text.is_empty(),
                    "a raster refusal must name its condition, got an empty message"
                );
                raster_unexpected += usize::from(!matches!(
                    error,
                    spore_texture::TextureError::PayloadNotMultiple { .. }
                        | spore_texture::TextureError::PayloadTooSmall { .. }
                        | spore_texture::TextureError::UnsupportedFourcc { .. }
                        | spore_texture::TextureError::ZeroDimension { .. }
                        | spore_texture::TextureError::ZeroMipCount
                        | spore_texture::TextureError::LuminanceRecordSize { .. }
                        | spore_texture::TextureError::LuminanceLengthPrefix { .. }
                        | spore_texture::TextureError::TruncatedEnvelope { .. }
                        | spore_texture::TextureError::RecordShorterThanEnvelope { .. }
                ));
            }
        }
    }
    println!("raster records decoded: {raster_ok}, refused: {raster_refused}");
    assert_eq!(
        raster_unexpected, 0,
        "every raster refusal must be one of the conditions the decoder names; an unnamed \\
         refusal is a decoder that met something it has no arm for"
    );

    // --- rw4: classified decodable, and every record the core says carries the
    //     magic really does parse.
    let mut rw4_parsed = 0usize;
    let mut rw4_refused = 0usize;
    for entry in package.index().entries() {
        if entry.type_id != type_id::RW4 {
            continue;
        }
        let bytes = spore_dbpf::extract_record(image, entry).expect("every rw4 record extracts");
        match spore_rw4::parse(&bytes) {
            Ok(_) => rw4_parsed += 1,
            Err(_) => rw4_refused += 1,
        }
    }
    println!("rw4 records parsed: {rw4_parsed}, refused: {rw4_refused}");
    assert_eq!(
        rw4_refused, 0,
        "the manifest classifies rw4 as decodable, so a record it refuses is the disagreement \
         this file exists to catch"
    );
}
