//! Hermetic tests for the resource dependency graph.
//!
//! Everything here is built in memory from synthetic bytes: no game install, no
//! GPU, no window, and **no Spore asset bytes**. The package builder below writes
//! a real DBPF v3 image (header, index, payload arena) and a real version-8 gmdl
//! record, because a test that fed the graph a hand-written adjacency would
//! prove nothing about the part that reads record bytes.
//!
//! Run with:
//!
//! ```text
//! cargo test -p spore-assets --test graph
//! ```

use spore_assets::graph::{
    read_reference_table, Edge, EdgeKind, GraphOptions, ResourceGraph, SkipReason, TraversalEntry,
};
use spore_assets::{ContentStore, Package};
use spore_core::record::type_id;
use spore_core::{EvidenceLevel, ResourceKey};

// ---------------------------------------------------------------------------
// Synthetic DBPF v3 package
// ---------------------------------------------------------------------------

const HEADER_SIZE: usize = 96;
const ROW_SIZE: usize = 28;

/// One record to place in the synthetic package.
struct Record {
    key: ResourceKey,
    payload: Vec<u8>,
}

impl Record {
    fn new(key: ResourceKey, payload: Vec<u8>) -> Self {
        Self { key, payload }
    }
}

/// Builds a DBPF v3 image holding `records`, uncompressed.
///
/// Written from the layout `spore_dbpf::index` documents, with the flags word
/// zeroed so the 28-byte plain row stride is used and no shared-id word appears.
fn build_package(records: &[Record]) -> Vec<u8> {
    let index_offset = HEADER_SIZE;
    let arena_offset = index_offset + 4 + records.len() * ROW_SIZE;

    let mut index = Vec::with_capacity(4 + records.len() * ROW_SIZE);
    index.extend_from_slice(&0u32.to_le_bytes()); // flags: nothing shared
    let mut arena: Vec<u8> = Vec::new();
    for record in records {
        let at = arena_offset + arena.len();
        index.extend_from_slice(&record.key.type_id.to_le_bytes());
        index.extend_from_slice(&record.key.group_id.to_le_bytes());
        index.extend_from_slice(&record.key.instance_id.to_le_bytes());
        index.extend_from_slice(&(at as u32).to_le_bytes());
        let size = record.payload.len() as u32;
        index.extend_from_slice(&size.to_le_bytes()); // compressed == stored
        index.extend_from_slice(&size.to_le_bytes()); // decompressed
        index.extend_from_slice(&0u16.to_le_bytes()); // compression: none
        index.push(0); // saved
        index.push(0); // pad
        arena.extend_from_slice(&record.payload);
    }

    let mut out = vec![0u8; HEADER_SIZE];
    out[0..4].copy_from_slice(b"DBPF");
    out[0x24..0x28].copy_from_slice(&(records.len() as u32).to_le_bytes());
    out[0x28..0x2c].copy_from_slice(&((4 + records.len() * ROW_SIZE) as u32).to_le_bytes());
    out[0x40..0x44].copy_from_slice(&(index_offset as u32).to_le_bytes());
    out.extend_from_slice(&index);
    out.extend_from_slice(&arena);
    out
}

fn store_of(records: &[Record]) -> ContentStore {
    let mut store = ContentStore::new();
    store.push(Package::from_vec("synthetic", build_package(records)).unwrap());
    store
}

// ---------------------------------------------------------------------------
// Synthetic gmdl v8 record
// ---------------------------------------------------------------------------

/// A synthetic version-8 gmdl: one mesh, one material with one texture set.
///
/// `references` is written into the `refCount` key table **big-endian**, which
/// is what the real corpus does (see `claims::REFERENCE_TABLE_ENDIANNESS`), and
/// `textures` into the `0x20D` set **little-endian**, which is also what the
/// real corpus does. Putting both conventions in one synthetic record is
/// deliberate: it is the asymmetry that a single-endian fixture would hide.
fn build_gmdl(references: &[(u32, u32, u32)], textures: &[(u32, u32)]) -> Vec<u8> {
    let mut out = build_gmdl_head(references);
    write_material_info_texture_set(&mut out, textures);
    write_trailer(&mut out);
    out
}

