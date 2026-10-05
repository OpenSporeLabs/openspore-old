//! The catalogue over the real install: `#[ignore]`d, and it prints its numbers.
//!
//! ```bash
//! cargo test -p spore-morphology --release -- --ignored --nocapture
//! ```
//!
//! Every number below is printed rather than only asserted, because the point of
//! a corpus census is that the numbers can be **checked** — against
//! `docs/CREATOR-CORE.md` §11 and `spore-rigblock`'s claim table, which state
//! some of them independently. A number that is only asserted cannot be
//! compared with anything.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use spore_core::ResourceKey;
use spore_creator_core::{parse_sporemodel, BlockId, CreatorDocument, Family};
use spore_dbpf::{extract_record, PackageIndex};
use spore_morphology::catalog::{Absent, DocumentRecord};
use spore_morphology::{Catalog, PackageSource};

/// The six type ids measured to hold `<sporemodel>` records, with their families.
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

/// One decoded creator document, with the record it came from.
type CorpusDocument = (ResourceKey, Family, CreatorDocument);

/// The catalogue over the whole install, or `None` when there is no install.
///
/// 68 documents: 47 `crt`, 13 `cll`, 4 `vcl`, 2 `ufo`, 1 `bld`, 1 `flr`.
fn creator_documents() -> Vec<CorpusDocument> {
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
        let Some((_, family)) = FAMILIES.iter().find(|(t, _)| *t == entry.type_id) else {
            continue;
        };
        let Ok(bytes) = extract_record(&image, entry) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        if let Ok(document) = parse_sporemodel(*family, text) {
            out.push((entry.key, *family, document));
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

/// The catalogue over the whole install, or `None` when there is no install.
fn corpus() -> Option<(Catalog, Vec<CorpusDocument>)> {
    let documents = creator_documents();
    if documents.is_empty() {
        return None;
    }
    let source = part_source();
    assert_eq!(
        source.package_count(),
        2,
        "a part spans two packages; a source holding one reports every part \\
         unresolved at the prop stage and looks like a resolver bug"
    );
    let records: Vec<DocumentRecord<'_>> = documents
        .iter()
        .map(|(key, family, document)| DocumentRecord {
            key: *key,
            family: *family,
            document,
        })
        .collect();
    Some((Catalog::build(&source, &records), documents))
}

#[test]
#[ignore = "needs SPORE/"]
fn the_catalogue_over_the_real_install_prints_every_number_it_claims() {
    let Some((catalog, documents)) = corpus() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };
    let counts = catalog.counts();

    eprintln!("--- population ---");
    eprintln!("documents: {}", documents.len());
    eprintln!(
        "  by family: {:?}",
        documents
            .iter()
            .fold(BTreeMap::new(), |mut acc, (_, family, _)| {
                *acc.entry(family.name()).or_insert(0usize) += 1;
                acc
            })
    );
    eprintln!("blocks: {}", counts.references);
    eprintln!("distinct (group, instance) entries: {}", counts.entries);
    eprintln!("document-to-part relationships: {}", counts.documents);

    eprintln!("--- resolution ---");
    eprintln!("resolved to a part: {}", counts.entries - counts.unresolved);
    eprintln!(
        "authorable (geometry decoded, no stop): {}",
        counts.authorable
    );
    eprintln!("with geometry: {}", counts.with_geometry);
    eprintln!("unresolved: {}", counts.unresolved);
    eprintln!("grades: {:?}", counts.grades);
    for ((stage, reason), count) in &counts.stops {
        eprintln!("  stopped at {stage} because {reason}: {count}");
    }

    eprintln!("--- skeleton ---");
    eprintln!("entries with a body graph: {}", counts.with_skeleton);
    eprintln!("bones: {}", counts.bones);
    eprintln!("bones with a recovered name: {}", counts.named_bones);
    eprintln!("names actually in use: {:?}", catalog.bone_name_histogram());

    eprintln!("--- dependencies ---");
    eprintln!(
        "entries whose container declared >= 1 cross-record reference: {}",
        catalog
            .entries()
            .iter()
            .filter(|entry| {
                entry
                    .dependencies
                    .value()
                    .is_some_and(|dependencies| !dependencies.is_empty())
            })
            .count()
    );
    eprintln!(
        "raster (0x2F4E681C) keys reported: {}",
        catalog
            .entries()
            .iter()
            .filter_map(|entry| entry.dependencies.value())
            .map(|dependencies| dependencies.rasters().count())
            .sum::<usize>()
    );

    eprintln!("--- absence accounting ---");
    eprintln!("fact groups that produced a value: {}", counts.absent_facts);
    eprintln!(
        "non-findings that describe the reader, not the data: {}",
        counts.not_a_finding
    );
    eprintln!("corpus findings by code: {:?}", counts.absent_codes);

    eprintln!("--- bounds box vs geometry extent ---");
    for (label, mut values) in [
        (
            "geometry / rigblock cell-derived span",
            axis_ratios(&catalog, BoundsReading::CellDerived),
        ),
        (
            "geometry / half of the cell-derived span",
            axis_ratios(&catalog, BoundsReading::HalfOfCellDerived),
        ),
        (
            "geometry / rigblock header-declared span",
            axis_ratios(&catalog, BoundsReading::HeaderDeclared),
        ),
        (
            "widest axis extent / rigblock widest header offset (one per part)",
            axis_ratios(&catalog, BoundsReading::WidestHeaderOffset),
        ),
    ] {
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
        let at = |q: f32| -> f32 {
            if values.is_empty() {
                return 0.0;
            }
            let index = ((values.len() as f32 - 1.0) * q).round() as usize;
            values[index.min(values.len() - 1)]
        };
        eprintln!(
            "  {label}: n={} min={:.3} p25={:.3} median={:.3} p75={:.3} max={:.3}",
            values.len(),
            values.first().copied().unwrap_or(0.0),
            at(0.25),
            at(0.5),
            at(0.75),
            values.last().copied().unwrap_or(0.0),
        );
    }
    eprintln!(
        "entries whose rigblock carries no cells at all (k == 0): {}",
        catalog
            .entries()
            .iter()
            .filter(|entry| entry.rigblock.value().is_some_and(|b| !b.has_cells()))
            .count()
    );
    eprintln!(
        "entries with no comparison available: {}",
        catalog
            .entries()
            .iter()
            .filter(|entry| !entry.extents.is_comparable())
            .count()
    );

    eprintln!("--- provenance ---");
    let histogram: BTreeMap<usize, usize> =
        catalog
            .entries()
            .iter()
            .fold(BTreeMap::new(), |mut acc, entry| {
                *acc.entry(entry.reference_count()).or_insert(0usize) += 1;
                acc
            });
    eprintln!("blocks per entry histogram: {histogram:?}");
    let most = catalog
        .entries()
        .iter()
        .max_by_key(|entry| entry.reference_count())
        .expect("entries");
    eprintln!("most-referenced entry: {}", most.summary());
    eprintln!("  0x{:08x}:0x{:08x}", most.group_id(), most.instance_id());
}

/// Per-axis ratios over the entries whose two extents both exist.
///
/// `reading` picks which of the rigblock's measurements is the denominator,
/// because they are not the same number and a report that quotes one without
/// naming it has said nothing checkable.
fn axis_ratios(catalog: &Catalog, reading: BoundsReading) -> Vec<f32> {
    let mut out = Vec::new();
    for entry in catalog.entries() {
        let Some((low, high)) = entry.extents.geometry_extent else {
            continue;
        };
        let extent = (0..3).map(|axis| high[axis] - low[axis]);
        match reading {
            BoundsReading::WidestHeaderOffset => {
                let Some(divisor) = entry.rigblock.value().and_then(|b| b.widest_offset) else {
                    continue;
                };
                // The projection test's own normalisation: the geometry's LARGEST
                // axis extent over the rigblock's widest offset, one number per
                // part rather than per axis.
                out.push(extent.fold(0.0f32, f32::max) / divisor);
            }
            _ => {
                let span = match reading {
                    BoundsReading::HeaderDeclared => entry
                        .rigblock
                        .value()
                        .and_then(|bounds| bounds.declared_span),
                    _ => entry.rigblock.value().and_then(|bounds| bounds.span),
                };
                let Some(span) = span else { continue };
                for (axis, extent) in extent.enumerate() {
                    let divisor = match reading {
                        BoundsReading::HalfOfCellDerived => span[axis] / 2.0,
                        _ => span[axis],
                    };
                    if divisor > 0.0 && extent > 0.0 {
                        out.push(extent / divisor);
                    }
                }
            }
        }
    }
    out
}

/// Which rigblock measurement is the denominator of an extent ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BoundsReading {
    /// The extent derived from the occupancy cells.
    CellDerived,
    /// Half of that extent, which is how a "half-extent" claim is usually written.
    HalfOfCellDerived,
    /// The span the record's own header declares, independent of its cells.
    HeaderDeclared,
    /// The furthest the header's six floats reach from the origin — the reading
    /// `tests/projection.rs` measures its worst-case ratio against, so quoting a
    /// median for it makes the two measurements comparable.
    WidestHeaderOffset,
}

