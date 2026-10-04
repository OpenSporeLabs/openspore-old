//! Is **material-info block *i* mesh *i*?** The load-bearing inference, measured.
//!
//! **Every test here is `#[ignore]`d and every one skips cleanly when the game is
//! not installed.** The bytes come from `SPORE/Data/Spore_Content.package`; no
//! Spore byte is copied, embedded or committed.
//!
//! ```text
//! cargo test -p spore-differential --release --test material_block_binding -- --ignored --nocapture
//! ```
//!
//! # What rests on it
//!
//! `spore_gmdl` flattens every material-info block's texture set into one list
//! and does not record which block each reference came from. `spore-material`
//! therefore cuts that list by **dividing by the mesh count** and hands each mesh
//! `total / meshes` references, and `BindingScope::is_inferred()` is `true` for
//! that reason. The engine renders per-mesh material bindings from it.
//!
//! The division needs **two** things, and this file separates them because they
//! have different standing:
//!
//! | assumption | how it is tested | standing |
//! |---|---|---|
//! | every block of a record contributed the same number of references | the oracle's per-block counts | measured, asserted exactly |
//! | the block count equals the mesh count | oracle block count vs `model.meshes.len()` | measured, asserted exactly |
//! | **block *i* is mesh *i*** | **nothing in the record states it** | **UNKNOWN** |
//!
//! So this file can only measure the first two. What it does for the third is
//! quantify **how much work the inference is doing**, which is the useful part of
//! an unprovable assumption:
//!
//! * records where every block names the **same** texture set: the block order
//!   carries no information, so a wrong order would be invisible;
//! * records where blocks **differ**: the order is load-bearing, and a
//!   permutation would move textures between meshes with nothing to detect it;
//! * slots whose *derived* ordinal disagrees with the referenced record's group
//!   id low byte -- two independent statements of one fact. `spore-material`
//!   calls these `ordinal_conflicts` and the engine reports them; the creature
//!   record carries one.
//!
//! # The oracle is the only source of block boundaries
//!
//! `spore_gmdl` destroyed them, so they are read from `gmdl.py`'s own
//! `materialInfos` tree through the driver. That is the Python reference, not a
//! Rust re-derivation, which is exactly what makes the comparison independent of
//! the code under test. `cmd_gmdl` reports `material_info_groups`,
//! `block_tex_counts` and `block_tex_sets`; the first was already there and the
//! other two were added for this file because no reply could express the
//! question.

use std::collections::BTreeMap;
use std::path::PathBuf;

use spore_assets::{ContentStore, Package};
use spore_core::record::type_id;
use spore_differential::{sample::sample_indices, Json, OracleConfig, OracleSession};

const PACKAGE: &str = "SPORE/Data/Spore_Content.package";

/// How many gmdl records the oracle is asked about. The oracle's own walk dies on
/// an impossible baked-deform count in the anim-data section, so a bounded
/// deterministic sample is the only shape this question can have; the budget is
/// stated rather than tuned.
const BUDGET: usize = 1_200;

/// The documented creature: `0x00e6bce5:0x40627100:0x067c79d2`, two meshes, and
/// the record whose mesh 1 carries an `ordinal_conflicts=1`.
const CREATURE: spore_core::ResourceKey =
    spore_core::ResourceKey::new(type_id::GMDL, 0x4062_7100, 0x067C_79D2);

fn content_store() -> Option<(ContentStore, PathBuf)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .ok()?;
    let path = root.join(PACKAGE);
    if !path.exists() {
        println!("SKIP: {PACKAGE} is not installed");
        return None;
    }
    let mut store = ContentStore::new();
    store.push(Package::open("Spore_Content", path.clone()).expect("package must open"));
    Some((store, path))
}

/// One record's block structure, from the oracle, beside the Rust walk's mesh
/// count.
struct Observation {
    key: spore_core::ResourceKey,
    meshes: usize,
    blocks: usize,
    /// Texture-set references per block, in block order.
    per_block: Vec<usize>,
    /// The flattened reference list, as `(instance, group)`.
    refs: Vec<(u32, u32)>,
}

