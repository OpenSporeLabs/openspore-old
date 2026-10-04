//! Role assignment: what a resolved texture's own bytes earn it, and what they
//! do not.
//!
//! **Synthetic fixtures only.** No Spore asset byte is committed here, so every
//! texel below is built in the test from the layouts `spore-texture` documents.
//! The corpus numbers quoted in the assertions come from the `#[ignore]`d
//! `real_corpus_roles.rs`, which needs the git-ignored `SPORE/` tree.

mod support;

use spore_core::evidence::EvidenceLevel;
use spore_gmdl::GmdlTextureRef;
use spore_material::{claims, resolve_texture, ChannelRole, Lane, TextureRole};
use spore_texture::{luminance_record_size, DXT5_FOURCC, LUMINANCE_FOURCC, RASTER_TYPE};
use support::*;

// ---------------------------------------------------------------------------
// Fixtures whose CONTENT matches the roles, not just their format words
// ---------------------------------------------------------------------------

/// A synthetic `0x15` record whose texels are the flat tangent-space normal,
/// **Z first**: every texel is `(255, 128, 128, lane)`.
///
/// Built from the `0x15` framing identity, so it is a record the real decoder
/// accepts; the difference from `support::luminance_raster` is that every lane
/// here is exactly the flat normal, which is what makes the role check pass on
/// a constant field.
fn flat_normal_raster(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = luminance_raster(width, height);
    let mut cursor = 28usize;
    for mip in 0..2u32 {
        let w = (width >> mip).max(1);
        let h = (height >> mip).max(1);
        if mip > 0 {
            cursor += 4;
        }
        for _ in 0..(w * h) {
            bytes[cursor] = 255;
            bytes[cursor + 1] = 0x80;
            bytes[cursor + 2] = 0x80;
            bytes[cursor + 3] = 0xFF;
            cursor += 4;
        }
    }
    debug_assert_eq!(
        bytes.len(),
        luminance_record_size(width, height, 2),
        "the fixture must satisfy the framing identity"
    );
    bytes
}

/// A synthetic `0x15` record whose lanes are a **full-range colour image**.
///
/// Same format, same framing, opposite content: this is the record that must be
/// reported [`TextureRole::Unidentified`] rather than keeping a normal-map role
/// its fourcc would otherwise hand it.
fn colour_raster(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = luminance_raster(width, height);
    let mut cursor = 28usize;
    for mip in 0..2u32 {
        let w = (width >> mip).max(1);
        let h = (height >> mip).max(1);
        if mip > 0 {
            cursor += 4;
        }
        for i in 0..(w * h) {
            let v = (i % 251) as u8;
            bytes[cursor] = v;
            bytes[cursor + 1] = 255 - v;
            bytes[cursor + 2] = v / 2;
            bytes[cursor + 3] = 0x40;
            cursor += 4;
        }
    }
    bytes
}

fn store_with(record: Vec<u8>, group_id: u32) -> spore_assets::ContentStore {
    store_over(&[(raster_key(group_id, DOCUMENTED_INSTANCE_ID), record)])
}

fn resolve_one(
    store: &spore_assets::ContentStore,
    group_id: u32,
) -> spore_material::ResolvedTexture {
    resolve_texture(
        store,
        GmdlTextureRef {
            instance_id: DOCUMENTED_INSTANCE_ID,
            group_id,
        },
        RASTER_TYPE,
    )
    .expect("the record decodes")
}

// ---------------------------------------------------------------------------
// The `0x15` family
// ---------------------------------------------------------------------------

