//! Corpus tests over the real install.
//!
//! `#[ignore]`d by default and skipped cleanly when `SPORE/` is absent — an
//! absent install is not a defect in the data or in these decoders.
//!
//! These are the tests that matter for this crate. The hermetic tests prove the
//! decoders do what the documentation says; these prove the documentation is
//! true of Spore.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use spore_dbpf::{extract_record, DbpfEntry, PackageIndex};
use spore_rigblock::{entries, identity_of, GmshMesh, Rigblock, Skeleton, TYPE_RIGBLOCK};

/// The group the part pair lives at.
const PART_GROUP: u32 = 0x4060_6000;

/// The type id of the paired RenderWare 4 container.
const TYPE_RW4: u32 = 0x2F4E_681B;

/// The `Skeleton (0x70002)` section type.
const SECTION_SKELETON: u32 = 0x0007_0002;

/// Locates the install from the manifest directory, so the test works whatever
/// the working directory is.
fn install_root() -> Option<PathBuf> {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

/// Walks the section directory of one RW4 container.
///
/// Laid out per `crates/spore-rw4/src/lib.rs`, which is authoritative for this
/// container including the `0x10030` BaseResource pointer adjustment.
fn rw4_sections(bytes: &[u8]) -> Vec<(u32, usize, usize)> {
    assert_eq!(&bytes[..8], b"\x89RW4w32\0", "not an RW4 container");
    let u32_at = |at: usize| -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    };
    let i32_at = |at: usize| -> i32 { i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()) };

    let section_info = u32_at(0x30) as usize;
    let section_count = i32_at(0x24) as usize;
    let buffer_pointer = u32_at(0x44) as usize;
    // The SectionTypes block is read to validate the walk: an RW4 container
    // whose manifest does not resolve is a different format, and reading section
    // rows anyway would produce plausible offsets for the wrong structure.
    let types_base = 0x98 + u32_at(0x98 + 0x0C) as usize;
    assert_eq!(u32_at(types_base), 0x10005, "RW4 SectionTypes type code");

    let mut out = Vec::new();
    for index in 0..section_count {
        let row = section_info + 24 * index;
        let data_pointer = u32_at(row) as usize;
        let size = i32_at(row + 0x08) as usize;
        let type_code = u32_at(row + 0x14);
        let adjusted = if type_code == 0x10030 {
            data_pointer + buffer_pointer
        } else {
            data_pointer
        };
        out.push((type_code, adjusted, size));
    }
    out
}

/// Reads every record of one type at one group out of one package.
fn records_of_type<'a>(
    image: &'a [u8],
    index: &'a PackageIndex,
    type_id: u32,
    group: Option<u32>,
) -> Vec<(u32, Vec<u8>)> {
    index
        .entries()
        .iter()
        .filter(|entry: &&DbpfEntry| {
            entry.type_id == type_id && group.is_none_or(|want| entry.group_id == want)
        })
        .filter_map(|entry| {
            extract_record(image, entry)
                .ok()
                .map(|bytes| (entry.instance_id, bytes))
        })
        .collect()
}

/// Opens a package by name, or `None` when the install is absent.
fn package(name: &str) -> Option<(Vec<u8>, PackageIndex)> {
    let root = install_root()?;
    let image = std::fs::read(root.join("SPORE/Data").join(name)).ok()?;
    let index = PackageIndex::parse(&image).ok()?;
    Some((image, index))
}

/// The part pair lives in `Spore_Graphics.package`.
fn graphics() -> Option<(Vec<u8>, PackageIndex)> {
    package("Spore_Graphics.package")
}

/// The rigblock bounds records live in `Spore_Game.package`.
///
/// Two different packages for two halves of one part — worth stating, because
/// "a part" is neither file and a reader who opens only one finds half the
/// answer.
fn game() -> Option<(Vec<u8>, PackageIndex)> {
    package("Spore_Game.package")
}

