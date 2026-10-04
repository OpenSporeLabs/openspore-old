//! Full-field parse of the committed fixture `tests/fixtures/mini.gmdl`.
//!
//! Every expected value here is cross-checked three ways: the generator
//! (`tests/fixtures/gen_fixtures.py:build_gmdl`) says what it wrote, the
//! repository's Python oracle (`tools/spore/gmdl/gmdl.py`) parses it
//! independently, and `tests/test_formats.py::TestGMDL::test_semantic_snapshot`
//! asserts the same semantic snapshot. Where this test hard-codes a number, it
//! is one of those three sources' output.

mod common;

use common::{mini_gmdl, Builder, MINI_GMDL_LEN};
use spore_core::ResourceKey;
use spore_gmdl::{parse, GmdlModel, TrailerWalk};

fn fixture_model() -> GmdlModel {
    let data = mini_gmdl();
    assert_eq!(data.len(), MINI_GMDL_LEN, "fixture length changed");
    parse(&data).expect("mini.gmdl must parse")
}

#[test]
fn header_fields_match_the_record() {
    let model = fixture_model();
    assert_eq!(model.version, 8);
    assert_eq!(model.version, spore_gmdl::SUPPORTED_VERSION);
    assert_eq!(
        spore_gmdl::GMDL_TYPE,
        0x00E6_BCE5,
        "gmdl type id is a documented constant"
    );
    assert_eq!(model.mesh_count, 1);
}

#[test]
fn the_single_referenced_file_is_reordered_from_its_on_disk_word_order() {
    let model = fixture_model();
    // The committed fixture writes its one ref key as
    // `{instance 0x11111111, group 0x22222222, type 0x00E6BCE5}`
    // (`tests/fixtures/gen_fixtures.py:182`), and `ResourceKey` spells identity
    // as type/group/instance, so the decoder reorders the three words. Getting
    // the reorder backwards produces a key that names a record which does not
    // exist.
    //
    // The *byte order* of those three words is the part this test can no longer
    // assert. The generator packs them little-endian
    // (`struct.pack('<III', ...)`), and the corpus proves that wrong: over
    // `Spore_Content.package`, reading every key big-endian resolves 7 989 of
    // 7 989 declared keys to real DBPF index rows and reading them
    // little-endian resolves 0 (see `docs/ANIMATION.md` §2, measured by
    // `tests/corpus_animation.rs`). So the fixture's third word is a byte
    // reversal of the identity it was written to mean, and this decoder — which
    // reads the whole table big-endian — reports exactly that.
    //
    // The fix belongs in `tests/fixtures/gen_fixtures.py` (`'<III'` -> `'>III'`),
    // which is outside this crate. Until it lands, the divergence is pinned here
    // rather than papered over: the value below is what the committed bytes
    // decode to, and the ordering is asserted on a record built big-endian.
    let key = model.referenced_files.first().expect("one reference");
    assert_eq!(model.referenced_files.len(), 1);
    assert_eq!(key.instance_id, 0x1111_1111, "palindromic, order-agnostic");
    assert_eq!(key.group_id, 0x2222_2222, "palindromic, order-agnostic");
    assert_eq!(
        key.type_id,
        u32::from_be_bytes(0x00E6_BCE5u32.to_le_bytes()),
        "0x00E6BCE5 packed little-endian and read back big-endian"
    );
    assert_ne!(
        key.type_id,
        spore_gmdl::GMDL_TYPE,
        "the committed fixture packs the type word the wrong way round; see above"
    );
}

#[test]
fn a_big_endian_reference_key_is_reordered_into_type_group_instance() {
    // The same three values, packed the way the corpus says they are stored.
    // This is the assertion that states the decoder's actual contract; the test
    // above states what the shared fixture's bytes happen to decode to.
    let data = Builder::minimal()
        .refs(vec![[0x1111_1111, 0x2222_2222, spore_gmdl::GMDL_TYPE]])
        .build();
    let model = parse(&data).expect("synthetic record");
    assert_eq!(
        model.referenced_files,
        vec![ResourceKey::new(
            spore_gmdl::GMDL_TYPE,
            0x2222_2222,
            0x1111_1111
        )]
    );
    let key = model.referenced_files.first().expect("one reference");
    assert_eq!(key.type_id, spore_gmdl::GMDL_TYPE);
    assert!(
        key.is_complete(),
        "no component of this key is the wildcard"
    );
}

#[test]
fn record_bounds_are_read_verbatim() {
    let model = fixture_model();
    // gen_fixtures.py: GMDL_BBOX_MIN/MAX/RADIUS. These are the *record's* box,
    // which is deliberately unrelated to the box its vertices imply.
    assert_eq!(model.bounds_min, [1.5, -2.25, 0.125]);
    assert_eq!(model.bounds_max, [3.5, 0.75, 2.0]);
    assert_eq!(model.radius, 4.25);
    for value in model
        .bounds_min
        .iter()
        .chain(model.bounds_max.iter())
        .chain([&model.radius])
    {
        assert!(value.is_finite());
    }
}

