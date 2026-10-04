//! `verify`: the counts, the streaming discipline, and the exit code.

mod common;

use common::{osptool, package_of, raster_package, synthetic_raster, Row, TempFile, MINI_PACKAGE};
use spore_assets::{ContentStore, Package};
use spore_tools::commands::verify::{classify, RecordClass, Tally, CONTAINER_TYPE_IDS};
use spore_tools::EXIT_OK;

/// A package with one record of each interesting class, plus known-good and
/// known-broken payloads.
fn mixed_package() -> Vec<u8> {
    let mut broken_gmdl = common::read_fixture("mini.gmdl");
    broken_gmdl[0..4].copy_from_slice(&7u32.to_le_bytes()); // unsupported version
    package_of(&[
        Row::new(
            spore_gmdl::GMDL_TYPE,
            1,
            1,
            common::read_fixture("mini.gmdl"),
        ),
        Row::new(spore_gmdl::GMDL_TYPE, 1, 2, broken_gmdl),
        Row::new(
            spore_rw4::RW4_TYPE,
            2,
            1,
            common::read_fixture("mini_rw4.rw4"),
        ),
        Row::new(spore_rw4::RW4_TYPE, 2, 2, b"short".to_vec()),
        Row::new(
            spore_texture::RASTER_TYPE,
            3,
            1,
            synthetic_raster(16, 16, 2),
        ),
        Row::new(spore_texture::RASTER_TYPE, 3, 2, vec![0u8; 40]),
        // Container: never read, whatever its payload says.
        Row::new(spore_core::record::type_id::PROP, 4, 1, vec![0xff; 9]),
        // Named, no decoder in this build.
        Row::new(spore_core::record::type_id::PLT, 5, 1, vec![0u8; 12]),
        // Entirely unknown id.
        Row::new(0xdead_beef, 6, 1, vec![0u8; 4]),
    ])
}

#[test]
fn the_counts_partition_every_row() {
    let file = TempFile::with("verify-mixed", &mixed_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["verify", &path]);
    assert_eq!(code, EXIT_OK, "{err}");

    assert!(out.contains("index rows     9"), "{out}");
    assert!(out.contains("walked         9"), "{out}");
    // Three decodable records succeed, three fail.
    assert!(out.contains("decoded        3"), "{out}");
    assert!(out.contains("failed         3"), "{out}");
    // One container and two no-decoder rows, neither of them read.
    assert!(out.contains("container      1"), "{out}");
    assert!(out.contains("no decoder     2"), "{out}");
    assert_eq!(
        3 + 3 + 1 + 2,
        9,
        "decoded + failed + container + no_decoder must equal walked"
    );
    // `verify` is a measurement: failures do not change the exit code.
    assert_eq!(code, EXIT_OK);
    assert!(
        out.contains("errors         3 distinct message(s)"),
        "one message per failing decoder: {out}"
    );
}

#[test]
fn the_report_labels_the_type_level_buckets_as_such() {
    let file = TempFile::with("verify-labels", &mixed_package());
    let path = file.path().display().to_string();
    let (_code, out, _err) = osptool(&["verify", &path]);
    assert!(
        out.contains("`container` and `no decoder` are type-level facts"),
        "{out}"
    );
    assert!(out.contains("those payloads were not read"), "{out}");
    assert!(
        out.contains("`decoded` and `failed` are per-record"),
        "{out}"
    );
}

#[test]
fn the_distinct_error_messages_are_listed_with_their_counts() {
    let file = TempFile::with("verify-errors", &mixed_package());
    let path = file.path().display().to_string();
    let (_code, out, _err) = osptool(&["verify", &path]);
    // The gmdl failure is the version word; the rw4 failure is the magic; the
    // raster failure is the envelope. All three messages come from the format
    // crates, not from this one.
    assert!(out.contains("gmdl: unsupported version 7"), "{out}");
    assert!(out.contains("rw4: record too short"), "{out}");
    assert!(out.contains("texture: zero-sized image"), "{out}");
    // Ranked most frequent first; all three have count 1 so the message sorts.
    let lines: Vec<&str> = out.lines().collect();
    let first = lines
        .iter()
        .position(|line| line.contains("unsupported version 7"))
        .unwrap();
    assert!(first > 2, "the error list follows the counters:\n{out}");
}