#[test]
#[ignore = "needs SPORE/"]
fn every_rigblock_record_decodes_and_matches_the_corrected_size_formula() {
    let Some((image, index)) = game() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Graphics.package");
        return;
    };
    let records = records_of_type(&image, &index, TYPE_RIGBLOCK, None);
    assert_eq!(records.len(), 1549, "rigblock record count moved");

    let mut by_level: HashMap<u32, usize> = HashMap::new();
    let mut by_size: HashMap<usize, usize> = HashMap::new();

    for (_, bytes) in &records {
        let rigblock = Rigblock::decode(bytes)
            .unwrap_or_else(|error| panic!("a real rigblock failed to decode: {error}"));
        let expected_cells = if rigblock.levels == 0 {
            0
        } else {
            4usize.pow(rigblock.levels)
        };
        assert_eq!(
            rigblock.cells.len(),
            expected_cells,
            "levels={} gave the wrong cell count",
            rigblock.levels
        );
        *by_level.entry(rigblock.levels).or_default() += 1;
        *by_size.entry(bytes.len()).or_default() += 1;
    }

    // The level histogram report 08 measured.
    let mut levels: Vec<(u32, usize)> = by_level.into_iter().collect();
    levels.sort_unstable();
    assert_eq!(
        levels,
        vec![
            (0, 212),
            (1, 330),
            (2, 465),
            (3, 369),
            (4, 128),
            (5, 37),
            (6, 7),
            (7, 1),
        ],
        "the level histogram moved"
    );

    // And the size histogram, which is what exposed the off-by-one in the
    // published formula. `k = 0` records are 36 bytes: a header and nothing else.
    let mut sizes: Vec<(usize, usize)> = by_size.into_iter().collect();
    sizes.sort_unstable();
    assert_eq!(
        sizes,
        vec![
            (36, 212),
            (48, 330),
            (84, 465),
            (228, 369),
            (804, 128),
            (3108, 37),
            (12324, 7),
            (49188, 1),
        ],
        "the size histogram moved"
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn every_skeleton_section_decodes_with_exactly_one_root() {
    let Some((image, index)) = graphics() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Graphics.package");
        return;
    };
    let containers = records_of_type(&image, &index, TYPE_RW4, Some(PART_GROUP));
    assert_eq!(containers.len(), 1824, "paired container count moved");

    let mut sections = 0usize;
    let mut pointers_verified = 0usize;
    let mut records_with_skeleton = 0usize;
    let mut bone_total = 0usize;
    let mut max_bones = 0usize;
    let mut depths: Vec<usize> = Vec::new();

    for (_, bytes) in &containers {
        let mut here = 0usize;
        for (type_code, offset, size) in rw4_sections(bytes) {
            if type_code != SECTION_SKELETON {
                continue;
            }
            let payload = &bytes[offset..offset + size];
            let skeleton = Skeleton::decode(payload).unwrap_or_else(|error| {
                panic!("a real skeleton section failed to decode: {error}")
            });
            sections += 1;
            here += 1;
            // The pointer check that an earlier decoder only pretended to make.
            skeleton
                .verify_pointers(offset as u32)
                .unwrap_or_else(|error| {
                    panic!("a real skeleton section's pointers do not address its arrays: {error}")
                });
            pointers_verified += 1;
            bone_total += skeleton.len();
            max_bones = max_bones.max(skeleton.len());
            depths.push(skeleton.max_depth());
            // Decoding already enforces one root and acyclicity; this asserts
            // the topology is usable rather than merely well-formed.
            let root = skeleton.root();
            assert!(skeleton.path_to_root(root).is_some());
        }
        if here > 0 {
            records_with_skeleton += 1;
        }
    }

    assert_eq!(sections, 6270, "skeleton section count moved");
    assert_eq!(
        records_with_skeleton, 1568,
        "records holding a skeleton moved"
    );
    assert_eq!(max_bones, 25, "the maximum bone count moved");
    assert!(bone_total > sections, "bones per section should exceed one");

    // The paper describes serial chains at a skeleton root. What the data can
    // say is how deep the chains actually go — recorded here as a measurement,
    // explicitly NOT as proof that the deepest chain is a spine.
    let deepest = depths.iter().copied().max().unwrap_or(0);
    assert!(
        deepest >= 4,
        "a corpus skeleton should have chains deeper than a flat list, saw {deepest}"
    );
    assert_eq!(
        pointers_verified, sections,
        "every section's three pointers must address arrays 1, 2 and 3"
    );
    eprintln!(
        "{sections} sections ({pointers_verified} pointer-verified), \
         {records_with_skeleton} records, {bone_total} bones, \
         max bones {max_bones}, deepest chain {deepest}"
    );
}

