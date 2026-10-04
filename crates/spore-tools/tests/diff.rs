//! `diff`: revision sets, not first-row-wins.
//!
//! The 264/276 case the command exists for is built here: one package holding
//! two revisions of the *same* identity, against one holding one.

mod common;

use common::{osptool, package_of, Row};

use spore_tools::{EXIT_OK, EXIT_USAGE};

const TYPE: u32 = spore_core::record::type_id::CELL_GLOBALS;
const GROUP: u32 = 0x4061_6201;
const KEY: &str = "0x2a3ce5b7:0x40616201:0x00000001";

/// A package over `(type, group, instance, payload)` rows.
fn package(rows: &[(u32, u32, u32, Vec<u8>)]) -> Vec<u8> {
    let built: Vec<Row> = rows
        .iter()
        .map(|(t, g, i, payload)| Row::new(*t, *g, *i, payload.clone()))
        .collect();
    package_of(&built)
}

#[test]
fn a_package_with_two_revisions_of_one_identity_is_not_reported_as_identical() {
    // The stock-install case: a 264-byte base revision and a 276-byte patched
    // one at the SAME (type, group, instance). A comparison that keeps only the
    // first index row would call this record unchanged.
    let base = vec![0xAA; 264];
    let patched = vec![0xBB; 276];
    let left = package(&[(TYPE, GROUP, 1, base)]);
    let right = package(&[(TYPE, GROUP, 1, patched.clone())]);

    let left_file = common::temp_package("diff-left", &left);
    let right_file = common::temp_package("diff-right", &right);
    let (code, out, err) = osptool(&[
        "diff",
        &left_file.path().display().to_string(),
        &right_file.path().display().to_string(),
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("changed        1"), "{out}");
    assert!(out.contains(KEY), "{out}");
    assert!(out.contains("stored=264"), "{out}");
    assert!(out.contains("stored=276"), "{out}");
    assert!(
        out.contains("identical      0"),
        "nothing about these two packages is identical: {out}"
    );
}

#[test]
fn two_revisions_against_one_revision_is_a_change_of_the_revision_set() {
    let left = package(&[
        (TYPE, GROUP, 1, vec![0xAA; 264]),
        (TYPE, GROUP, 1, vec![0xBB; 276]),
    ]);
    let right = package(&[(TYPE, GROUP, 1, vec![0xAA; 264])]);
    let left_file = common::temp_package("diff-two-left", &left);
    let right_file = common::temp_package("diff-two-right", &right);
    let (code, out, err) = osptool(&[
        "diff",
        &left_file.path().display().to_string(),
        &right_file.path().display().to_string(),
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("changed        1"), "{out}");
    assert!(
        out.contains("left  offset=") && out.contains("right offset="),
        "both sides of the revision set are printed: {out}"
    );
}

#[test]
fn added_and_removed_records_are_counted_on_the_right_sides() {
    let left = package(&[(TYPE, GROUP, 1, vec![1; 8]), (TYPE, GROUP, 2, vec![2; 8])]);
    let right = package(&[(TYPE, GROUP, 2, vec![2; 8]), (TYPE, GROUP, 3, vec![3; 8])]);
    let left_file = common::temp_package("diff-ar-left", &left);
    let right_file = common::temp_package("diff-ar-right", &right);
    let (code, out, err) = osptool(&[
        "diff",
        &left_file.path().display().to_string(),
        &right_file.path().display().to_string(),
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("only in left   1"), "{out}");
    assert!(out.contains("only in right  1"), "{out}");
    assert!(out.contains("changed        0"), "{out}");
    assert!(out.contains("identical      1"), "{out}");
    assert!(out.contains("0x00000001"), "{out}");
    assert!(out.contains("0x00000003"), "{out}");
}

#[test]
fn a_package_compared_with_itself_is_all_identical_and_reads_no_payloads() {
    // Without `--bytes` the comparison is a pure function of the index, so it
    // works on an image whose payloads are unreadable. Proven by pointing the
    // rows past the end of the file: the report must still come out complete.
    let mut broken = package(&[(TYPE, GROUP, 1, vec![7; 8])]);
    // Rewrite the stored-size word so it claims far more than the file holds.
    let offset = 96 + 4 + 16;
    broken[offset..offset + 4].copy_from_slice(&(0x0010_0000u32 | 0x8000_0000).to_le_bytes());
    let file = common::temp_package("diff-self", &broken);
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["diff", &path, &path]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("identical      1"), "{out}");
    assert!(
        out.contains("index rows only"),
        "and the report says it did not read a payload: {out}"
    );
}

#[test]
fn bytes_mode_reports_records_the_index_calls_identical_whose_bytes_differ() {
    // Two rows, identical index words, different content. Impossible for a
    // well-formed package, and exactly what `--bytes` exists to catch.
    let left = package(&[(TYPE, GROUP, 1, vec![0xAA; 32])]);
    let mut right = left.clone();
    // Replace the payload byte of the right package in place.
    let payload_at = 96 + 4 + 28;
    right[payload_at] = 0x5A;
    // Both index rows are byte-identical: only the payload moved.
    assert_eq!(
        left[..payload_at],
        right[..payload_at],
        "the index must be identical for this test to mean anything"
    );
    let left_file = common::temp_package("diff-bytes-left", &left);
    let right_file = common::temp_package("diff-bytes-right", &right);
    let (code, out, err) = osptool(&[
        "diff",
        &left_file.path().display().to_string(),
        &right_file.path().display().to_string(),
        "--bytes",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("index says identical, bytes differ: 1"),
        "{out}"
    );
    assert!(out.contains("same-index-different-bytes"), "{out}");
    assert!(
        out.contains("the DBPF index does not determine"),
        "the consequence is stated, not just the count: {out}"
    );
}

#[test]
fn the_comparison_is_deterministic_and_states_which_level_it_used() {
    let left = package(&[(TYPE, GROUP, 1, vec![1; 8]), (TYPE, GROUP, 2, vec![2; 8])]);
    let right = package(&[(TYPE, GROUP, 2, vec![9; 8])]);
    let left_file = common::temp_package("diff-det-left", &left);
    let right_file = common::temp_package("diff-det-right", &right);
    let path_left = left_file.path().display().to_string();
    let path_right = right_file.path().display().to_string();
    let args = ["diff", &path_left, &path_right];
    let (first_code, first, first_err) = osptool(&args);
    let (second_code, second, second_err) = osptool(&args);
    assert_eq!(first_code, EXIT_OK, "{first_err}");
    assert_eq!(second_code, EXIT_OK, "{second_err}");
    assert_eq!(first, second);
    assert!(first.contains("index rows only"), "{first}");

    let (_code, bytes, _err) = osptool(&["diff", &path_left, &path_right, "--bytes"]);
    assert!(
        bytes.contains("index rows AND decompressed content"),
        "the report says which comparison it performed: {bytes}"
    );
}

#[test]
fn the_json_document_is_json_and_every_difference_carries_its_revisions() {
    let left = package(&[(TYPE, GROUP, 1, vec![0xAA; 264])]);
    let right = package(&[(TYPE, GROUP, 1, vec![0xBB; 276])]);
    let left_file = common::temp_package("diff-json-left", &left);
    let right_file = common::temp_package("diff-json-right", &right);
    let (code, out, err) = osptool(&[
        "diff",
        &left_file.path().display().to_string(),
        &right_file.path().display().to_string(),
        "--json",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    let document = spore_tools::json::parse(&out).expect("valid JSON on stdout");
    let spore_tools::json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    let differences = pairs
        .iter()
        .find(|(k, _)| k == "differences")
        .map(|(_, v)| v.clone())
        .expect("differences key");
    let spore_tools::json::Json::Arr(rows) = differences else {
        panic!("differences must be an array")
    };
    assert_eq!(rows.len(), 1);
    let spore_tools::json::Json::Obj(row) = &rows[0] else {
        panic!("each row is an object")
    };
    let get = |name: &str| {
        row.iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| panic!("no key {name}"))
    };
    assert_eq!(get("kind"), spore_tools::json::Json::from("changed"));
    assert_eq!(get("record"), spore_tools::json::Json::from(KEY));
    for side in ["left", "right"] {
        let spore_tools::json::Json::Arr(entries) = get(side) else {
            panic!("{side} must be an array")
        };
        assert_eq!(entries.len(), 1, "{side}");
    }
    // And stdout carried only the document.
    assert!(!out.contains("only in left"), "{out}");
}

#[test]
fn two_packages_are_required_and_differences_are_not_a_failure() {
    let file = common::temp_package("diff-args", &package(&[(TYPE, GROUP, 1, vec![1; 8])]));
    let path = file.path().display().to_string();
    let (code, _out, err) = osptool(&["diff"]);
    assert_eq!(code, EXIT_USAGE, "{err}");
    let (code, _out, err) = osptool(&["diff", &path]);
    assert_eq!(code, EXIT_USAGE, "{err}");
    // Differences are the output, so this is exit 0 even though the packages
    // certainly differ.
    let other = common::temp_package("diff-args-b", &package(&[(TYPE, GROUP, 9, vec![2; 8])]));
    let (code, out, err) = osptool(&["diff", &path, &other.path().display().to_string()]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("only in right  1"), "{out}");
}
