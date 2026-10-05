//! The projection against the real corpus, over every creator document.
//!
//! `#[ignore]`d by default and skipped cleanly when `SPORE/` is absent, matching
//! every other corpus test in this workspace.
//!
//! The point of these is not that the numbers are large. It is that the projection
//! is DERIVED from the document rather than restated from the composition, so
//! re-deriving them catches a transform that was dropped, a depth that was
//! miscounted, or a slot that no longer lines up with its block.

use std::path::{Path, PathBuf};

use spore_creator_core::{parse_sporemodel, CreatorDocument, Family};
use spore_dbpf::{extract_record, PackageIndex};
use spore_morphology::{compose, PackageSource};

/// The five type ids measured to hold `<sporemodel>` records, with their families.
const FAMILIES: [(u32, Family); 6] = [
    (0x2b97_8c46, Family::Creature),
    (0x3d97_a8e4, Family::Cell),
    (0x2468_2294, Family::Vehicle),
    (0x476a_98c7, Family::Ufo),
    (0x2399_be55, Family::Building),
    (0x438f_6347, Family::Flora),
];

fn install_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// Every decoded creator document in the shipped install.
///
/// 68 documents: 47 `crt`, 13 `cll`, 4 `vcl`, 2 `ufo`, 1 `bld`, 1 `flr`.
fn creator_documents() -> Vec<(Family, CreatorDocument)> {
    let Some(root) = install_root() else {
        return Vec::new();
    };
    let Ok(image) = std::fs::read(root.join("SPORE/Data/Spore_Game.package")) else {
        return Vec::new();
    };
    let Ok(index) = PackageIndex::parse(&image) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in index.entries() {
        let Some((_, family)) = FAMILIES
            .iter()
            .find(|(type_id, _)| *type_id == entry.type_id)
        else {
            continue;
        };
        let Ok(bytes) = extract_record(&image, entry) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        if let Ok(document) = parse_sporemodel(*family, text) {
            out.push((*family, document));
        }
    }
    out
}

/// A source holding both packages a part spans.
fn part_source() -> PackageSource {
    let Some(root) = install_root() else {
        return PackageSource::new();
    };
    PackageSource::new()
        .open_in(&root, "Spore_Game.package")
        .open_in(&root, "Spore_Graphics.package")
}

#[test]
#[ignore = "needs SPORE/"]
fn every_creator_document_projects_one_entry_per_block_and_never_lies_about_coverage() {
    let documents = creator_documents();
    if documents.is_empty() {
        eprintln!("SKIP: no SPORE/Data install");
        return;
    }
    assert_eq!(
        documents.len(),
        68,
        "the corpus is 68 creator documents; a different count means the family table \
         or the record source moved"
    );
    let source = part_source();

    let mut total_blocks = 0usize;
    let mut total_resolved = 0usize;
    let mut total_vertices = 0usize;
    let mut total_triangles = 0usize;
    let mut total_bones = 0usize;
    let mut deepest = 0usize;

    for (family, document) in &documents {
        let composition = compose(document, &source).expect("composition");
        let projection = spore_morphology::project(document, &composition)
            .expect("document and composition agree");

        // INVARIANT 1: one entry per block, always. Never a shortened list.
        assert_eq!(
            projection.blocks().len(),
            document.blocks.len(),
            "a {family:?} document of {} blocks projected to {} entries",
            document.blocks.len(),
            projection.blocks().len(),
        );
        for (index, block) in projection.blocks().iter().enumerate() {
            assert_eq!(
                block.block, index,
                "the projection must be positional so blocks()[i] is block i"
            );
        }

        // INVARIANT 2: resolved + unresolved is the block count, always, and the two
        // halves are disjoint. A partial creature must be countable as partial.
        let counts = projection.counts();
        assert_eq!(counts.total(), document.blocks.len());
        assert_eq!(counts.resolved, projection.resolved().count());
        assert_eq!(counts.unresolved, projection.unresolved().count());

        // INVARIANT 3: authored values are carried verbatim, not recomputed.
        for (block, projected) in document.blocks.iter().zip(projection.blocks()) {
            assert_eq!(
                projected.local_position, block.transform.position,
                "position must be carried, not recomputed"
            );
            assert_eq!(
                projected.local_scale.to_bits(),
                block.transform.scale.to_bits(),
                "scale must be carried bit-for-bit"
            );
            assert_eq!(
                projected.local_orientation.as_matrix(),
                block.transform.orientation,
                "the authored orientation must survive verbatim, not be narrowed \
                 into a Y rotation"
            );
        }

        // Every unresolved block names a stage and a reason. Never a bare absence.
        for block in projection.unresolved() {
            let unresolved = block
                .unresolved
                .as_ref()
                .expect("an unresolved block carries its reason");
            assert!(
                !unresolved.reason.as_str().is_empty(),
                "an unresolved block must say why, not merely fail"
            );
        }

        total_blocks += document.blocks.len();
        total_resolved += counts.resolved;
        total_vertices += projection.total_vertices();
        total_triangles += projection.total_triangles();
        total_bones += projection.total_bones();
        deepest = deepest.max(projection.max_depth());
    }

    // The numbers the expedition measured by hand, now re-derived through the
    // projection rather than the composition.
    assert_eq!(total_blocks, 680, "680 blockrefs across the 68 documents");
    assert_eq!(total_resolved, 678, "678 blocks resolve to a part");
    assert_eq!(total_blocks - total_resolved, 2, "the two known failures");
    assert_eq!(
        total_vertices, 110_037,
        "vertices across every resolved part"
    );
    assert_eq!(
        total_triangles, 108_433,
        "triangles across every resolved part"
    );
    assert_eq!(
        total_bones, 677,
        "bones across every part that has a body graph"
    );

    // A corpus where nothing composed would still pass every invariant above. The
    // depth assertion is what says the world transforms actually walked the graph.
    assert_eq!(deepest, 19, "the authored forest is 19 deep at its maximum");
}

