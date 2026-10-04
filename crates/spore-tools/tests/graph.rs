//! `graph`: the traversal, the dangling counts and the unresolved list.
//!
//! Built over synthetic packages, because the graph is `spore-assets`' and this
//! is a test of what `osptool graph` does with what it returns. The corpus run
//! lives in `real_package.rs` behind `#[ignore]`.

mod common;

use common::{osptool, package_of, Row};

use spore_tools::{EXIT_OK, EXIT_USAGE};

/// A package holding one `gmdl` record (the committed fixture) at a real gmdl
/// type id, plus a `raster` record at the same group and instance id — which is
/// exactly the adjacency `model->texture-set` is defined on.
fn model_package() -> Vec<u8> {
    package_of(&[
        Row::new(
            spore_gmdl::GMDL_TYPE,
            0x4061_6201,
            0x067a_0801,
            common::read_fixture("mini.gmdl"),
        ),
        Row::new(
            spore_texture::RASTER_TYPE,
            0x4061_6201,
            0x067a_0801,
            common::synthetic_raster(8, 8, 1, 1),
        ),
    ])
}

const MODEL_KEY: &str = "0x00e6bce5:0x40616201:0x067a0801";

#[test]
fn the_report_names_the_loaded_set_the_depth_and_the_kinds_it_built() {
    let file = common::temp_package("graph-report", &model_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["graph", MODEL_KEY, "--package", &path]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains(&format!("root          {MODEL_KEY}")), "{out}");
    assert!(out.contains("records       2 distinct identities"), "{out}");
    assert!(out.contains("depth         1"), "{out}");
    assert!(
        out.contains("gmdl->texture") && out.contains("gmdl->reference"),
        "the default builds every kind except sibling adjacency: {out}"
    );
    assert!(
        !out.contains("group-sibling"),
        "sibling adjacency is off unless asked for: {out}"
    );
}

#[test]
fn every_built_edge_kind_reports_its_evidence_grade() {
    let file = common::temp_package("graph-evidence", &model_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--json"]);
    assert_eq!(code, EXIT_OK, "{err}");
    let document = spore_tools::json::parse(&out).expect("valid JSON");
    let spore_tools::json::Json::Obj(pairs) = &document else {
        panic!("expected an object")
    };
    let edges = pairs
        .iter()
        .find(|(name, _)| name == "edges")
        .map(|(_, value)| value.clone())
        .expect("edges key");
    let spore_tools::json::Json::Arr(rows) = edges else {
        panic!("edges must be an array")
    };
    // All four kinds are listed, built or not, so a reader never has to infer
    // "absent" from a missing element.
    assert_eq!(rows.len(), 4);
    let by_kind: Vec<(String, String, bool)> = rows
        .iter()
        .map(|row| {
            let spore_tools::json::Json::Obj(fields) = row else {
                panic!("each row is an object")
            };
            let get = |name: &str| {
                fields
                    .iter()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| panic!("no key {name}"))
            };
            let spore_tools::json::Json::Str(kind) = get("kind") else {
                panic!("kind is a string")
            };
            let spore_tools::json::Json::Str(level) = get("evidence_level") else {
                panic!("evidence_level is a string")
            };
            let spore_tools::json::Json::Bool(built) = get("built") else {
                panic!("built is a boolean")
            };
            (kind, level, built)
        })
        .collect();
    assert!(by_kind.contains(&("gmdl->texture".to_owned(), "OBSERVED".to_owned(), true)));
    assert!(by_kind.contains(&("gmdl->reference".to_owned(), "VERIFIED".to_owned(), true)));
    assert!(by_kind.contains(&("model->texture-set".to_owned(), "INFERRED".to_owned(), true)));
    assert!(by_kind.contains(&("group-sibling".to_owned(), "INFERRED".to_owned(), false)));
}

