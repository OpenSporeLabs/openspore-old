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
    assert_eq!(
        role.fact.level(),
        claims::bc3_claim_ceiling(),
        "the role subject sits at the BC3 ceiling; the normal-map CONTENT claim is \
         separately VERIFIED upstream because the 0x15 family is uncompressed"
    );
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
        role.fact
            .value()
            .unwrap()
            .contains("no signature FOR ITS FORMAT WORD"),
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
fn a_dxt5_record_is_graded_as_colour_and_mask_not_as_a_flat_field() {
    // `support::raster_record` builds real BC3 blocks, so the decoded texels
    // carry the corpus's signature: three colour lanes that each take many
    // distinct values, over a fourth plane.
    let store = store_with(raster_record(16, 16, 2, DXT5_FOURCC), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.role, TextureRole::ColourAndMask);
    // The corpus measurement, restated on a synthetic record: every colour lane
    // is far above the floor of 8. Over the real corpus the minimum is 32.
    assert!(
        resolved.statistics.distinct[Lane::Red.index()] >= 8,
        "lane 0 takes at least the floor: {}",
        resolved.statistics.distinct[Lane::Red.index()]
    );
    assert!(
        resolved.statistics.distinct[Lane::Green.index()] >= 8,
        "lane 1 takes at least the floor: {}",
        resolved.statistics.distinct[Lane::Green.index()]
    );
    assert!(
        resolved.statistics.distinct[Lane::Blue.index()] >= 8,
        "lane 2 takes at least the floor: {}",
        resolved.statistics.distinct[Lane::Blue.index()]
    );
    assert!(
        resolved.statistics.distinct[Lane::Alpha.index()] >= 2,
        "the fourth plane is not constant here"
    );
    // And the evidence is published, not just the verdict: the three colour
    // lanes agree with one another and the fourth does not follow them.
    assert!(
        resolved
            .statistics
            .mean_colour_correlation()
            .expect("defined")
            > 0.5,
        "three mutually-correlated lanes are one colour image"
    );
    assert!(
        resolved
            .statistics
            .mean_alpha_correlation_abs()
            .expect("defined")
            < 0.5,
        "and the fourth plane is not one of them"
    );
    // The colour planes are real colour, which is the whole refutation: the old
    // model required green and blue to be CONSTANT and the corpus has zero
    // records where they are.
    assert_ne!(
        resolved.statistics.distinct[Lane::Green.index()],
        1,
        "the refuted model asserted green takes exactly one value"
    );
    assert_ne!(resolved.statistics.distinct[Lane::Blue.index()], 1);

    // The "there is no colour" statement is recorded **as a refutation**, under
    // the subject that used to assert it, and it names what is true instead.
    let refuted = resolved
        .claim(claims::DXT5_CARRIES_NO_COLOUR)
        .expect("subject");
    assert!(refuted.fact.is_available(), "a refutation carries a value");
    assert_eq!(refuted.fact.level(), EvidenceLevel::Verified);
    let text = refuted.fact.value().expect("a value");
    assert!(
        text.to_ascii_lowercase().contains("refuted"),
        "the recorded text must say it is a refutation: {text}"
    );
    assert!(
        text.contains("4436"),
        "and must carry the measurement that refuted it: {text}"
    );
    // The claim that replaced it, at the BC3 ceiling and no higher.
    let colour = resolved
        .claim(claims::COLOUR_AND_MASK_CONTENT)
        .expect("subject");
    assert_eq!(colour.fact.level(), EvidenceLevel::Inferred);
    assert_eq!(colour.fact.level(), claims::bc3_claim_ceiling());

    // While the purpose stays unknown, which is the distinction the whole
    // exercise turns on -- and which is now the *only* reason there is no
    // `Diffuse` variant.
    let purpose = resolved.claim(claims::MASK_LAYER_PURPOSE).expect("subject");
    assert!(purpose.fact.is_unavailable());
    assert_eq!(purpose.fact.reason(), Some(claims::MEANING_NOT_ESTABLISHED));
}

#[test]
fn a_constant_fourth_plane_is_still_a_colour_and_mask_record() {
    // The measured 46-of-1747 case: colour planes that vary, fourth plane that
    // does not. A signature requiring the fourth plane to vary would refuse it,
    // so the per-record answer is published instead of assumed.
    let store = store_with(flat_alpha_raster(16, 16, 2), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.role, TextureRole::ColourAndMask);
    assert!(resolved.statistics.is_constant(Lane::Alpha));
    assert_eq!(
        resolved.mask_plane_varies(),
        Some(false),
        "and the per-record answer is reported rather than assumed by the role"
    );
}