/// Everything a gmdl record carries before its material-info table.
///
/// Split out so a test can append a *different* material-info tail -- one that
/// stops the walk -- without first having to defeat the texture-set reader. A
/// record that stops at an undocumented shader-data id cannot be produced by
/// corrupting a well-formed one: the walk has to reach the id.
fn build_gmdl_head(references: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&spore_gmdl::SUPPORTED_VERSION.to_le_bytes());
    out.extend_from_slice(&(references.len() as u32).to_be_bytes());
    for (type_id, group_id, instance_id) in references {
        // On-disk order is instance, group, type; every word big-endian.
        out.extend_from_slice(&instance_id.to_be_bytes());
        out.extend_from_slice(&group_id.to_be_bytes());
        out.extend_from_slice(&type_id.to_be_bytes());
    }

    // mesh_count = 1, then the bounding box and the radius.
    out.extend_from_slice(&1u32.to_le_bytes());
    for value in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0, 2.0] {
        out.extend_from_slice(&value.to_le_bytes());
    }

    // One index buffer: triangle list, 3 indices, 16-bit, 6 bytes of payload.
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&spore_gmdl::PRIM_TRIANGLE_LIST.to_le_bytes());
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&6u32.to_le_bytes());
    out.extend_from_slice(&[0, 1, 2, 0, 1, 2]);

    // One descriptor: POSITION/FLOAT3 at 0, TEXCOORD/FLOAT2 at 12 -> stride 20.
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    for (offset, decl_type, decl_usage) in [(0u16, 2u8, 0u8), (12, 1, 5)] {
        out.extend_from_slice(&0u16.to_le_bytes()); // stream
        out.extend_from_slice(&offset.to_le_bytes());
        out.push(decl_type);
        out.push(0); // method
        out.push(decl_usage);
        out.push(0); // usage index
        out.extend_from_slice(&0u32.to_le_bytes()); // type code
    }

    // One vertex buffer over that descriptor: 3 vertices x 20 bytes.
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // descriptor index
    out.extend_from_slice(&3u32.to_le_bytes()); // vertex count
    out.extend_from_slice(&60u32.to_le_bytes()); // buffer size
    out.extend_from_slice(&[0u8; 60]);

    // Mesh table: one {index buffer, vertex buffer} pair, one material id, then
    // the observed-zero word.
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&7u32.to_le_bytes()); // material id, carried verbatim
    out.extend_from_slice(&0u32.to_le_bytes()); // the zero word

    out
}

/// One material carrying one shader-data entry, which is a `0x20D` texture set.
fn write_material_info_texture_set(out: &mut Vec<u8>, textures: &[(u32, u32)]) {
    out.extend_from_slice(&1u32.to_le_bytes()); // one material
    out.extend_from_slice(&1u32.to_le_bytes()); // one shader-data entry
    write_texture_set(out, textures);
}

/// A `0x20D` entry followed by its texture-set entries.
fn write_texture_set(out: &mut Vec<u8>, textures: &[(u32, u32)]) {
    out.extend_from_slice(&spore_gmdl::TEXTURE_SET_ID.to_le_bytes());
    out.extend_from_slice(&(textures.len() as u32).to_le_bytes());
    for (ordinal, (instance_id, group_id)) in textures.iter().enumerate() {
        out.extend_from_slice(&(ordinal as u32).to_le_bytes()); // sampler
        out.extend_from_slice(&[0u8; 12]); // opaque, skipped by the decoder
        out.extend_from_slice(&instance_id.to_le_bytes());
        out.extend_from_slice(&group_id.to_le_bytes());
    }
}

