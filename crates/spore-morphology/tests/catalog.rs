//! Hermetic catalogue tests: synthetic records, no game install.
//!
//! The corpus tests measure the real numbers. What is pinned here is every
//! *behaviour* the catalogue claims, and above all the distinctions it exists to
//! preserve: absent versus empty versus not-looked-for, the rigblock box versus
//! the geometry extent, a recovered vocabulary versus a vocabulary in use, and
//! technical identity versus a display name that does not exist.

use std::collections::BTreeMap;

use spore_core::{EvidenceLevel, ResourceKey};
use spore_creator_core::{Block, BlockId, BlockTransform, CreatorDocument, Family};
use spore_morphology::catalog::{self, Absent, DocumentRecord};
use spore_morphology::resolve::{GROUP_PART, TYPE_GMSH, TYPE_PROP, TYPE_RW4};
use spore_morphology::{Catalog, Grade, Lookup, MemorySource, RecordSource, Stage, StopReason};
use spore_rigblock::TYPE_RIGBLOCK as RIGBLOCK;

const CREATURE_GROUP: u32 = 0x4062_6000;
const INSTANCE: u32 = 0x0001_3530;
const CELL_GROUP: u32 = 0x4061_6000;
const OTHER_INSTANCE: u32 = 0x0002_0001;
const DOCUMENT: ResourceKey = ResourceKey::new(0x2b97_8c46, CREATURE_GROUP, 0x145f_f205);

/// A minimal rigblock record: `k = 0`, so a 36-byte header and no cells.
fn rigblock_record() -> Vec<u8> {
    let mut out = vec![0u8; 36];
    for axis in 0..6 {
        out[4 * axis..4 * axis + 4].copy_from_slice(&1.0f32.to_le_bytes());
    }
    out
}

/// A rigblock with `k == 1`, so it carries four cells and therefore an envelope.
///
/// The header's six floats are `max_low = [0,0,0]` and `min_high = [1,1,1]` with
/// a step of 1.0 and every cell nibble zero, which is the only ordering that
/// yields a positive derived span: the field names read backwards, which is
/// recorded in `spore_rigblock` and is why this fixture's numbers are stated
/// rather than assumed. Every cell therefore spans `0..1` on every axis, so the
/// envelope's span is `[1,1,1]` and the comparison against a `1 x 1 x 0` geometry
/// reads `1, 1, None`.
fn rigblock_with_one_cell() -> Vec<u8> {
    let mut out = vec![0u8; 36 + 3 * 4];
    for (axis, value) in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0].iter().enumerate() {
        out[4 * axis..4 * axis + 4].copy_from_slice(&value.to_le_bytes());
    }
    out[0x18..0x1C].copy_from_slice(&1.0f32.to_le_bytes());
    out[0x1C] = 4;
    out
}

/// A `prop` record carrying one validated 16-byte pointer entry.
fn prop_record(instance: u32) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&instance.to_le_bytes());
    out.extend_from_slice(&TYPE_RW4.to_le_bytes());
    out.extend_from_slice(&GROUP_PART.to_le_bytes());
    out.extend_from_slice(&0x00A0_0CE4u32.to_le_bytes());
    out
}

/// A RW4 container with one `Skeleton (0x70002)` section.
fn container_with_skeleton(bones: &[(u32, i32)], extra: &[u8]) -> Vec<u8> {
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
    out.extend_from_slice(extra);
    out
}