#[test]
#[ignore = "needs SPORE/"]
fn the_corpus_projections_are_stable_across_repeated_runs() {
    let documents = creator_documents();
    if documents.is_empty() {
        eprintln!("SKIP: no SPORE/Data install");
        return;
    }
    let source = part_source();
    // A projection must be a pure function of its two inputs, or "project, save,
    // reload, project, compare" cannot verify anything.
    for (_family, document) in &documents {
        let project_once = || {
            let composition = compose(document, &source).expect("composition");
            spore_morphology::project(document, &composition).expect("projection")
        };
        let first = project_once();
        let second = project_once();
        assert_eq!(
            first.blocks().len(),
            second.blocks().len(),
            "a repeated projection must produce the same entries"
        );
        for (a, b) in first.blocks().iter().zip(second.blocks()) {
            assert_eq!(
                a.world_position, b.world_position,
                "a repeated projection must land blocks in the same place"
            );
            assert_eq!(
                a.world_orientation, b.world_orientation,
                "a repeated projection must produce the same orientation"
            );
            assert_eq!(a.ancestor_depth, b.ancestor_depth);
        }
    }
}

#[test]
#[ignore = "needs SPORE/"]
fn every_authored_orientation_is_close_to_a_real_rotation() {
    let documents = creator_documents();
    if documents.is_empty() {
        eprintln!("SKIP: no SPORE/Data install");
        return;
    }
    // 680 real authored orientations, measured. If the game writes shear or a hidden
    // scale into an orientation, the error is non-trivial and that is a fact about
    // the format worth recording rather than absorbing.
    let mut worst = 0.0f32;
    let mut worst_at: Option<(usize, usize)> = None;
    let mut blocks = 0usize;
    for (_family, document) in &documents {
        for (index, block) in document.blocks.iter().enumerate() {
            blocks += 1;
            let error = spore_core::Orientation::error_of(block.transform.orientation);
            if error > worst {
                worst = error;
                worst_at = Some((index, blocks));
            }
        }
    }
    assert_eq!(blocks, 680, "every authored block carries an orientation");
    eprintln!("worst authored orthonormality error over 680 orientations: {worst}");
    // A regression tripwire for "the game started writing something else", not a
    // licence to accept a large error. Loose on purpose because float orthonormality
    // is never exact.
    assert!(
        worst < 1e-3,
        "authored orientations drifted from orthonormality: worst error {worst} at \
         block index {:?} of the running count",
        worst_at
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn every_inferred_hop_is_carried_through_the_projection() {
    let documents = creator_documents();
    if documents.is_empty() {
        eprintln!("SKIP: no SPORE/Data install");
        return;
    }
    let source = part_source();
    // 485 of 678 parts rest on an `Inferred` hop: the first validated prop pointer is
    // frequently not the block's own instance. The projection must carry that grade
    // rather than presenting those parts as measured.
    let mut inferred = 0usize;
    let mut resolved = 0usize;
    for (_family, document) in &documents {
        let composition = compose(document, &source).expect("composition");
        let projection = spore_morphology::project(document, &composition).expect("projection");
        for block in projection.resolved() {
            resolved += 1;
            let part = block.part.as_ref().expect("resolved carries a part");
            if part.weakest_grade == spore_morphology::Grade::Inferred {
                inferred += 1;
            }
        }
    }
    assert_eq!(resolved, 678, "resolved parts across the corpus");
    assert_eq!(
        inferred, 485,
        "the first validated prop pointer is often not the block's own instance, \
         which downgrades the hop to Inferred"
    );
}