#[test]
#[ignore = "needs SPORE/"]
fn the_catalogue_population_is_the_block_population_deduplicated() {
    let Some((catalog, documents)) = corpus() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };
    let counts = catalog.counts();

    assert_eq!(documents.len(), 68, "the creator document count moved");
    assert_eq!(counts.references, 680, "the <blockref> count moved");
    assert_eq!(
        counts.entries, 60,
        "680 blocks name 60 distinct (group, instance) pairs"
    );
    assert!(
        counts.entries < counts.references / 4,
        "a catalogue listing one row per block would be a census of blocks \
         pretending to be a list of parts"
    );

    // Every entry is reachable from at least one block, and every reference names
    // an entry that exists: the two index spaces agree.
    let identities: BTreeSet<(u32, u32)> = catalog
        .entries()
        .iter()
        .map(|entry| (entry.group_id(), entry.instance_id()))
        .collect();
    let mut named: BTreeSet<(u32, u32)> = BTreeSet::new();
    for (_, _, document) in &documents {
        for block in &document.blocks {
            if let Some(BlockId {
                group_id,
                instance_id,
            }) = block.block_id
            {
                named.insert((group_id, instance_id));
            }
        }
    }
    assert_eq!(identities, named, "the entry set is the named-identity set");
    assert_eq!(
        counts.references,
        documents
            .iter()
            .map(|(_, _, document)| document.blocks.len())
            .sum::<usize>()
    );

    // 58 of the 68 documents place exactly one distinct part, and 46 of the 47
    // creature documents do. A creature document is a repeated block.
    let single = documents
        .iter()
        .filter(|(_, _, document)| {
            let distinct: BTreeSet<_> = document.blocks.iter().filter_map(|b| b.block_id).collect();
            distinct.len() == 1
        })
        .count();
    assert_eq!(single, 58, "the single-part document count moved");
    let creature_single = documents
        .iter()
        .filter(|(key, family, document)| {
            *family == Family::Creature
                && key.type_id == Family::Creature.type_id()
                && document
                    .blocks
                    .iter()
                    .filter_map(|b| b.block_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == 1
        })
        .count();
    assert_eq!(creature_single, 46, "46 of the 47 creature documents");
    eprintln!(
        "{single}/68 documents place one distinct part; {creature_single}/47 creature \\
         documents do"
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn the_vocabulary_reported_is_the_one_in_use() {
    let Some((catalog, _)) = corpus() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };
    let histogram = catalog.bone_name_histogram();
    let counts = catalog.counts();
    eprintln!(
        "{} entries, {} body graphs, {} bones, {} named, histogram {histogram:?}",
        counts.entries, counts.with_skeleton, counts.bones, counts.named_bones
    );

    // The negative assertion, on purpose. 244 bone identities are recovered and
    // the set includes `hip`, `knee`, `eye`, `iris`, `jaw` and `shoulder`; over
    // the distinct parts a creator document actually names, none of them appear.
    // Every reported name starts with `joint`.
    let anatomical: Vec<&str> = histogram
        .keys()
        .copied()
        .filter(|name| !name.starts_with("joint"))
        .collect();
    assert!(
        anatomical.is_empty(),
        "non-`joint` names appear in the creator-referenced parts: {anatomical:?}"
    );
    assert!(
        counts.named_bones <= counts.bones,
        "a name count above the bone count would be a duplicated dictionary entry"
    );
    assert!(
        counts.named_bones < counts.bones,
        "every bone being named would mean the dictionary was presented as the \
         population; over the blocks it is 557 of 677"
    );
    assert!(
        spore_rigblock::vocabulary::entries().len() > counts.named_bones,
        "the recovered vocabulary is far larger than what any of these parts uses"
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn no_part_declares_a_material_or_raster_record_reference() {
    let Some((catalog, _)) = corpus() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };
    let mut with_declarations = 0usize;
    let mut rasters = 0usize;
    for entry in catalog.entries() {
        let Some(dependencies) = entry.dependencies.value() else {
            continue;
        };
        if !dependencies.is_empty() {
            with_declarations += 1;
            eprintln!("{} declares {:?}", entry.summary(), dependencies.declared);
        }
        rasters += dependencies.rasters().count();
    }
    eprintln!(
        "{with_declarations} entries declare a cross-record reference; {rasters} raster \
         keys in total"
    );

    // Measured over all 1824 `rw4` records at 0x40606000: zero validated
    // `{instance, type, group, 0x00A00CE4}` entries of any type. The part layer
    // has no material or raster *record* dependency, and this is where the
    // catalogue says so instead of printing an invented key.
    assert_eq!(
        with_declarations, 0,
        "a part container declared a cross-record reference, which contradicts the \\
         measurement this module's claims rest on"
    );
    assert_eq!(rasters, 0, "and no raster key is reported as a dependency");

    // Every entry's dependency group is `Found` even when empty, because the
    // container was read. An empty collection here is a measurement.
    for entry in catalog.entries() {
        if let Some(stop) = &entry.unresolved {
            assert_eq!(
                entry.dependencies.absent(),
                Some(&Absent::NotAttempted { stage: stop.stage }),
                "{}: the chain stopped before the container, so the scan never ran",
                entry.summary()
            );
            continue;
        }
        assert_eq!(
            entry.dependencies.absent(),
            None,
            "{}: the container was read and holds no reference, which is a finding",
            entry.summary()
        );
    }
}

#[test]
#[ignore = "needs SPORE/"]
fn a_miss_an_error_and_a_never_attempted_lookup_stay_three_things() {
    let Some((catalog, _)) = corpus() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };

    // A real corpus run must produce real misses, and must never produce an
    // unreadable one over a source holding both packages: an unreadable record
    // is a defect in the install or in the reader, not a property of Spore.
    //
    // The misses of this corpus live in `stops`, not in a per-group code, and
    // that is the right place for them: the rigblock and the prop of a stopped
    // entry *were* found, so the finding is "the chain ran out of road here",
    // not "this group has nothing".
    let misses = catalog
        .counts()
        .stops
        .iter()
        .filter(|((_, reason), _)| *reason == "no_such_record")
        .map(|(_, count)| count)
        .sum::<usize>();
    assert!(
        misses > 0,
        "the corpus has entries whose chain stops, so misses must be reported"
    );
    eprintln!("{misses} stops are a miss: a key was tried and held nothing");
    assert!(
        !catalog.counts().absent_codes.contains_key("no_such_record"),
        "no group of a stopped entry failed on its own key; the stop is the finding"
    );
    assert!(
        !catalog.counts().absent_codes.contains_key("unreadable"),
        "an unreadable record over a complete install is a defect, not a finding"
    );
    assert!(
        !catalog.counts().absent_codes.contains_key("not_searched"),
        "both packages were opened, so every lookup really was attempted"
    );
    for entry in catalog.entries() {
        if let Some(stop) = &entry.unresolved {
            assert!(
                stop.tried.is_some(),
                "{}: a stop without the key it tried cannot say 'we looked here'",
                entry.summary()
            );
        }
    }

    // And the difference is visible in the API, not only in the counts: an entry
    // that stopped reports its downstream groups as never-attempted, so a caller
    // cannot read a missing geometry as an absent one.
    let stopped = catalog
        .entries()
        .iter()
        .find(|entry| entry.unresolved.is_some())
        .expect("the corpus has stopped entries");
    assert!(stopped.geometry.absent().is_some());
    assert_eq!(
        stopped.geometry.absent().map(|absent| absent.is_finding()),
        Some(false),
        "a geometry nobody looked for is not a finding about the corpus"
    );
    eprintln!("stopped entry: {}", stopped.summary());
}

#[test]
#[ignore = "needs SPORE/"]
fn the_catalogue_agrees_with_a_direct_composition_of_the_same_blocks() {
    // The catalogue resolves each identity through a one-block document. This
    // drives the real documents through `compose` and checks that the two agree
    // on every block, so the deduplication cannot have changed a verdict.
    let Some((catalog, documents)) = corpus() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };
    let source = part_source();

    let mut blocks = 0usize;
    let mut resolved = 0usize;
    let mut inferred = 0usize;
    let mut named = 0usize;
    let mut bone_histogram: BTreeMap<&'static str, usize> = BTreeMap::new();
    for (_, _, document) in &documents {
        let composition = spore_morphology::compose(document, &source).expect("composes");
        let counts = composition.counts();
        blocks += counts.total();
        resolved += counts.resolved;
        for part in composition.resolved() {
            if part.trail.weakest_grade() == spore_morphology::Grade::Inferred {
                inferred += 1;
            }
            for (name, _) in part.named_bones() {
                named += 1;
                *bone_histogram.entry(name).or_insert(0usize) += 1;
            }
        }
    }
    eprintln!(
        "{blocks} blocks -> {resolved} resolved, {inferred} with an inferred hop, \
         {named} named bones, histogram {bone_histogram:?}"
    );

    assert_eq!(blocks, 680, "the <blockref> count moved");
    assert_eq!(resolved, 678, "the per-block resolved count moved");
    assert_eq!(
        inferred, 485,
        "485 of 678 resolved blocks carry a prop pointer that disagrees with the \
         block's own instance; deduplicating by identity drops that to a much \
         smaller number, which is exactly why both are reported"
    );
    assert_eq!(
        bone_histogram.keys().copied().collect::<Vec<_>>(),
        vec!["joint1"],
        "every recovered name over the blocks is joint1"
    );
    assert_eq!(named, 557, "the named-bone count over the blocks moved");

    // The catalogue's own population is the deduplicated one, so its named-bone
    // count is necessarily smaller — and that gap is the reason the two numbers
    // are both printed.
    assert!(
        catalog.counts().named_bones < named,
        "60 entries cannot hold more named bones than 680 blocks"
    );
    let entries = catalog.counts();
    assert_eq!(
        entries.with_geometry + entries.unresolved,
        entries.entries,
        "every entry either decoded its geometry or stopped, and the two add up"
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn the_catalogue_answers_the_browse_and_replace_question() {
    let Some((catalog, _)) = corpus() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };
    let authorable: Vec<_> = catalog.authorable().collect();
    eprintln!(
        "{} of {} entries are placeable",
        authorable.len(),
        catalog.counts().entries
    );
    for entry in authorable.iter().take(5) {
        eprintln!("  {}", entry.summary());
        eprintln!(
            "    rigblock span {:?}, geometry extent {:?}, per-axis ratio {:?}",
            entry.extents.rigblock_span, entry.extents.geometry_extent, entry.extents.axis_ratio
        );
    }

    assert!(
        !authorable.is_empty(),
        "a catalogue with nothing placeable is not a catalogue"
    );
    // Every placeable entry's alternatives are placeable too, or the catalogue is
    // offering a replacement that cannot be placed — which is the one thing a
    // browse list must never do.
    let anchor = authorable[0];
    let alternatives: Vec<_> = catalog.alternatives(anchor.id).collect();
    assert_eq!(alternatives.len(), catalog.counts().entries - 1);
    assert!(alternatives.iter().all(|entry| !entry.id.eq(&anchor.id)));

    // The group id is the document family's bucket, not an anatomy class: the
    // alternatives span every family the corpus uses.
    let groups: BTreeSet<u32> = alternatives.iter().map(|entry| entry.group_id()).collect();
    eprintln!("alternatives span {} group ids: {groups:?}", groups.len());
    assert!(
        groups.len() > 1,
        "if every entry shared one group the alternatives list would be a family, \
         not a set of parts"
    );

    // And the identity round-trips through the canonical spelling, because that is
    // what a `<blockid>` is written with.
    for entry in &authorable {
        let spec = format!("0x{:08x}, 0x{:08x}", entry.group_id(), entry.instance_id());
        assert_eq!(
            spore_creator_core::BlockId::parse(&spec, 0).expect("parses"),
            entry.id,
            "an entry's identity must round-trip through the corpus spelling"
        );
        let key = ResourceKey::new(
            spore_rigblock::TYPE_RIGBLOCK,
            entry.group_id(),
            entry.instance_id(),
        );
        assert_eq!(
            ResourceKey::parse_tgi(&key.to_tgi()).expect("parses"),
            key,
            "and through the canonical T:G:I spelling"
        );
    }
}