fn ask(session: &mut OracleSession, row: usize) -> Option<Observation> {
    let reply = session.request_record("gmdl", row).ok()?;
    if reply.get("ok").and_then(Json::as_bool) != Some(true) {
        return None;
    }
    let meshes = reply.get("mesh_count").and_then(Json::as_u32).unwrap_or(0) as usize;
    let blocks = reply
        .get("material_info_groups")
        .and_then(Json::as_u32)
        .unwrap_or(0) as usize;
    let per_block = reply
        .get("block_tex_counts")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_u32().unwrap_or(0) as usize)
                .collect()
        })
        .unwrap_or_default();
    let refs = reply
        .get("texture_refs")
        .and_then(Json::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let pair = item.as_array().unwrap_or_default();
                    (
                        pair.first().and_then(Json::as_u32).unwrap_or(0),
                        pair.get(1).and_then(Json::as_u32).unwrap_or(0),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Observation {
        key: spore_core::ResourceKey::new(type_id::GMDL, 0, 0),
        meshes,
        blocks,
        per_block,
        refs,
    })
}

/// Whether two blocks of one record name the same texture set, which is what
/// decides whether the block ORDER carries information for that record.
fn blocks_are_distinct(sets: &[Vec<Vec<u32>>]) -> bool {
    let mut distinct: Vec<&Vec<Vec<u32>>> = Vec::new();
    for set in sets {
        if !distinct.contains(&set) {
            distinct.push(set);
        }
    }
    distinct.len() > 1
}