/// A minimal real `gmsh` record: 3 vertices, 1 triangle, 1 bone.
///
/// Vertex span is `1 x 1 x 0`, so a comparison against a unit rigblock envelope
/// gives ratios of `1, 1, None` — the `None` on the third axis is the point: a
/// ratio against a degenerate extent has no value.
fn gmsh_record() -> Vec<u8> {
    let mut out = vec![0u8; 0x18];
    out[0x00..0x04].copy_from_slice(&0x0100_0000u32.to_le_bytes());
    out[0x04..0x08].copy_from_slice(&0x0100_0000u32.to_le_bytes());
    out[0x08..0x0C].copy_from_slice(&0x0000_0400u32.to_le_bytes());
    // Recipe 0x00010300 declares a 3-block chain, so three blocks follow.
    out[0x0C..0x10].copy_from_slice(&0x0001_0300u32.to_le_bytes());
    out[0x10..0x14].copy_from_slice(&3u32.to_le_bytes());
    out[0x14..0x16].copy_from_slice(&3u16.to_be_bytes());
    out[0x16] = 0x00;
    out[0x17] = 0x0C;
    for corner in [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
        for value in corner {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    // Block fields are big-endian: id, count, elemSize.
    out.extend_from_slice(&0x0000_0001u32.to_be_bytes());
    out.extend_from_slice(&3u32.to_be_bytes());
    out.extend_from_slice(&2u16.to_be_bytes());
    for index in [0u16, 1, 2] {
        out.extend_from_slice(&index.to_le_bytes());
    }
    out.extend_from_slice(&0x1500_0608u32.to_be_bytes());
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&4u16.to_be_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0x0A00_0400u32.to_be_bytes());
    out.extend_from_slice(&3u32.to_be_bytes());
    out.extend_from_slice(&16u16.to_be_bytes());
    for _ in 0..3 {
        for value in [0.0f32, 0.0, 0.0, 1.0] {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    // Tail: nv, K1 = 2, the K1 payload, nBones, the 4/0/0/0 word, one range,
    // then the 14-byte T2 entry.
    out.extend_from_slice(&3u32.to_be_bytes());
    out.push(2);
    out.push(0);
    for index in 0..2u8 {
        out.push(index);
        out.extend_from_slice(&[0xFF, 0xFF, 0x00]);
    }
    out.extend_from_slice(&1u32.to_be_bytes());
    out.extend_from_slice(&[4, 0, 0, 0]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&4u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&31u16.to_be_bytes());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    out
}

/// A source holding the whole chain for one `(group, instance)`.
fn chain(group: u32, instance: u32) -> MemorySource {
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, group, instance),
        rigblock_record(),
    );
    source.insert(
        ResourceKey::new(TYPE_PROP, group, instance),
        prop_record(instance),
    );
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, instance),
        container_with_skeleton(
            &[(spore_rigblock::identity_of("root").unwrap().to_raw(), -1)],
            &[],
        ),
    );
    source.insert(
        ResourceKey::new(TYPE_GMSH, GROUP_PART, instance),
        gmsh_record(),
    );
    source
}

/// A source whose rigblock carries a cell, so it has an envelope.
fn chain_with_bounds(group: u32, instance: u32) -> MemorySource {
    let mut source = chain(group, instance);
    source.insert(
        ResourceKey::new(RIGBLOCK, group, instance),
        rigblock_with_one_cell(),
    );
    source
}

/// A document whose blocks all name `ids`, in order.
fn document(ids: &[(u32, u32)]) -> CreatorDocument {
    CreatorDocument {
        format_version: Some(16),
        properties: Default::default(),
        blocks: ids
            .iter()
            .map(|(group, instance)| Block {
                block_id: Some(BlockId::new(*group, *instance)),
                transform: BlockTransform::default(),
                ..Block::default()
            })
            .collect(),
    }
}

fn catalogue(
    source: &dyn spore_morphology::RecordSource,
    documents: &[(ResourceKey, Family, CreatorDocument)],
) -> Catalog {
    let records: Vec<DocumentRecord<'_>> = documents
        .iter()
        .map(|(key, family, document)| DocumentRecord {
            key: *key,
            family: *family,
            document,
        })
        .collect();
    Catalog::build(source, &records)
}

#[test]
fn one_entry_per_distinct_identity_and_every_block_is_referenced() {
    // Five blocks, two identities, two documents. The catalogue is a list of
    // parts, so two entries; the block census is five, so five references.
    let creature = document(&[
        (CREATURE_GROUP, INSTANCE),
        (CREATURE_GROUP, INSTANCE),
        (CREATURE_GROUP, OTHER_INSTANCE),
    ]);
    let cell = document(&[(CELL_GROUP, INSTANCE), (CELL_GROUP, INSTANCE)]);
    let source = MemorySource::new();
    let catalog = catalogue(
        &source,
        &[
            (DOCUMENT, Family::Creature, creature),
            (
                ResourceKey::new(0x3d97_a8e4, CELL_GROUP, 0x0c79_63fb),
                Family::Cell,
                cell,
            ),
        ],
    );

    assert_eq!(
        catalog.len(),
        3,
        "three distinct identities: the group is part of the identity"
    );
    let counts = catalog.counts();
    assert_eq!(counts.entries, 3);
    assert_eq!(counts.references, 5, "every block is accounted for");
    assert_eq!(
        counts.documents, 3,
        "each entry is named by one document, and the two documents together name \
         three entries"
    );

    // Entries come out in ascending (group, instance) order, not document order,
    // so two runs over the same documents produce the same list.
    let order: Vec<(u32, u32)> = catalog
        .entries()
        .iter()
        .map(|e| (e.group_id(), e.instance_id()))
        .collect();
    let mut sorted = order.clone();
    sorted.sort_unstable();
    assert_eq!(order, sorted);

    let entry = catalog
        .entry(BlockId::new(CELL_GROUP, INSTANCE))
        .expect("entry");
    assert_eq!(entry.reference_count(), 2);
    assert_eq!(entry.document_count(), 1);
    assert_eq!(entry.families(), vec![Family::Cell]);
    let creature_entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    assert_eq!(creature_entry.reference_count(), 2);
    assert_eq!(creature_entry.families(), vec![Family::Creature]);
}

