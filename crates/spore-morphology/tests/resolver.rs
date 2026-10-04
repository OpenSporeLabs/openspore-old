//! Hermetic resolver tests: synthetic records, no game install.
//!
//! These build the smallest records that exercise each hop, because the point of
//! a hermetic test here is to pin **the chain's control flow** — especially where
//! it stops — not to re-measure the corpus. The corpus tests do that.

use spore_core::ResourceKey;
use spore_creator_core::{Block, BlockId, BlockTransform, CreatorDocument, Family};
use spore_morphology::resolve::{GROUP_PART, TYPE_GMSH, TYPE_PROP, TYPE_RW4};
use spore_morphology::{
    compose, MemorySource, PartSlot, ResolvedPart, Resolver, Stage, StopReason,
};
use spore_rigblock::TYPE_RIGBLOCK as RIGBLOCK;

/// A creature-family rigblock group.
const CREATURE_GROUP: u32 = 0x4062_6000;
const INSTANCE: u32 = 0x0001_3530;

/// Builds a minimal rigblock record: `k = 0`, so a 36-byte header and no cells.
fn rigblock_record() -> Vec<u8> {
    let mut out = vec![0u8; 36];
    for axis in 0..6 {
        out[4 * axis..4 * axis + 4].copy_from_slice(&1.0f32.to_le_bytes());
    }
    out
}

/// Builds a `prop` record carrying one validated 16-byte pointer entry.
///
/// The form is `{ instance, type, group, 0x00A00CE4 }`, validated on 388/388
/// resolving keys elsewhere in Spore. A looser pattern would find keys that do
/// not resolve, which is the failure this shape prevents.
fn prop_record(instance: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&instance.to_le_bytes());
    out.extend_from_slice(&TYPE_RW4.to_le_bytes());
    out.extend_from_slice(&GROUP_PART.to_le_bytes());
    out.extend_from_slice(&0x00A0_0CE4u32.to_le_bytes());
    out
}

/// Builds an RW4 container with one `Skeleton (0x70002)` section.
fn container_with_skeleton(bones: &[(u32, i32)]) -> Vec<u8> {
    let n = bones.len();
    let payload_len = 0x18 + 12 * n;
    let mut payload = vec![0u8; payload_len];
    let a1 = 0x18;
    let a2 = a1 + 4 * n;
    let a3 = a2 + 4 * n;
    payload[0x0C..0x10].copy_from_slice(&(n as u32).to_le_bytes());
    payload[0x14..0x18].copy_from_slice(&(n as u32).to_le_bytes());
    for (index, (identity, parent)) in bones.iter().enumerate() {
        payload[a1 + 4 * index..a1 + 4 * index + 4].copy_from_slice(&identity.to_le_bytes());
        payload[a3 + 4 * index..a3 + 4 * index + 4]
            .copy_from_slice(&(*parent as u32).to_le_bytes());
    }

    // A container with one section row and nothing else. The resolver's walker
    // reads the section rows directly and does not consult the SectionTypes
    // block, so this fixture does not fabricate one — building it at 0xB4 would
    // have overwritten the payload that starts at 0xB0.
    const HEADER_END: usize = 0x98;
    let section_row = HEADER_END;
    let payload_at = HEADER_END + 24;
    let mut out = vec![0u8; payload_at + payload_len];
    out[..8].copy_from_slice(b"\x89RW4w32\0");
    out[0x24..0x28].copy_from_slice(&1i32.to_le_bytes());
    out[0x30..0x34].copy_from_slice(&(section_row as u32).to_le_bytes());
    out[section_row..section_row + 4].copy_from_slice(&(payload_at as u32).to_le_bytes());
    out[section_row + 8..section_row + 12].copy_from_slice(&(payload_len as i32).to_le_bytes());
    out[section_row + 0x14..section_row + 0x18]
        .copy_from_slice(&spore_rigblock::SECTION_SKELETON.to_le_bytes());
    out[payload_at..].copy_from_slice(&payload);
    out
}

/// A document with `count` blocks all pointing at the same rigblock.
fn document(count: usize) -> CreatorDocument {
    CreatorDocument {
        format_version: Some(16),
        properties: Default::default(),
        blocks: (0..count)
            .map(|_| Block {
                block_id: Some(BlockId::new(CREATURE_GROUP, INSTANCE)),
                transform: BlockTransform::default(),
                ..Block::default()
            })
            .collect(),
    }
}

/// A source holding the full chain for `INSTANCE`.
fn complete_source() -> MemorySource {
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        rigblock_record(),
    );
    source.insert(
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
        prop_record(INSTANCE),
    );
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
        container_with_skeleton(&[(spore_rigblock::identity_of("root").unwrap().to_raw(), -1)]),
    );
    source.insert(
        ResourceKey::new(TYPE_GMSH, GROUP_PART, INSTANCE),
        vec![1, 2, 3, 4],
    );
    source
}