/// The corpus measurement.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn the_per_mesh_cut_is_measured_where_it_can_be_and_reported_where_it_cannot() {
    let Some((store, path)) = content_store() else {
        return;
    };
    let package = &store.packages()[0];
    let image = package.bytes();
    let rows: Vec<(usize, spore_dbpf::DbpfEntry)> = package
        .index()
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.type_id == type_id::GMDL)
        .map(|(row, entry)| (row, *entry))
        .collect();
    let config = OracleConfig::detect();
    assert!(
        config.is_available(),
        "SKIP: the oracle driver is unavailable: {}",
        config.unavailable_reason().unwrap_or_default()
    );
    let mut session = OracleSession::spawn(&config).expect("driver must start");
    session.open(&path).expect("driver must open the package");

    let sampled = sample_indices(rows.len(), BUDGET);
    let mut walked = 0usize;
    let mut oracle_refused = 0usize;
    let mut block_count_disagrees_with_mesh_count = 0usize;
    let mut blocks_are_not_uniform = 0usize;
    let mut multi_block_records = 0usize;
    let mut block_order_is_load_bearing = 0usize;
    let mut records_with_no_blocks = 0usize;
    let mut slot_conflicts = 0usize;
    let mut slots_compared = 0usize;
    let mut cut_would_be_wrong = 0usize;
    let mut disagreements: Vec<String> = Vec::new();
    let mut shapes: BTreeMap<(usize, usize, usize), usize> = BTreeMap::new();

    for sample in &sampled {
        let Some((row, entry)) = rows.get(*sample) else {
            continue;
        };
        let Some(mut observation) = ask(&mut session, *row) else {
            oracle_refused += 1;
            continue;
        };
        observation.key = entry.key;
        let bytes = match spore_dbpf::extract_record(image, entry) {
            Ok(bytes) => bytes,
            Err(_) => {
                oracle_refused += 1;
                continue;
            }
        };
        let model = spore_gmdl::parse_recovering(&bytes)
            .expect("the header walk itself does not fail")
            .model;
        walked += 1;

        // --- (1) does the block count equal the mesh count?
        assert_eq!(
            observation.meshes,
            model.meshes.len(),
            "the oracle's meshCount and spore_gmdl's mesh table must agree before anything else \
             here means anything"
        );
        assert_eq!(
            observation.refs.len(),
            model.texture_refs.len(),
            "the oracle's flattened reference list and spore_gmdl's must agree"
        );
        if observation.meshes != observation.blocks {
            block_count_disagrees_with_mesh_count += 1;
            if disagreements.len() < 10 {
                disagreements.push(format!(
                    "{}: {} mesh(es), {} material-info block(s)",
                    entry.key, observation.meshes, observation.blocks
                ));
            }
        }
        if observation.blocks == 0 {
            records_with_no_blocks += 1;
        }

        // --- (2) did every block contribute the same number of references?
        let uniform = observation
            .per_block
            .windows(2)
            .all(|pair| pair[0] == pair[1]);
        if !uniform {
            blocks_are_not_uniform += 1;
            if disagreements.len() < 20 {
                disagreements.push(format!(
                    "{}: uneven blocks {:?}",
                    entry.key, observation.per_block
                ));
            }
        }

        // --- (3) would dividing by the mesh count reproduce the block boundaries?
        //     This is the arithmetic the per-mesh cut rests on, and it is
        //     checkable without believing anything about block ORDER: the cut is
        //     right iff every block held exactly `total / meshes` references.
        let total = observation.refs.len();
        let cut_sound = observation.meshes > 0
            && observation.blocks == observation.meshes
            && uniform
            && total % observation.meshes == 0
            && total / observation.meshes == observation.per_block.first().copied().unwrap_or(0);
        if !cut_sound && total > 0 {
            cut_would_be_wrong += 1;
            if disagreements.len() < 10 {
                disagreements.push(format!(
                    "{}: {} mesh(es), {} block(s), per block {:?}, {} reference(s) flat",
                    entry.key, observation.meshes, observation.blocks, observation.per_block, total
                ));
            }
        }
        *shapes
            .entry((
                observation.meshes,
                observation.blocks,
                observation.per_block.first().copied().unwrap_or(0),
            ))
            .or_default() += 1;

        // --- (4) how much work is the block-ORDER inference doing?
        let sets: Vec<Vec<Vec<u32>>> = {
            let mut out: Vec<Vec<Vec<u32>>> = Vec::with_capacity(observation.blocks);
            let mut at = 0usize;
            for count in &observation.per_block {
                out.push(
                    observation.refs[at..at + count]
                        .iter()
                        .map(|(instance, group)| vec![*instance, *group])
                        .collect(),
                );
                at += count;
            }
            out
        };
        if observation.blocks > 1 {
            multi_block_records += 1;
            if blocks_are_distinct(&sets) {
                block_order_is_load_bearing += 1;
                if disagreements.len() < 20 {
                    disagreements.push(format!(
                        "{}: {} block(s) name DIFFERENT texture sets, so the block order decides \
                         which mesh gets which",
                        entry.key, observation.blocks
                    ));
                }
            }
        }

        // --- (5) the two independent statements of each slot's ordinal.
        if cut_sound {
            let per_mesh = total / observation.meshes;
            for (flat, (_, group)) in observation.refs.iter().enumerate() {
                let derived = (flat % per_mesh) as u32;
                slots_compared += 1;
                if derived != group & 0xFF {
                    slot_conflicts += 1;
                }
            }
        }
    }

    println!("sampled: {}", sampled.len());
    println!("walked by both sides: {walked}, oracle refused: {oracle_refused}");
    println!("block count != mesh count: {block_count_disagrees_with_mesh_count}");
    println!("records with uneven blocks: {blocks_are_not_uniform}");
    println!("records with no blocks: {records_with_no_blocks}");
    println!("records with more than one block: {multi_block_records}");
    println!("records where the block ORDER is load-bearing: {block_order_is_load_bearing}");
    println!("records whose cut would be wrong: {cut_would_be_wrong}");
    println!("slot ordinal conflicts: {slot_conflicts} of {slots_compared}");
    println!("(meshes, blocks, refs-per-block) shapes:");
    for (shape, count) in &shapes {
        println!("  {shape:?}: {count} record(s)");
    }
    for line in &disagreements {
        println!("  disagreement: {line}");
    }

    assert!(walked > 0, "no record was walked by both sides");
    assert_eq!(
        cut_would_be_wrong, 0,
        "dividing the flat reference list by the mesh count must reproduce the material-info \
         block boundaries; these records are the ones where it would not"
    );
    assert_eq!(
        block_count_disagrees_with_mesh_count, 0,
        "the block count must equal the mesh count, or dividing by the mesh count is dividing by \
         a different number"
    );
    assert_eq!(
        blocks_are_not_uniform, 0,
        "every block of a record must contribute the same number of references, or the division \
         is not the block boundaries"
    );
    assert!(
        multi_block_records > 0,
        "no record in the sample had more than one material-info block, so nothing here tested \
         the block-order inference at all"
    );
    // The ordinal cross-check is EXPECTED to have conflicts -- the documented
    // creature carries one -- so it is reported, not asserted clean. Asserted
    // instead: it was compared at all, and the conflict count is small enough
    // that "the cut is systematically wrong" is excluded.
    assert!(slots_compared > 0, "no slot was cross-checked");
    assert!(
        slot_conflicts * 100 < slots_compared,
        "{slot_conflicts} of {slots_compared} slots disagree between the derived ordinal and the \
         group id's low byte, which is too many to be the documented per-record counter-example"
    );
}