#[test]
fn a_flat_normal_record_is_graded_as_a_tangent_space_normal_map() {
    let store = store_with(flat_normal_raster(32, 32), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.role, TextureRole::TangentSpaceNormal);
    assert_eq!(resolved.role_evidence(), EvidenceLevel::Observed);
    // The evidence is published, not just the verdict.
    assert_eq!(resolved.statistics.texels, 32 * 32);
    assert_eq!(resolved.statistics.distinct[Lane::Alpha.index()], 1);
    assert_eq!(resolved.statistics.mean(Lane::Red), Some(255.0));
    assert_eq!(resolved.statistics.mean(Lane::Green), Some(128.0));
    assert_eq!(resolved.statistics.mean(Lane::Blue), Some(128.0));

    let role = resolved
        .claim(claims::TEXTURE_CONTENT_ROLE)
        .expect("subject");
    assert!(role.fact.is_available());
    assert_eq!(role.fact.level(), EvidenceLevel::Observed);
    assert!(
        role.fact.value().unwrap().contains("Z FIRST"),
        "the recorded text must say the channel order is unusual: {}",
        role.fact.value().unwrap()
    );
}

#[test]
fn the_normal_map_finding_is_delegated_to_the_crate_that_measured_it() {
    let store = store_with(flat_normal_raster(32, 32), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    let republished = resolved.normal_map_claim().expect("subject");
    assert_eq!(republished.subject, claims::NORMAL_MAP_CONTENT);
    assert_eq!(
        republished.fact.level(),
        EvidenceLevel::Verified,
        "a downgrade upstream must not silently survive the delegation"
    );
    // And it is the *upstream* grade, matching spore-texture's own table.
    let upstream = spore_texture::claims::claim(spore_texture::claims::LUMINANCE_TEXEL_LAYOUT)
        .expect("spore-texture declares the 0x15 texel layout");
    assert_eq!(republished.fact.level(), upstream.meaning.level());
}

#[test]
fn a_colour_image_in_the_normal_map_format_is_not_given_the_normal_map_role() {
    // The failure this crate exists to prevent: a role assigned from the format
    // word alone. Same fourcc, same framing, opposite content.
    let store = store_with(colour_raster(32, 32), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(
        resolved.role,
        TextureRole::Unidentified,
        "lane 0 averaging ~125 is not a normal's Z"
    );
    let role = resolved
        .claim(claims::TEXTURE_CONTENT_ROLE)
        .expect("subject");
    assert!(
        role.fact.value().unwrap().contains("matched no signature"),
        "{}",
        role.fact.value().unwrap()
    );
    // Unidentified is a landing place, not a blank: every channel is still
    // reported, as unknown.
    assert_eq!(resolved.channels().len(), 4);
    assert!(resolved
        .channels()
        .iter()
        .all(|channel| channel.role == ChannelRole::UnidentifiedLane));
}

// ---------------------------------------------------------------------------
// The DXT5 family
// ---------------------------------------------------------------------------

#[test]
fn a_dxt5_record_is_graded_as_a_layered_alpha_mask_not_as_colour() {
    // `support::raster_record` builds real DXT5 blocks, so the decoded texels
    // carry the corpus's signature: a degenerate colour field over a full-range
    // alpha plane.
    let store = store_with(raster_record(8, 8, 2, DXT5_FOURCC, 2), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.role, TextureRole::LayeredAlphaMask);
    assert_eq!(
        resolved.statistics.distinct[Lane::Green.index()],
        1,
        "the corpus measurement: green is constant in every texel"
    );
    assert_eq!(resolved.statistics.distinct[Lane::Blue.index()], 1);
    assert!(
        resolved.statistics.distinct[Lane::Alpha.index()] >= 2,
        "the alpha plane is not constant; the corpus measures 65-256"
    );

    // The "not a colour map" statement is recorded, and it names the
    // measurement rather than the opinion.
    let no_colour = resolved
        .claim(claims::DXT5_CARRIES_NO_COLOUR)
        .expect("subject");
    assert_eq!(no_colour.fact.level(), EvidenceLevel::Verified);
    assert!(no_colour.fact.value().unwrap().contains("477771"));
    // While the purpose stays unknown, which is the distinction the whole
    // exercise turns on.
    let purpose = resolved.claim(claims::MASK_LAYER_PURPOSE).expect("subject");
    assert!(purpose.fact.is_unavailable());
    assert_eq!(purpose.fact.reason(), Some(claims::MEANING_NOT_ESTABLISHED));
}

#[test]
fn there_is_no_diffuse_role_because_no_raster_record_in_the_corpus_is_one() {
    // Corpus: over 1374 decodable DXT5 records the decoded red channel takes
    // exactly two values and green and blue exactly one each, and the only
    // decodable raster format in Spore_Content besides DXT5 is the `0x15` normal
    // map. A `Diffuse` variant would be a claim about a record that does not
    // exist, so the enum must not grow one.
    let store = store_with(raster_record(8, 8, 2, DXT5_FOURCC, 2), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_ne!(resolved.role.as_str(), "diffuse");
    let channels = resolved.channels();
    assert!(
        channels
            .iter()
            .filter(|channel| channel.role.purpose_established())
            .count()
            == 0,
        "a DXT5 record has no channel whose purpose the corpus established"
    );
}

#[test]
fn both_layers_of_a_dxt5_record_are_published_as_distinct_surfaces() {
    // `support::raster_record` writes different block values per layer, so the
    // two decoded layers differ. Before this change only layer 0 was published
    // and layer 1 was discarded — a different image, not a duplicate. Corpus:
    // 1233/1233 referenced records differ byte-for-byte.
    let store = store_with(raster_record(8, 8, 2, DXT5_FOURCC, 2), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.layers.len(), 2);
    assert_eq!(resolved.layer_count, 2);
    assert_eq!(resolved.layer(1).expect("layer 1").pixels.len(), 8 * 8 * 4);
    assert_ne!(
        resolved.layers[0].pixels, resolved.layers[1].pixels,
        "a DXT5 record's layers are two surfaces, not one image and a copy"
    );
    // `image` stays as layer 0, because existing callers read it.
    assert_eq!(resolved.image.pixels, resolved.layers[0].pixels);
    // And each layer contributes its own four channels.
    assert_eq!(resolved.channels().len(), 8);
    let recorded = resolved
        .claim(claims::DXT5_LAYERS_ARE_DISTINCT)
        .expect("subject");
    assert_eq!(recorded.fact.level(), EvidenceLevel::Verified);
    assert!(recorded.fact.value().unwrap().contains("1233"));
}

#[test]
fn a_layer_index_past_the_end_is_none_rather_than_a_substitute() {
    let store = store_with(raster_record(8, 8, 2, DXT5_FOURCC, 2), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.layer(2), None);
    assert_eq!(resolved.layer(usize::MAX), None);
}

// ---------------------------------------------------------------------------
// Channels: the Z-first order, which is the trap
// ---------------------------------------------------------------------------

#[test]
fn the_normal_channels_are_named_in_file_order_so_the_caller_cannot_get_it_wrong() {
    let store = store_with(flat_normal_raster(32, 32), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    let channels = resolved.channels();
    assert_eq!(
        channels.len(),
        4,
        "the 0x15 family has one layer of four lanes"
    );
    assert_eq!(
        (channels[0].lane, channels[0].role),
        (Lane::Red, ChannelRole::NormalZ),
        "file lane 0 is Z. A renderer reading it as R gets a map with Z and X \
         swapped, which looks almost right and is not."
    );
    assert_eq!(channels[1].role, ChannelRole::NormalX);
    assert_eq!(channels[2].role, ChannelRole::NormalY);
    assert_eq!(
        channels[3].role,
        ChannelRole::UnidentifiedLane,
        "the fourth lane's meaning is unknown and is reported as unknown"
    );
    // Only the three normal components have an established purpose.
    assert!(channels.iter().all(|channel| channel.layer_index == 0));
    assert!(!channels[3].role.purpose_established());
}

#[test]
fn the_mask_channels_name_the_alpha_plane_and_degrade_the_colour_planes() {
    let store = store_with(raster_record(8, 8, 2, DXT5_FOURCC, 2), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    let channels = resolved.channels();
    for layer in 0..2usize {
        let at = |lane: usize| channels[layer * 4 + lane];
        assert_eq!(at(0).role, ChannelRole::DegenerateColour);
        assert_eq!(at(1).role, ChannelRole::DegenerateColour);
        assert_eq!(at(2).role, ChannelRole::DegenerateColour);
        assert_eq!(at(3).role, ChannelRole::MaskChannel);
        assert!(!at(3).role.purpose_established());
    }
}

// ---------------------------------------------------------------------------
// The two roles are kept apart from the sampler role
// ---------------------------------------------------------------------------

#[test]
fn a_resolved_texture_states_a_content_role_and_leaves_the_sampler_role_unresolved() {
    let store = store_with(flat_normal_raster(32, 32), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    // Content: answered.
    assert_eq!(resolved.role, TextureRole::TangentSpaceNormal);
    assert_eq!(
        claims::grade_of(claims::TEXTURE_CONTENT_ROLE),
        EvidenceLevel::Observed
    );
    // Sampler stage: still a non-finding, and now for a measured reason.
    assert_eq!(
        claims::grade_of(claims::SAMPLER_ROLE),
        EvidenceLevel::Unknown
    );
    let subjects: Vec<&str> = resolved.claims.iter().map(|c| c.subject.as_str()).collect();
    assert!(subjects.contains(&claims::TEXTURE_CONTENT_ROLE));
    assert!(
        !subjects.contains(&claims::SAMPLER_ROLE),
        "the sampler non-finding lives on the material, not on the resolved record, \
         which has no sampler to report"
    );
}

#[test]
fn an_envelope_inspection_reports_no_role_because_it_looked_at_no_texels() {
    let store = store_with(flat_normal_raster(32, 32), 0x4063_2900);
    let inspected = spore_material::inspect_envelope(
        &store,
        GmdlTextureRef {
            instance_id: DOCUMENTED_INSTANCE_ID,
            group_id: 0x4063_2900,
        },
        RASTER_TYPE,
    )
    .expect("the envelope reads");
    assert!(!inspected.pixels_decoded());
    assert_eq!(
        inspected.role,
        TextureRole::Unidentified,
        "a header read is not a content check"
    );
    assert_eq!(inspected.statistics.texels, 0);
    assert_eq!(inspected.layers.len(), 0);
    assert_eq!(
        inspected.channels().len(),
        0,
        "no texel was examined, so no channel is claimed"
    );
    assert_eq!(inspected.layer_count, 0);
    // And the identity of the record is still answerable: not knowing what the
    // texels are is not the same as not knowing what the record is.
    assert_eq!(inspected.envelope.fourcc, LUMINANCE_FOURCC);
    assert_eq!(inspected.envelope.width, 32);
}

#[test]
fn statistics_are_exact_over_every_byte_of_every_layer() {
    // A 4x4 all-`0x15` record: 16 texels of layer 0 and 4 of layer 1, and the
    // statistics must count exactly the base mip and no more.
    let store = store_with(flat_normal_raster(4, 4), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.statistics.texels, 16);
    assert_eq!(resolved.statistics.sum[Lane::Red.index()], 16 * 255);
    assert_eq!(resolved.statistics.sum[Lane::Green.index()], 16 * 0x80);
    assert_eq!(resolved.mip_count, 2, "the record declares two mips");
    // The 0x15 family has exactly one layer, and the published image is its base
    // mip; the rest of the chain is the decode's to hold.
    assert_eq!(resolved.layers.len(), 1);
    assert_eq!(resolved.layers[0].width, 4);
    assert_eq!(resolved.image.width, 4);
    assert_eq!(resolved.pixel_len(), 4 * 4 * 4);
    assert!(resolved.pixels_decoded());
}