/// The best-effort tail: zero bone ranges, zero anim records, the trailing key.
///
/// The material-info walk requires four bytes past its table, so a record with no
/// tail of some kind never gets that far.
fn write_trailer(out: &mut Vec<u8>) {
    out.extend_from_slice(&0u32.to_le_bytes()); // bone range count
    out.extend_from_slice(&0u32.to_le_bytes()); // anim data count
    out.extend_from_slice(&[0u8; 12]); // trailing key
}

// ---------------------------------------------------------------------------
// Fixture identities
// ---------------------------------------------------------------------------

const MODEL_A: ResourceKey = ResourceKey {
    type_id: 0x00E6_BCE5,
    group_id: 0x4064_6201,
    instance_id: 0x0000_0AAA,
};
const MODEL_B: ResourceKey = ResourceKey {
    type_id: 0x00E6_BCE5,
    group_id: 0x4063_6201,
    instance_id: 0x0000_0BBB,
};
const TEX_A0: ResourceKey = ResourceKey {
    type_id: 0x2F4E_681C,
    group_id: 0x4064_2900,
    instance_id: 0x0000_0AAA,
};
const TEX_A1: ResourceKey = ResourceKey {
    type_id: 0x2F4E_681C,
    group_id: 0x4064_2901,
    instance_id: 0x0000_0AAA,
};
const REF_A: ResourceKey = ResourceKey {
    type_id: 0x2F4E_681B,
    group_id: 0x4160_6100,
    instance_id: 0x0000_C0DE,
};
const REF_MISSING: ResourceKey = ResourceKey {
    type_id: 0x2F4E_681B,
    group_id: 0x4160_6100,
    instance_id: 0x0000_DEAD,
};
const PNG_A: ResourceKey = ResourceKey {
    type_id: 0x2F7D_0004,
    group_id: 0x4064_6201,
    instance_id: 0x0000_0AAA,
};
/// A `prop` record in the same group *and* with the same instance as `MODEL_A`,
/// which is what makes it a sibling and not a reference: nothing names it.
const PROP_A: ResourceKey = ResourceKey {
    type_id: 0x00B1_B104,
    group_id: 0x4064_6201,
    instance_id: 0x0000_0AAA,
};

fn payload() -> Vec<u8> {
    // The raster and png payloads are never decoded by the graph: only the index
    // matters, because a reference resolves against identities. Real PNG magic
    // here purely so the fixture does not look like an RW4 record by accident.
    let mut png = vec![0u8; 8];
    png[..8].copy_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    png
}

fn raster() -> Vec<u8> {
    // 32-byte envelope; only its existence in the index matters here.
    let mut raster = vec![0u8; 64];
    raster[0..4].copy_from_slice(&1u32.to_le_bytes());
    raster
}

fn rw4() -> Vec<u8> {
    // A directory the graph never opens. Present only so the reference resolves.
    let mut rw4 = vec![0u8; 256];
    rw4[..8].copy_from_slice(&spore_rw4::MAGIC[..8]);
    rw4
}

/// A package with two models, two textures, one `rw4` reference and one `png`.
fn full_fixture() -> ContentStore {
    let model_a = build_gmdl(
        &[(REF_A.type_id, REF_A.group_id, REF_A.instance_id)],
        &[
            (TEX_A0.instance_id, TEX_A0.group_id),
            (TEX_A1.instance_id, TEX_A1.group_id),
        ],
    );
    // Model B names a reference no loaded package holds. That is the whole
    // point of it: a dangling edge has to be produced on purpose.
    let model_b = build_gmdl(
        &[(
            REF_MISSING.type_id,
            REF_MISSING.group_id,
            REF_MISSING.instance_id,
        )],
        &[],
    );
    store_of(&[
        Record::new(MODEL_A, model_a),
        Record::new(MODEL_B, model_b),
        Record::new(TEX_A0, raster()),
        Record::new(TEX_A1, raster()),
        Record::new(REF_A, rw4()),
        Record::new(PNG_A, payload()),
        Record::new(PROP_A, payload()),
    ])
}

