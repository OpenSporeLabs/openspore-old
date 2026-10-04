//! Real-corpus measurement for the resource dependency graph.
//!
//! **Every test here is `#[ignore]`d and every one skips cleanly when the game is
//! not installed.** They read `SPORE/Data/Spore_Content.package` from the
//! operator's disk; they never copy, embed or commit a Spore byte, and they hold
//! no asset fixture. Without `SPORE/` they print a skip line and return, so a
//! hermetic checkout runs this file and gets nothing rather than a failure.
//!
//! Run with:
//!
//! ```text
//! cargo test -p spore-assets --release --test graph_corpus -- --ignored --nocapture
//! ```
//!
//! # Every number below is measured, and asserted exactly
//!
//! The assertions are equalities against counts measured over the installed
//! corpus, not tolerances. Three of them distinguish **declared references** from
//! **stored edges**, because those are different facts: a record may name the same
//! target more than once, and 5 265 of the 16 628 texture-set entries in this
//! package are exactly that. Collapsing the two numbers would be the same class of
//! error as reporting one graph node where the record says two.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use spore_assets::graph::{EdgeKind, GraphOptions, ResourceGraph, SkipReason};
use spore_assets::{ContentStore, Package};
use spore_core::record::type_id;
use spore_core::ResourceKey;

/// The package every number in this file is measured over.
const PACKAGE: &str = "SPORE/Data/Spore_Content.package";

/// Index rows in `Spore_Content.package`. The denominator for everything below.
const RECORDS: usize = 17_119;

/// Opens the content package, or `None` when the game is not installed.
///
/// Returning `None` rather than panicking is the whole point: the repository's
/// hermetic test run must not require a 995 MB game install, and "the corpus is
/// absent" is a non-finding, not a test failure.
fn content_store() -> Option<ContentStore> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .ok()?;
    let path = root.join(PACKAGE);
    if !path.exists() {
        println!("SKIP: {} is not installed", PACKAGE);
        return None;
    }
    println!("corpus: {}", path.display());
    let mut store = ContentStore::new();
    store.push(Package::open("Spore_Content", path).expect("package must open"));
    Some(store)
}

/// Builds the default graph over the corpus and prints the summary.
fn measured_graph() -> Option<ResourceGraph> {
    let store = content_store()?;
    let graph = ResourceGraph::build(&store);
    println!(
        "known records: {}   edges: {}",
        graph.known_records(),
        graph.edge_count()
    );
    for row in graph.edge_summary() {
        println!(
            "  {:<20} declared={:<6} edges={:<6} dangling={:<5} grade={}",
            row.kind.as_str(),
            row.declared,
            row.edges,
            row.dangling,
            row.level
        );
    }
    Some(graph)
}

/// Counts the `unresolved` entries carrying `predicate`, by group id.
fn skips_by_group(
    graph: &ResourceGraph,
    predicate: impl Fn(&SkipReason) -> bool,
) -> BTreeMap<u32, usize> {
    let mut counts = BTreeMap::new();
    for entry in graph.unresolved() {
        if predicate(&entry.reason) {
            *counts.entry(entry.key.group_id).or_insert(0) += 1;
        }
    }
    counts
}

#[test]
#[ignore = "needs SPORE/; run with --ignored --nocapture"]
fn the_corpus_is_seventeen_thousand_records() {
    let Some(store) = content_store() else {
        return;
    };
    assert_eq!(store.packages()[0].record_count(), RECORDS);
}

#[test]
#[ignore = "needs SPORE/; run with --ignored --nocapture"]
fn every_reference_table_key_resolves_and_the_graph_reads_it_big_endian() {
    let Some(graph) = measured_graph() else {
        return;
    };

    // ---- gmdl -> rw4, from the `refCount` key table ----
    // 1510 of the 4209 gmdl records carry at least one key; the other 2699
    // declare none. 7989 keys are declared and they collapse to 6076 distinct
    // edges, so records repeat targets.
    assert_eq!(graph.declared_entry_count(EdgeKind::GmdlReference), 7_989);
    assert_eq!(graph.edge_count_of_kind(EdgeKind::GmdlReference), 6_076);

    // Every single one resolves. This is the measurement behind
    // `claims::REFERENCE_TABLE_ENDIANNESS` and behind grading the kind
    // `Verified`: read little-endian, all 7989 are byte-swapped and none resolve.
    assert_eq!(graph.dangling_of_kind(EdgeKind::GmdlReference).len(), 0);

    let mut targets_by_type: BTreeMap<u32, usize> = BTreeMap::new();
    for edge in graph.known_keys().flat_map(|key| graph.neighbours(&key)) {
        if edge.kind == EdgeKind::GmdlReference {
            *targets_by_type.entry(edge.to.type_id).or_insert(0) += 1;
        }
    }
    assert_eq!(
        targets_by_type,
        BTreeMap::from([(type_id::RW4, 6_076)]),
        "every measured reference-table key names an rw4 record and nothing else"
    );

    // ---- gmdl -> raster, from the 0x20D texture sets ----
    assert_eq!(graph.declared_entry_count(EdgeKind::GmdlTexture), 16_628);
    assert_eq!(graph.edge_count_of_kind(EdgeKind::GmdlTexture), 11_363);

    // ---- co-membership adjacency ----
    assert_eq!(
        graph.declared_entry_count(EdgeKind::ModelTextureSet),
        13_682
    );
    assert_eq!(graph.edge_count_of_kind(EdgeKind::ModelTextureSet), 13_682);
    assert_eq!(
        graph.dangling_of_kind(EdgeKind::ModelTextureSet).len(),
        0,
        "both endpoints came out of the index, so a dangling adjacency would be a bug"
    );
    assert_eq!(graph.edge_count(), 11_363 + 6_076 + 13_682);
}