/// An independent walk of the material-info section, for the records the oracle
/// cannot reach.
///
/// Written from `gmdl.py`'s own layout rules -- in particular an index buffer's
/// length is `index_count * max(1, index_bits / 8)`, not the fourth header word
/// -- so it is a second implementation rather than a call into `spore-gmdl`. It
/// is validated before it is believed: the flat reference list it collects must
/// equal the one `spore_gmdl` reports, which can only happen if the walk landed
/// on the same byte.
///
/// `spore_gmdl::shader_data_size` is used for the skip sizes, because those are
/// a lookup table rather than an inference and re-deriving them here would be
/// inventing a second source of truth instead of a second opinion.
struct BlockWalk {
    /// The declared material-info block count.
    blocks: usize,
    /// References contributed by each block, in block order. Short when the walk
    /// stopped early.
    per_block: Vec<usize>,
    /// The `(instance, group)` references, flattened in encounter order.
    refs: Vec<(u32, u32)>,
    /// The shader-data id the walk stopped on, if any.
    stopped_on: Option<u32>,
}

fn walk_material_info(data: &[u8]) -> BlockWalk {
    fn le(data: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
    }
    fn be(data: &[u8], at: usize) -> Option<u32> {
        Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
    }

    let mut at = 0usize;
    assert_eq!(le(data, at), Some(spore_gmdl::SUPPORTED_VERSION));
    at += 4;
    let ref_count = be(data, at).expect("refCount") as usize;
    at += 4 + ref_count * 12;
    let mesh_count = le(data, at).expect("meshCount") as usize;
    at += 4;
    at += 28; // bbox min/max + radius

    let index_buffers = le(data, at).expect("numIndexBuffers") as usize;
    at += 4;
    for _ in 0..index_buffers {
        let count = le(data, at + 4).expect("indexCount") as usize;
        let bits = le(data, at + 8).expect("indexBits") as usize;
        at += 16 + count * bits.div_ceil(8).max(1);
    }
    let descriptors = le(data, at).expect("numVertexDescriptors") as usize;
    at += 4;
    for _ in 0..descriptors {
        let elements = le(data, at).expect("elementCount") as usize;
        at += 4 + elements * 12;
    }
    let vertex_buffers = le(data, at).expect("numVertexBuffers") as usize;
    at += 4;
    for _ in 0..vertex_buffers {
        let size = le(data, at + 8).expect("vertexBufferSize") as usize;
        at += 12 + size;
    }
    at += mesh_count * 8; // mesh table
    at += mesh_count * 4; // material ids
    at += 4; // the unknown word before the material-info table

    let blocks = le(data, at).expect("materialInfoCount") as usize;
    at += 4;
    let mut walk = BlockWalk {
        blocks,
        per_block: Vec::new(),
        refs: Vec::new(),
        stopped_on: None,
    };
    for _ in 0..blocks {
        let entries = le(data, at).expect("material-info entry count") as usize;
        at += 4;
        let mut contributed = 0usize;
        for _ in 0..entries {
            let id = le(data, at).expect("shader-data id");
            at += 4;
            if id == spore_gmdl::TEXTURE_SET_ID {
                let count = le(data, at).expect("texture-set count") as usize;
                at += 4;
                for _ in 0..count {
                    at += 16; // the entry's sampler + opaque words
                    let instance = le(data, at).expect("texture instance");
                    let group = le(data, at + 4).expect("texture group");
                    at += 8;
                    walk.refs.push((instance, group));
                    contributed += 1;
                }
            } else {
                let Some(size) = spore_gmdl::shader_data_size(id) else {
                    walk.per_block.push(contributed);
                    walk.stopped_on = Some(id);
                    return walk;
                };
                at += size as usize;
            }
        }
        walk.per_block.push(contributed);
    }
    walk
}

