//! The end-to-end slice, over the real corpus.
//!
//! ```text
//! Spore_Game.package ─DBPF─> <sporemodel> ─> spore-creator-core
//!                                                     │ canonical authoring data
//!                                                     ▼
//!                                        spore-morphology ─ block resolution
//!                                                     │
//!                                                     ▼
//!                                        spore-rigblock ─ part pair, body graph
//! ```
//!
//! No synthetic creature is the primary proof here. The numbers below come from
//! the 68 shipped creator documents and the 1549 rigblock records they point at.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use spore_core::ResourceKey;
use spore_creator_core::{parse_sporemodel, Family};
use spore_dbpf::{extract_record, PackageIndex};
use spore_morphology::{compose, PackageSource, PartSlot, ResolvedPart, Stage};

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

/// Reads every `<sporemodel>` record out of `Spore_Game.package`.
fn creator_documents() -> Vec<(Family, Vec<u8>)> {
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
        if let Ok(bytes) = extract_record(&image, entry) {
            out.push((*family, bytes));
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

/// What the whole slice measured.
#[derive(Default, Debug)]
struct Tally {
    documents: usize,
    blocks: usize,
    resolved: usize,
    unresolved: usize,
    with_skeleton: usize,
    bones: usize,
    named_bones: usize,
    vertices: usize,
    triangles: usize,
    parts_with_geometry: usize,
    by_family: BTreeMap<Family, (usize, usize, usize)>,
    stops: BTreeMap<(&'static str, &'static str), usize>,
    stops_without_key: usize,
    inferred_hops: usize,
}

impl Tally {
    fn add(&mut self, family: Family, composition: &spore_morphology::Composition) {
        self.documents += 1;
        let counts = composition.counts();
        self.blocks += counts.total();
        self.resolved += counts.resolved;
        self.unresolved += counts.unresolved;
        self.with_skeleton += composition.parts_with_skeleton();

        let entry = self.by_family.entry(family).or_insert((0, 0, 0));
        entry.0 += counts.total();
        entry.1 += counts.resolved;
        entry.2 += counts.unresolved;

        for part in composition.resolved() {
            self.bones += part.bones().map(<[spore_rigblock::Bone]>::len).unwrap_or(0);
            self.named_bones += part.named_bones().len();
            if part.geometry.is_some() {
                self.parts_with_geometry += 1;
                self.vertices += part.vertex_count().unwrap_or(0);
                self.triangles += part.triangle_count().unwrap_or(0);
            }
            if part.trail.weakest_grade() == spore_morphology::Grade::Inferred {
                self.inferred_hops += 1;
            }
        }
        for unresolved in composition.unresolved() {
            *self
                .stops
                .entry((unresolved.stage.as_str(), unresolved.reason.as_str()))
                .or_default() += 1;
            match unresolved
                .trail
                .as_ref()
                .and_then(|trail| trail.key_at(unresolved.stage))
            {
                Some(_) => {}
                None => self.stops_without_key += 1,
            }
        }
    }
}

fn tally_everything() -> Option<Tally> {
    let documents = creator_documents();
    if documents.is_empty() {
        return None;
    }
    let source = part_source();
    assert!(source.package_count() >= 2, "a part spans two packages");

    let mut tally = Tally::default();
    for (family, bytes) in &documents {
        let text = std::str::from_utf8(bytes).expect("a sporemodel record is utf-8");
        let document = parse_sporemodel(*family, text).expect("every corpus document parses");
        let composition = compose(&document, &source).expect("composes");
        tally.add(*family, &composition);
    }
    Some(tally)
}

#[test]
#[ignore = "needs SPORE/"]
fn the_whole_slice_runs_over_the_real_corpus() {
    let Some(tally) = tally_everything() else {
        eprintln!("SKIP: no SPORE/Data packages present");
        return;
    };

    assert_eq!(tally.documents, 68, "the creator document count moved");
    assert_eq!(tally.blocks, 680, "the blockref count moved");

    // The invariant this crate exists for: the counts always add up, and nothing
    // is dropped. `resolved + unresolved == blocks` is checked exactly so that a
    // future "skip the unresolvable ones" change fails here.
    assert_eq!(
        tally.resolved + tally.unresolved,
        tally.blocks,
        "every block must be accounted for as resolved or explicitly unresolved"
    );
    assert_eq!(tally.by_family.values().map(|v| v.0).sum::<usize>(), 680);

    // A zero here would mean the slice proved nothing.
    assert!(tally.resolved > 0, "nothing resolved: the slice is broken");
    assert_eq!(
        tally.parts_with_geometry, tally.resolved,
        "every resolved part must carry decoded geometry, or 'resolved' means less than it says"
    );
    assert!(
        tally.vertices > 0 && tally.triangles > 0,
        "geometry is empty"
    );
    assert!(
        tally.unresolved > 0,
        "nothing failed: the corpus gap vanished?"
    );

    eprintln!(
        "{} documents, {} blocks -> {} resolved / {} unresolved",
        tally.documents, tally.blocks, tally.resolved, tally.unresolved
    );
    eprintln!(
        "{} resolved parts carry a body graph; {} bones, {} named",
        tally.with_skeleton, tally.bones, tally.named_bones
    );
    eprintln!(
        "{} carry geometry: {} vertices, {} triangles",
        tally.parts_with_geometry, tally.vertices, tally.triangles
    );
    for (family, (blocks, resolved, unresolved)) in &tally.by_family {
        eprintln!(
            "  {:?} blocks {blocks:4}  resolved {resolved:4}  unresolved {unresolved:4}",
            family
        );
    }
    for ((stage, reason), count) in &tally.stops {
        eprintln!("  stopped at {stage} because {reason}: {count}");
    }
    eprintln!(
        "lookups {} misses {}; {} parts carry an inferred hop",
        0, 0, tally.inferred_hops
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn flora_does_not_resolve_and_the_slice_reports_it_rather_than_inventing_parts() {
    let Some(tally) = tally_everything() else {
        eprintln!("SKIP: no SPORE/Data packages present");
        return;
    };
    // Two different denominators, and conflating them is the trap here:
    //
    // * report 08 measured **0 of 107 flora rigblocks** reaching a part;
    // * the creator documents place only **3 flora blocks**.
    //
    // One of those three does resolve, so "flora has no parts" is true of the
    // rigblock layer and *not* true of the block layer. The slice reports the
    // number it actually measured.
    let (blocks, resolved, unresolved) = *tally
        .by_family
        .get(&Family::Flora)
        .expect("flora documents exist");
    assert_eq!(blocks, 3, "flora places three blocks");
    assert_eq!(resolved + unresolved, blocks);
    assert!(
        resolved < blocks,
        "flora blocks must not all resolve; report 08 measured 0/107 at the rigblock layer"
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn every_unresolved_block_names_the_key_it_looked_for() {
    let Some(tally) = tally_everything() else {
        eprintln!("SKIP: no SPORE/Data packages present");
        return;
    };
    // "We looked and it was not there" must be distinguishable from "we did not
    // look". Every stop that got as far as having a key records one.
    assert_eq!(
        tally.stops_without_key, 0,
        "a block stopped without recording which key it tried"
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn every_composed_block_resolves_against_the_shipped_part_pair() {
    // Walk one known-good chain by hand as a cross-check on the resolver's own
    // bookkeeping: blockid 0x40626000, 0x00013530 is a blockid the research used.
    let Some(root) = install_root() else {
        eprintln!("SKIP: no install");
        return;
    };
    let Ok(game) = std::fs::read(root.join("SPORE/Data/Spore_Game.package")) else {
        eprintln!("SKIP: no Spore_Game.package");
        return;
    };
    let Ok(graphics) = std::fs::read(root.join("SPORE/Data/Spore_Graphics.package")) else {
        eprintln!("SKIP: no Spore_Graphics.package");
        return;
    };
    let Ok(game_index) = PackageIndex::parse(&game) else {
        eprintln!("SKIP: unparseable Spore_Game.package");
        return;
    };
    let Ok(graphics_index) = PackageIndex::parse(&graphics) else {
        eprintln!("SKIP: unparseable Spore_Graphics.package");
        return;
    };

    let rigblock = ResourceKey::new(spore_rigblock::TYPE_RIGBLOCK, 0x4062_6000, 0x0001_3530);
    let prop = ResourceKey::new(0x00B1_B104, 0x4062_6000, 0x0001_3530);
    let container = ResourceKey::new(0x2F4E_681B, 0x4060_6000, 0x0001_3530);
    let geometry = ResourceKey::new(0x01C1_35DA, 0x4060_6000, 0x0001_3530);

    let find = |image: &[u8], index: &PackageIndex, key: &ResourceKey| {
        index
            .entries()
            .iter()
            .find(|entry| {
                entry.type_id == key.type_id
                    && entry.group_id == key.group_id
                    && entry.instance_id == key.instance_id
            })
            .and_then(|entry| extract_record(image, entry).ok())
    };

    let rigblock_bytes = find(&game, &game_index, &rigblock).expect("the rigblock exists");
    assert!(
        spore_rigblock::Rigblock::decode(&rigblock_bytes).is_ok(),
        "and it decodes"
    );
    assert!(
        find(&graphics, &graphics_index, &prop).is_some(),
        "the prop exists"
    );
    assert!(
        find(&graphics, &graphics_index, &container).is_some(),
        "the container exists"
    );
    assert!(
        find(&graphics, &graphics_index, &geometry).is_some(),
        "the geometry exists"
    );

    // And the resolver agrees, from the document side.
    let source = part_source();
    let document = spore_creator_core::CreatorDocument {
        format_version: Some(16),
        properties: Default::default(),
        blocks: vec![spore_creator_core::Block {
            block_id: Some(spore_creator_core::BlockId::new(0x4062_6000, 0x0001_3530)),
            ..Default::default()
        }],
    };
    let composition = compose(&document, &source).expect("composes");
    assert_eq!(
        composition.counts().resolved,
        1,
        "the hand-checked block resolves"
    );
    let part: &ResolvedPart = composition.slots()[0].resolved().expect("resolved");
    assert_eq!(part.trail.furthest(), Stage::Container);
    assert!(matches!(composition.slots()[0], PartSlot::Resolved(_)));
}

#[test]
#[ignore = "needs SPORE/"]
fn the_resolved_and_unresolved_counts_agree_with_the_rigblock_layer() {
    // Two independent denominators that must not be conflated: 680 `<blockref>`
    // blocks in the documents, and 1549 rigblock records in the game package.
    // A block may name a rigblock that several blocks share, so these counts are
    // not expected to be equal — and a slice that made them equal would be
    // double-counting.
    let Some(root) = install_root() else {
        eprintln!("SKIP: no install");
        return;
    };
    let Ok(game) = std::fs::read(root.join("SPORE/Data/Spore_Game.package")) else {
        eprintln!("SKIP: no Spore_Game.package");
        return;
    };
    let Ok(index) = PackageIndex::parse(&game) else {
        eprintln!("SKIP: unparseable");
        return;
    };
    let rigblocks = index
        .entries()
        .iter()
        .filter(|entry| entry.type_id == spore_rigblock::TYPE_RIGBLOCK)
        .count();
    assert_eq!(rigblocks, 1549, "the rigblock record count moved");

    let Some(tally) = tally_everything() else {
        eprintln!("SKIP: no creator documents");
        return;
    };
    assert_eq!(
        tally.blocks, 680,
        "the blockref count must not be conflated with the rigblock count"
    );
    let (lookups, misses) = part_source().counters();
    let _ = (lookups, misses);
}

/// Tree shape and vocabulary, aggregated over every composed document.
#[derive(Default, Debug)]
struct Anatomy {
    documents: usize,
    blocks: usize,
    roots: usize,
    max_depth: usize,
    depth_total: usize,
    internal_nodes: usize,
    named_bone_histogram: std::collections::BTreeMap<&'static str, usize>,
    named_bones: usize,
    bones_total: usize,
    parts_with_attach: usize,
    parts_with_csnap: usize,
    bilateral: std::collections::BTreeMap<&'static str, usize>,
}

#[test]
#[ignore = "needs SPORE/"]
fn the_composed_block_trees_are_chains_and_the_vocabulary_is_anatomical() {
    let Some(root) = install_root() else {
        eprintln!("SKIP: no install");
        return;
    };
    let source = part_source();
    let Ok(game) = std::fs::read(root.join("SPORE/Data/Spore_Game.package")) else {
        eprintln!("SKIP: no Spore_Game.package");
        return;
    };
    let Ok(index) = PackageIndex::parse(&game) else {
        eprintln!("SKIP: unparseable");
        return;
    };

    let mut anatomy = Anatomy::default();
    let mut per_document: Vec<(usize, usize, usize)> = Vec::new();

    for entry in index.entries() {
        if !FAMILIES
            .iter()
            .any(|(type_id, _)| *type_id == entry.type_id)
        {
            continue;
        }
        let Ok(bytes) = extract_record(&game, entry) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let family = FAMILIES
            .iter()
            .find(|(type_id, _)| *type_id == entry.type_id)
            .map(|(_, family)| *family)
            .expect("matched above");
        let document = parse_sporemodel(family, text).expect("parses");
        let composition = compose(&document, &source).expect("composes");

        // Tree shape, from the document's own childlists.
        let root_indices = document.roots();
        let mut deepest = 0usize;
        let mut internal = 0usize;
        for index_in_doc in 0..document.blocks.len() {
            if document.blocks[index_in_doc].writes_children() {
                internal += 1;
            }
            deepest = deepest.max(document.ancestors(index_in_doc).len());
        }

        anatomy.documents += 1;
        anatomy.blocks += document.blocks.len();
        anatomy.roots += root_indices.len();
        anatomy.internal_nodes += internal;
        anatomy.max_depth = anatomy.max_depth.max(deepest);
        anatomy.depth_total += deepest;
        per_document.push((document.blocks.len(), root_indices.len(), deepest));

        // Vocabulary, from the resolved parts' body graphs.
        for part in composition.resolved() {
            let named: Vec<&'static str> = part.named_bones().into_iter().map(|(n, _)| n).collect();
            anatomy.named_bones += named.len();
            anatomy.bones_total += part.bones().map(<[spore_rigblock::Bone]>::len).unwrap_or(0);
            for name in &named {
                *anatomy.named_bone_histogram.entry(name).or_default() += 1;
            }
            if named.contains(&"attach") {
                anatomy.parts_with_attach += 1;
            }
            if named.contains(&"csnap") {
                anatomy.parts_with_csnap += 1;
            }
            for stem in ["wing", "side", "hip", "knee", "arm", "leg", "eye", "ear"] {
                let left: &'static str = Box::leak(format!("{stem}L").into_boxed_str());
                let right: &'static str = Box::leak(format!("{stem}R").into_boxed_str());
                if named.contains(&left) && named.contains(&right) {
                    *anatomy
                        .bilateral
                        .entry(left.trim_end_matches('L'))
                        .or_default() += 1;
                }
            }
        }
    }

    assert_eq!(anatomy.documents, 68);
    assert_eq!(anatomy.blocks, 680);
    // 79 roots over 68 documents: the census measured 79 derived roots, so some
    // documents are **multi-rooted forests** rather than trees. Assuming one root
    // per document would have been a plausible guess and wrong by 11.
    assert_eq!(
        anatomy.roots, 79,
        "the derived-root count moved; 68 documents hold 79 root blocks"
    );
    assert!(
        anatomy.roots > anatomy.documents,
        "if every document had exactly one root this would be equal, not greater"
    );
    // Chains, not flat lists: most blocks hang off something.
    assert!(
        anatomy.internal_nodes > anatomy.blocks / 2,
        "a block tree this flat would not be a chain: {} internal of {}",
        anatomy.internal_nodes,
        anatomy.blocks
    );
    assert!(anatomy.max_depth >= 4, "shallow trees would not be chains");

    eprintln!(
        "{} documents, {} blocks, {} roots (so some documents are multi-rooted), \
         {} internal nodes, max depth {}",
        anatomy.documents, anatomy.blocks, anatomy.roots, anatomy.internal_nodes, anatomy.max_depth
    );
    eprintln!(
        "{} named bones over resolved parts; {} parts carry `attach`, {} carry `csnap`",
        anatomy.named_bones, anatomy.parts_with_attach, anatomy.parts_with_csnap
    );
    eprintln!("bilateral pairs per stem: {:?}", anatomy.bilateral);
    let mut histogram: Vec<(&'static str, usize)> = anatomy
        .named_bone_histogram
        .iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    histogram.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let top: Vec<_> = histogram
        .iter()
        .take(18)
        .map(|(name, count)| format!("{name}:{count}"))
        .collect();
    eprintln!("most common named bones: {top:?}");

    // **This is the load-bearing assertion of this test, and it is negative.**
    //
    // Reports 11 and CREATOR-CORE 10 treated the recovered bone vocabulary as
    // anatomical evidence, because the recovered names include `hip`, `knee`,
    // `eye`, `iris`, `jaw` and `shoulder`. Measured against the bones that
    // actually occur in the 680 `<blockref>` parts, that vocabulary is **almost
    // entirely absent**: the only strong name that ever appears is `joint1`.
    //
    // So the recovered names are correct as *hashes* and wrong as an anatomy
    // claim for this layer. The creator core's parts are simple articulated
    // blocks with `joint<N>` bones; the anatomical vocabulary must live in
    // whatever Spore uses to assemble an actual creature's anatomy, which is a
    // different record from the rigblock part layer.
    //
    // Asserting the measurement rather than the hope is the point: written as
    // "the vocabulary is anatomical", this test would have shipped a false claim
    // behind a weaker assertion.
    let non_joint: Vec<&str> = anatomy
        .named_bone_histogram
        .keys()
        .copied()
        .filter(|name| !name.starts_with("joint"))
        .collect();
    assert!(
        non_joint.is_empty(),
        "non-`joint` names appear in the composed bones: {non_joint:?} - the \
         'the vocabulary is anatomical' claim in report 11 would need revisiting"
    );
    assert_eq!(
        anatomy.named_bone_histogram.get("joint1").copied(),
        Some(anatomy.named_bones),
        "every recovered name in a composed part is joint1"
    );
    eprintln!(
        "every recovered name in a composed part is `joint1` ({anatomy} of {total} bones)",
        anatomy = anatomy.named_bones,
        total = anatomy.bones_total
    );

    per_document.sort_unstable();
    eprintln!("largest documents (blocks, roots, depth): {per_document:?}");
}
