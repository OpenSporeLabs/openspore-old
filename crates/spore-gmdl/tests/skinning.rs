//! The skinning surface, end to end, on synthetic records only.
//!
//! `src/skin.rs` unit-tests the accessors against hand-built models. This file
//! tests the same accessors against records that went through [`parse`], so the
//! path being pinned is the real one: bytes → descriptor → binding. Every record
//! here is built by `tests/common::Builder`; no Spore byte is committed.
//!
//! What is deliberately *not* tested: any joint transform, hierarchy or clip.
//! None of those exist in a gmdl record, so a test for them would have to
//! manufacture them first. `docs/ANIMATION.md` §5 states where they are.

mod common;

use common::{Builder, Element, Trailer, VertexBufferSpec};
use spore_gmdl::{parse, DeclType, DeclUsage};

/// POSITION/FLOAT3, NORMAL/UBYTE4, BLENDINDICES/UBYTE4, BLENDWEIGHT/UBYTE4N,
/// TEXCOORD/FLOAT2 — the most common skinned corpus shape's offsets.
fn skinned_descriptor() -> Vec<Element> {
    vec![
        Element::new(0, 2, DeclUsage::Position.as_code()),
        Element::new(12, 5, DeclUsage::Normal.as_code()),
        Element::new(16, 5, DeclUsage::BlendIndices.as_code()),
        Element::new(20, 8, DeclUsage::BlendWeight.as_code()),
        Element::new(24, 1, DeclUsage::TexCoord.as_code()),
    ]
}

/// The same layout with no weight element — a real corpus shape, 829 times over.
fn indices_only_descriptor() -> Vec<Element> {
    vec![
        Element::new(0, 2, DeclUsage::Position.as_code()),
        Element::new(12, 5, DeclUsage::Normal.as_code()),
        Element::new(16, 5, DeclUsage::BlendIndices.as_code()),
        Element::new(20, 1, DeclUsage::TexCoord.as_code()),
    ]
}

fn static_descriptor() -> Vec<Element> {
    vec![
        Element::new(0, 2, DeclUsage::Position.as_code()),
        Element::new(12, 5, DeclUsage::Normal.as_code()),
        Element::new(16, 1, DeclUsage::TexCoord.as_code()),
    ]
}

#[test]
fn a_parsed_skinned_record_yields_one_complete_binding() {
    let data = Builder::minimal()
        .descriptors(vec![skinned_descriptor()])
        .vertex_buffers(vec![VertexBufferSpec::new(0, 3, vec![0; 3 * 28])])
        .build();
    let model = parse(&data).expect("skinned record");
    assert!(model.uses_skinning());
    let bindings = model.skin_bindings();
    assert_eq!(bindings.len(), 1);
    let binding = bindings.first().expect("one binding");
    assert!(binding.is_complete());
    assert_eq!(binding.vertex_buffer, 0);
    assert_eq!(binding.descriptor, 0);
    assert_eq!(binding.indices_offset(), Some(16));
    assert_eq!(binding.index_slots(), Some(4));
    assert_eq!(binding.weight_slots(), Some(4));
    // The slot widths are the declared `D3DDECLTYPE` sizes, looked up through the
    // crate's own table rather than hard-coded twice.
    assert_eq!(
        binding.index_slots(),
        DeclType::from_code(binding.indices.expect("indices").decl_type).map(DeclType::byte_size)
    );
}

#[test]
fn a_parsed_static_record_declares_no_skinning() {
    let data = Builder::minimal()
        .descriptors(vec![static_descriptor()])
        .vertex_buffers(vec![VertexBufferSpec::new(0, 3, vec![0; 3 * 24])])
        .build();
    let model = parse(&data).expect("static record");
    assert!(!model.uses_skinning());
    assert!(
        model.skin_bindings().is_empty(),
        "a static mesh contributes no row, not a row with empty fields"
    );
}

#[test]
fn a_record_mixing_static_and_skinned_buffers_reports_only_the_skinned_one() {
    let data = Builder::minimal()
        .descriptors(vec![static_descriptor(), skinned_descriptor()])
        .vertex_buffers(vec![
            VertexBufferSpec::new(0, 3, vec![0; 3 * 24]),
            VertexBufferSpec::new(1, 3, vec![0; 3 * 28]),
        ])
        .meshes(vec![(0, 0), (0, 1)])
        .build();
    let model = parse(&data).expect("mixed record");
    let bindings = model.skin_bindings();
    assert_eq!(bindings.len(), 1, "the static buffer is absent, not empty");
    assert_eq!(bindings.first().expect("one").vertex_buffer, 1);
    assert_eq!(bindings.first().expect("one").descriptor, 1);
}