// ---------------------------------------------------------------------------
// Edges from record bytes
// ---------------------------------------------------------------------------

#[test]
fn a_models_texture_refs_become_raster_edges_in_record_order() {
    let graph = ResourceGraph::build(&full_fixture());
    let edges = graph.neighbours_of_kind(&MODEL_A, EdgeKind::GmdlTexture);
    assert_eq!(
        edges,
        vec![
            Edge {
                kind: EdgeKind::GmdlTexture,
                from: MODEL_A,
                to: TEX_A0,
            },
            Edge {
                kind: EdgeKind::GmdlTexture,
                from: MODEL_A,
                to: TEX_A1,
            },
        ]
    );
    // The type came from context, not from the record, so this kind is
    // `Observed` and not `Verified`.
    assert_eq!(
        EdgeKind::GmdlTexture.evidence_level(),
        EvidenceLevel::Observed
    );
}

#[test]
fn a_models_reference_table_becomes_edges_with_the_target_type_read_from_the_record() {
    let graph = ResourceGraph::build(&full_fixture());
    let edges = graph.neighbours_of_kind(&MODEL_A, EdgeKind::GmdlReference);
    assert_eq!(
        edges,
        vec![Edge {
            kind: EdgeKind::GmdlReference,
            from: MODEL_A,
            to: REF_A,
        }]
    );
    // The reference entry carries the whole triple, so this kind carries the
    // strongest grade in the vocabulary.
    assert_eq!(
        EdgeKind::GmdlReference.evidence_level(),
        EvidenceLevel::Verified
    );
    assert_ne!(REF_A.type_id, MODEL_A.type_id);
}

/// The byte order is settled by the corpus, and now BOTH readers agree.
///
/// This test used to be named `..._and_spore_gmdl_does_not` and asserted that
/// `spore_gmdl` read the key table little-endian while the graph read it
/// big-endian. That divergence was real for one commit and is now FIXED: the
/// `refCount` *count* word was already big-endian while the three key words
/// behind it were not, so `spore_gmdl::GmdlModel::referenced_files` carried
/// byte-swapped keys on all 1 510 records that have a reference table.
///
/// Measured on `Spore_Content.package`: read big-endian, 7 989 of 7 989 declared
/// keys resolve to index rows that exist. Read little-endian, 0 of 7 989.
///
/// So the test inverted rather than being deleted. A deleted test would have left
/// the byte order unpinned, and the little-endian reading is the plausible-looking
/// one that would come back.
#[test]
fn the_graph_and_spore_gmdl_now_read_the_reference_table_the_same_way() {
    // A record whose reference table holds one key with three byte-reversal
    // distinct words, so a wrong order cannot accidentally agree.
    let bytes = build_gmdl(
        &[(0xA1B2_C3D4, 0x1122_3344, 0xAABB_CCDD)],
        &[(TEX_A0.instance_id, TEX_A0.group_id)],
    );
    let expected = ResourceKey::new(0xA1B2_C3D4, 0x1122_3344, 0xAABB_CCDD);

    assert_eq!(read_reference_table(&bytes).unwrap(), vec![expected]);

    let model = spore_gmdl::parse_recovering(&bytes).unwrap().model;
    assert_eq!(
        model.referenced_files,
        vec![expected],
        "spore-gmdl reads the key table big-endian, like the graph and like the corpus"
    );

    // And the byte-reversed reading is still recorded as what it is, so the
    // historical divergence stays visible instead of becoming folk memory.
    assert_ne!(
        model.referenced_files[0],
        ResourceKey::new(
            u32::from_le_bytes(0xA1B2_C3D4u32.to_be_bytes()),
            u32::from_le_bytes(0x1122_3344u32.to_be_bytes()),
            u32::from_le_bytes(0xAABB_CCDDu32.to_be_bytes()),
        ),
        "the little-endian reading is the one that resolved 0 of 7 989 real keys"
    );
}