#[test]
fn a_block_that_wrote_no_blockid_is_not_a_catalog_entry() {
    // A document can place a block with no readable id. There is no identity to
    // catalogue, and inventing one would be inventing a part.
    let mut document = document(&[(CREATURE_GROUP, INSTANCE)]);
    document.blocks.push(Block {
        block_id: None,
        ..Block::default()
    });
    let source = MemorySource::new();
    let catalog = catalogue(&source, &[(DOCUMENT, Family::Creature, document)]);
    assert_eq!(catalog.len(), 1, "only the readable id is an entry");
    assert_eq!(catalog.counts().references, 1);
}

#[test]
fn a_resolved_entry_carries_real_counts_and_no_invented_name() {
    let source = chain(CREATURE_GROUP, INSTANCE);
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");

    assert!(entry.is_authorable());
    assert_eq!(entry.unresolved, None);
    assert_eq!(entry.weakest_grade, Some(Grade::Measured));
    assert_eq!(
        entry.rigblock_key,
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE)
    );
    assert_eq!(
        entry.prop_key,
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE)
    );
    assert_eq!(
        entry.container_key,
        Some(ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE))
    );
    assert_eq!(
        entry.geometry_key,
        Some(ResourceKey::new(TYPE_GMSH, GROUP_PART, INSTANCE))
    );

    let geometry = entry.geometry.value().expect("geometry decoded");
    assert_eq!(geometry.vertices, 3);
    assert_eq!(geometry.triangles, 1);
    let (low, high) = geometry.extent.expect("the mesh has vertices");
    assert_eq!(low, [0.0, 0.0, 0.0]);
    assert_eq!(high, [1.0, 1.0, 0.0]);

    let skeleton = entry.skeleton.value().expect("body graph");
    assert_eq!(skeleton.bones, 1);
    assert_eq!(skeleton.named, 1);
    assert_eq!(skeleton.names, vec![("root", 0)]);

    // The identity is the whole of it: no type in this crate has a name field,
    // so `summary` is pinned to exactly what a report gets — the pair, the
    // reference counts, the real counts and the grade.
    assert_eq!(
        entry.summary(),
        "0x40626000:0x00013530  1 block(s) in 1 document(s)  3 verts / 1 tris  \
         1 bones, 1 named  grade measured"
    );
}

#[test]
fn the_rigblock_bounds_box_and_the_geometry_extent_are_two_measurements() {
    let source = chain_with_bounds(CREATURE_GROUP, INSTANCE);
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");

    let bounds = entry.rigblock.value().expect("rigblock decoded");
    assert_eq!(bounds.levels, 1);
    assert_eq!(bounds.cells, 4, "`k == 1` carries 4^1 cells");
    assert!(bounds.has_cells());
    assert!(bounds.span.is_some());

    // Both are present, and they are different fields.
    assert!(entry.extents.rigblock_span.is_some());
    assert!(entry.extents.geometry_extent.is_some());
    assert!(entry.extents.is_comparable());
    // 1x1x0 geometry against a 1x1x1 rigblock cell: two ratios, and `None` on
    // the third because the geometry is flat there. A ratio against a degenerate
    // extent is `None`, not `0.0` and not infinity.
    assert_eq!(entry.extents.axis_ratio[0], Some(1.0));
    assert_eq!(entry.extents.axis_ratio[1], Some(1.0));
    assert_eq!(entry.extents.axis_ratio[2], None);
    assert_eq!(entry.extents.widest_axis_ratio(), Some(1.0));
}

#[test]
fn the_two_rigblock_measurements_are_both_reported_and_are_not_the_same_number() {
    // The header declares where the box is; the cells say how much of it is
    // occupied. A report that says "the rigblock bounds" without naming which one
    // has not said anything checkable, so the entry carries both.
    //
    // Header `0..1` per axis with a step of 0.5 and every cell nibble `q_low = 1`,
    // `q_high = 0`: the declared span is 1 and the cell-derived span is 1.5,
    // because quantisation rounds a cell outward by up to one step.
    let mut out = vec![0u8; 36 + 3 * 4];
    for (axis, value) in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0].iter().enumerate() {
        out[4 * axis..4 * axis + 4].copy_from_slice(&value.to_le_bytes());
    }
    out[0x18..0x1C].copy_from_slice(&0.5f32.to_le_bytes());
    out[0x1C] = 4;
    for cell in 0..4 {
        out[0x24 + 3 * cell] = 0x10;
        out[0x24 + 3 * cell + 1] = 0x10;
        out[0x24 + 3 * cell + 2] = 0x10;
    }

    let mut source = chain(CREATURE_GROUP, INSTANCE);
    source.insert(ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE), out);
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    let bounds = entry.rigblock.value().expect("the record decoded");
    assert_eq!(bounds.declared_span, Some([1.0, 1.0, 1.0]));
    assert_eq!(bounds.span, Some([1.5, 1.5, 1.5]));
    assert_ne!(
        bounds.declared_span, bounds.span,
        "the two readings are different numbers and reporting one of them as the \
         rigblock bounds would be choosing a factor of two silently"
    );
    // `ExtentComparison` uses the cell-derived span, and says which.
    assert_eq!(entry.extents.rigblock_span, bounds.span);
    assert_eq!(entry.rigblock_span(), bounds.span);
}