#[test]
fn half_a_skinning_pair_is_reported_as_half_and_never_completed() {
    let data = Builder::minimal()
        .descriptors(vec![indices_only_descriptor()])
        .vertex_buffers(vec![VertexBufferSpec::new(0, 3, vec![0; 3 * 24])])
        .build();
    let model = parse(&data).expect("indices-only record");
    let bindings = model.skin_bindings();
    assert_eq!(bindings.len(), 1);
    let binding = bindings.first().expect("one binding");
    assert!(!binding.is_complete());
    assert!(binding.declares_skinning());
    assert_eq!(binding.index_slots(), Some(4));
    assert_eq!(
        binding.weight_slots(),
        None,
        "no weight element is an absent width, not a width of zero"
    );
    assert!(model.uses_skinning());
}

#[test]
fn a_weight_element_without_an_index_element_is_reported_too() {
    // The corpus has none of these, but the accessor must not assume they cannot
    // exist: `is_complete()` being false is the answer, and dropping the row
    // would make the record look static.
    let mut elements = static_descriptor();
    elements.push(Element::new(24, 8, DeclUsage::BlendWeight.as_code()));
    let data = Builder::minimal()
        .descriptors(vec![elements])
        .vertex_buffers(vec![VertexBufferSpec::new(0, 3, vec![0; 3 * 28])])
        .build();
    let model = parse(&data).expect("weight-only record");
    let bindings = model.skin_bindings();
    assert_eq!(bindings.len(), 1);
    let binding = bindings.first().expect("one binding");
    assert!(!binding.is_complete());
    assert!(binding.declares_skinning());
    assert_eq!(binding.index_slots(), None);
    assert_eq!(binding.weight_slots(), Some(4));
}

#[test]
fn the_bone_range_span_is_the_sum_of_the_declared_counts() {
    let data = Builder::minimal()
        .descriptors(vec![skinned_descriptor()])
        .vertex_buffers(vec![VertexBufferSpec::new(0, 3, vec![0; 3 * 28])])
        .trailer(Trailer::Full {
            bone_ranges: vec![(0, 17), (17, 5)],
            anim_datas: Vec::new(),
            unknown_key: [0, 0, 0],
        })
        .build();
    let model = parse(&data).expect("record with bone ranges");
    assert!(model.declares_bone_ranges());
    assert_eq!(model.bone_range_span(), 22);
    assert_eq!(model.bone_ranges, vec![(0, 17), (17, 5)]);
    assert!(
        model.trailer.is_complete(),
        "a complete walk is what makes an empty table mean 'no table'"
    );
}

#[test]
fn a_record_whose_trailer_walk_stopped_declares_nothing_rather_than_zero_joints() {
    // The cut lands inside the bone-range count word, so the walk never reached
    // it. `bone_range_span()` is 0 either way; `declares_bone_ranges()` and
    // `trailer` are what keep the two apart.
    // Six filler bytes: the bone-range count word reads 0xA5A5A5A5 and the walk
    // stops immediately, having consumed four of the six.
    let data = Builder::minimal()
        .descriptors(vec![skinned_descriptor()])
        .vertex_buffers(vec![VertexBufferSpec::new(0, 3, vec![0; 3 * 28])])
        .trailer(Trailer::Filler(6))
        .build();
    let model = parse(&data).expect("a record with a short tail still parses");
    assert!(!model.declares_bone_ranges());
    assert_eq!(model.bone_range_span(), 0);
    assert!(model.trailer.is_truncated(), "{:?}", model.trailer);
    assert_eq!(
        model.trailer.stopped_at(),
        Some(spore_gmdl::TrailerStage::BoneRanges)
    );
    assert_eq!(
        model.strict_consumed,
        data.len() - 2,
        "four of six bytes read"
    );
    // And here the two byte-counts come apart: every byte of the input was
    // consumed, yet two of them were never validated. `fully_walked()` is a
    // statement about bytes, not about meaning.
    assert_eq!(model.consumed, data.len());
    assert!(!model.fully_walked());
}

#[test]
fn skinning_reads_nothing_out_of_the_vertex_payload() {
    // The accessors are descriptor-driven on purpose. A buffer whose bytes are
    // all zero has no joint ids, no weights and no transforms, and the binding
    // must be identical to one over a buffer full of noise — otherwise the
    // accessor is reading data, not the record's declaration of where the data
    // lives.
    let binding_for = |payload: Vec<u8>| {
        let data = Builder::minimal()
            .descriptors(vec![skinned_descriptor()])
            .vertex_buffers(vec![VertexBufferSpec::new(0, 3, payload)])
            .build();
        let model = parse(&data).expect("record");
        model.skin_bindings().first().copied().expect("one binding")
    };
    assert_eq!(
        binding_for(vec![0; 3 * 28]),
        binding_for(vec![0xA5; 3 * 28])
    );
}