#[test]
fn a_texture_ref_group_of_zero_is_a_dangling_reference_not_a_missing_edge() {
    // Group 0x00000000 is a real observation in the corpus (150 records name it),
    // and treating it as "no group" would silently discard the reference.
    let model = build_gmdl(&[], &[(0x552A_5077, 0x0000_0000)]);
    let graph = ResourceGraph::build(&store_of(&[
        Record::new(MODEL_A, model),
        Record::new(TEX_A0, raster()),
    ]));
    let edges = graph.neighbours_of_kind(&MODEL_A, EdgeKind::GmdlTexture);
    assert_eq!(
        edges.len(),
        1,
        "the edge must survive the unresolvable target"
    );
    assert_eq!(
        edges[0].to,
        ResourceKey::new(type_id::RASTER, 0, 0x552A_5077)
    );

    let dangling = graph.dangling_of_kind(EdgeKind::GmdlTexture);
    assert_eq!(dangling.len(), 1);
    assert_eq!(dangling[0].from, MODEL_A);
    assert_eq!(dangling[0].to, edges[0].to);
    assert_eq!(
        graph.missing_reference_counts()[&EdgeKind::GmdlTexture],
        1,
        "a dangling edge is counted, never dropped"
    );
}

#[test]
fn a_model_with_no_texture_set_is_a_finding_and_not_a_decode_failure() {
    let model = build_gmdl(&[], &[]);
    let graph = ResourceGraph::build(&store_of(&[Record::new(MODEL_A, model)]));
    assert!(graph.neighbours(&MODEL_A).is_empty());
    let unresolved = graph.unresolved();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].key, MODEL_A);
    assert_eq!(
        unresolved[0].reason,
        SkipReason::NoTextureSetDeclared,
        "the walk reached the material info, so the absence is a finding"
    );
    assert!(!matches!(
        unresolved[0].reason,
        SkipReason::WalkFailed { .. }
    ));
}

#[test]
fn a_record_whose_material_info_does_not_close_yields_a_prefix_and_says_so() {
    // A record that is version 8 and stops at the undocumented shader-data id
    // 0x218 -- the id 619 real records name, including every record of the
    // creature-model group 0x40627100. One texture set precedes it; a second
    // material after it is never reached, so the reference list is a prefix.
    let mut bytes = build_gmdl_head(&[]);
    bytes.extend_from_slice(&2u32.to_le_bytes()); // two materials
    bytes.extend_from_slice(&2u32.to_le_bytes()); // the first holds two entries
    write_texture_set(&mut bytes, &[(TEX_A0.instance_id, TEX_A0.group_id)]);
    bytes.extend_from_slice(&0x218u32.to_le_bytes()); // no documented payload size
    write_trailer(&mut bytes);

    let graph = ResourceGraph::build(&store_of(&[
        Record::new(MODEL_A, bytes),
        Record::new(TEX_A0, raster()),
    ]));

    // The entry the walk *did* read is real and is kept: it was bounds-checked out
    // of the record, so discarding it would trade something known for something
    // unknown.
    assert_eq!(
        graph.neighbours_of_kind(&MODEL_A, EdgeKind::GmdlTexture),
        vec![Edge {
            kind: EdgeKind::GmdlTexture,
            from: MODEL_A,
            to: TEX_A0,
        }]
    );

    let unresolved = graph.unresolved();
    assert_eq!(unresolved.len(), 1);
    assert!(
        matches!(
            &unresolved[0].reason,
            SkipReason::PartialTextureSet { detail }
                if detail.contains("material-info") && detail.contains("0x218")
        ),
        "got {:?}",
        unresolved[0].reason
    );
    assert!(
        !matches!(unresolved[0].reason, SkipReason::NoTextureSetDeclared),
        "a prefix is not an empty set"
    );
}