#[test]
fn a_rigblock_with_no_cells_still_declares_a_span_and_says_which_it_is() {
    // A `k == 0` record has no cells, so no derived extent — but its header still
    // declares one. Dropping the declared span because the cells are missing
    // would lose the only bounds measurement such a record carries.
    let source = chain(CREATURE_GROUP, INSTANCE);
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    let bounds = entry.rigblock.value().expect("the record decoded");
    assert_eq!(bounds.span, None, "no cells, no derived extent");
    assert_eq!(
        bounds.declared_span, None,
        "this fixture's k == 0 header states zero on every axis, and a zero extent \
         has no magnitude to compare — reported as absent rather than as 0.0"
    );
}

#[test]
fn a_k_zero_rigblock_reports_no_envelope_rather_than_a_zero_one() {
    // A `k == 0` rigblock is a 36-byte header with no cells. Its envelope is
    // absent, not `0.0`, and `is_comparable` is therefore false: there is
    // nothing to compare and the catalogue does not pretend otherwise.
    let source = chain(CREATURE_GROUP, INSTANCE);
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    let bounds = entry.rigblock.value().expect("the record decoded");
    assert_eq!(bounds.levels, 0);
    assert_eq!(bounds.cells, 0);
    assert!(!bounds.has_cells());
    assert_eq!(bounds.span, None);
    assert_eq!(bounds.envelope, None);
    assert_eq!(entry.rigblock_span(), None);
    assert!(
        entry.geometry_extent().is_some(),
        "the geometry is unaffected"
    );
    assert!(
        !entry.extents.is_comparable(),
        "one side of the comparison does not exist"
    );
}

#[test]
fn the_names_reported_are_the_ones_in_use_and_never_the_dictionary() {
    // `hip` and `knee` are in the recovered vocabulary. Reporting them here
    // would be presenting a dictionary as anatomy; a part that does not use them
    // must not be credited with them.
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
        container_with_skeleton(
            &[
                (spore_rigblock::identity_of("joint1").unwrap().to_raw(), -1),
                (spore_rigblock::identity_of("knee").unwrap().to_raw(), 0),
                (0xDEAD_BEEF, 1),
            ],
            &[],
        ),
    );
    source.insert(
        ResourceKey::new(TYPE_GMSH, GROUP_PART, INSTANCE),
        gmsh_record(),
    );

    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    let skeleton = entry.skeleton.value().expect("body graph");
    assert_eq!(skeleton.bones, 3);
    assert_eq!(skeleton.named, 2);
    assert_eq!(
        skeleton.names,
        vec![("joint1", 0), ("knee", 1)],
        "one entry per recovered bone, in array order, and the unrecovered \
         identity contributes nothing"
    );
    assert_eq!(
        catalog.bone_name_histogram(),
        BTreeMap::from([("joint1", 1), ("knee", 1)]),
        "the histogram is over this catalogue's population, not the dictionary"
    );
    assert!(
        spore_rigblock::vocabulary::entries().len() > skeleton.names.len(),
        "the dictionary is much larger than what any part uses, which is why it \
         is not what the catalogue reports"
    );
}

#[test]
fn a_container_with_no_skeleton_is_a_finding_not_an_absence_of_the_part() {
    // 256 of 1824 real containers hold no skeleton section. The part resolves;
    // the skeleton is reported `not_present`, and the geometry is unaffected.
    let mut source = chain(CREATURE_GROUP, INSTANCE);
    let mut container = vec![0u8; 0x98];
    container[..8].copy_from_slice(b"\x89RW4w32\0");
    container[0x24..0x28].copy_from_slice(&0i32.to_le_bytes());
    source.insert(ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE), container);

    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    assert!(entry.is_authorable(), "the part still resolved");
    assert_eq!(entry.skeleton.value(), None);
    assert_eq!(
        entry.skeleton.absent(),
        Some(&Absent::NotPresent {
            key: ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
            what: "skeleton section",
        })
    );
    assert!(entry.geometry.is_found());
    assert_eq!(catalog.counts().absent_codes.get("not_present"), Some(&1));
}

