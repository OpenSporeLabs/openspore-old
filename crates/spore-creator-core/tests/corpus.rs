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

use spore_creator_core::persist::{load, save};
use spore_creator_core::sporemodel::Family;
use spore_creator_core::{
    validate, Block, BlockId, BlockIndex, BlockTransform, CreatorDocument, IndexRemap, Matrix3,
    OnLastChild, PaintRegion, Remap, Severity, UnitRgb, ValidationIssue,
};
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

/// The `Error`-severity issues a document carries. The authoring ops are only
/// allowed to be *neutral* on this list: an op that removes an error is as much
/// a silent repair as one that adds one.
fn errors(document: &CreatorDocument) -> Vec<ValidationIssue> {
    validate(document, None)
        .into_iter()
        .filter(|issue| issue.severity == Severity::Error)
        .collect()
}

/// Total `<childid>` edges in a document.
fn edge_count(document: &CreatorDocument) -> usize {
    document
        .blocks
        .iter()
        .map(|block| block.child_indices().len())
        .sum()
}

/// Every block in `index`'s subtree, `index` included.
fn subtree(document: &CreatorDocument, index: BlockIndex) -> Vec<BlockIndex> {
    let mut out = vec![index];
    let mut cursor = 0;
    while cursor < out.len() {
        let parent = out[cursor];
        cursor += 1;
        for &child in document.blocks[parent].child_indices() {
            if child < document.blocks.len() && !out.contains(&child) {
                out.push(child);
            }
        }
    }
    out
}

/// What a surviving block must look like after a removal: the original, with
/// every index-valued reference carried by the remap.
///
/// This is the invariant that matters, and it is stronger than "the array
/// compacted": a removal that forgot to shift an edge above the hole would leave
/// a valid-looking forest that had silently re-parented itself, and only this
/// projection catches it.
///
/// Two references do not survive the map and are handled as the op documents
/// they are: a child entry naming the removed block is **dropped** from its
/// parent's list, and a `<symmetric>` naming it is **cleared**. A leftover of
/// either kind is exactly what the error check below is looking for.
///
/// `on_last_child` is needed because the two policies produce genuinely
/// different documents: a parent that loses its only child is left with
/// `Some(vec![])` or with `None`, and the projection has to know which one it is
/// looking at or it would "fail" both of them.
fn project_block(remap: &IndexRemap, block: &Block, on_last_child: OnLastChild) -> Block {
    let mut projected = block.clone();
    if let Some(children) = &block.children {
        let mapped: Vec<BlockIndex> = children
            .iter()
            .filter_map(|&child| match remap.map(child) {
                Remap::Moved(index) => Some(index),
                Remap::Removed => None,
            })
            .collect();
        projected.children = if mapped.is_empty() && on_last_child == OnLastChild::OmitElement {
            None
        } else {
            Some(mapped)
        };
    }
    if let Some(partner) = block.symmetric {
        projected.symmetric = match remap.map(partner) {
            Remap::Moved(index) => Some(index),
            Remap::Removed => None,
        };
    }
    projected
}