/// Every gmdl record whose material-info section closes, walked independently.
///
/// This is the corpus-wide version of the question the oracle sample answers, and
/// it covers **all** the records rather than the 487 of 4 209 the Python oracle
/// can walk: the oracle dies on an impossible baked-deform count in the anim-data
/// section, which is *after* material-info, so its refusals say nothing about the
/// material-info structure. The independent walk below has no such ceiling.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn every_record_whose_material_info_closes_has_blocks_equal_to_its_mesh_count() {
    let Some((store, _)) = content_store() else {
        return;
    };
    let package = &store.packages()[0];
    let image = package.bytes();
    let rows: Vec<spore_dbpf::DbpfEntry> = package
        .index()
        .entries()
        .iter()
        .copied()
        .filter(|entry| entry.type_id == type_id::GMDL)
        .collect();

    let mut closed = 0usize;
    let mut stopped_early = 0usize;
    let mut walk_disagrees_with_spore_gmdl = 0usize;
    let mut blocks_differ_from_meshes = 0usize;
    let mut blocks_not_uniform = 0usize;
    let mut cut_would_be_wrong = 0usize;
    let mut stopped_mid_record = 0usize;
    let mut partial_list_cut_as_per_mesh = 0usize;
    let mut stopped_in_last_block_uneven = 0usize;
    let mut stopped_mid_but_cut_succeeds = 0usize;
    let mut stopped_mid_record_examples: Vec<String> = Vec::new();
    let mut cut_succeeds_examples: Vec<String> = Vec::new();
    let mut examples: Vec<String> = Vec::new();
    let mut shapes: BTreeMap<(usize, usize, usize), usize> = BTreeMap::new();

    for entry in &rows {
        let bytes = match spore_dbpf::extract_record(image, entry) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let recovered = match spore_gmdl::parse_recovering(&bytes) {
            Ok(recovered) => recovered,
            Err(_) => continue,
        };
        let walk = walk_material_info(&bytes);
        let walked: Vec<(u32, u32)> = recovered
            .model
            .texture_refs
            .iter()
            .map(|reference| (reference.instance_id, reference.group_id))
            .collect();
        if walk.refs != walked {
            walk_disagrees_with_spore_gmdl += 1;
            if examples.len() < 5 {
                examples.push(format!(
                    "{}: the independent walk found {} reference(s), spore_gmdl {}",
                    entry.key,
                    walk.refs.len(),
                    walked.len()
                ));
            }
            continue;
        }

        if let Some(id) = walk.stopped_on {
            stopped_early += 1;
            // Every record that stops here yields whatever references it collected so
            // far, and `partition_texture_refs` will happily divide THOSE. So the
            // scope it reports is `PerMeshBlock` from a list that is provably
            // not the record's whole texture set.
            let meshes = recovered.model.meshes.len();
            let total: usize = walk.per_block.iter().sum();
            if meshes > 0 && total > 0 && total % meshes == 0 {
                partial_list_cut_as_per_mesh += 1;
            }
            if walk.per_block.len() == walk.blocks
                && !walk.per_block.windows(2).all(|p| p[0] == p[1])
            {
                stopped_in_last_block_uneven += 1;
            }
            // The sharp case: the walk stopped BEFORE the last block, so the
            // references it collected do not cover one block per mesh and the
            // division by mesh count is being applied to a partial list.
            let covered = walk.per_block.len();
            if covered < walk.blocks {
                stopped_mid_record += 1;
                // The dangerous shape: the reference list is PARTIAL and the
                // division still comes out even, so the cut succeeds and puts a
                // block's references on the wrong meshes with nothing to report.
                let total: usize = walk.per_block.iter().sum();
                let meshes = recovered.model.meshes.len();
                if meshes > 0 && total % meshes == 0 {
                    stopped_mid_but_cut_succeeds += 1;
                    if cut_succeeds_examples.len() < 5 {
                        cut_succeeds_examples.push(format!(
                            "{}: {} mesh(es), {} block(s) declared, the walk covered {} and \
                             collected {} reference(s) before stopping on 0x{id:03x}; \
                             refs_per_mesh is therefore {total}/{meshes} = {}",
                            entry.key,
                            meshes,
                            walk.blocks,
                            covered,
                            total,
                            total / meshes
                        ));
                    }
                }
                if stopped_mid_record_examples.len() < 5 {
                    stopped_mid_record_examples.push(format!(
                        "{}: {} block(s) declared, the walk covered {} and contributed {:?} \
                         before stopping on 0x{id:03x}",
                        entry.key, walk.blocks, covered, walk.per_block
                    ));
                }
            }
            continue;
        }
        closed += 1;

        let meshes = recovered.model.meshes.len();
        if walk.blocks != meshes {
            blocks_differ_from_meshes += 1;
            if examples.len() < 10 {
                examples.push(format!(
                    "{}: {} mesh(es), {} block(s)",
                    entry.key, meshes, walk.blocks
                ));
            }
        }
        let uniform = walk.per_block.windows(2).all(|pair| pair[0] == pair[1]);
        if !uniform {
            blocks_not_uniform += 1;
        }
        let total: usize = walk.per_block.iter().sum();
        // A model with no meshes and no blocks referenced nothing: that is a
        // consistent absence, which `partition_texture_refs` answers with an
        // empty cut, so it is not a record where the cut goes wrong.
        let sound = if meshes == 0 && total == 0 {
            true
        } else {
            meshes > 0
                && walk.blocks == meshes
                && uniform
                && total % meshes == 0
                && total / meshes == walk.per_block.first().copied().unwrap_or(0)
        };
        if !sound {
            cut_would_be_wrong += 1;
            if examples.len() < 10 {
                examples.push(format!(
                    "{}: {} mesh(es), {} block(s), per block {:?}, {total} reference(s)",
                    entry.key, meshes, walk.blocks, walk.per_block
                ));
            }
        }
        *shapes
            .entry((
                meshes,
                walk.blocks,
                walk.per_block.first().copied().unwrap_or(0),
            ))
            .or_default() += 1;
    }

    println!("gmdl records walked: {}", rows.len());
    println!("material-info closed: {closed}, stopped early: {stopped_early}");
    println!("walk disagreed with spore_gmdl: {walk_disagrees_with_spore_gmdl}");
    println!("block count != mesh count: {blocks_differ_from_meshes}");
    println!("records with uneven blocks: {blocks_not_uniform}");
    println!("records whose even cut would be wrong: {cut_would_be_wrong}");
    println!("records whose walk stopped before the last block: {stopped_mid_record}");
    println!(
        "of those, records where the cut on the PARTIAL list still comes out even: \
         {stopped_mid_but_cut_succeeds}"
    );
    println!(
        "records that stopped early and STILL produce a PerMeshBlock scope: \
         {partial_list_cut_as_per_mesh}"
    );
    println!(
        "records that stopped inside the last block leaving its count different from its \
         neighbours': {stopped_in_last_block_uneven}"
    );
    println!("(meshes, blocks, refs-per-block) shapes among closed records:");
    for (shape, count) in &shapes {
        println!("  {shape:?}: {count} record(s)");
    }
    for line in stopped_mid_record_examples.iter().take(5) {
        println!("  stopped mid-record: {line}");
    }
    for line in cut_succeeds_examples.iter().take(5) {
        println!("  cut succeeds on a partial list: {line}");
    }
    for line in &examples {
        println!("  example: {line}");
    }

    assert_eq!(
        walk_disagrees_with_spore_gmdl, 0,
        "the independent walk and spore_gmdl must collect the same references, or the block \
         boundaries below are aligned to nothing"
    );
    assert!(closed > 0, "no record's material-info section closed");
    assert_eq!(
        blocks_differ_from_meshes, 0,
        "among records whose material-info closes, the block count must equal the mesh count"
    );
    assert_eq!(
        blocks_not_uniform, 0,
        "among records whose material-info closes, every block must contribute the same number \
         of references"
    );
    assert_eq!(
        cut_would_be_wrong, 0,
        "among records whose material-info closes, dividing the flat reference list by the mesh \
         count must reproduce the block boundaries"
    );
    assert_eq!(
        stopped_early, 619,
        "the count of records whose material-info walk stops on the undocumented shader-data id \
         0x218 is a measured fact about this install"
    );
    assert_eq!(
        stopped_mid_record, 100,
        "records whose walk stops BEFORE the last declared block, so the reference list does \
         not even cover one entry per block"
    );
    assert_eq!(
        stopped_mid_but_cut_succeeds, stopped_mid_record,
        "FINDING: every one of those records has a reference count that divides evenly by its \
         mesh count, so `partition_texture_refs` returns a `PerMeshBlock` scope from a PARTIAL \
         list and nothing reports it. The per-mesh binding for these records is computed over \
         references that do not include every block."
    );
    assert_eq!(
        partial_list_cut_as_per_mesh, stopped_early,
        "FINDING: EVERY record whose material-info walk stops on 0x218 still reports a \
         `PerMeshBlock` scope, so the engine binds textures per mesh from a reference list the \
         walk did not finish reading. `WholeModel` -- the scope that would have said 'we could \
         not cut this' -- is produced by none of them."
    );
    assert_eq!(
        stopped_in_last_block_uneven, 0,
        "for the records that stop inside the LAST block, that block's collected count still \
         matches its neighbours', so a partial list there is indistinguishable from a complete \
         one; the 100 above are not"
    );
}

