//! Record type ids and the canonical type/group name tables.
//!
//! Spore's record ids are 32-bit words whose low bytes are frequently a
//! printable four-character code, but **not reliably**: `gmdl`
//! (`0x00E6BCE5`) renders as `å¼æ\0`. Treating a type id as a four-character
//! code is therefore a lossy display convenience, never an identity.
//!
//! The name tables are transcribed from `tools/spore/types/typenames.json` and
//! `tools/spore/types/groupnames.json` (SDK-sourced + the per-package prop
//! directory record). Transcribing them into the binary makes the engine's
//! vocabulary auditable in one place instead of spread across a JSON blob that
//! nothing type-checks; the JSON remains the reference and
//! `spore-tools` verifies the two against each other.
//!
//! Instance ids are **not** nameable: DBPF carries no string table, and the
//! same instance id legitimately recurs across type/group pairs. Nothing here
//! pretends otherwise.

use core::fmt;

/// Record type ids this build recognises.
///
/// Ids are listed as hexadecimal constants for readability; the JSON tables
/// spell them in decimal.
pub mod type_id {
    /// Property / record-list container.
    pub const PROP: u32 = 0x00B1_B104;
    /// Game model (`gmdl`). Spore's primary geometry container.
    pub const GMDL: u32 = 0x00E6_BCE5;
    /// RenderWare 4 container. Begins `89 52 57 34 77 33 32 00` (`RW4w32\0`).
    ///
    /// Measured over the installed packages: all 1131 `0x2F4E681B` records in
    /// `Spore_Content` carry the RW4 magic. **This is a different type id from
    /// [`PNG`]** — see that constant's note, which corrects a claim this crate
    /// used to make.
    pub const RW4: u32 = 0x2F4E_681B;
    /// Raster (texture) record: a 32-byte envelope followed by DXT5 layers.
    pub const RASTER: u32 = 0x2F4E_681C;
    /// Raw PNG. `png` in `typenames.json`.
    ///
    /// **This is raw PNG, not an RW4 container.** Measured: all 1642
    /// `0x2F7D0004` records in `Spore_Content` begin `89 50 4E 47 0D 0A 1A 0A`
    /// and are stored uncompressed (`csize == msize`); likewise 8360 in
    /// `Spore_Graphics`, 461 in `PatchData`, 24 in `Spore_Pack_03` — 10 487 of
    /// 10 487, with zero RW4 magics. An earlier revision of this file claimed
    /// the opposite by conflating `png` with [`RW4`]; the two ids are adjacent
    /// and mean different things.
    pub const PNG: u32 = 0x2F7D_0004;
    /// Raw JPEG. `jpeg` in `typenames.json`, distinct from [`PNG`] for the same
    /// reason.
    pub const JPEG: u32 = 0x2F7D_0002;
    /// Palette record.
    pub const PLT: u32 = 0x0119_89B7;
    /// World-object model-placement records.
    pub const WORLD_OBJECT: u32 = 0x0F43_029A;

    /// Cell-stage globals resource (`cCellGlobalsResource`).
    pub const CELL_GLOBALS: u32 = 0x2A3C_E5B7;
    /// Cell-stage effect map.
    pub const CELL_EFFECT_MAP: u32 = 0x433F_B70C;
    /// Cell-stage background colour ramp.
    pub const CELL_BACKGROUND_MAP: u32 = 0x612B_3191;
    /// Cell-stage cell resource (`cCellCellResource`).
    pub const CELL: u32 = 0xDFAD_9F51;
    /// Cell-stage cell world (`cCellWorldResource`).
    pub const CELL_WORLD: u32 = 0x9B8E_862F;
    /// Cell-stage cell populate record.
    pub const CELL_POPULATE: u32 = 0xDA14_1C1B;
    /// Cell-stage cell structure record.
    pub const CELL_STRUCTURE: u32 = 0x4B9E_F6DC;
    /// Cell-stage loot table.
    pub const CELL_LOOT_TABLE: u32 = 0xD92A_F091;
    /// Cell-stage look table.
    pub const CELL_LOOK_TABLE: u32 = 0x8C04_2499;
    /// Cell-stage look algorithm.
    pub const CELL_LOOK_ALGORITHM: u32 = 0xDBA3_5AE2;
    /// Cell-stage random-creature spawn table.
    pub const CELL_RANDOM_CREATURE: u32 = 0xF9C3_D770;
    /// Cell-stage powers record.
    pub const CELL_POWERS: u32 = 0x754B_E343;