#[test]
fn there_is_no_diffuse_role_because_no_shader_has_been_read() {
    // **The reason for this test changed on 2026-10-05, and the change matters.**
    //
    // It used to say: "no raster record in the corpus decodes as a colour map" —
    // which was FALSE, and false because this crate's own codec was reading 8
    // bytes per block. Read as BC3, 1 747 records carry a full-range,
    // mutually-correlated colour image.
    //
    // It now says the only thing that is still true: the colour planes are real
    // and **what they colour is not established**. No shader has been read, no
    // referenced group id has a canonical name, and not one lane of a DXT5 record
    // has an established purpose. A `Diffuse` variant would convert an
    // unestablished purpose into a confident role.
    let store = store_with(raster_record(16, 16, 2, DXT5_FOURCC), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_ne!(resolved.role.as_str(), "diffuse");
    assert_eq!(
        resolved
            .channels()
            .iter()
            .filter(|channel| channel.role.purpose_established())
            .count(),
        0,
        "a DXT5 record has no lane whose purpose the corpus established"
    );
}

#[test]
fn a_record_holds_one_chain_and_index_one_past_it_is_none() {
    // **Rewritten, not deleted.** It used to assert that a DXT5 record's two
    // layers are two distinct surfaces and that layer 1 is a real image. The
    // corpus refuted the two-layer model outright: every raster record is ONE
    // chain, so there is no layer 1 to be a second surface. The old measurement
    // was real arithmetic over the wrong thing — it compared the two halves of
    // one base mip, which an 8-byte block stride had split in two.
    //
    // What survives, and what this now pins, is the refutation itself plus the
    // property that made it dangerous: an out-of-range index must answer `None`
    // rather than substituting something.
    let store = store_with(raster_record(16, 16, 2, DXT5_FOURCC), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.layers.len(), 1);
    assert_eq!(resolved.layer_count, 1);
    assert_eq!(
        resolved.layer(0).expect("chain 0").pixels.len(),
        16 * 16 * 4
    );
    assert_eq!(resolved.layer(1), None, "there is no second chain");
    assert_eq!(resolved.layer(usize::MAX), None);
    assert_eq!(
        resolved.channels().len(),
        4,
        "one chain of four lanes. The old 'eight' was two records x two layers."
    );
    // `image` is that chain's base mip.
    assert_eq!(resolved.image.pixels, resolved.layers[0].pixels);

    // And the refutation is recorded under the original subject, with the old
    // numbers visible so a reader can see what changed.
    let refuted = resolved
        .claim(claims::DXT5_LAYERS_ARE_DISTINCT)
        .expect("subject");
    assert!(refuted.fact.is_available());
    let text = refuted.fact.value().expect("a value");
    assert!(text.to_ascii_lowercase().contains("refuted"), "{text}");
    assert!(
        text.contains("1233"),
        "the refuted measurement must stay visible: {text}"
    );
    let layer_count = resolved
        .claim(claims::LAYER_COUNT_IS_DERIVED)
        .expect("subject");
    assert!(layer_count
        .fact
        .value()
        .expect("a value")
        .to_ascii_lowercase()
        .contains("one chain"));
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
fn the_colour_channels_name_three_colour_planes_and_one_mask_plane() {
    // **Rewritten, not deleted.** It used to assert that lanes 0-2 of a DXT5
    // record were `DegenerateColour` — a spelling that measured this crate's own
    // broken codec rather than any record. Read as BC3 those lanes are a
    // mutually-correlated colour image, and the fourth is a plane that does not
    // follow them.
    let store = store_with(raster_record(16, 16, 2, DXT5_FOURCC), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    let channels = resolved.channels();
    assert_eq!(channels.len(), 4, "one chain of four lanes");
    for (index, lane) in [Lane::Red, Lane::Green, Lane::Blue].into_iter().enumerate() {
        assert_eq!(channels[index].lane, lane);
        assert_eq!(channels[index].role, ChannelRole::ColourPlane);
        assert!(
            !channels[index].role.purpose_established(),
            "the colour is real; what it is FOR is not established"
        );
    }
    assert_eq!(channels[3].lane, Lane::Alpha);
    assert_eq!(channels[3].role, ChannelRole::MaskChannel);
    assert!(!channels[3].role.purpose_established());
    assert_eq!(resolved.mask_plane_varies(), Some(true));
}

#[test]
fn a_resolved_texture_states_a_content_role_and_leaves_the_sampler_role_unresolved() {
    let store = store_with(flat_normal_raster(32, 32), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    // Content: answered. The subject sits at the BC3 codec's grade, because one
    // subject records both content families and the colour reading cannot outrank
    // the codec that produced it.
    assert_eq!(resolved.role, TextureRole::TangentSpaceNormal);
    assert_eq!(
        claims::grade_of(claims::TEXTURE_CONTENT_ROLE),
        claims::bc3_claim_ceiling()
    );
    // And this record's own check ran, which is the separate question
    // `role_evidence` answers.
    assert_eq!(resolved.role_evidence(), EvidenceLevel::Observed);
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
    assert_eq!(inspected.statistics, spore_material::MipStatistics::empty());
    assert_eq!(inspected.layers.len(), 0);
    assert_eq!(
        inspected.channels().len(),
        0,
        "no texel was examined, so no channel is claimed"
    );
    assert_eq!(inspected.layer_count, 0);
    assert_eq!(
        inspected.mask_plane_varies(),
        None,
        "'we did not look' is not 'it does not vary'"
    );
    // And the identity of the record is still answerable: not knowing what the
    // texels are is not the same as not knowing what the record is.
    assert_eq!(inspected.envelope.fourcc, LUMINANCE_FOURCC);
    assert_eq!(inspected.envelope.width, 32);
}

#[test]
fn statistics_are_exact_over_every_byte_of_the_base_mip() {
    // A 4x4 all-`0x15` record: 16 texels in the base mip and 4 in the next
    // level, and the statistics must count exactly the base mip and no more.
    let store = store_with(flat_normal_raster(4, 4), 0x4063_2900);
    let resolved = resolve_one(&store, 0x4063_2900);
    assert_eq!(resolved.statistics.texels, 16);
    assert_eq!(resolved.statistics.sum[Lane::Red.index()], 16 * 255);
    assert_eq!(resolved.statistics.sum[Lane::Green.index()], 16 * 0x80);
    assert_eq!(resolved.mip_count, 2, "the record declares two mips");
    // The `0x15` family has exactly one chain, and the published image is its
    // base mip; the rest of the chain is the decode's to hold.
    assert_eq!(resolved.layers.len(), 1);
    assert_eq!(resolved.layers[0].width, 4);
    assert_eq!(resolved.image.width, 4);
    assert_eq!(resolved.pixel_len(), 4 * 4 * 4);
    assert!(resolved.pixels_decoded());
}