#[test]
fn a_depth_bound_is_honoured_and_zero_returns_the_root_alone() {
    let file = common::temp_package("graph-depth", &model_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--depth", "0"]);
    assert_eq!(code, EXIT_OK, "{err}");
    let traversal = out
        .split("traversal")
        .nth(1)
        .expect("the traversal section");
    assert!(
        traversal.contains("1 record(s) within depth 0"),
        "{traversal}"
    );

    // One hop out of the fixture's model reaches two records: the `raster` at
    // the same group and instance (`model->texture-set`) and the record its
    // `refCount` key table names (`gmdl->reference`), which is in no package.
    let (_code, out, _err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--depth", "1"]);
    assert!(
        out.contains("3 record(s) within depth 1"),
        "root plus both kinds of neighbour: {out}"
    );
    assert!(
        out.contains("DANGLING: not in any loaded package"),
        "the unreferenced target is marked at the point it appears: {out}"
    );
}

#[test]
fn two_runs_over_the_same_package_print_the_same_bytes() {
    let file = common::temp_package("graph-determinism", &model_package());
    let path = file.path().display().to_string();
    let args = ["graph", MODEL_KEY, "--package", &path, "--depth", "3"];
    let (first_code, first, first_err) = osptool(&args);
    let (second_code, second, second_err) = osptool(&args);
    assert_eq!(first_code, EXIT_OK, "{first_err}");
    assert_eq!(second_code, EXIT_OK, "{second_err}");
    assert_eq!(
        first, second,
        "BFS order is ascending by ResourceKey, so two runs must agree byte for byte"
    );
}

#[test]
fn a_kinds_list_narrows_what_is_built_not_only_what_is_followed() {
    let file = common::temp_package("graph-kinds", &model_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&[
        "graph",
        MODEL_KEY,
        "--package",
        &path,
        "--kinds",
        "model->texture-set",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("edge kinds    model->texture-set"), "{out}");
    assert!(
        !out.contains("gmdl->texture     ") && !out.contains("gmdl->reference  "),
        "an unbuilt kind must not appear in the edge summary: {out}"
    );
    // Its JSON carries the unbuilt kinds as `built: false` rather than dropping
    // them, so the document is self-describing.
    let (_code, out, _err) = osptool(&[
        "graph",
        MODEL_KEY,
        "--package",
        &path,
        "--kinds",
        "model->texture-set",
        "--json",
    ]);
    assert!(out.contains("\"built\":false"), "{out}");
}

#[test]
fn an_unrecognised_edge_kind_is_refused_with_the_closed_vocabulary() {
    let file = common::temp_package("graph-badkind", &model_package());
    let path = file.path().display().to_string();
    for bad in ["gmdl->texture,typo", "GMDL->TEXTURE", "gmdl", " "] {
        let (code, _out, err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--kinds", bad]);
        assert_eq!(code, EXIT_USAGE, "`--kinds {bad}`: {err}");
        assert!(
            err.contains("group-sibling"),
            "the vocabulary is named: {err}"
        );
    }
}

#[test]
fn sibling_adjacency_is_built_only_when_it_is_asked_for() {
    let file = common::temp_package("graph-siblings", &model_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&[
        "graph",
        MODEL_KEY,
        "--package",
        &path,
        "--kinds",
        "gmdl->texture,group-sibling",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("group-sibling"), "{out}");
    assert!(
        out.contains("adjacency, not a named reference"),
        "the report distinguishes a declared reference from adjacency: {out}"
    );
}

#[test]
fn dependants_are_reported_and_their_absence_says_which_of_two_empties_it_is() {
    let file = common::temp_package("graph-dependants", &model_package());
    let path = file.path().display().to_string();
    let raster = "0x2f4e681c:0x40616201:0x067a0801";
    let (code, out, err) = osptool(&["graph", raster, "--package", &path, "--dependants"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("1 record(s) name it"),
        "the raster is the adjacency target of one model: {out}"
    );
    assert!(
        out.contains("model->texture-set 0x00e6bce5:0x40616201:0x067a0801"),
        "the dependants section names the edge kind, not just the count: {out}"
    );

    // The empty case, twice, and the two are different sentences: "nothing names
    // a record that is here" and "nothing names a record that is not here".
    let (_code, out, _err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--dependants"]);
    assert!(
        out.contains("no record in the loaded package(s) names"),
        "{out}"
    );
    assert!(
        !out.contains("the root is itself absent"),
        "the root IS here, so the second clause must not appear: {out}"
    );

    let (_code, out, _err) = osptool(&[
        "graph",
        "0x2f4e681c:0x40616201:0x000000ff",
        "--package",
        &path,
        "--dependants",
    ]);
    assert!(
        out.contains("the root is itself absent from the loaded packages"),
        "an empty list is ambiguous on its own, so both causes are stated: {out}"
    );
}

#[test]
fn a_root_that_is_not_in_the_package_is_still_walked_and_says_so() {
    // Deliberately NOT exit 3. A record can be *named* by records that are
    // loaded while itself being absent, and "which records reference this
    // missing texture?" is the question this command exists to answer. So an
    // absent root is a report, not a miss, and it says so on the first line that
    // matters.
    let file = common::temp_package("graph-absent", &model_package());
    let path = file.path().display().to_string();
    let absent = "0x2f4e681c:0x40616201:0x00000042";
    let (code, out, err) = osptool(&["graph", absent, "--package", &path, "--depth", "0"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("root present  no"), "{out}");
    assert!(out.contains(&format!("0  {absent}  DANGLING")), "{out}");
}

#[test]
fn a_package_flag_is_required_and_may_be_repeated() {
    let (code, _out, err) = osptool(&["graph", MODEL_KEY]);
    assert_eq!(code, EXIT_USAGE, "{err}");
    assert!(err.contains("`--package` needs a value"), "{err}");

    let file = common::temp_package("graph-repeat", &model_package());
    let path = file.path().display().to_string();
    // Repeating accumulates rather than last-wins: the loaded set decides what
    // resolves, so the second package is not an override, it is more context.
    let (code, out, err) = osptool(&[
        "graph",
        MODEL_KEY,
        "--package",
        &path,
        "--package",
        &path,
        "--depth",
        "0",
    ]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(out.contains("records       2 distinct"), "{out}");
}

#[test]
fn the_row_limit_caps_the_listing_and_says_how_many_it_dropped() {
    let file = common::temp_package("graph-limit", &model_package());
    let path = file.path().display().to_string();
    let (code, out, err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--limit", "1"]);
    assert_eq!(code, EXIT_OK, "{err}");
    assert!(
        out.contains("2 further record(s) within depth, not shown"),
        "the traversal reaches three records and the cap shows one: {out}"
    );

    // And zero means every row, not no rows — in every section, since a cap that
    // means "none" in one list and "all" in another is not a cap.
    let (_code, all, _err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--limit", "0"]);
    assert!(all.contains("3 record(s) within depth 1"), "{all}");
    assert!(!all.contains("further record(s) within depth"), "{all}");
    assert!(!all.contains("further dangling"), "{all}");
    assert!(!all.contains("further dependant(s)"), "{all}");
    // The dangling section still prints its evidence, not just its count.
    assert!(
        all.contains("-> 0xe5bce600:0x22222222:0x11111111")
            || all.contains("gmdl->reference              1"),
        "`--limit 0` must print the dangling sample, not only the count: {all}"
    );
}

#[test]
fn each_section_is_capped_independently() {
    // A traversal cap must not silently become the dangling cap: they are
    // separate lists with separate sizes, and passing one number for `shown`
    // rather than for the request's limit silently capped the wrong one.
    let file = common::temp_package("graph-independent", &model_package());
    let path = file.path().display().to_string();
    let (_code, out, _err) = osptool(&["graph", MODEL_KEY, "--package", &path, "--limit", "1"]);
    assert!(
        out.contains("further record(s) within depth"),
        "the traversal is capped: {out}"
    );
    assert!(
        !out.contains("further dangling"),
        "the dangling list holds one row and must not claim a cap: {out}"
    );
}