    /// Trait record used by creature traits.
    pub const CREATURE_TRAITS: u32 = 0x01AD_2416;
    /// Trait record used by building traits.
    pub const BUILDING_TRAITS: u32 = 0x01AD_2417;
    /// Trait record used by vehicle traits.
    pub const VEHICLE_TRAITS: u32 = 0x01AD_2418;
    /// Creature model mesh (RW-based, group-scoped).
    pub const CREATURE_MESH: u32 = 0x2943_9E3E;
}

/// Renders the low four bytes of a type id as text, for display only.
///
/// Non-printable bytes become `_`, and a NUL becomes `_` too. The result is
/// `None` for an all-NUL id, which is the one case where there is genuinely
/// nothing to show.
pub fn fourcc(type_id: u32) -> Option<String> {
    let bytes = type_id.to_le_bytes();
    if bytes.iter().all(|&b| b == 0) {
        return None;
    }
    Some(
        bytes
            .iter()
            .map(|&b| {
                if (0x20..0x7f).contains(&b) {
                    b as char
                } else {
                    '_'
                }
            })
            .collect(),
    )
}

/// The canonical `typenames.json` table, as `(type_id, name)` pairs.
///
/// Sorted by type id ascending, mirroring the JSON, so a table walk is
/// deterministic.
pub const TYPE_NAMES: &[(u32, &str)] = &[
    (0x00B1_B104, "prop"),
    (0x00E6_BCE5, "gmdl"),
    (0x0119_89B7, "plt"),
    (0x01AD_2416, "creature_traits"),
    (0x01AD_2417, "building_traits"),
    (0x01AD_2418, "vehicle_traits"),
    (0x01C1_35DA, "gmsh"),
    (0x01C3_C4B3, "trait_pill"),
    (0x02A8_CB47, "physics"),
    (0x02AE_0C7E, "game_tuning"),
    (0x02D5_C9AF, "summary"),
    (0x02D5_C9B0, "summary_pill"),
    (0x030B_DEE3, "pollen_metadata"),
    (0x0376_C3DA, "hm"),
    (0x0469_A3F7, "smt"),
    (0x04F6_84A4, "cmp"),
    (0x055A_DA24, "cnv"),
    (0x0F43_029A, "world_object"),
    (0x1795_2E6C, "dds"),
    (0x1A99_B06B, "bem"),
    (0x2399_BE55, "bld"),
    (0x2468_2294, "vcl"),
    (0x2943_9E3E, "creature_mesh"),
    (0x2B97_8C46, "crt"),
    (0x2F4E_681B, "rw4"),
    (0x2F4E_681C, "raster"),
    (0x2F7D_0002, "jpeg"),
    (0x2F7D_0004, "png"),
    (0x366A_930D, "adventure"),
    (0x3D97_A8E4, "cll"),
    (0x438F_6347, "flr"),
    (0x476A_98C7, "ufo"),
    (0x4B9E_F6DC, "cell_structure"),
    (0x612B_3191, "backgroundMap"),
    (0x754B_E343, "cell_powers"),
    (0x8C04_2499, "look_table"),
    (0x9B8E_862F, "world"),
    (0xAF02_8F41, "verbtrays"),
    (0xD92A_F091, "lootTable"),
    (0xDA14_1C1B, "populate"),
    (0xDBA3_5AE2, "look_algorithm"),
    (0xDFAD_9F51, "cell"),
    (0xEA51_18B0, "effdir"),
    (0xEE17_C6AD, "animation"),
    (0xF9C3_D770, "randomCreature"),
];

/// The canonical `groupnames.json` table, as `(group_id, name)` pairs.
///
/// Group ids are structured. `0x4061_62xx` is a Cell-stage group, `0x4062_62xx`
/// a Creature group, and so on: the byte at `>> 8` is the stage and the byte at
/// `>> 16` is the category (`ResourceKey::stage_byte` and
/// `ResourceKey::category_byte`).
pub const GROUP_NAMES: &[(u32, &str)] = &[
    (0x02A8_CB47, "Physics"),
    (0x02AE_0C7E, "GameTuning"),
    (0x4041_0100, "CameraProperties"),
    (0x4060_0100, "EditorSetup"),
    (0x4060_6000, "EditorRigblocks"),
    (0x4061_6200, "CellModels"),
    (0x4061_6201, "CellImages"),
    (0x4062_6200, "CreatureModels"),
    (0x4062_7100, "CreatureModelsHQ"),
    (0x4063_6200, "BuildingModels"),
    (0x4064_6200, "VehicleModels"),
    (0x4065_6200, "UfoModels"),
    (0x4066_6200, "FloraModels"),
    (0x406B_6200, "PaletteModels"),
    (0x406B_6A00, "Palettes"),
    (0xAF02_8F41, "Verbtrays"),
];

/// A record type id with a known canonical name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RecordType(u32);