#[test]
fn a_complete_chain_resolves_and_carries_its_skeleton() {
    let source = complete_source();
    let composition = compose(&document(3), &source).expect("composes");

    let counts = composition.counts();
    assert_eq!(
        counts,
        spore_morphology::Counts {
            resolved: 3,
            unresolved: 0
        }
    );
    assert!(counts.is_complete());
    assert_eq!(composition.parts_with_skeleton(), 3);

    let part = composition.resolved().next().expect("one resolved part");
    assert_eq!(part.block_id, BlockId::new(CREATURE_GROUP, INSTANCE));
    assert!(
        part.geometry_key.is_some(),
        "the geometry half must be present"
    );
    assert_eq!(part.trail.furthest(), Stage::Container);
    assert!(part.trail.is_complete());
    assert_eq!(part.named_bones(), vec![("root", Some(0))]);
}

#[test]
fn every_block_keeps_its_slot_even_when_it_fails_to_resolve() {
    // Nothing is in the source at all: every block stops at the first hop.
    let source = MemorySource::new();
    let composition = compose(&document(4), &source).expect("composes");

    assert_eq!(composition.slots().len(), 4, "the index space must survive");
    let counts = composition.counts();
    assert_eq!(counts.resolved, 0);
    assert_eq!(counts.unresolved, 4);
    assert!(!counts.is_complete());
    for (index, slot) in composition.slots().iter().enumerate() {
        assert_eq!(slot.unresolved().expect("unresolved").block, index);
    }
}

#[test]
fn a_missing_rigblock_stops_at_that_hop_and_says_which_key_it_tried() {
    // Present: the prop and the pair. Absent: only the rigblock.
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
        prop_record(INSTANCE),
    );
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
        container_with_skeleton(&[(spore_rigblock::identity_of("root").unwrap().to_raw(), -1)]),
    );
    source.insert(
        ResourceKey::new(TYPE_GMSH, GROUP_PART, INSTANCE),
        vec![1, 2, 3, 4],
    );

    let key = ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE);
    assert!(!source.contains(&key), "the rigblock must really be absent");

    let composition = compose(&document(1), &source).expect("composes");
    let unresolved = composition.unresolved().next().expect("unresolved");
    assert_eq!(unresolved.stage, Stage::Rigblock);
    assert_eq!(unresolved.reason, StopReason::NoSuchRecord);
    let trail = unresolved.trail.as_ref().expect("a trail exists");
    assert_eq!(
        trail.key_at(Stage::Rigblock),
        Some(key),
        "the key we looked at must be recorded"
    );
    assert!(unresolved.summary().contains("no_such_record"));
}

#[test]
fn a_missing_prop_stops_after_the_rigblock() {
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        rigblock_record(),
    );
    let composition = compose(&document(1), &source).expect("composes");
    let unresolved = composition.unresolved().next().expect("unresolved");
    assert_eq!(unresolved.stage, Stage::Prop);
    assert_eq!(unresolved.reason, StopReason::NoSuchRecord);
    assert!(unresolved
        .trail
        .as_ref()
        .expect("trail")
        .key_at(Stage::Rigblock)
        .is_some());
}

#[test]
fn a_prop_without_the_validated_pointer_is_wrong_structure_not_missing() {
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        rigblock_record(),
    );
    // A record with no 16-byte tagged entry: present, but not the expected shape.
    source.insert(
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
        vec![0xABu8; 64],
    );
    let composition = compose(&document(1), &source).expect("composes");
    let unresolved = composition.unresolved().next().expect("unresolved");
    assert_eq!(unresolved.stage, Stage::Prop);
    assert_eq!(
        unresolved.reason,
        StopReason::WrongStructure,
        "'present but unrecognised' must not read as 'absent'"
    );
}

#[test]
fn an_undecodable_rigblock_is_undecodable_not_absent() {
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        vec![0u8; 4],
    );
    let composition = compose(&document(1), &source).expect("composes");
    let unresolved = composition.unresolved().next().expect("unresolved");
    assert_eq!(unresolved.stage, Stage::Rigblock);
    assert_eq!(unresolved.reason, StopReason::Undecodable);
}

#[test]
fn a_container_without_a_skeleton_resolves_with_no_body_graph() {
    // 256 of 1824 real containers hold no skeleton. That is a resolved part with
    // no graph, never an error and never an invented empty skeleton.
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        rigblock_record(),
    );
    source.insert(
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
        prop_record(INSTANCE),
    );
    let mut empty_container = vec![0u8; 0x98];
    empty_container[..8].copy_from_slice(b"\x89RW4w32\0");
    empty_container[0x24..0x28].copy_from_slice(&0i32.to_le_bytes());
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
        empty_container,
    );
    source.insert(ResourceKey::new(TYPE_GMSH, GROUP_PART, INSTANCE), vec![9]);

    let composition = compose(&document(1), &source).expect("composes");
    let counts = composition.counts();
    assert_eq!(counts.unresolved, 0, "the part itself resolved");
    let part = composition.resolved().next().expect("resolved");
    assert!(part.skeleton.is_none());
    assert_eq!(part.bones(), None);
    assert!(part.named_bones().is_empty());
}

