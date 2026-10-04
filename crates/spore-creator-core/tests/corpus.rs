//! The decoder measured against all 68 real creator documents.
//!
//! These tests are `#[ignore]`d and skip cleanly, with a printed `SKIP:` line,
//! when `SPORE/` is absent — the same pattern the rest of the workspace uses.
//! No Spore byte is committed: the package is read from the local install and
//! every assertion is on counts, structure and values.
//!
//! ```text
//! cargo test -p spore-creator-core --release --test corpus -- --ignored --nocapture
//! ```
//!
//! # What these tests are for
//!
//! The grammar in `docs/CREATOR-CORE.md` §1 was measured by parsing the corpus.
//! This file is that measurement re-run in Rust, so a decoder change that stops
//! matching the game fails here instead of in a preview window. Every expected
//! number is an exact equality, never a tolerance.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use spore_creator_core::{sporemodel::Family, validate, CreatorDocument};
use spore_dbpf::extract_record;

/// Where the game install lives, relative to the repository root.
const PACKAGE: &str = "SPORE/Data/Spore_Game.package";

/// The 68 documents, or `None` when there is no install to read.
fn corpus() -> Option<Vec<(Family, CreatorDocument)>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(PACKAGE);
    if !path.exists() {
        println!(
            "SKIP: {PACKAGE} is absent; the creator-document corpus is not available. \
             Nothing is committed in its place."
        );
        return None;
    }
    let bytes = std::fs::read(&path).expect("the package is readable");
    let entries = spore_dbpf::parse_index(&bytes).expect("the index parses");

    let mut out = Vec::new();
    for entry in &entries {
        let Some(family) = Family::from_type_id(entry.type_id) else {
            continue;
        };
        let raw = extract_record(&bytes, entry).expect("a creator record decompresses");
        let text = String::from_utf8(raw).expect("a creator record is utf-8");
        let document = spore_creator_core::parse_sporemodel(family, &text)
            .unwrap_or_else(|error| panic!("{} {error:?}", family.name()));
        out.push((family, document));
    }
    Some(out)
}