/// The documented creature, on its own: two meshes, two blocks, and the one slot
/// whose derived ordinal and group id disagree.
///
/// The oracle refuses this record -- its known anim-data ceiling -- so the block
/// structure is read by the independent walk above instead. That walk is
/// validated against `spore_gmdl` before it is used, and the result is a finding
/// rather than a confirmation: **the walk stops inside this record's material-info
/// section**, on the undocumented shader-data id `0x218`. So for the one record
/// that provides the counter-example to the even cut, the cut's own premise --
/// that every block contributes the same number of references -- cannot be
/// checked at all.
#[test]
#[ignore = "needs the git-ignored SPORE/ tree (~1 GB)"]
fn the_documented_creature_is_the_live_counter_example_to_the_even_cut() {
    let Some((store, _)) = content_store() else {
        return;
    };
    if !store.contains(&CREATURE) {
        println!("SKIP: the documented creature {CREATURE} is not in this install");
        return;
    }
    let record = store
        .read(&CREATURE)
        .expect("the creature record must read");
    let recovered = spore_gmdl::parse_recovering(&record).expect("the header walk must not fail");
    let model = recovered.model;
    println!(
        "{}: {} mesh(es), {} texture reference(s), stop={:?}",
        CREATURE,
        model.meshes.len(),
        model.texture_refs.len(),
        recovered.stop
    );
    assert_eq!(model.meshes.len(), 2, "two meshes, as documented");
    assert_eq!(model.texture_refs.len(), 2, "two texture references");

    // The independent walk, validated before it is believed.
    let walk = walk_material_info(&record);
    let walked: Vec<(u32, u32)> = model
        .texture_refs
        .iter()
        .map(|entry| (entry.instance_id, entry.group_id))
        .collect();
    assert_eq!(
        walk.refs, walked,
        "the independent walk must land on the same references as spore_gmdl, or its block \
         boundaries are aligned to nothing"
    );
    assert_eq!(
        walk.blocks, 2,
        "the record declares two material-info blocks"
    );
    println!(
        "material-info blocks: {}; contributed {:?}; stopped on {:?}",
        walk.blocks, walk.per_block, walk.stopped_on
    );

    // The two statements of each slot's ordinal. `refs_per_mesh` is 1, so both
    // slots have derived ordinal 0, while the group ids state 0 and 1.
    let per_mesh = model.texture_refs.len() / model.meshes.len();
    assert_eq!(per_mesh, 1, "`refs_per_mesh` is 1 for this record");
    let mut conflicts = 0usize;
    for (mesh, entry) in model.texture_refs.iter().enumerate() {
        let derived = (mesh % per_mesh) as u32;
        let stated = entry.group_id & 0xFF;
        if derived != stated {
            conflicts += 1;
            println!("  mesh {mesh}: derived ordinal {derived}, group id states {stated}");
        }
    }
    assert_eq!(
        conflicts, 1,
        "the creature is the documented counter-example: exactly one slot's derived ordinal \
         disagrees with the group id, which is what keeps `BindingScope::is_inferred` true"
    );

    // And the sharp part: this record cannot have its block uniformity checked,
    // because the walk stops before the second block's contribution is known.
    assert_eq!(
        walk.stopped_on,
        Some(0x218),
        "the creature's material-info walk stops on the undocumented shader-data id, so the \
         premise that every block contributes the same number of references is UNVERIFIABLE for \
         exactly the record that motivates the inference"
    );
    assert!(
        recovered.stop.is_some(),
        "and spore_gmdl records the stop as a typed StopReason rather than a silent truncation"
    );
}