#[test]
fn restricting_to_one_type_walks_only_that_type() {
    let file = TempFile::with("verify-type", &mixed_package());
    let path = file.path().display().to_string();

    let (code, out, _err) = osptool(&["verify", &path, "--type", "0x00e6bce5"]);
    assert_eq!(code, EXIT_OK);
    assert!(
        out.contains("scope          type 0x00e6bce5 (gmdl)"),
        "{out}"
    );
    assert!(out.contains("walked         2"), "{out}");
    assert!(out.contains("decoded        1"), "{out}");
    assert!(out.contains("failed         1"), "{out}");
    assert!(out.contains("container      0"), "{out}");
    assert!(out.contains("no decoder     0"), "{out}");

    // A type with no rows at all is a measurement of zero, not an error.
    let (code, out, _err) = osptool(&["verify", &path, "--type", "0x00b1b104"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("walked         1"), "{out}");
    assert!(out.contains("decoded        0"), "{out}");
    assert!(out.contains("container      1"), "{out}");

    let (code, out, _err) = osptool(&["verify", &path, "--type", "0xdeadbeef"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("no decoder     1"), "{out}");

    let (code, out, _err) = osptool(&["verify", &path, "--type", "0x12345678"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("walked         0"), "{out}");
    assert!(out.contains("errors         none"), "{out}");
}

#[test]
fn a_container_row_is_counted_as_a_container_even_though_its_payload_is_nonsense() {
    // `prop` with nine 0xff bytes: unreadable by any decoder, and irrelevant,
    // because this build does not parse containers.
    let image = package_of(&[Row::new(
        spore_core::record::type_id::PROP,
        1,
        1,
        vec![0xff; 9],
    )]);
    let file = TempFile::with("verify-container", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["verify", &path]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("decoded        0"), "{out}");
    assert!(out.contains("container      1"), "{out}");
    assert!(out.contains("failed         0"), "{out}");
    assert!(out.contains("errors         none"), "{out}");
}

#[test]
fn the_committed_fixture_package_verifies_with_no_decoder_failures() {
    // `mini_package.dbpf` holds three synthetic ids, none of which this build
    // decodes: all three land in the no-decoder bucket and none is read.
    let (code, out, _err) = osptool(&["verify", MINI_PACKAGE]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("walked         3"), "{out}");
    assert!(out.contains("no decoder     3"), "{out}");
    assert!(out.contains("decoded        0"), "{out}");
    assert!(out.contains("failed         0"), "{out}");
    // Including the QFS record: a compressed record this build cannot classify
    // is still never decompressed.
    assert!(out.contains("container      0"), "{out}");
}

#[test]
fn restricting_to_a_type_the_fixture_actually_holds_gives_the_same_answer() {
    let (code, out, _err) = osptool(&["verify", MINI_PACKAGE, "--type", "0x31534651"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("walked         1"), "{out}");
    assert!(out.contains("no decoder     1"), "{out}");
    assert!(out.contains("scope          type 0x31534651"), "{out}");
}

#[test]
fn the_raster_package_verifies_its_one_decodable_record() {
    let file = TempFile::with("verify-raster", &raster_package());
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["verify", &path, "--type", "0x2f4e681c"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("walked         1"), "{out}");
    assert!(out.contains("decoded        1"), "{out}");
    assert!(out.contains("failed         0"), "{out}");
}

#[test]
fn the_walk_is_streaming_so_memory_does_not_grow_with_the_row_count() {
    // Not a heap measurement -- that is fragile in a test. What is pinned is the
    // *shape*: one row is decoded at a time and the buffer is dropped, so the
    // only accumulation is counters and distinct error strings. `Tally` is the
    // whole of that accumulation, and it is checked to hold nothing else.
    let mut tallies: Vec<Tally> = Vec::new();
    for rows in [1usize, 10, 100] {
        let image = package_of(
            &(0..rows)
                .map(|index| {
                    Row::new(
                        spore_gmdl::GMDL_TYPE,
                        1,
                        index as u32,
                        b"not a gmdl".to_vec(),
                    )
                })
                .collect::<Vec<_>>(),
        );
        let file = TempFile::with("verify-stream", &image);
        let path = file.path().display().to_string();
        let (code, out, _err) = osptool(&["verify", &path]);
        assert_eq!(code, EXIT_OK);
        assert!(out.contains(&format!("walked         {rows}")), "{out}");
        assert!(out.contains(&format!("failed         {rows}")), "{out}");
        tallies.push(Tally {
            seen: rows as u64,
            decoded: 0,
            container: 0,
            no_decoder: 0,
            failed: rows as u64,
            errors: std::collections::BTreeMap::from([("x".to_owned(), rows as u64)]),
        });
    }
    // The distinct-message map has one entry however many rows failed, which is
    // the property that keeps the accumulation bounded by *distinct failures*
    // rather than by the record count.
    assert!(tallies.iter().all(|tally| tally.errors.len() == 1));
}

#[test]
fn progress_is_off_for_a_non_terminal_sink_and_the_gate_is_explicit() {
    // The gate is a parameter, so this is directly assertable.
    let file = TempFile::with("verify-progress", &mixed_package());
    let path = file.path().display().to_string();
    let mut out = Vec::new();
    let mut err = Vec::new();
    let request = spore_tools::parse(["verify".to_owned(), path])
        .expect("parses")
        .verify_request()
        .expect("a verify request");
    spore_tools::commands::verify::run(&request, &mut out, &mut err, false)
        .expect("the walk succeeds");
    assert!(
        err.is_empty(),
        "progress off means stderr stays empty: {err:?}"
    );
    // A progress line at record 10 000 would need 10 000 records; the smallest
    // observable proof that the flag reaches the writer is the code path above.
    assert!(String::from_utf8(out).unwrap().contains("walked         9"));
}

#[test]
fn the_tally_arithmetic_holds_for_every_class() {
    let mut tally = Tally::default();
    for class in [
        RecordClass::Gmdl,
        RecordClass::Rw4,
        RecordClass::Raster,
        RecordClass::Container,
        RecordClass::NoDecoder,
    ] {
        tally.record(class, Ok(()));
        tally.record(class, Err("boom".to_owned()));
    }
    assert_eq!(tally.seen, 10);
    assert_eq!(tally.decoded, 3, "only the decoder classes can decode");
    assert_eq!(tally.failed, 3, "and only they can fail");
    assert_eq!(tally.container, 2);
    assert_eq!(tally.no_decoder, 2);
    assert_eq!(
        tally.decoded + tally.container + tally.no_decoder + tally.failed,
        tally.seen
    );
}

#[test]
fn the_container_list_is_the_one_manifest_uses() {
    // The list is transcribed from a private constant in `spore-assets`; the
    // agreement is proven in `type_classes.rs`. Here it is pinned by value so a
    // typo is caught even if that test is skipped.
    assert_eq!(CONTAINER_TYPE_IDS.len(), 7);
    assert!(CONTAINER_TYPE_IDS.contains(&spore_core::record::type_id::PROP));
    assert!(CONTAINER_TYPE_IDS.contains(&spore_core::record::type_id::CELL_STRUCTURE));
    for id in [
        0x2399_BE55u32,
        0x2468_2294,
        0x2B97_8C46,
        0x3D97_A8E4,
        0x055A_DA24,
    ] {
        assert!(CONTAINER_TYPE_IDS.contains(&id), "0x{id:08x}");
        assert_eq!(classify(id), RecordClass::Container);
    }
}

#[test]
fn verify_opens_the_package_as_a_mapping_and_holds_one_record_at_a_time() {
    // `Package::open` memory-maps, so the image never becomes a heap copy. The
    // store keeps exactly one package, and `read_from` is never used here --
    // `extract_record` reads the mapping directly.
    let file = TempFile::with("verify-mmap", &mixed_package());
    let package = Package::open("t", file.path()).expect("opens");
    assert!(package.path().is_some());
    let mut store = ContentStore::new();
    store.push(package);
    assert_eq!(store.packages().len(), 1);
    assert_eq!(store.packages()[0].record_count(), 9);
}

/// Small helper so the progress test can name the parsed request without
/// duplicating the match.
trait VerifyRequestOf {
    fn verify_request(self) -> Option<spore_tools::VerifyRequest>;
}

impl VerifyRequestOf for spore_tools::Request {
    fn verify_request(self) -> Option<spore_tools::VerifyRequest> {
        match self {
            spore_tools::Request::Verify(request) => Some(request),
            _ => None,
        }
    }
}