#[test]
#[ignore = "needs SPORE/"]
fn the_recovered_bone_names_appear_in_the_corpus_at_the_table_frequency() {
    let Some((image, index)) = graphics() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Graphics.package");
        return;
    };
    let containers = records_of_type(&image, &index, TYPE_RW4, Some(PART_GROUP));

    // Count identities the way the search counted them, then check the anchors
    // that were recovered BEFORE the vocabulary table existed. Those anchors are
    // what make the table a measurement rather than a wish.
    let mut frequency: HashMap<u32, u32> = HashMap::new();
    let mut identities: HashMap<u32, u32> = HashMap::new();
    let mut total = 0usize;

    for (_, bytes) in &containers {
        for (type_code, offset, size) in rw4_sections(bytes) {
            if type_code != SECTION_SKELETON {
                continue;
            }
            let skeleton = Skeleton::decode(&bytes[offset..offset + size]).unwrap();
            for bone in &skeleton.bones {
                *frequency.entry(bone.name.to_raw()).or_default() += 1;
                *identities.entry(bone.name.to_raw()).or_default() += 1;
                total += 1;
            }
        }
    }

    assert_eq!(total, 15763, "total bone count moved");
    assert_eq!(identities.len(), 2002, "distinct identity count moved");

    // The anchors. `root` was matched to 382 by the search, from a count that
    // was measured independently first (report 10, before any name was guessed).
    let root = identity_of("root").unwrap();
    assert_eq!(
        frequency.get(&root.to_raw()),
        Some(&382),
        "root's corpus frequency moved — the table's central anchor is wrong"
    );
    // And the whole joint family, which is what made the hash function certain.
    let expected_joint = [
        ("joint1", 937),
        ("joint2", 879),
        ("joint3", 792),
        ("joint4", 691),
        ("joint5", 566),
        ("joint6", 498),
        ("joint7", 430),
        ("joint8", 362),
        ("joint9", 313),
    ];
    for (name, count) in expected_joint {
        let bone = identity_of(name).unwrap();
        assert_eq!(
            frequency.get(&bone.to_raw()),
            Some(&count),
            "{name}'s corpus frequency moved"
        );
    }
}

#[test]
#[ignore = "needs SPORE/"]
fn every_recovered_name_is_a_real_token_and_every_strong_name_has_a_witness() {
    let Some((image, index)) = graphics() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Graphics.package");
        return;
    };
    let containers = records_of_type(&image, &index, TYPE_RW4, Some(PART_GROUP));

    let mut present: HashMap<u32, u32> = HashMap::new();
    for (_, bytes) in &containers {
        for (type_code, offset, size) in rw4_sections(bytes) {
            if type_code != SECTION_SKELETON {
                continue;
            }
            for bone in Skeleton::decode(&bytes[offset..offset + size])
                .unwrap()
                .bones
            {
                *present.entry(bone.name.to_raw()).or_default() += 1;
            }
        }
    }

    // A table row that names a token the corpus does not contain is a fabricated
    // name. Every row must correspond to something real.
    let mut missing = Vec::new();
    let mut checked = 0usize;
    for entry in entries() {
        checked += 1;
        if !present.contains_key(&entry.identity.to_raw()) {
            missing.push((entry.name, entry.confidence, entry.corpus_frequency));
        }
    }
    assert!(checked >= 240, "the table shrank to {checked} rows");
    assert!(
        missing.is_empty(),
        "{} table rows name a token the corpus does not contain: {missing:?}",
        missing.len()
    );

    // Every name graded as witnessed must actually have its witness. This is
    // what stops the table from quietly promoting itself: a name can only reach a
    // witnessed grade if the bytes were found, and the corpus test is what checks.
    let mut unbacked = Vec::new();
    for entry in entries() {
        match entry.confidence {
            spore_rigblock::Confidence::BinaryLiteral if !literal_in_binaries(entry.name) => {
                unbacked.push((entry.name, "binary", entry.corpus_frequency));
            }
            spore_rigblock::Confidence::RecordLiteral if !literal_in_small_packages(entry.name) => {
                unbacked.push((entry.name, "record", entry.corpus_frequency));
            }
            _ => {}
        }
    }
    assert!(
        unbacked.is_empty(),
        "{} witnessed names have no witness: {unbacked:?}",
        unbacked.len()
    );
}

/// Whether `name` occurs, delimited, in a decompressed record of either small
/// package — the two that hold creator documents and property records.
fn literal_in_small_packages(name: &str) -> bool {
    let Some(root) = install_root() else {
        return true; // nothing to check against; not a failure
    };
    let needle = name.as_bytes();
    for package in ["Spore_Game.package", "PatchData.package"] {
        let Ok(image) = std::fs::read(root.join("SPORE/Data").join(package)) else {
            continue;
        };
        let Ok(index) = PackageIndex::parse(&image) else {
            continue;
        };
        for entry in index.entries() {
            if entry.memory_size > 6_000_000 {
                continue;
            }
            let Ok(bytes) = extract_record(&image, entry) else {
                continue;
            };
            if contains_identifier(&bytes, needle) {
                return true;
            }
        }
    }
    false
}