#[test]
fn a_container_that_declares_nothing_is_found_and_empty_and_never_absent() {
    // The measured state of all 1824 real part containers: no validated
    // `{instance, type, group, 0x00A00CE4}` entry of any type. Reporting that as
    // an absence would make a complete measurement look like nobody looked.
    let source = chain(CREATURE_GROUP, INSTANCE);
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    let dependencies = entry.dependencies.value().expect("the scan ran");
    assert!(dependencies.is_empty(), "scanned and found nothing");
    assert_eq!(dependencies.declared.len(), 0);
    assert_eq!(dependencies.rasters().count(), 0);
    assert_eq!(dependencies.unresolved().count(), 0);
    assert_eq!(entry.dependencies.absent(), None);
    assert_eq!(entry.dependencies.level(), EvidenceLevel::Observed);
}

#[test]
fn a_declared_raster_reference_is_reported_as_a_real_resource_key() {
    // A container carrying a validated `{instance, type, group, 0x00A00CE4}`
    // entry that names a `0x2F4E681C` raster the source really holds.
    const RASTER_GROUP: u32 = 0x4063_2900;
    const RASTER_INSTANCE: u32 = 0x0000_00C1;
    const RASTER_KEY: ResourceKey =
        ResourceKey::new(catalog::TYPE_RASTER, RASTER_GROUP, RASTER_INSTANCE);

    let mut entry = Vec::new();
    entry.extend_from_slice(&RASTER_INSTANCE.to_le_bytes());
    entry.extend_from_slice(&catalog::TYPE_RASTER.to_le_bytes());
    entry.extend_from_slice(&RASTER_GROUP.to_le_bytes());
    entry.extend_from_slice(&0x00A0_0CE4u32.to_le_bytes());

    let mut source = chain(CREATURE_GROUP, INSTANCE);
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
        container_with_skeleton(
            &[(spore_rigblock::identity_of("joint1").unwrap().to_raw(), -1)],
            &entry,
        ),
    );
    source.insert(RASTER_KEY, vec![0u8; 4]);

    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    let dependencies = entry.dependencies.value().expect("the scan ran");
    assert!(!dependencies.is_empty());
    let rasters: Vec<ResourceKey> = dependencies.rasters().map(|d| d.key).collect();
    assert_eq!(
        rasters,
        vec![RASTER_KEY],
        "the real key, not a guessed role"
    );
    assert_eq!(
        dependencies.rasters().count(),
        dependencies.resolved().count()
    );
    assert_eq!(dependencies.unresolved().count(), 0);
}

#[test]
fn a_declared_reference_to_a_record_nobody_holds_is_reported_as_unresolved() {
    // Declared is a fact about the part; resolvable is a fact about the source.
    // Conflating them would make a missing texture file invisible.
    let mut declared = Vec::new();
    declared.extend_from_slice(&0x0000_00C2u32.to_le_bytes());
    declared.extend_from_slice(&catalog::TYPE_RASTER.to_le_bytes());
    declared.extend_from_slice(&0x4063_2900u32.to_le_bytes());
    declared.extend_from_slice(&0x00A0_0CE4u32.to_le_bytes());

    let mut source = chain(CREATURE_GROUP, INSTANCE);
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
        container_with_skeleton(
            &[(spore_rigblock::identity_of("joint1").unwrap().to_raw(), -1)],
            &declared,
        ),
    );

    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    let dependencies = entry.dependencies.value().expect("the scan ran");
    assert_eq!(dependencies.declared.len(), 1);
    assert!(!dependencies.declared[0].resolved);
    assert_eq!(dependencies.rasters().count(), 1);
    assert_eq!(dependencies.resolved().count(), 0);
    assert_eq!(dependencies.unresolved().count(), 1);
}

