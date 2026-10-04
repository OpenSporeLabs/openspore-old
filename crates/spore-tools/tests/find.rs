//! `find`: a hit names the right package and sizes; a miss exits 3 and names
//! the record.

mod common;

use common::{gmdl_package, osptool, synthetic_raster, Row, MINI_PACKAGE};
use spore_assets::ContentStore;
use spore_assets::Package;
use spore_tools::{json::Json, EXIT_NOT_FOUND, EXIT_OK};

const TSTX: &str = "0x58545354:0x11111111:0x22222222";

#[test]
fn a_hit_reports_the_answering_package_and_the_index_extent() {
    let (code, out, err) = osptool(&["find", MINI_PACKAGE, TSTX]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(err.is_empty(), "nothing to diagnose: {err}");
    assert!(out.contains(TSTX), "{out}");
    assert!(out.contains("mini_package"), "{out}");

    // The numbers below are the fixture's, read from the index rather than
    // re-derived: 16 stored, 16 in memory, uncompressed.
    assert!(out.contains("stored size   16 bytes"), "{out}");
    assert!(out.contains("memory size   16 bytes"), "{out}");
    assert!(out.contains("offset        0x000000b8"), "{out}");
    assert!(out.contains("compression   none"), "{out}");
    assert!(out.contains("bytes read    16 bytes"), "{out}");
    // One package was searched, and it is priority 0.
    assert!(out.contains("(priority 0 of 1)"), "{out}");
}

#[test]
fn a_compressed_record_reports_its_compression_word() {
    let (code, out, _err) = osptool(&["find", MINI_PACKAGE, "0x31534651:0x33333333:0x44444444"]);
    assert_eq!(code, EXIT_OK);
    // Stored 421 bytes, 412 in memory: the QFS round trip is visible in the index.
    assert!(out.contains("compression   qfs"), "{out}");
    assert!(out.contains("stored size   421 bytes"), "{out}");
    assert!(out.contains("bytes read    412 bytes"), "{out}");
}

#[test]
fn a_decodable_record_reports_a_decoded_verdict_with_its_numbers() {
    let image = gmdl_package();
    let file = common::temp_package("find-gmdl", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["find", &path, "0x00e6bce5:0x40616201:0x067a0801"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("decode        decoded"), "{out}");
    // The numbers are the fixture's, and they came out of a decoder.
    assert!(out.contains("gmdl version 8"), "{out}");
    assert!(out.contains("1 mesh"), "{out}");
    assert!(out.contains("50 vertices"), "{out}");
    assert!(out.contains("153 indices"), "{out}");
    // And the type is named, so the reader does not have to know the id.
    assert!(out.contains("0x00e6bce5 (gmdl)"), "{out}");
    assert!(out.contains("CellImages"), "{out}");
}

#[test]
fn a_record_with_no_decoder_says_so_instead_of_claiming_a_decode() {
    // `plt`: named in the type table, no decoder in this build.
    let image = common::raster_package();
    let file = common::temp_package("find-plt", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["find", &path, "0x011989b7:0x406b6a00:0x067a0902"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("decode        not decoded"), "{out}");
    assert!(out.contains("type-level only"), "{out}");
    assert!(out.contains("0x011989b7 (plt)"), "{out}");
    // The critical negative: no mesh count, no section count, no dimensions.
    for forbidden in ["vertices", "indices", "mip", "section row"] {
        assert!(
            !out.contains(forbidden),
            "a no-decoder verdict must not print `{forbidden}`: {out}"
        );
    }
}

#[test]
fn a_container_record_is_distinguished_from_a_plain_unknown_type() {
    let image = common::raster_package();
    let file = common::temp_package("find-prop", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["find", &path, "0x00b1b104:0x00000000:0x00000001"]);
    assert_eq!(code, EXIT_OK);
    assert!(
        out.contains("is a container this build does not decode"),
        "{out}"
    );
}

#[test]
fn a_present_record_that_does_not_decode_still_exits_zero() {
    // A gmdl-typed record whose payload is junk: found, unreadable, verdict
    // printed. `find` locating a record is a success; `describe` is where a
    // decode failure is fatal.
    let image = common::package_of(&[Row::new(
        spore_gmdl::GMDL_TYPE,
        1,
        2,
        b"not a gmdl at all".to_vec(),
    )]);
    let file = common::temp_package("find-bad-gmdl", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["find", &path, "0x00e6bce5:0x00000001:0x00000002"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("decode        decode FAILED"), "{out}");
    assert!(out.contains("gmdl:"), "{out}");
}

#[test]
fn a_raster_that_decodes_reports_its_dimensions_and_one_chain() {
    // The corpus refuted the two-layer reading, so the summary states the chain
    // count that is actually established — one — and says "one chain" rather than
    // quoting a derived layer count of 2.
    let image = common::package_of(&[Row::new(
        spore_texture::RASTER_TYPE,
        0x4066_2900,
        0x067a_0901,
        synthetic_raster(32, 16, 3),
    )]);
    let file = common::temp_package("find-raster", &image);
    let path = file.path().display().to_string();
    let (code, out, _err) = osptool(&["find", &path, "0x2f4e681c:0x40662900:0x067a0901"]);
    assert_eq!(code, EXIT_OK);
    assert!(out.contains("decode        decoded"), "{out}");
    assert!(out.contains("raster 32x16 base level"), "{out}");
    assert!(out.contains("3 mip level(s) in one chain"), "{out}");
    assert!(
        !out.contains("2 layer(s)"),
        "the derived two-layer count is the refuted premise: {out}"
    );
}

#[test]
fn the_json_view_is_valid_json_with_the_documented_keys() {
    let (code, out, _err) = osptool(&["find", MINI_PACKAGE, TSTX, "--json"]);
    assert_eq!(code, EXIT_OK);
    let value = spore_tools::json::parse(out.trim()).expect("valid JSON");
    let Json::Obj(pairs) = &value else {
        panic!("expected an object, got {value}")
    };
    let keys: Vec<&str> = pairs.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(
        keys,
        vec![
            "found",
            "key",
            "package",
            "package_priority",
            "packages_searched",
            "type_id",
            "type_name",
            "group_id",
            "group_name",
            "instance_id",
            "offset",
            "stored_size",
            "memory_size",
            "compression",
            "compressed",
            "bytes_read",
            "decode",
            "decode_detail",
        ],
        "key order is fixed"
    );
    assert!(
        value.render().contains(TSTX),
        "the key is rendered in the canonical T:G:I spelling: {value}"
    );
}

#[test]
fn a_miss_exits_three_and_the_message_names_the_record() {
    let (code, out, err) = osptool(&["find", MINI_PACKAGE, "0xdeadbeef:0x11111111:0x22222222"]);
    assert_eq!(code, EXIT_NOT_FOUND);
    assert!(out.is_empty(), "a miss writes nothing to stdout: {out}");
    assert!(
        err.contains("no record `0xdeadbeef:0x11111111:0x22222222`"),
        "{err}"
    );
    assert!(err.contains("1 loaded package(s)"), "{err}");
}

#[test]
fn a_miss_offers_the_candidates_the_store_already_computed() {
    // Right type, wrong instance: `ContentStore::find` collects up to eight
    // near misses from the first package holding that type. This tool prints
    // them; it does not search for them.
    let (code, _out, err) = osptool(&["find", MINI_PACKAGE, "0x58545354:0x11111111:0xdeadbeef"]);
    assert_eq!(code, EXIT_NOT_FOUND);
    assert!(err.contains("near-miss candidates"), "{err}");
    assert!(err.contains("0x58545354:0x11111111:0x22222222"), "{err}");
}

#[test]
fn the_miss_message_is_the_librarys_own_wording() {
    // Pinned so a change to `AssetError`'s Display, or to this tool's framing of
    // it, is a deliberate act.
    let (_code, _out, err) = osptool(&["find", MINI_PACKAGE, "0xdeadbeef:1:2"]);
    assert!(err.starts_with("osptool: "), "{err}");
    assert!(
        err.contains(
            "no record `0xdeadbeef:0x00000001:0x00000002` in any of the 1 loaded package(s)"
        ),
        "{err}"
    );
}

#[test]
fn a_package_is_opened_read_only_so_a_second_find_sees_the_same_bytes() {
    let store_path = MINI_PACKAGE;
    let before = std::fs::read(store_path).expect("fixture is readable");
    let _ = osptool(&["find", store_path, TSTX]);
    let after = std::fs::read(store_path).expect("fixture is readable");
    assert_eq!(before, after, "find must not modify the package");
    // And the crate really does open it as a mapping.
    let package = Package::open("mini", store_path).unwrap();
    assert!(package.path().is_some());
    let mut store = ContentStore::new();
    store.push(package);
    assert_eq!(store.len(), 1);
}