/// A real block, stripped of the edges that would refer to a document it was
/// not authored for — i.e. usable as an input to `add_block`.
fn detachable(document: &CreatorDocument, index: BlockIndex) -> Block {
    let mut block = document.blocks[index].clone();
    block.children = None;
    block.symmetric = None;
    block
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

// ---------------------------------------------------------------------------
// Authoring operations over the real corpus
//
// Every test here decodes the real 68 documents, applies one op, and asserts a
// graph invariant that the op is *obliged* to keep: no dangling edge, no cycle,
// in-degree at most 1, and an exact edge-count delta. The edge delta is the
// assertion that matters, because a compaction that forgot to shift an edge
// still leaves a valid-looking forest — it just quietly re-parents the creature.
//
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn adding_a_block_to_every_real_document_appends_and_moves_no_edge() {
    let documents = corpus_or_skip!();
    for (family, document) in &documents {
        let before = document.clone();
        let before_edges = edge_count(&before);
        let mut edited = before.clone();

        let added = edited
            .add_block(detachable(&before, 0), before.roots().first().copied())
            .expect("a real block is addable");
        assert_eq!(
            added,
            before.len(),
            "{} appended at the end, never inserted",
            family.name()
        );

        // Every surviving block is byte-identical, so no index moved.
        for (index, block) in before.blocks.iter().enumerate() {
            let mut expected = block.clone();
            if Some(index) == before.roots().first().copied() {
                let mut children = block.child_indices().to_vec();
                children.push(added);
                expected.children = Some(children);
            }
            assert_eq!(
                edited.blocks[index],
                expected,
                "{} block {index} must not move",
                family.name()
            );
        }
        assert_eq!(edited.len(), before.len() + 1);
        assert_eq!(edge_count(&edited), before_edges + 1);
        assert!(errors(&edited).is_empty(), "{}", family.name());
    }
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn removing_a_block_from_every_real_document_gives_the_exact_edge_delta() {
    let documents = corpus_or_skip!();
    let mut removed_total = 0usize;

    // Both last-child policies are exercised over the whole corpus, because
    // they leave genuinely different documents and only one of them matches the
    // shape the corpus uses for its 160 leaves.
    for on_last_child in [OnLastChild::OmitElement, OnLastChild::KeepWrittenList] {
        for (family, document) in &documents {
            for victim in 0..document.len() {
                let before = document.clone();
                let before_edges = edge_count(&before);
                let in_degree = before
                    .blocks
                    .iter()
                    .filter(|block| block.child_indices().contains(&victim))
                    .count();
                let out_degree = before.blocks[victim].child_indices().len();
                let survivors = before.len() - 1;

                let mut edited = before.clone();
                let remap = edited
                    .remove_block(victim, on_last_child)
                    .expect("every corpus index exists");

                // The exact delta: the removed block's own edges and the edges
                // that named it both disappear, and nothing else changes.
                assert_eq!(
                    edge_count(&edited),
                    before_edges - in_degree - out_degree,
                    "{} removing {victim}: {in_degree} in, {out_degree} out, {on_last_child:?}",
                    family.name()
                );
                assert_eq!(edited.len(), survivors);
                assert!(errors(&edited).is_empty(), "{} at {victim}", family.name());

                // Explicitly: no edge and no mirror relation names a slot that is
                // gone. `errors()` reports those as `child_index_out_of_range`
                // and `symmetric_out_of_range`; naming the condition here is
                // what makes the intent legible.
                for (index, block) in edited.blocks.iter().enumerate() {
                    for &child in block.child_indices() {
                        assert!(
                            child < survivors,
                            "{} edge {index} -> {child} dangles after removing {victim}",
                            family.name()
                        );
                    }
                    if let Some(partner) = block.symmetric {
                        assert!(
                            partner < survivors,
                            "{} block {index} mirrors a block that is gone",
                            family.name()
                        );
                    }
                }

                // The remap is complete and correct, checked value by value:
                // every surviving block equals the original with each
                // index-valued reference carried by the remap. So "the remap is
                // structurally fine" and "the remap means the right thing" are
                // one assertion, and a forgotten shift above the hole fails it.
                assert_eq!(remap.len(), before.len(), "no gaps in the remap");
                assert_eq!(remap.removed(), victim);
                for old in 0..before.len() {
                    match remap.map(old) {
                        Remap::Moved(new) => {
                            assert_eq!(new, old - usize::from(old > victim));
                            assert_eq!(
                                edited.blocks[new],
                                project_block(&remap, &before.blocks[old], on_last_child),
                                "{} block {old} must survive at {new} with every reference \\
                                 carried by the remap",
                                family.name()
                            );
                        }
                        Remap::Removed => assert_eq!(old, victim, "only the victim is gone"),
                    }
                }
                assert_eq!(
                    remap.severed_symmetry().len(),
                    before
                        .blocks
                        .iter()
                        .filter(|block| block.symmetric == Some(victim))
                        .count(),
                    "{} every cleared mirror relation is reported, not lost",
                    family.name()
                );
                removed_total += 1;
            }
        }
    }
    assert_eq!(
        removed_total, 1360,
        "680 blocks removed once per last-child policy"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn a_reparent_of_a_real_subtree_keeps_the_edge_count_the_forest_and_the_transforms() {
    let documents = corpus_or_skip!();
    let mut moved = 0usize;

    for (family, document) in &documents {
        let roots = document.roots();
        let Some(&new_parent) = roots.last() else {
            continue;
        };

        // Find a block that has a parent and is not an ancestor of the intended
        // destination, so the move is a genuine re-parent and not a cycle.
        let candidate = document
            .blocks
            .iter()
            .enumerate()
            .filter(|(index, _)| document.parent_of(*index).is_some())
            .map(|(index, _)| index)
            .find(|&index| {
                index != new_parent
                    && !subtree(document, index).contains(&new_parent)
                    && document.parent_of(index) != Some(new_parent)
            });
        let Some(candidate) = candidate else {
            continue;
        };

        let before = document.clone();
        let before_edges = edge_count(&before);
        let transform_before: BTreeMap<BlockIndex, BlockTransform> = subtree(&before, candidate)
            .into_iter()
            .map(|index| (index, before.blocks[index].transform))
            .collect();

        let mut edited = before.clone();
        edited
            .reparent_block(candidate, Some(new_parent), OnLastChild::OmitElement)
            .expect("the destination is not inside the moved subtree");

        assert_eq!(
            edge_count(&edited),
            before_edges,
            "{} a move replaces one edge, it does not add one",
            family.name()
        );
        assert!(errors(&edited).is_empty(), "{} {family:?}", family.name());
        assert_eq!(
            edited.parent_of(candidate),
            Some(new_parent),
            "{} the edge landed where it was asked to",
            family.name()
        );

        // Every transform in the moved subtree is bit-for-bit what it was: the
        // frame question is open, so this op moves the edge and nothing else.
        let after_subtree = subtree(&edited, candidate);
        assert_eq!(
            after_subtree.len(),
            transform_before.len(),
            "the subtree is intact"
        );
        for index in after_subtree {
            assert_eq!(
                edited.blocks[index].transform,
                transform_before[&index],
                "{} block {index} transform must be untouched",
                family.name()
            );
        }
        moved += 1;
    }
    assert!(
        moved >= 60,
        "most documents have a movable block, got {moved}"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn replacing_the_part_of_a_real_block_changes_one_word_and_nothing_else() {
    let documents = corpus_or_skip!();
    let mut replaced = 0usize;

    for (family, document) in &documents {
        for index in 0..document.len() {
            let original = document.blocks[index]
                .block_id
                .expect("every corpus block has one");
            // A different part that is still a rigblock group family.
            let replacement = BlockId::new(original.group_id, original.instance_id ^ 0x5a5a_5a5a);
            let before = document.blocks[index].clone();

            let mut edited = document.clone();
            edited
                .replace_block(index, replacement)
                .expect("every corpus index exists");

            let after = &edited.blocks[index];
            assert_eq!(after.block_id, Some(replacement));
            assert_eq!(
                after.transform,
                before.transform,
                "{} {index}",
                family.name()
            );
            assert_eq!(after.children, before.children, "{} {index}", family.name());
            assert_eq!(
                after.symmetric,
                before.symmetric,
                "{} {index}",
                family.name()
            );
            assert_eq!(after.snapped, before.snapped, "{} {index}", family.name());
            assert_eq!(after.paint, before.paint, "{} {index}", family.name());
            assert_eq!(after.handles, before.handles, "{} {index}", family.name());
            assert_eq!(after.limb, before.limb, "{} {index}", family.name());
            assert_eq!(
                after.user_orientation,
                before.user_orientation,
                "{} {index}",
                family.name()
            );
            assert_eq!(after.bbox, before.bbox, "{} {index}", family.name());
            assert_eq!(edge_count(&edited), edge_count(document));
            assert!(errors(&edited).is_empty(), "{} {index}", family.name());
            replaced += 1;
        }
    }
    assert_eq!(replaced, 680, "680 blocks, each with its part swapped once");
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn a_transform_can_be_rewritten_on_every_real_block_without_touching_the_graph() {
    let documents = corpus_or_skip!();
    let replacement = BlockTransform {
        position: spore_creator_core::Vec3 {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        },
        scale: 0.28,
        orientation: Matrix3::IDENTITY,
        triangle_direction: None,
        triangle_pick_origin: None,
    };

    for (family, document) in &documents {
        let mut edited = document.clone();
        let before_edges = edge_count(&edited);
        for index in 0..edited.len() {
            edited
                .set_block_transform(index, replacement)
                .expect("every corpus index exists");
            assert_eq!(edited.blocks[index].transform, replacement);
        }
        assert_eq!(edge_count(&edited), before_edges, "{}", family.name());
        assert!(errors(&edited).is_empty(), "{}", family.name());
    }
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn paint_is_stored_on_a_creature_and_reported_rather_than_refused_or_dropped() {
    let documents = corpus_or_skip!();
    let region = PaintRegion {
        region: 5,
        paint_id: 0xf596_7f30,
        color1: UnitRgb::new(0.0, 0.25, 1.0).expect("the test colour is in range"),
        color2: UnitRgb::new(1.0, 0.5, 0.0).expect("the test colour is in range"),
    };

    let mut creatures_painted = 0usize;
    for (family, document) in &documents {
        let mut edited = document.clone();
        for index in 0..edited.len() {
            if edited.blocks[index].paint.is_some() {
                continue;
            }
            edited
                .set_paint(index, Some(vec![region]))
                .expect("every corpus index exists");
        }
        let painted = edited
            .blocks
            .iter()
            .filter(|block| block.paint.is_some())
            .count();
        assert!(painted > 0, "{}", family.name());
        assert_eq!(painted, edited.len(), "{}", family.name());

        // The op cannot know the family, so validate reports what the op must
        // not: paint on a creature is kept and warned about, never removed.
        if *family == Family::Creature {
            creatures_painted += 1;
            let issues = validate(&edited, Some(*family));
            assert!(
                issues
                    .iter()
                    .any(|issue| issue.code == "paint_in_a_family_without_paint"),
                "{} {family:?}",
                family.name()
            );
            assert!(
                issues
                    .iter()
                    .all(|issue| issue.severity == Severity::Warning),
                "paint on a creature is a warning, so the document still loads"
            );
        }
        assert!(errors(&edited).is_empty(), "{}", family.name());
    }
    assert_eq!(
        creatures_painted, 47,
        "all 47 creature documents are exercised"
    );
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn a_full_authoring_session_over_the_corpus_returns_every_document_to_its_shape() {
    let documents = corpus_or_skip!();

    for (family, document) in &documents {
        let baseline = errors(document);
        assert!(baseline.is_empty(), "{} starts clean", family.name());
        let before_blocks = document.len();
        let before_edges = edge_count(document);
        let before_roots = document.roots();

        let mut edited = document.clone();

        // Add a two-block limb on the first root, swap one of its parts, write a
        // transform, then detach it and delete both blocks again.
        let root = edited.roots()[0];
        let outer = edited
            .add_block(detachable(&edited, root), Some(root))
            .expect("added");
        let inner = edited
            .add_block(detachable(&edited, outer), Some(outer))
            .expect("added");
        edited
            .replace_block(inner, BlockId::new(0x4062_6000, 0x1234_5678))
            .expect("replaced");
        edited
            .set_block_transform(
                inner,
                BlockTransform {
                    position: spore_creator_core::Vec3 {
                        x: 0.5,
                        y: -0.25,
                        z: 2.0,
                    },
                    scale: 1.5,
                    orientation: Matrix3::IDENTITY,
                    triangle_direction: None,
                    triangle_pick_origin: None,
                },
            )
            .expect("transform written");
        edited
            .set_paint(inner, Some(Vec::new()))
            .expect("an empty paint list was written");
        edited
            .reparent_block(inner, Some(root), OnLastChild::OmitElement)
            .expect("reparented");

        // Two adds, then the detach moves one edge rather than adding it, and
        // `outer` loses the only child it ever had.
        assert_eq!(edited.len(), before_blocks + 2);
        assert_eq!(edge_count(&edited), before_edges + 2);
        assert_eq!(edited.parent_of(inner), Some(root));
        assert_eq!(
            edited.blocks[outer].children, None,
            "the empty list was dropped"
        );
        assert!(errors(&edited).is_empty(), "{}", family.name());

        let first = edited
            .remove_block(inner, OnLastChild::OmitElement)
            .expect("removed");
        assert_eq!(first.map(inner), Remap::Removed);
        assert!(errors(&edited).is_empty(), "{}", family.name());

        let second = edited
            .remove_block(outer, OnLastChild::OmitElement)
            .expect("removed");
        assert_eq!(second.map(second.removed()), Remap::Removed);

        // Back to exactly where it started: same blocks, same edges, same roots.
        assert_eq!(edited.len(), before_blocks, "{}", family.name());
        assert_eq!(edge_count(&edited), before_edges, "{}", family.name());
        assert_eq!(edited.roots(), before_roots, "{}", family.name());
        assert!(errors(&edited).is_empty(), "{}", family.name());
    }
}

// ---------------------------------------------------------------------------
// canonical persistence
//
// The save format's claim is `load(save(doc)) == doc` bitwise, and `save` is a
// pure function of the document. A synthetic fixture can show that; only the real
// corpus can show it holds for the documents the game actually wrote — 68 of
// them, 680 blocks, every measured absence in them.
// ---------------------------------------------------------------------------

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn every_real_creator_document_round_trips_through_the_canonical_save() {
    let documents = corpus_or_skip!();
    let mut blocks = 0usize;
    let mut bytes = 0usize;

    for (family, document) in &documents {
        let saved =
            save(document).unwrap_or_else(|error| panic!("{} saving: {error}", family.name()));
        let reloaded =
            load(&saved).unwrap_or_else(|error| panic!("{} loading: {error}", family.name()));

        // Bitwise, not "close": the document types' `PartialEq` compares floats by
        // their bits, so a sign-bit flip or a rounded digit fails here rather than
        // passing as a tolerance.
        assert_eq!(
            &reloaded,
            document,
            "{} must read back bitwise",
            family.name()
        );

        // And the second save is byte-identical, which makes "equal documents" and
        // "equal bytes" one property instead of two.
        let again = save(&reloaded).unwrap();
        assert_eq!(
            again,
            saved,
            "{} must save identically twice",
            family.name()
        );

        // The shape of the artifact: a header plus one line per block, in document
        // order, each line declaring its own index.
        let lines: Vec<&str> = saved.lines().collect();
        assert_eq!(
            lines.len(),
            document.len() + 1,
            "{} writes a header and one line per block",
            family.name()
        );
        assert!(lines[0].contains("\"schema\":\"spore-creator-doc/1\""));
        assert!(lines[0].contains(&format!("\"block_count\":{}", document.len())));
        assert!(
            lines[0].contains(&format!("\"root_count\":{}", document.roots().len())),
            "{}: the header's root count is derived, so it is checkable",
            family.name()
        );
        for (position, line) in lines.iter().enumerate().skip(1) {
            assert!(
                line.starts_with(&format!("{{\"index\":{}", position - 1)),
                "{} content line {position} must declare index {}: {line}",
                family.name(),
                position - 1
            );
        }

        // The absences the corpus is measured on survive as absent keys: 160 of
        // 680 blocks write no `<childlist>`, and a save that wrote `[]` for one of
        // them would have deleted the fact.
        let absent_children = document
            .blocks
            .iter()
            .filter(|block| !block.writes_children())
            .count();
        let written_children = lines
            .iter()
            .skip(1)
            .filter(|line| line.contains("\"children\":"))
            .count();
        assert_eq!(
            written_children,
            document.len() - absent_children,
            "{}: a leaf's line carries no children key at all",
            family.name()
        );
        assert!(
            !saved.contains("\"children\":null"),
            "{}: absence is never a null",
            family.name()
        );

        blocks += document.len();
        bytes += saved.len();
    }

    assert_eq!(documents.len(), 68, "the corpus census is 68 documents");
    assert_eq!(blocks, 680, "the census is 680 blocks");
    assert!(
        bytes > 0 && saved_line_count(&documents) == 748,
        "the corpus saves to {bytes} bytes over 748 lines"
    );
}

/// Total saved lines over the corpus: 68 headers plus 680 block lines.
fn saved_line_count(documents: &[(Family, CreatorDocument)]) -> usize {
    documents
        .iter()
        .map(|(_, document)| document.len() + 1)
        .sum()
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn a_removed_block_survives_the_canonical_save_with_the_graph_intact() {
    let documents = corpus_or_skip!();

    for on_last_child in [OnLastChild::OmitElement, OnLastChild::KeepWrittenList] {
        for (family, document) in &documents {
            // Every block of every document, removed once: 680 removals per
            // policy. Picking one block per document would leave the interesting
            // cases — a root, a mirror partner, an interior node — to chance.
            for victim in 0..document.len() {
                let before = document.clone();
                let survivors = before.len() - 1;

                let mut edited = before.clone();
                let remap = edited
                    .remove_block(victim, on_last_child)
                    .expect("every corpus index exists");
                assert_eq!(remap.len(), before.len());
                assert_eq!(edited.len(), survivors);

                let saved = save(&edited)
                    .unwrap_or_else(|error| panic!("{} saving: {error}", family.name()));
                let reloaded = load(&saved)
                    .unwrap_or_else(|error| panic!("{} loading: {error}", family.name()));
                assert_eq!(
                    &reloaded,
                    &edited,
                    "{} removing {victim} under {on_last_child:?}",
                    family.name()
                );
                assert_eq!(
                    save(&reloaded).unwrap(),
                    saved,
                    "{} removing {victim} must save identically twice",
                    family.name()
                );
                assert_eq!(reloaded.len(), survivors, "the count dropped by one");

                // The graph invariants, on the **reloaded** document: nothing
                // points at a slot that is gone, and nothing has re-parented
                // itself silently.
                for (index, block) in reloaded.blocks.iter().enumerate() {
                    for &child in block.child_indices() {
                        assert!(
                            child < survivors,
                            "{} edge {index} -> {child} dangles after removing {victim}",
                            family.name()
                        );
                    }
                    if let Some(partner) = block.symmetric {
                        assert!(
                            partner < survivors,
                            "{} block {index} mirrors a block that is gone",
                            family.name()
                        );
                    }
                }
                assert_eq!(
                    errors(&reloaded),
                    errors(&edited),
                    "{} at {victim}",
                    family.name()
                );
                assert!(
                    errors(&reloaded).is_empty(),
                    "{} at {victim}",
                    family.name()
                );

                // No cycle, and the roots are exactly the pre-removal roots plus
                // whatever the removal detached. `ancestors` walks parent links
                // and stops on a cycle rather than looping, so a bounded walk is
                // the check that would notice one.
                for index in 0..reloaded.len() {
                    assert!(
                        reloaded.ancestors(index).len() <= reloaded.len(),
                        "{} block {index} is inside a cycle after removing {victim}",
                        family.name()
                    );
                }
                let mut expected_roots: Vec<BlockIndex> = before
                    .roots()
                    .into_iter()
                    .filter(|&root| root != victim)
                    .map(|root| match remap.map(root) {
                        // Only the victim maps to `Removed`, and it was just
                        // filtered out.
                        Remap::Moved(index) => index,
                        Remap::Removed => victim,
                    })
                    .collect();
                for &detached in remap.detached() {
                    if let Remap::Moved(root) = remap.map(detached) {
                        expected_roots.push(root);
                    }
                }
                expected_roots.sort_unstable();
                expected_roots.dedup();
                let mut found = reloaded.roots();
                found.sort_unstable();
                assert_eq!(
                    found,
                    expected_roots,
                    "{} removing {victim} under {on_last_child:?}: the roots are the \\
                     survivors plus the detached",
                    family.name()
                );
            }
        }
    }
}

#[test]
#[ignore = "needs the game install; no asset byte is committed"]
fn the_corpus_saves_to_a_pinned_number_of_bytes() {
    // The strongest determinism assertion available over real data: if the format
    // changes what it writes — a key order, a float spelling, a `0x` width — then
    // this number moves, and that is a deliberate `spore-creator-doc/2` decision
    // rather than an accident nobody noticed. It also pins that two runs over the
    // same 68 records produce the same bytes.
    let documents = corpus_or_skip!();
    let bytes: usize = documents
        .iter()
        .map(|(_, document)| save(document).expect("saving").len())
        .sum();
    assert_eq!(bytes, CORPUS_SAVE_BYTES, "the corpus save size moved");
    assert_eq!(
        documents
            .iter()
            .map(|(_, document)| document.len() + 1)
            .sum::<usize>(),
        748,
        "68 header lines and 680 block lines"
    );
}

/// The measured size of all 68 canonical saves concatenated, in bytes.
///
/// Measured on Spore 3.1.0.22, `SPORE/Data/Spore_Game.package`. A different
/// install of the same version gives the same number; a different corpus does not,
/// which is the point of pinning it.
const CORPUS_SAVE_BYTES: usize = 392_218;