#[test]
fn a_record_that_fails_before_the_material_info_yields_no_texture_edges_at_all() {
    // A model identity whose payload declares gmdl version 9, which this build
    // refuses by name. The walk refuses it, so nothing is read out of the record
    // and no texture edge may exist.
    let mut bytes = vec![0u8; 64];
    bytes[0..4].copy_from_slice(&9u32.to_le_bytes());
    let graph = ResourceGraph::build(&store_of(&[Record::new(MODEL_A, bytes.clone())]));
    assert!(graph.neighbours(&MODEL_A).is_empty());
    let unresolved = graph.unresolved();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].key, MODEL_A);
    assert!(
        matches!(&unresolved[0].reason, SkipReason::WalkFailed { detail } if detail.contains("version 9")),
        "got {:?}",
        unresolved[0].reason
    );
    // The graph reads the reference table itself and refuses the same bytes, for
    // the same stated reason. Both refusals name the version rather than
    // reporting an empty key list.
    assert!(spore_assets::read_reference_table(&bytes)
        .unwrap_err()
        .to_string()
        .contains("version 9"));
}

#[test]
fn a_record_that_is_not_a_model_is_not_a_skip_it_is_never_considered() {
    let graph = ResourceGraph::build(&full_fixture());
    // PNG_A sits in the same group as MODEL_A. It is never walked, so it appears
    // in no skip list: "we did not consider this" and "we considered it and it
    // declared nothing" must not share a bucket.
    assert!(graph
        .unresolved()
        .iter()
        .all(|entry| entry.key.type_id == type_id::GMDL));
    assert!(graph.contains(&PNG_A));
}

// ---------------------------------------------------------------------------
// Adjacency edges
// ---------------------------------------------------------------------------

#[test]
fn co_membership_pairs_a_model_with_the_raster_sharing_its_identity() {
    let graph = ResourceGraph::build(&full_fixture());
    let edges = graph.neighbours_of_kind(&MODEL_A, EdgeKind::ModelTextureSet);
    // Category byte 0x40 is shared, and so is the instance id, so both of the
    // family's rasters are adjacent to the model -- whether or not the record
    // names them. In this fixture both happen to be named.
    assert_eq!(edges.len(), 2);
    assert!(edges
        .iter()
        .all(|edge| edge.kind == EdgeKind::ModelTextureSet));
    assert_eq!(
        EdgeKind::ModelTextureSet.evidence_level(),
        EvidenceLevel::Inferred,
        "adjacency is never promoted above Inferred"
    );

    // MODEL_B is in category 0x40 too but has no raster with its instance.
    assert!(graph
        .neighbours_of_kind(&MODEL_B, EdgeKind::ModelTextureSet)
        .is_empty());
}

#[test]
fn sibling_adjacency_is_off_unless_asked_for_and_never_dangles_when_on() {
    let blind = ResourceGraph::build(&full_fixture());
    assert!(blind
        .neighbours_of_kind(&MODEL_A, EdgeKind::SiblingRecord)
        .is_empty());
    assert!(!blind
        .missing_reference_counts()
        .contains_key(&EdgeKind::SiblingRecord));

    let aware = ResourceGraph::build_with(&full_fixture(), GraphOptions::all());
    // Same group family (0x40646200) and same instance as MODEL_A: the png.
    assert_eq!(
        aware.neighbours_of_kind(&MODEL_A, EdgeKind::SiblingRecord),
        vec![
            // Ascending by key: prop (0x00b1b104) before png (0x2f7d0004).
            Edge {
                kind: EdgeKind::SiblingRecord,
                from: MODEL_A,
                to: PROP_A,
            },
            Edge {
                kind: EdgeKind::SiblingRecord,
                from: MODEL_A,
                to: PNG_A,
            },
        ],
        "same group family and same instance, and nothing anywhere names them"
    );
    // The rasters live in a different group family (0x40642900, the stage byte
    // is 0x29 rather than 0x62), so they are not siblings -- adjacency is by
    // group family, not by category byte.
    assert!(!aware
        .neighbours_of_kind(&MODEL_A, EdgeKind::SiblingRecord)
        .iter()
        .any(|edge| edge.to == TEX_A0));
    // Both endpoints came out of the index, so this kind cannot dangle. That is a
    // structural zero, not a measurement, and it is asserted as one.
    assert_eq!(
        aware.missing_reference_counts()[&EdgeKind::SiblingRecord],
        0,
        "an adjacency edge whose endpoint is absent would mean the graph is broken"
    );
    assert!(aware
        .missing_references()
        .all(|missing| missing.kind != EdgeKind::SiblingRecord));
}