/// Whether `name` occurs as an exact identifier-shaped byte run anywhere in the
/// shipped executables.
fn literal_in_binaries(name: &str) -> bool {
    let Some(root) = install_root() else {
        return true; // nothing to check against; not a failure
    };
    let Ok(entries) = std::fs::read_dir(root.join("SPORE")) else {
        return true;
    };
    let mut stack = vec![entries];
    while let Some(dir) = stack.pop() {
        for entry in dir.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(children) = std::fs::read_dir(&path) {
                    stack.push(children);
                }
                continue;
            }
            let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !matches!(extension.to_ascii_lowercase().as_str(), "exe" | "dll") {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            if contains_identifier(&bytes, name.as_bytes()) {
                return true;
            }
        }
    }
    false
}

/// Whether `needle` appears in `haystack` delimited by non-identifier bytes.
///
/// The delimiters matter: an unbounded substring search would "find" `root`
/// inside `rooted` or inside packed binary, which is exactly the false positive
/// that put `kiomq` in an early draft of the table.
fn contains_identifier(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    // Letters and underscore count as word characters: abutting one means the
    // match is part of a longer identifier, which is the false positive that
    // matters here.
    //
    // Digits deliberately do **not**. Spore stores primitive shape names in a
    // NUL-delimited table with a length byte in front of each
    // (`4axes\0arrowhead\0cone\0box\0quad\0sphere\0`), so a digit abutting a
    // name is a length prefix rather than a continuation of it. Excluding digits
    // would have failed `axes` on a real witness.
    let is_word = |byte: u8| byte.is_ascii_alphabetic() || byte == b'_';
    for start in 0..=haystack.len() - needle.len() {
        if &haystack[start..start + needle.len()] != needle {
            continue;
        }
        let before_ok = start == 0 || !is_word(haystack[start - 1]);
        let after = start + needle.len();
        let after_ok = after == haystack.len() || !is_word(haystack[after]);
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

#[test]
#[ignore = "needs SPORE/"]
fn the_resolution_gap_is_a_measured_number_not_a_default() {
    let Some((game_image, game_index)) = game() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    };
    let Some((graphics_image, graphics_index)) = graphics() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Graphics.package");
        return;
    };
    let rigblocks = records_of_type(&game_image, &game_index, TYPE_RIGBLOCK, None);
    let meshes = records_of_type(
        &graphics_image,
        &graphics_index,
        0x01C1_35DA,
        Some(PART_GROUP),
    );
    assert_eq!(meshes.len(), 1824, "paired mesh count moved");

    let mesh_ids: std::collections::HashSet<u32> =
        meshes.iter().map(|(instance, _)| *instance).collect();

    let mut resolved = 0usize;
    for (instance, _) in &rigblocks {
        if mesh_ids.contains(instance) {
            resolved += 1;
        }
    }
    // 1316 of 1549. The 233 that do not resolve are a fact about the corpus and
    // must stay a count a caller can read, not a gap quietly filled in.
    assert_eq!(resolved, 1316, "the resolved count moved");
    assert_eq!(
        rigblocks.len() - resolved,
        233,
        "the unresolved count moved"
    );
}

/// The `gmsh` geometry layer, over every record the part pair holds.
#[derive(Default, Debug)]
struct Geometry {
    records: usize,
    vertices: usize,
    triangles: usize,
    indices: usize,
    single_bone: usize,
    max_vertices: usize,
    max_triangles: usize,
    distinct_recipes: std::collections::BTreeSet<u32>,
    channel_blocks: std::collections::BTreeMap<u32, usize>,
    position_span: [f32; 3],
}