#[test]
#[ignore = "needs SPORE/; run with --ignored --nocapture"]
fn one_hundred_and_thirty_two_texture_references_dangle_and_are_all_the_same_value() {
    let Some(graph) = measured_graph() else {
        return;
    };

    // The only dangling references in the whole corpus. Every one names group
    // 0x00000000 and instance 0x552A5077, and that instance appears in **no** row
    // of the 17 119-row index. They are kept, counted and reported: dropping them
    // would turn "the record points at something we cannot find" into "the record
    // points at nothing".
    let dangling = graph.dangling_of_kind(EdgeKind::GmdlTexture);
    assert_eq!(dangling.len(), 132);

    let sources: BTreeSet<ResourceKey> = dangling.iter().map(|entry| entry.from).collect();
    assert_eq!(
        sources.len(),
        132,
        "one per model, not 132 from a few records"
    );

    let groups: BTreeSet<u32> = sources.iter().map(|key| key.group_id).collect();
    assert_eq!(
        groups,
        BTreeSet::from([0x4061_6201u32, 0x4061_6202, 0x4061_7100]),
        "all three cell-stage model groups, 44 records each"
    );
    for entry in dangling {
        assert_eq!(entry.to.type_id, type_id::RASTER);
        assert_eq!(entry.to.group_id, 0x0000_0000);
        assert_eq!(entry.to.instance_id, 0x552A_5077);
    }

    // No model names the same dangling target twice, so the declared count and the
    // edge count agree here even though they disagree globally.
    assert_eq!(
        graph.missing_reference_counts()[&EdgeKind::GmdlTexture],
        132
    );
    assert_eq!(
        graph.missing_reference_counts()[&EdgeKind::GmdlReference],
        0,
        "a verified kind with a dangling reference would falsify its grade"
    );
}

#[test]
#[ignore = "needs SPORE/; run with --ignored --nocapture"]
fn no_model_fails_to_read_and_the_six_hundred_nineteen_partial_reads_are_named() {
    let Some(graph) = measured_graph() else {
        return;
    };

    // Every gmdl in this corpus decodes as far as its material-info table. The 619
    // records that name the undocumented shader-data id 0x218 stop *inside* that
    // table with their geometry already validated; nothing stops at or before the
    // mesh table, which the recovering parse treats as a hard error.
    assert_eq!(
        skips_by_group(&graph, |reason| matches!(
            reason,
            SkipReason::WalkFailed { .. }
        )),
        BTreeMap::new(),
    );
    assert!(graph
        .unresolved()
        .iter()
        .all(|entry| entry.key.type_id == type_id::GMDL));

    // The 619 partial reads, by group. Four creature-model groups at 97 each, two
    // more at 97 each, and 37 of the 67 palette-model records.
    assert_eq!(
        skips_by_group(&graph, |reason| matches!(
            reason,
            SkipReason::PartialTextureSet { .. }
        )),
        BTreeMap::from([
            (0x4062_6202u32, 97),
            (0x4062_6203, 97),
            (0x4062_7100, 97),
            (0x4062_7101, 97),
            (0x406B_6200, 37),
            (0x4162_6202, 97),
            (0x4162_6203, 97),
        ]),
    );
    assert!(graph.unresolved().iter().any(|entry| matches!(
        &entry.reason,
        SkipReason::PartialTextureSet { detail } if detail.contains("0x218")
    )));

    // Exactly one real model declares no texture set at all. That is a *finding*:
    // its walk closed on the record end, so the empty set is observed, not assumed.
    let empty = skips_by_group(&graph, |reason| *reason == SkipReason::NoTextureSetDeclared);
    assert_eq!(empty, BTreeMap::from([(0x4063_7E03u32, 1)]));
}