// ---------------------------------------------------------------------------
// Traversal
// ---------------------------------------------------------------------------

#[test]
fn traversal_is_breadth_first_deterministic_and_depth_bounded() {
    let graph = ResourceGraph::build_with(&full_fixture(), GraphOptions::all());
    let walk = graph.traverse(&MODEL_A, 8, &[]);

    assert_eq!(
        walk[0],
        TraversalEntry {
            key: MODEL_A,
            depth: 0
        }
    );
    // Every level ascending by (type, group, instance), every entry once.
    let mut previous_depth = 0;
    for window in walk.windows(2) {
        assert!(window[0].depth <= window[1].depth, "not breadth-first");
        if window[0].depth == window[1].depth {
            assert!(window[0].key < window[1].key, "level not sorted by key");
        }
        previous_depth = window[1].depth;
    }
    assert!(
        previous_depth >= 1,
        "a traversal that reached nothing is not a walk"
    );
    assert_eq!(
        walk.iter().filter(|entry| entry.key == MODEL_A).count(),
        1,
        "the root must not repeat, however many cycles run through it"
    );

    // Depth 0 returns the root alone; depth 1 returns its direct neighbours.
    assert_eq!(
        graph.traverse(&MODEL_A, 0, &[]),
        vec![TraversalEntry {
            key: MODEL_A,
            depth: 0
        }]
    );
    let depth_one = graph.traverse(&MODEL_A, 1, &[]);
    assert!(depth_one.iter().all(|entry| entry.depth <= 1));
    assert!(depth_one.len() > 1);

    // Two runs are identical. This is the property the manifest is built on and
    // it is asserted rather than assumed.
    assert_eq!(graph.traverse(&MODEL_A, 8, &[]), walk);
}

#[test]
fn traversal_survives_a_cycle_and_a_kind_filter_selects_what_is_followed() {
    // Sibling adjacency is symmetric, so MODEL_A <-> PNG_A is a two-edge cycle.
    let graph = ResourceGraph::build_with(&full_fixture(), GraphOptions::all());
    let cyclic = graph.traverse(&MODEL_A, 32, &[EdgeKind::SiblingRecord]);
    assert_eq!(cyclic[0].key, MODEL_A);
    assert_eq!(
        cyclic.iter().filter(|entry| entry.key == MODEL_A).count(),
        1,
        "a cycle must be entered, not looped"
    );

    let textures_only = graph.traverse(&MODEL_A, 8, &[EdgeKind::GmdlTexture]);
    let reached: Vec<ResourceKey> = textures_only.iter().map(|entry| entry.key).collect();
    assert_eq!(reached, vec![MODEL_A, TEX_A0, TEX_A1]);

    // An empty kind list follows everything; an unbuilt kind is simply not there.
    let unbuilt = graph.traverse(&MODEL_A, 8, &[EdgeKind::GmdlReference]);
    assert!(unbuilt
        .iter()
        .all(|entry| entry.key == MODEL_A || entry.key == REF_A));
}