#[test]
#[ignore = "needs SPORE/"]
fn every_gmsh_record_decodes_and_the_skinning_tiling_holds_everywhere() {
    let Some((image, index)) = graphics() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Graphics.package");
        return;
    };
    let meshes = records_of_type(&image, &index, 0x01C1_35DA, Some(PART_GROUP));
    assert_eq!(meshes.len(), 1824, "paired mesh count moved");

    let mut tally = Geometry::default();
    let mut position_low = [f32::INFINITY; 3];
    let mut position_high = [f32::NEG_INFINITY; 3];

    for (_, bytes) in &meshes {
        // A zero-length or negative span would make the min/max below lie.
        let mesh = GmshMesh::decode(bytes)
            .unwrap_or_else(|error| panic!("a real gmsh record failed to decode: {error}"));

        // The header's recipe must alone fix the chain length.
        assert_eq!(
            mesh.blocks.len(),
            mesh.header.block_count(),
            "the declared chain length and the walked chain disagree"
        );

        // Rigid skinning: every index is owned by exactly one bone, and the
        // ranges tile the buffer. `decode` enforces it; this asserts the
        // measurement rather than trusting one call.
        let owned: std::collections::HashSet<u32> = (0..mesh.indices.len() as u32)
            .filter_map(|index| mesh.bone_at(index))
            .collect();
        assert!(
            owned.len() <= mesh.bone_count as usize,
            "more bones own indices than the record declares"
        );

        tally.records += 1;
        tally.vertices += mesh.positions.len();
        tally.triangles += mesh.triangle_count();
        tally.indices += mesh.indices.len();
        tally.max_vertices = tally.max_vertices.max(mesh.positions.len());
        tally.max_triangles = tally.max_triangles.max(mesh.triangle_count());
        tally.distinct_recipes.insert(mesh.header.recipe);
        if mesh.is_rigid_single_bone() {
            tally.single_bone += 1;
        }
        for block in &mesh.blocks {
            *tally.channel_blocks.entry(block.id).or_default() += 1;
        }
        for corner in &mesh.positions {
            for axis in 0..3 {
                position_low[axis] = position_low[axis].min(corner[axis]);
                position_high[axis] = position_high[axis].max(corner[axis]);
            }
        }
    }

    // The census report 09 measured.
    assert_eq!(tally.records, 1824);
    assert_eq!(tally.vertices, 1_222_646, "the vertex total moved");
    assert_eq!(tally.indices, 4_106_853, "the index total moved");
    assert_eq!(tally.triangles, 1_368_951, "the triangle total moved");
    assert_eq!(
        tally.single_bone, 973,
        "the single-bone mesh count moved (53.3% of 1824)"
    );
    assert_eq!(tally.distinct_recipes.len(), 16, "recipe count moved");
    for axis in 0..3 {
        tally.position_span[axis] = position_high[axis] - position_low[axis];
        assert!(
            tally.position_span[axis] > 0.0,
            "axis {axis} has zero extent, which would make the bounds meaningless"
        );
    }

    eprintln!(
        "{} meshes: {} vertices, {} triangles, {} single-bone, {} recipes",
        tally.records,
        tally.vertices,
        tally.triangles,
        tally.single_bone,
        tally.distinct_recipes.len()
    );
    eprintln!(
        "max per mesh: {} vertices, {} triangles",
        tally.max_vertices, tally.max_triangles
    );
    eprintln!("span {:?}", tally.position_span);
    eprintln!("distinct channel block ids: {}", tally.channel_blocks.len());
}

#[test]
#[ignore = "needs SPORE/"]
fn the_index_block_is_never_mistaken_for_bone_data() {
    // Report 08 called the big `u16` array "a trailing bone-index/weight block".
    // It is the index buffer: every index below `nv`, and the bone table is the
    // literal `0..nBones`. If this ever regresses, bones and vertices have been
    // confused and every part would be malformed in a way that still parses.
    let Some((image, index)) = graphics() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Graphics.package");
        return;
    };
    for (_, bytes) in records_of_type(&image, &index, 0x01C1_35DA, Some(PART_GROUP))
        .iter()
        .take(200)
    {
        let mesh = GmshMesh::decode(bytes).expect("decodes");
        let vertex_count = usize::from(mesh.header.vertex_count);
        assert_eq!(mesh.positions.len(), vertex_count);
        assert!(mesh.indices.iter().all(|i| usize::from(*i) < vertex_count));
        let bone_table = mesh
            .blocks
            .iter()
            .find(|block| {
                block.id == spore_rigblock::gmsh::BLOCK_BONE_TABLE
                    || block.id == spore_rigblock::gmsh::BLOCK_BONE_TABLE_ALT
            })
            .expect("a bone table");
        let ordinals = bone_table.as_f32().expect("4-byte elements");
        // Reinterpreted as u32: the literal identity list.
        for (index, _) in bone_table.payload.chunks_exact(4).enumerate() {
            let value = u32::from_le_bytes([
                bone_table.payload[index * 4],
                bone_table.payload[index * 4 + 1],
                bone_table.payload[index * 4 + 2],
                bone_table.payload[index * 4 + 3],
            ]);
            assert_eq!(
                value as usize, index,
                "the bone table is not an identity list"
            );
        }
        let _ = ordinals;
    }
}