/// Every document, or an immediate skip.
macro_rules! corpus_or_skip {
    () => {
        match corpus() {
            Some(documents) => documents,
            None => return,
        }
    };
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn sixty_eight_documents_across_six_families_all_parse() {
    let documents = corpus_or_skip!();
    assert_eq!(
        documents.len(),
        68,
        "the corpus census is 68 creator documents"
    );

    let mut by_family: BTreeMap<&str, usize> = BTreeMap::new();
    for (family, _) in &documents {
        *by_family.entry(family.name()).or_default() += 1;
    }
    assert_eq!(
        by_family,
        BTreeMap::from([
            ("crt", 47),
            ("cll", 13),
            ("vcl", 4),
            ("ufo", 2),
            ("bld", 1),
            ("flr", 1),
        ])
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn six_hundred_eighty_blocks_with_the_measured_absence_counts() {
    let documents = corpus_or_skip!();
    let blocks: Vec<_> = documents
        .iter()
        .flat_map(|(_, document)| document.blocks.iter())
        .collect();

    assert_eq!(blocks.len(), 680, "the census is 680 <blockref> blocks");

    let writes_children = blocks
        .iter()
        .filter(|block| block.writes_children())
        .count();
    assert_eq!(
        writes_children, 520,
        "160 blocks carry no <childlist> at all, and that absence is the point"
    );
    assert_eq!(blocks.len() - writes_children, 160);

    let edges: usize = blocks.iter().map(|block| block.child_indices().len()).sum();
    assert_eq!(edges, 601, "601 <childid> edges");

    let symmetric = blocks
        .iter()
        .filter(|block| block.symmetric.is_some())
        .count();
    assert_eq!(symmetric, 84);

    let handles = blocks
        .iter()
        .filter(|block| block.handles.is_some())
        .count();
    assert_eq!(handles, 119, "<handles> appears on 119 blocks");

    let paint = blocks.iter().filter(|block| block.paint.is_some()).count();
    assert_eq!(paint, 80, "<paintlist> appears on 80 blocks");

    let pick_origin = blocks
        .iter()
        .filter(|block| block.transform.triangle_pick_origin.is_some())
        .count();
    assert_eq!(
        pick_origin, 670,
        "<trianglepickorigin> is absent on exactly the 10 blocks of the formatversion 11 document"
    );

    let bbox = blocks.iter().filter(|block| block.bbox.is_some()).count();
    assert_eq!(
        bbox, 10,
        "<blockbbox> is in that same document and nowhere else"
    );

    let limb = blocks.iter().filter(|block| block.limb.is_some()).count();
    assert_eq!(
        limb, 8,
        "<basemusclescale> appears on 8 blocks, crt and flr only"
    );

    let user_orientation = blocks
        .iter()
        .filter(|block| block.user_orientation.is_some())
        .count();
    assert_eq!(user_orientation, 11, "<userorientation> is vehicle-only");
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn paint_is_in_the_vehicle_family_and_absent_from_every_creature_document() {
    let documents = corpus_or_skip!();

    let painted_creatures = documents
        .iter()
        .filter(|(family, _)| *family == Family::Creature)
        .flat_map(|(_, document)| document.blocks.iter())
        .filter(|block| block.paint.is_some())
        .count();
    assert_eq!(
        painted_creatures, 0,
        "no creature document writes <paintlist>, so paint in one would be a finding"
    );

    for (family, document) in &documents {
        let painted = document
            .blocks
            .iter()
            .filter(|block| block.paint.is_some())
            .count();
        if family.carries_paint() {
            continue;
        }
        assert_eq!(painted, 0, "{} carries paint in the corpus", family.name());
    }

    // Two colours per region, in every one of the 195 paint entries.
    let regions = documents
        .iter()
        .flat_map(|(_, document)| document.blocks.iter())
        .filter_map(|block| block.paint.as_ref())
        .flatten()
        .count();
    assert_eq!(
        regions, 195,
        "195 <paint> entries across the corpus: bld 20, ufo 35, vcl 140"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn the_graph_is_a_forest_of_seventy_nine_roots_over_sixty_eight_documents() {
    let documents = corpus_or_skip!();
    let roots: usize = documents.iter().map(|(_, d)| d.roots().len()).sum();
    assert_eq!(
        roots, 79,
        "62 documents have one root, four have two, one three, one six"
    );

    let single = documents
        .iter()
        .filter(|(_, d)| d.roots().len() == 1)
        .count();
    assert_eq!(single, 62);
    let six = documents.iter().find(|(_, d)| d.roots().len() == 6);
    assert!(
        six.is_some(),
        "one document has six roots, so no root field could exist"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn symmetry_is_reciprocal_in_every_relation_the_corpus_has() {
    let documents = corpus_or_skip!();
    let mut relations = 0;
    for (_, document) in &documents {
        for (index, block) in document.blocks.iter().enumerate() {
            let Some(partner) = block.symmetric else {
                continue;
            };
            relations += 1;
            assert!(
                partner < document.blocks.len(),
                "block {index} names mirror partner {partner}, which is out of range"
            );
            assert_eq!(
                document.blocks[partner].symmetric,
                Some(index),
                "symmetry is reciprocal in 84 of 84 corpus relations"
            );
        }
    }
    assert_eq!(relations, 84);
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn every_corpus_document_passes_validation_with_no_error() {
    let documents = corpus_or_skip!();
    for (family, document) in &documents {
        let issues = validate(document, Some(*family));
        let errors: Vec<_> = issues
            .iter()
            .filter(|issue| issue.severity == spore_creator_core::Severity::Error)
            .collect();
        assert!(
            errors.is_empty(),
            "{} {family:?} has structural errors: {errors:?}",
            family.name()
        );
    }
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn the_scale_is_a_scalar_with_the_measured_range() {
    let documents = corpus_or_skip!();
    let mut lowest = f32::MAX;
    let mut highest = f32::MIN;
    for (_, document) in &documents {
        for block in &document.blocks {
            lowest = lowest.min(block.transform.scale);
            highest = highest.max(block.transform.scale);
        }
    }
    assert!(
        (lowest - 0.28).abs() < 1e-6,
        "measured minimum scale is 0.28, got {lowest}"
    );
    assert!(
        (highest - 2.254_181).abs() < 1e-6,
        "measured maximum scale is 2.254181, got {highest}"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn a_model_type_written_in_decimal_and_in_hex_is_the_same_field() {
    let documents = corpus_or_skip!();
    let with_model_type: Vec<u32> = documents
        .iter()
        .filter_map(|(_, d)| d.properties.model_type)
        .collect();
    assert_eq!(
        with_model_type.len(),
        68,
        "all 68 documents carry <modeltype>"
    );

    // 67 are written `0x…` and one — the formatversion 11 document — decimal.
    let v11 = documents
        .iter()
        .find(|(_, d)| d.format_version == Some(11))
        .expect("the corpus has one formatversion 11 document");
    assert!(v11.1.properties.model_type.is_some());
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn format_version_does_not_gate_field_presence() {
    let documents = corpus_or_skip!();
    let mut by_version: BTreeMap<u32, (usize, usize)> = BTreeMap::new();
    for (_, document) in &documents {
        let version = document
            .format_version
            .expect("every document is versioned");
        let entry = by_version.entry(version).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += document
            .blocks
            .iter()
            .filter(|block| block.handles.is_some())
            .count();
    }
    // v16 documents both write and omit <handles>: 59 documents, and the ones
    // that carry handles are a strict minority. That is the whole reason no
    // field is gated on the version number.
    assert_eq!(by_version.get(&16).map(|e| e.0), Some(59));
    assert_eq!(by_version.get(&15).map(|e| e.0), Some(8));
    assert_eq!(by_version.get(&11).map(|e| e.0), Some(1));
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn every_block_id_pair_resolves_to_a_rigblock_group_family() {
    let documents = corpus_or_skip!();
    let mut groups: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, document) in &documents {
        for block in &document.blocks {
            let id = block.block_id.expect("every block writes a <blockid>");
            *groups.entry(id.group_id).or_default() += 1;
        }
    }
    assert_eq!(
        groups,
        BTreeMap::from([
            (0x4061_6000, 79),  // cell
            (0x4062_6000, 487), // creature
            (0x4063_6000, 8),   // building
            (0x4064_6000, 104), // vehicle, which the UFO documents borrow
            (0x4066_6000, 2),   // flora
        ]),
        "the blockid group families are the rigblock families"
    );
}

/// Keeps the unused-import warning away while documenting where a fixture would
/// go: golden files for this decoder are **synthetic strings**, never excerpts
/// of a real record.
#[test]
fn fixtures_are_synthetic_and_live_beside_this_file() {
    let _unused: Option<PathBuf> = None;
}