#[test]
fn neighbours_of_a_record_no_package_holds_is_empty_and_lookups_report_it() {
    let graph = ResourceGraph::build(&full_fixture());
    let absent = ResourceKey::new(type_id::GMDL, 0x4064_6201, 0x0000_0FFF);
    assert!(!graph.contains(&absent));
    assert!(graph.neighbours(&absent).is_empty());
    // The distinction the API exists for: an empty neighbour list is ambiguous on
    // its own, and `contains` is what resolves it.
    let present = graph.traverse(&MODEL_A, 1, &[EdgeKind::GmdlTexture]);
    assert!(present.len() > 1);
    assert_eq!(
        graph.traverse(&absent, 4, &[]),
        vec![TraversalEntry {
            key: absent,
            depth: 0
        }]
    );
}

// ---------------------------------------------------------------------------
// Reporting and determinism
// ---------------------------------------------------------------------------

#[test]
fn the_summary_counts_every_kind_it_built_and_no_others() {
    let graph = ResourceGraph::build(&full_fixture());
    let summary = graph.edge_summary();
    assert_eq!(
        summary.iter().map(|row| row.kind).collect::<Vec<_>>(),
        vec![
            EdgeKind::GmdlTexture,
            EdgeKind::GmdlReference,
            EdgeKind::ModelTextureSet
        ]
    );
    let texture_row = summary[0];
    assert_eq!(texture_row.edges, 2);
    assert_eq!(texture_row.dangling, 0);
    let reference_row = summary[1];
    assert_eq!(reference_row.edges, 2, "one resolvable, one not");
    assert_eq!(
        reference_row.dangling, 1,
        "REF_MISSING is absent on purpose"
    );
    assert_eq!(reference_row.level, EvidenceLevel::Verified);
    assert_eq!(summary[2].level, EvidenceLevel::Inferred);

    let total: usize = summary.iter().map(|row| row.edges).sum();
    assert_eq!(total, graph.edge_count());
    assert_eq!(graph.known_records(), 7);
    assert_eq!(
        graph.missing_reference_counts()[&EdgeKind::GmdlReference],
        1
    );
}

#[test]
fn two_builds_over_the_same_packages_produce_identical_output() {
    let first = ResourceGraph::build_with(&full_fixture(), GraphOptions::all());
    let second = ResourceGraph::build_with(&full_fixture(), GraphOptions::all());
    assert_eq!(
        first.traverse(&MODEL_A, 8, &[]),
        second.traverse(&MODEL_A, 8, &[])
    );
    assert_eq!(first.edge_summary(), second.edge_summary());
    assert_eq!(first.unresolved(), second.unresolved());
    let flat = |graph: &ResourceGraph| -> Vec<(ResourceKey, Edge)> {
        graph
            .known_keys()
            .flat_map(|key| graph.neighbours(&key).iter().map(move |edge| (key, *edge)))
            .collect()
    };
    assert_eq!(flat(&first), flat(&second));
}

#[test]
fn an_empty_store_yields_an_empty_graph_rather_than_a_failed_build() {
    let graph = ResourceGraph::build(&ContentStore::new());
    assert_eq!(graph.edge_count(), 0);
    assert_eq!(graph.known_records(), 0);
    assert_eq!(
        graph.edge_summary().len(),
        3,
        "three kinds looked at, none built"
    );
    assert!(graph.edge_summary().iter().all(|row| row.edges == 0));
    assert!(graph.missing_references().next().is_none());
}

#[test]
fn a_package_with_no_models_still_reports_its_records_as_known() {
    let store = store_of(&[Record::new(TEX_A0, raster()), Record::new(PNG_A, payload())]);
    let graph = ResourceGraph::build(&store);
    assert_eq!(graph.known_records(), 2);
    assert_eq!(graph.edge_count(), 0);
    assert!(
        graph.unresolved().is_empty(),
        "neither record was a model candidate"
    );
}