#[test]
fn an_unreadable_record_is_not_the_same_finding_as_a_missing_one() {
    // The distinction the brief is about, pinned at the value level: a miss, a
    // record that would not come out, and a source that could not look at all
    // are three different codes and three different `Absent` values.
    let source = MemorySource::new();
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");

    // A searchable source with nothing in it: looked, absent.
    assert_eq!(
        entry.rigblock.absent(),
        Some(&Absent::NoSuchRecord {
            key: ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE)
        })
    );
    // The chain stopped at the first hop, so the geometry was never looked for.
    // Reporting it absent would be a negative nobody measured.
    assert_eq!(
        entry.geometry.absent(),
        Some(&Absent::NotAttempted {
            stage: Stage::Rigblock
        })
    );
    assert_eq!(
        entry.geometry.absent().map(Absent::code),
        Some("not_attempted"),
        "and it is a different code from `no_such_record`"
    );

    let stop = entry.unresolved.as_ref().expect("the chain stopped");
    assert_eq!(stop.stage, Stage::Rigblock);
    assert_eq!(stop.reason, StopReason::NoSuchRecord);
    assert_eq!(
        stop.tried,
        Some(ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE)),
        "the key it tried is recorded, so 'we looked here' is answerable"
    );
    assert!(entry.trail.stopped_because() == Some(StopReason::NoSuchRecord));

    let counts = catalog.counts();
    assert_eq!(counts.unresolved, 1);
    assert_eq!(counts.with_geometry, 0);
    assert_eq!(
        counts.not_a_finding, 3,
        "geometry, skeleton and dependencies were never looked for"
    );
    assert_eq!(counts.absent_codes.get("no_such_record"), Some(&1));
    assert_eq!(counts.absent_codes.get("not_attempted"), None);
}

#[test]
fn a_source_that_could_not_look_at_all_says_so_instead_of_reporting_misses() {
    // A `MemorySource` is searchable by definition; a `PackageSource` with no
    // package open is not. Every non-finding must become `not_searched`, which is
    // a statement about the reader and never a finding about the corpus.
    let source = spore_morphology::PackageSource::new();
    assert!(!source.is_searchable());
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");

    assert_eq!(entry.rigblock.absent(), Some(&Absent::NotSearched));
    assert_eq!(entry.dependencies.absent(), Some(&Absent::NotSearched));
    assert_eq!(
        entry.dependencies.absent().map(Absent::code),
        Some("not_searched")
    );
    assert_eq!(
        entry.dependencies.absent().and_then(Absent::key),
        None,
        "there was no key to try: the source had nothing to try it with"
    );
    let counts = catalog.counts();
    assert_eq!(counts.not_a_finding, 4, "every group of the one entry");
    assert_eq!(
        counts.absent_codes.get("not_searched"),
        None,
        "a non-finding that describes the reader must not be counted as one"
    );
}

#[test]
fn a_present_but_unrecognised_record_is_wrong_structure_not_missing() {
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        vec![0u8; 4],
    );
    source.insert(
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
        vec![0xABu8; 64],
    );
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    assert_eq!(
        entry.rigblock.absent(),
        Some(&Absent::WrongStructure {
            key: ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE)
        })
    );
    let stop = entry.unresolved.as_ref().expect("stopped");
    assert_eq!(stop.stage, Stage::Rigblock);
    assert_eq!(stop.reason, StopReason::Undecodable);
    assert_eq!(
        catalog.counts().absent_codes.get("wrong_structure"),
        Some(&1)
    );
}

#[test]
fn the_catalog_agrees_with_the_resolver_on_every_entry() {
    // The catalogue must not be a second implementation of the chain. Driven over
    // a source with a hole in it at each stage, every entry must say the same
    // thing the resolver says and stop at the same stage with the same reason.
    let cases: [(MemorySource, Stage, StopReason); 3] = [
        (
            MemorySource::new(),
            Stage::Rigblock,
            StopReason::NoSuchRecord,
        ),
        (
            {
                let mut source = MemorySource::new();
                source.insert(
                    ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
                    rigblock_record(),
                );
                source
            },
            Stage::Prop,
            StopReason::NoSuchRecord,
        ),
        (
            {
                let mut source = MemorySource::new();
                source.insert(
                    ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
                    rigblock_record(),
                );
                source.insert(
                    ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
                    vec![0xABu8; 64],
                );
                source
            },
            Stage::Prop,
            StopReason::WrongStructure,
        ),
    ];

    for (source, stage, reason) in cases {
        let document = document(&[(CREATURE_GROUP, INSTANCE)]);
        let records = vec![DocumentRecord {
            key: DOCUMENT,
            family: Family::Creature,
            document: &document,
        }];
        let catalog = Catalog::build(&source, &records);
        let entry = catalog
            .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
            .expect("entry");

        let composition = spore_morphology::compose(&document, &source).expect("composes");
        let unresolved = composition.unresolved().next().expect("unresolved");
        let stop = entry.unresolved.as_ref().expect("the entry stopped too");
        assert_eq!(stop.stage, unresolved.stage, "same stage");
        assert_eq!(stop.reason, unresolved.reason, "same reason");
        assert_eq!(stop.stage, stage);
        assert_eq!(stop.reason, reason);
        assert_eq!(
            stop.tried,
            unresolved.trail.as_ref().and_then(|t| t.key_at(stage))
        );
        assert_eq!(
            entry.trail.hops(),
            unresolved.trail.as_ref().expect("trail").hops()
        );
        assert_eq!(entry.weakest_grade, None, "no chain to grade");
    }
}