#[test]
fn the_index_buffer_header_and_payload_match() {
    let model = fixture_model();
    assert_eq!(model.index_buffers.len(), 1);
    let buffer = model.index_buffers.first().expect("one index buffer");
    assert_eq!(buffer.prim_type, 4, "4 = triangle list");
    assert_eq!(
        buffer.prim_type,
        spore_gmdl::Topology::TriangleList.prim_code()
    );
    assert_eq!(buffer.index_count, 153);
    assert_eq!(buffer.index_bits, 16);
    assert_eq!(buffer.bytes.len(), 306, "153 * 2 bytes");
    assert_eq!(buffer.bytes.len(), buffer.index_count as usize * 2);
}

#[test]
fn the_vertex_descriptor_table_matches_element_for_element() {
    let model = fixture_model();
    assert_eq!(model.descriptors.len(), 1);
    let descriptor = model.descriptors.first().expect("one descriptor");
    assert_eq!(descriptor.len(), 2);
    // POSITION/FLOAT3 at byte 0, then TEXCOORD0/FLOAT2 at byte 8.
    let position = descriptor.first().expect("position element");
    assert_eq!(position.stream, 0);
    assert_eq!(position.offset, 0);
    assert_eq!(position.decl_type, 2);
    assert_eq!(
        spore_gmdl::DeclType::from_code(position.decl_type),
        Some(spore_gmdl::DeclType::Float3)
    );
    assert_eq!(position.decl_method, 0);
    assert_eq!(position.decl_usage, 0);
    assert_eq!(
        spore_gmdl::DeclUsage::from_code(position.decl_usage),
        Some(spore_gmdl::DeclUsage::Position)
    );
    assert_eq!(position.usage_index, 0);
    assert_eq!(position.type_code, 0);

    let uv = descriptor.get(1).expect("texcoord element");
    assert_eq!(uv.stream, 0);
    assert_eq!(uv.offset, 8);
    assert_eq!(uv.decl_type, 1);
    assert_eq!(
        spore_gmdl::DeclType::from_code(uv.decl_type),
        Some(spore_gmdl::DeclType::Float2)
    );
    assert_eq!(uv.decl_method, 0);
    assert_eq!(uv.decl_usage, 5);
    assert_eq!(
        spore_gmdl::DeclUsage::from_code(uv.decl_usage),
        Some(spore_gmdl::DeclUsage::TexCoord)
    );
    assert_eq!(uv.usage_index, 0);
    assert_eq!(uv.type_code, 0);
}

#[test]
fn the_vertex_buffer_header_and_payload_match() {
    let model = fixture_model();
    assert_eq!(model.vertex_buffers.len(), 1);
    let buffer = model.vertex_buffers.first().expect("one vertex buffer");
    assert_eq!(buffer.desc_index, 0);
    assert_eq!(buffer.vertex_count, 50);
    assert_eq!(buffer.bytes.len(), 800, "50 * 16 bytes");
    assert_eq!(buffer.bytes.len(), buffer.vertex_count as usize * 16);
}

#[test]
fn the_mesh_table_material_ids_and_empty_tables_match() {
    let model = fixture_model();
    assert_eq!(model.meshes.len(), 1);
    let mesh = model.meshes.first().expect("one mesh");
    assert_eq!(mesh.index_buffer, 0);
    assert_eq!(mesh.vertex_buffer, 0);
    assert_eq!(model.material_ids, vec![0x1234_5678]);
    // The fixture carries no material info at all, so there is no texture set
    // to decode and no shader data to skip.
    assert!(model.texture_refs.is_empty());
    assert!(model.bone_ranges.is_empty());
    assert_eq!(model.unknown_key, [0, 0, 0]);
}

#[test]
fn the_walk_reaches_the_end_of_the_record() {
    let data = mini_gmdl();
    let model = fixture_model();
    // The strict sections end after the material info (offset 1246: the
    // material-info count word is the last strict read) and the trailer walk
    // consumes the remaining 20 bytes exactly, so on this fixture the two
    // agree. They differ only for records whose trailer cannot be walked in
    // full; see `strict_and_best_effort_tails_differ_only_when_the_tail_is_short`
    // in tests/parse_errors.rs.
    assert_eq!(model.consumed, data.len());
    assert_eq!(model.strict_consumed, data.len());
    assert!(model.fully_walked());
    assert_eq!(model.trailer, TrailerWalk::Complete);
}

#[test]
fn parsing_is_deterministic() {
    // Two parses of the same bytes must be indistinguishable, or a byte-identical
    // asset manifest is impossible.
    let data = mini_gmdl();
    assert_eq!(parse(&data).expect("parse"), parse(&data).expect("parse"));
}