#[test]
#[ignore = "needs SPORE/; run with --ignored --nocapture"]
fn two_builds_over_the_corpus_produce_identical_graphs_and_traversals() {
    let Some(store) = content_store() else {
        return;
    };
    let first = ResourceGraph::build(&store);
    let second = ResourceGraph::build(&store);
    assert_eq!(first.edge_summary(), second.edge_summary());
    assert_eq!(first.unresolved(), second.unresolved());
    assert_eq!(first.known_records(), second.known_records());
    assert_eq!(
        first.missing_references().collect::<Vec<_>>(),
        second.missing_references().collect::<Vec<_>>()
    );

    // A representative root: a vehicle model. Depth 6 reaches 9 records and stops:
    // the model, the five `rw4` records its reference table names, and the three
    // rasters of its texture family. The `rw4` records have no outgoing edges
    // because no section payload is decoded, and the rasters declare nothing.
    let root = first_model_in_group(&store, 0x4064_6201);
    println!("traversal from {root}:");
    let walk = first.traverse(&root, 6, &[]);
    for entry in &walk {
        println!("  depth {} {}", entry.depth, entry.key);
    }
    assert_eq!(walk, second.traverse(&root, 6, &[]));
    assert_eq!(walk.len(), 9);
    assert_eq!(walk[0].key, root);
    assert_eq!(walk[0].depth, 0);
    assert!(walk[1..].iter().all(|entry| entry.depth == 1));
    let reached: BTreeSet<u32> = walk.iter().map(|entry| entry.key.type_id).collect();
    assert_eq!(
        reached,
        BTreeSet::from([type_id::GMDL, type_id::RW4, type_id::RASTER]),
        "a model reaches its geometry, its referenced containers and its textures"
    );

    // The two kinds a loader follows, and nothing else.
    let declared = first.traverse(&root, 6, &[EdgeKind::GmdlTexture, EdgeKind::GmdlReference]);
    assert_eq!(
        declared.len(),
        9,
        "the co-membership edges add nothing here"
    );
    assert!(declared.iter().all(|entry| entry.depth <= 1));
}

#[test]
#[ignore = "needs SPORE/; run with --ignored --nocapture"]
fn sibling_adjacency_is_seventy_seven_thousand_weak_edges_in_nine_families() {
    let Some(store) = content_store() else {
        return;
    };
    // Off by default. Built here only to measure it, because a number nobody has
    // measured is how a "cheap" adjacency rule quietly becomes load-bearing.
    let graph = ResourceGraph::build_with(&store, GraphOptions::all());
    let row = graph
        .edge_summary()
        .into_iter()
        .find(|row| row.kind == EdgeKind::SiblingRecord)
        .expect("GraphOptions::all builds every kind");
    assert_eq!(row.edges, 77_130);
    assert_eq!(row.dangling, 0, "adjacency cannot dangle by construction");

    let mut per_family: BTreeMap<u32, usize> = BTreeMap::new();
    for key in graph.known_keys() {
        let degree = graph
            .neighbours_of_kind(&key, EdgeKind::SiblingRecord)
            .len();
        if degree > 0 {
            *per_family.entry(key.group_id & 0xFFFF_FF00).or_insert(0) += degree;
        }
    }
    assert_eq!(
        per_family,
        BTreeMap::from([
            (0x4061_6200u32, 2_336),
            (0x4062_6200, 18_464),
            (0x4063_6200, 14_472),
            (0x4063_7E00, 1_312),
            (0x4064_6200, 22_176),
            (0x4065_6200, 7_080),
            (0x4066_6200, 10_098),
            (0x406B_6200, 804),
            (0x4162_6200, 388),
        ]),
        "sibling adjacency lives in the nine model families and nowhere else"
    );
    // No texture group family appears, which is the check that makes this rule
    // safe to leave off by default: it never links a model to a raster by naming.
    assert!(per_family
        .keys()
        .all(|family| family & 0x0000_FF00 != 0x2900));
}

/// The first `gmdl` record in `group`, ascending by instance.
fn first_model_in_group(store: &ContentStore, group: u32) -> ResourceKey {
    store
        .packages()
        .iter()
        .flat_map(|package| package.index().of_type(type_id::GMDL))
        .filter(|entry| entry.group_id == group)
        .map(|entry| entry.key)
        .min()
        .unwrap_or_else(|| panic!("no gmdl record in group 0x{group:08x}"))
}