#[test]
fn alternatives_offer_the_whole_reachable_set_and_claim_no_taxonomy() {
    let source = chain(CREATURE_GROUP, INSTANCE);
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[
                (CREATURE_GROUP, INSTANCE),
                (CELL_GROUP, INSTANCE),
                (CREATURE_GROUP, OTHER_INSTANCE),
            ]),
        )],
    );
    let alternatives: Vec<(u32, u32)> = catalog
        .alternatives(BlockId::new(CREATURE_GROUP, INSTANCE))
        .map(|entry| (entry.group_id(), entry.instance_id()))
        .collect();
    assert_eq!(
        alternatives,
        vec![(CELL_GROUP, INSTANCE), (CREATURE_GROUP, OTHER_INSTANCE)],
        "every other entry, in identity order, including a different group: the \
         group id is the document family's bucket and not an anatomy class, so \
         nothing here is grouped by it"
    );
    assert_eq!(
        catalog
            .alternatives(BlockId::new(CELL_GROUP, INSTANCE))
            .count(),
        2
    );
    assert_eq!(
        catalog
            .alternatives(BlockId::new(CREATURE_GROUP, OTHER_INSTANCE))
            .count(),
        2
    );
}

#[test]
fn an_empty_catalogue_is_a_statement_about_documents_not_about_spore() {
    let source = MemorySource::new();
    let catalog = catalogue(
        &source,
        &[(DOCUMENT, Family::Creature, CreatorDocument::default())],
    );
    assert!(catalog.is_empty());
    assert_eq!(catalog.len(), 0);
    assert_eq!(catalog.counts().entries, 0);
    assert_eq!(catalog.counts().references, 0);
    assert_eq!(
        catalog
            .alternatives(BlockId::new(CREATURE_GROUP, INSTANCE))
            .count(),
        0
    );
    assert_eq!(catalog.bone_name_histogram(), BTreeMap::new());
}

#[test]
fn an_unresolved_entry_reports_the_stop_and_never_an_authorable_verdict() {
    let mut source = chain(CREATURE_GROUP, INSTANCE);
    // Remove the geometry half: the container exists, the pair does not.
    source = {
        let mut kept = MemorySource::new();
        for key in [
            ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
            ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
            ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE),
        ] {
            kept.insert(key, source.record(&key).expect("present"));
        }
        kept
    };
    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    assert!(!entry.is_authorable());
    assert_eq!(entry.container_key, None);
    assert_eq!(entry.geometry_key, None);
    let stop = entry.unresolved.as_ref().expect("stopped");
    assert_eq!(stop.stage, Stage::PartPair);
    assert_eq!(stop.reason, StopReason::NoSuchRecord);
    assert_eq!(
        stop.tried,
        Some(ResourceKey::new(TYPE_RW4, GROUP_PART, INSTANCE)),
        "the container key is the one that was tried"
    );
    assert!(stop.summary().contains("part_pair"));
    assert!(entry.summary().contains("unresolved at part_pair"));
    // The rigblock did decode, so it is reported even though the part did not.
    assert!(entry.rigblock.is_found());
    assert_eq!(
        entry.geometry.absent(),
        Some(&Absent::NotAttempted {
            stage: Stage::PartPair
        })
    );
}

#[test]
fn an_inferred_hop_is_reported_and_the_counts_keep_it_visible() {
    // The prop names an instance that is not the block's own. The chain follows
    // it — that is the measured rule — and grades the hop `Inferred` because two
    // signals did not agree.
    const ELSEWHERE: u32 = 0x0003_0007;
    let mut source = MemorySource::new();
    source.insert(
        ResourceKey::new(RIGBLOCK, CREATURE_GROUP, INSTANCE),
        rigblock_record(),
    );
    source.insert(
        ResourceKey::new(TYPE_PROP, CREATURE_GROUP, INSTANCE),
        prop_record(ELSEWHERE),
    );
    source.insert(
        ResourceKey::new(TYPE_RW4, GROUP_PART, ELSEWHERE),
        container_with_skeleton(
            &[(spore_rigblock::identity_of("joint1").unwrap().to_raw(), -1)],
            &[],
        ),
    );
    source.insert(
        ResourceKey::new(TYPE_GMSH, GROUP_PART, ELSEWHERE),
        gmsh_record(),
    );

    let catalog = catalogue(
        &source,
        &[(
            DOCUMENT,
            Family::Creature,
            document(&[(CREATURE_GROUP, INSTANCE)]),
        )],
    );
    let entry = catalog
        .entry(BlockId::new(CREATURE_GROUP, INSTANCE))
        .expect("entry");
    assert!(entry.is_authorable());
    assert_eq!(entry.weakest_grade, Some(Grade::Inferred));
    assert_eq!(
        entry.container_key,
        Some(ResourceKey::new(TYPE_RW4, GROUP_PART, ELSEWHERE)),
        "the part is where the prop pointed, not where the block is"
    );
    let counts = catalog.counts();
    assert_eq!(counts.grades.get("inferred"), Some(&1));
    assert_eq!(counts.grades.get("measured"), None);
    assert!(entry.summary().contains("grade inferred"));
}