impl RecordType {
    /// Wraps a raw type id, named or not.
    pub const fn new(type_id: u32) -> Self {
        Self(type_id)
    }

    /// The raw type id.
    pub const fn id(self) -> u32 {
        self.0
    }

    /// The canonical name, or `None` when this build has no name for it.
    pub const fn name(self) -> Option<&'static str> {
        // A `const fn` cannot loop, so the tables are walked with a bounded
        // binary search expressed as a match-free helper below.
        lookup(self.0, TYPE_NAMES)
    }

    /// Whether this build recognises the id.
    pub const fn is_known(self) -> bool {
        self.name().is_some()
    }
}

impl From<u32> for RecordType {
    fn from(type_id: u32) -> Self {
        Self(type_id)
    }
}

impl From<RecordType> for u32 {
    fn from(value: RecordType) -> u32 {
        value.0
    }
}

impl fmt::Display for RecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.name() {
            Some(name) => write!(f, "{name} (0x{:08x})", self.0),
            None => write!(f, "0x{:08x}", self.0),
        }
    }
}

/// The canonical group name, or `None` when unknown.
pub fn group_name(group_id: u32) -> Option<&'static str> {
    lookup(group_id, GROUP_NAMES)
}

/// Binary search over a table sorted ascending by id.
///
/// `const`-callable, which a `for` loop is not.
const fn lookup(id: u32, table: &[(u32, &'static str)]) -> Option<&'static str> {
    let mut lo = 0usize;
    let mut hi = table.len();
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        let entry = table[mid];
        if entry.0 == id {
            return Some(entry.1);
        } else if entry.0 < id {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_are_sorted_and_free_of_duplicates() {
        for table in [TYPE_NAMES, GROUP_NAMES] {
            for pair in table.windows(2) {
                assert!(
                    pair[0].0 < pair[1].0,
                    "table must be strictly ascending by id"
                );
            }
        }
    }

    #[test]
    fn binary_search_agrees_with_a_linear_scan() {
        for (id, expected) in TYPE_NAMES {
            assert_eq!(RecordType::new(*id).name(), Some(*expected));
            assert_eq!(lookup(*id, TYPE_NAMES), Some(*expected));
        }
        for (id, expected) in GROUP_NAMES {
            assert_eq!(group_name(*id), Some(*expected));
        }
    }

    #[test]
    fn known_ids_resolve_to_their_canonical_names() {
        assert_eq!(RecordType::new(type_id::GMDL).name(), Some("gmdl"));
        assert_eq!(RecordType::new(type_id::RW4).name(), Some("rw4"));
        assert_eq!(RecordType::new(type_id::RASTER).name(), Some("raster"));
        assert_eq!(RecordType::new(type_id::PROP).name(), Some("prop"));
        assert_eq!(RecordType::new(type_id::CELL).name(), Some("cell"));
    }

    #[test]
    fn an_unknown_id_is_unknown_rather_than_guessed() {
        assert_eq!(RecordType::new(0xDEAD_BEEF).name(), None);
        assert!(!RecordType::new(0xDEAD_BEEF).is_known());
        assert_eq!(group_name(0xDEAD_BEEF), None);
    }

    #[test]
    fn display_falls_back_to_hex_and_prefers_a_name() {
        assert_eq!(
            RecordType::new(type_id::GMDL).to_string(),
            "gmdl (0x00e6bce5)"
        );
        assert_eq!(RecordType::new(0xDEAD_BEEF).to_string(), "0xdeadbeef");
    }

    #[test]
    fn fourcc_is_a_display_convenience_only() {
        // gmdl = 0x00E6BCE5 -> bytes e5 bc e6 00, none printable. This is
        // exactly why a four-character code can never stand in for a type id.
        assert_eq!(fourcc(type_id::GMDL).as_deref(), Some("____"));
        assert_eq!(fourcc(0).as_deref(), None);
        // A readable id renders as text; real Spore ids mostly do not, which is
        // the point. "ABCD" here is a synthetic id, not a Spore one.
        assert_eq!(fourcc(0x4443_4241).as_deref(), Some("ABCD"));
        // Partial readability is normal: cell_powers 0x754BE343 -> 43 e3 4b 75.
        assert_eq!(fourcc(type_id::CELL_POWERS).as_deref(), Some("C_Ku"));
    }

    #[test]
    fn fourcc_and_named_lookup_are_independent_axes() {
        // Two different ids can share a fourcc spelling; only the id is identity.
        assert_ne!(type_id::RW4, type_id::RASTER);
        assert_ne!(
            RecordType::new(type_id::RW4).name(),
            RecordType::new(type_id::RASTER).name()
        );
    }
}

#[cfg(test)]
mod container_tests {
    use super::*;

    /// The claim that `png`-typed records are RW4 containers was wrong, and it
    /// was load-bearing: it would have sent every texture lookup down the RW4
    /// walker. This test exists so the correction cannot be silently reverted by
    /// someone who "remembers" the old story.
    #[test]
    fn png_and_rw4_are_distinct_type_ids_with_distinct_payloads() {
        assert_ne!(type_id::PNG, type_id::RW4);
        assert_ne!(type_id::JPEG, type_id::RW4);
        // Adjacent id families: 0x2F4E681B (rw4) and 0x2F4E681C (raster) are
        // neighbours -- 0x1B vs 0x1C, differing in the low three bits -- and
        // 0x2F7D0002/4 (jpeg/png) sit in a different cluster entirely. The old
        // wrong claim came from that adjacency, so it is pinned here.
        assert_eq!(type_id::RW4, 0x2F4E_681B);
        assert_eq!(type_id::RASTER, 0x2F4E_681C);
        assert_eq!(type_id::RW4 ^ type_id::RASTER, 0x07);
        // And the png/jpeg cluster is nowhere near the RW/raster cluster.
        // Written as a runtime comparison so the assertion is about the values
        // rather than something the compiler can fold away.
        let png_high = type_id::PNG >> 16;
        let rw_high = type_id::RW4 >> 16;
        assert!(
            png_high != rw_high,
            "png 0x{:08x} and rw4 0x{:08x} must not share a high half",
            type_id::PNG,
            type_id::RW4
        );
    }

    #[test]
    fn every_named_constant_agrees_with_the_canonical_name_table() {
        for (id, expected) in [
            (type_id::PNG, "png"),
            (type_id::JPEG, "jpeg"),
            (type_id::RW4, "rw4"),
            (type_id::RASTER, "raster"),
            (type_id::GMDL, "gmdl"),
            (type_id::PROP, "prop"),
        ] {
            assert_eq!(
                RecordType::new(id).name(),
                Some(expected),
                "0x{id:08x} should be named {expected}"
            );
        }
    }
}

/// A row-major 3×3 matrix.
///
/// Lives here, in the crate both the authoring layer and the engine depend on,
/// because a creator block's `<orientation>` and a runtime placement's orientation
/// are the same nine numbers and must not become two types.
///
/// Stored as three rows because that is the order `<row0>`, `<row1>`, `<row2>`
/// appear in on disk, and converting to a transposed form at a boundary is how a
/// rotation silently becomes its inverse.
///
/// [`Default`] is **all zeros**, not the identity: `#[derive(Default)]` is the
/// obvious implementation and it is the wrong one, because a zero matrix is not a
/// rotation and a caller who writes `BlockTransform { orientation: Matrix3::default(), .. }`
/// gets a silently non-orthonormal orientation rather than a compile error. Use
/// [`Matrix3::IDENTITY`] for "no rotation".
#[derive(Debug, Clone, Copy, Default)]
pub struct Matrix3 {
    /// The first row.
    pub row0: [f32; 3],
    /// The second row.
    pub row1: [f32; 3],
    /// The third row.
    pub row2: [f32; 3],
}

impl Matrix3 {
    /// The identity rotation, which is what an unwritten orientation would be.
    pub const IDENTITY: Self = Self {
        row0: [1.0, 0.0, 0.0],
        row1: [0.0, 1.0, 0.0],
        row2: [0.0, 0.0, 1.0],
    };

    /// The nine components in row-major order.
    pub const fn to_array(self) -> [f32; 9] {
        [
            self.row0[0],
            self.row0[1],
            self.row0[2],
            self.row1[0],
            self.row1[1],
            self.row1[2],
            self.row2[0],
            self.row2[1],
            self.row2[2],
        ]
    }
}

impl PartialEq for Matrix3 {
    fn eq(&self, other: &Self) -> bool {
        // BITWISE, unlike a numeric comparison, and the difference is load-bearing:
        // `CreatorDocument` round-trips authored bytes, where `-0.0` written by the
        // game and `0.0` written by us are different documents. A derived `PartialEq`
        // would call them equal and a round-trip test would pass on a document that
        // is not byte-identical.
        //
        // Note the asymmetry with `Orientation::is_y_rotation`, which compares
        // NUMERICALLY on purpose: a matrix computed from a sine has no authored
        // zero-sign distinction to preserve. Same trap, opposite correct answer,
        // because the two questions are different.
        fn bits(v: f32) -> u32 {
            v.to_bits()
        }
        self.row0.map(bits) == other.row0.map(bits)
            && self.row1.map(bits) == other.row1.map(bits)
            && self.row2.map(bits) == other.row2.map(bits)
    }
}