#[test]
fn a_container_with_no_geometry_half_records_the_asymmetry() {
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        rigblock_record(),
    );
    source.insert(
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
        prop_record(INSTANCE),
    );
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
        container_with_skeleton(&[(spore_rigblock::identity_of("root").unwrap().to_raw(), -1)]),
    );
    // No gmsh.

    let composition = compose(&document(1), &source).expect("composes");
    let unresolved = composition.unresolved().next().expect("unresolved");
    assert_eq!(unresolved.stage, Stage::PartPair);
    assert_eq!(unresolved.reason, StopReason::NoSuchRecord);
}

#[test]
fn the_first_pointer_wins_and_a_disagreement_with_the_block_downgrades_the_hop() {
    // Measured rule: the prop's **first** validated container entry is the part,
    // and it is frequently NOT the block's own instance. Two pointers, the first
    // of which is elsewhere; only the first one's pair exists in the source.
    const OTHER: u32 = INSTANCE ^ 0xFFFF;
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        rigblock_record(),
    );
    let mut prop = Vec::new();
    for instance in [OTHER, INSTANCE] {
        prop.extend_from_slice(&instance.to_le_bytes());
        prop.extend_from_slice(&TYPE_RW4.to_le_bytes());
        prop.extend_from_slice(&GROUP_PART.to_le_bytes());
        prop.extend_from_slice(&0x00A0_0CE4u32.to_le_bytes());
    }
    source.insert(ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE), prop);
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, OTHER),
        container_with_skeleton(&[(spore_rigblock::identity_of("root").unwrap().to_raw(), -1)]),
    );
    source.insert(ResourceKey::new(TYPE_GMSH, GROUP_PART, OTHER), vec![7]);

    let composition = compose(&document(1), &source).expect("composes");
    let part = composition.resolved().next().expect("resolves");
    assert_eq!(
        part.container_key,
        ResourceKey::new(TYPE_RW4, GROUP_PART, OTHER),
        "the first pointer selects the part, not the block's own instance"
    );
    assert_eq!(
        part.trail.weakest_grade(),
        spore_morphology::Grade::Inferred,
        "a pointer that disagrees with the block must downgrade the hop, not vanish"
    );
    assert!(part.skeleton.is_some());
}

#[test]
fn a_pointer_that_agrees_with_the_block_is_measured() {
    let source = complete_source();
    let composition = compose(&document(1), &source).expect("composes");
    let part = composition.resolved().next().expect("resolves");
    assert_eq!(
        part.trail.weakest_grade(),
        spore_morphology::Grade::Measured,
        "two signals agreeing is a measured hop"
    );
}

#[test]
fn an_empty_document_is_an_error_not_an_empty_composition() {
    let source = complete_source();
    assert_eq!(
        compose(&CreatorDocument::default(), &source).err(),
        Some(spore_morphology::MorphologyError::NoBlocksToResolve)
    );
}

#[test]
fn a_document_that_names_no_family_still_composes() {
    // `compose` takes the already-parsed document, so family is not re-decided
    // here; this pins that the resolver does not require one.
    let mut document = document(1);
    document.format_version = None;
    let source = complete_source();
    assert_eq!(
        compose(&document, &source)
            .expect("composes")
            .counts()
            .resolved,
        1
    );
    let _ = Family::Creature;
}

#[test]
fn the_provenance_trail_names_every_hop_it_took() {
    let source = complete_source();
    let composition = compose(&document(1), &source).expect("composes");
    let part: &ResolvedPart = composition.resolved().next().expect("resolved");
    let stages: Vec<Stage> = part.trail.hops().iter().map(|hop| hop.stage).collect();
    assert_eq!(stages, Stage::ALL.to_vec(), "the whole chain, in order");
    assert!(part.trail.as_provenance().len() >= 4);
    assert_eq!(part.trail.grades().len(), part.trail.as_provenance().len());
}

#[test]
fn slots_are_in_document_order_and_indexed_by_block() {
    let source = complete_source();
    let mut document = document(2);
    // Second block points somewhere that does not exist.
    document.blocks[1].block_id = Some(BlockId::new(CREATURE_GROUP, 0xDEAD_BEEF));

    let resolver = Resolver::new(&source);
    let composition = resolver.compose(&document).expect("composes");
    assert!(matches!(composition.slots()[0], PartSlot::Resolved(_)));
    assert!(matches!(composition.slots()[1], PartSlot::Unresolved(_)));
    assert_eq!(
        composition.slots()[0].resolved().expect("resolved").block(),
        0
    );
    assert_eq!(
        composition.slots()[1]
            .unresolved()
            .expect("unresolved")
            .block,
        1
    );
    assert_eq!(
        composition.counts(),
        spore_morphology::Counts {
            resolved: 1,
            unresolved: 1
        }
    );
}