#[test]
fn absence_codes_are_a_closed_vocabulary_and_unique() {
    // A closed vocabulary is only closed if a caller can enumerate it. The
    // `Debug` spelling is the enumeration, and it is pinned so a new variant
    // cannot appear without a test noticing.
    let codes = [
        Absent::NotSearched.code(),
        Absent::NoSuchRecord {
            key: ResourceKey::new(1, 2, 3),
        }
        .code(),
        Absent::Unreadable {
            key: ResourceKey::new(1, 2, 3),
            compression: 4,
        }
        .code(),
        Absent::WrongStructure {
            key: ResourceKey::new(1, 2, 3),
        }
        .code(),
        Absent::NotPresent {
            key: ResourceKey::new(1, 2, 3),
            what: "skeleton section",
        }
        .code(),
        Absent::NotAttempted {
            stage: Stage::BlockId,
        }
        .code(),
    ];
    let total = codes.len();
    let mut sorted = codes.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), total, "two absences share a code");

    // Only the four that describe the data are findings about it.
    assert!(!Absent::NotSearched.is_finding());
    assert!(!Absent::NotAttempted {
        stage: Stage::BlockId
    }
    .is_finding());
    assert!(Absent::NoSuchRecord {
        key: ResourceKey::new(1, 2, 3)
    }
    .is_finding());
    assert!(Absent::Unreadable {
        key: ResourceKey::new(1, 2, 3),
        compression: 0
    }
    .is_finding());
}

#[test]
fn a_found_lookup_keeps_its_grade_and_an_absent_one_is_unknown() {
    let found: Lookup<u32> = Lookup::found(
        EvidenceLevel::Observed,
        vec![spore_core::Provenance::derived("tests")],
        7,
    );
    assert!(found.is_found());
    assert_eq!(found.level(), EvidenceLevel::Observed);
    assert_eq!(found.value(), Some(&7));
    assert_eq!(found.absent(), None);

    let absent: Lookup<u32> = Lookup::Absent(Absent::NotSearched);
    assert!(!absent.is_found());
    assert_eq!(
        absent.level(),
        EvidenceLevel::Unknown,
        "a non-finding is UNKNOWN, which is a real grade and not a default"
    );
    assert_eq!(absent.value(), None);
    assert_eq!(absent.absent(), Some(&Absent::NotSearched));
    assert_eq!(absent.into_value(), None);
}

#[test]
#[should_panic(expected = "must carry provenance")]
fn a_found_lookup_without_provenance_is_refused_at_construction() {
    let _ = Lookup::found(EvidenceLevel::Observed, Vec::new(), 1u8);
}

#[test]
fn the_claims_module_records_what_the_catalogue_does_not_know() {
    let claims = catalog::claims::all();
    let subjects: Vec<&str> = claims.iter().map(|claim| claim.subject).collect();
    for required in [
        catalog::claims::PART_DISPLAY_NAME,
        catalog::claims::MATERIAL_REGISTRY,
        catalog::claims::INLINE_RASTER_SECTION,
        catalog::claims::SUBSTITUTABILITY,
        catalog::claims::RIGBLOCK_LEVEL_SEMANTICS,
    ] {
        assert!(subjects.contains(&required), "{required} is not recorded");
    }
    let total = subjects.len();
    let mut sorted = subjects.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), total, "two claims share a subject");

    // Every non-finding must be UNKNOWN/MISSING with no provenance, and every
    // graded claim must carry one. `Fact`'s own invariants make that so, but the
    // point here is that this module keeps producing the *kind* of claim it is
    // supposed to: five non-findings and two measurements.
    let unavailable = claims
        .iter()
        .filter(|claim| claim.fact.is_unavailable())
        .count();
    assert_eq!(
        unavailable, 4,
        "the four things the catalogue does not know must all be recorded as \
         non-findings, and the two it did measure must not be"
    );
    for claim in &claims {
        if claim.fact.is_available() {
            assert!(
                !claim.fact.provenance().is_empty(),
                "{}: an available claim with no source",
                claim.subject
            );
        }
    }
}