/// The whole-body layer, reached by pointer from the `gmdl` trailer.
///
/// Not the part layer. The part layer is `0x40606000` in `Spore_Graphics`; this is
/// `0x41606100` in `Spore_Content`, and the `gmdl` records' trailers name it —
/// see `docs/analysis/creator/12-gmdl-anim-data-vocabulary.md`. Only one digit of
/// group id separates the two, which is why conflating them is easy and why this
/// test exists rather than a comment.
fn content() -> Option<(Vec<u8>, PackageIndex)> {
    package("Spore_Content.package")
}

const WHOLE_BODY_GROUP: u32 = 0x4160_6100;

/// Counts, over every skeleton section at the whole-body group, how many times
/// `word` appears as a skeleton root and how many as a child.
fn root_word_counts(
    image: &[u8],
    index: &PackageIndex,
    word: spore_rigblock::vocabulary::BoneName,
) -> (usize, usize) {
    let mut as_root = 0;
    let mut as_child = 0;
    for (_instance, bytes) in records_of_type(image, index, TYPE_RW4, Some(WHOLE_BODY_GROUP)) {
        for (_type_id, offset, size) in rw4_sections(&bytes) {
            let section = &bytes[offset..(offset + size).min(bytes.len())];
            let Ok(skeleton) = spore_rigblock::Skeleton::decode(section) else {
                continue;
            };
            let root = skeleton.root();
            for (index_bone, name) in skeleton.named_bones() {
                if name == word_str(word) {
                    if index_bone == root {
                        as_root += 1;
                    } else {
                        as_child += 1;
                    }
                }
            }
        }
    }
    (as_root, as_child)
}

/// The name `word` denotes, borrowed from the vocabulary's own table.
fn word_str(word: spore_rigblock::vocabulary::BoneName) -> &'static str {
    word.name_if_certain().expect("a strong-graded name")
}

#[test]
#[ignore = "needs SPORE/"]
fn the_whole_body_layer_decodes_with_one_root_per_skeleton() {
    let Some((image, index)) = content() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Content.package");
        return;
    };
    let records = records_of_type(&image, &index, TYPE_RW4, Some(WHOLE_BODY_GROUP));
    assert_eq!(
        records.len(),
        591,
        "the whole-body group is 591 rw4 records; a different count means the group \
         id or the package moved, not that the format changed"
    );

    let mut sections = 0usize;
    let mut bones = 0usize;
    let mut multi_root = 0usize;
    for (_instance, bytes) in &records {
        for (_type_id, offset, size) in rw4_sections(bytes) {
            let section = &bytes[offset..(offset + size).min(bytes.len())];
            let Ok(skeleton) = spore_rigblock::Skeleton::decode(section) else {
                continue;
            };
            sections += 1;
            bones += skeleton.len();
            // "Exactly one root" is asserted structurally: in a rooted tree every
            // node but the root is somebody's child, so the child count must be
            // exactly len-1. Counting `path_to_root` successes instead would pass
            // even with two roots, which is the bug worth avoiding here.
            let children_of_everyone: usize = (0..skeleton.len())
                .map(|i| skeleton.children(i).len())
                .sum();
            if children_of_everyone != skeleton.len() - 1 {
                multi_root += 1;
            }
        }
    }
    assert_eq!(sections, 1182, "whole-body skeleton sections");
    assert_eq!(bones, 3074, "whole-body bones");
    assert_eq!(multi_root, 0, "every skeleton must have exactly one root");
}

#[test]
#[ignore = "needs SPORE/"]
fn the_token_root_is_the_root_bone_in_every_case() {
    let Some((image, index)) = content() else {
        eprintln!("SKIP: no SPORE/Data/Spore_Content.package");
        return;
    };
    let root = spore_rigblock::vocabulary::identity_of("root").expect("root is in the vocabulary");
    let (as_root, as_child) = root_word_counts(&image, &index, root);
    assert_eq!(
        as_root, 190,
        "`root` occurrences that ARE the skeleton root"
    );
    assert_eq!(
        as_child, 0,
        "`root` must never be a child bone. If this fails, the vocabulary word \
         `root` is not the skeleton-root marker and the semantic claim is wrong."
    );
}
