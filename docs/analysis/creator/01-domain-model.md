# CREATOR 01 — the minimal reusable creator-document data model

**Agent:** CREATOR-DOMAIN. **Date:** 2026-10-04. **Status:** research proposal, nothing implemented.
**Reads:** `AGENTS.md`, `crates/spore-engine/src/domain/**`, `docs/RUST-ENGINE.md`,
`docs/ENGINE-SCENE.md`, `docs/RESOURCE-GRAPH.md`, `docs/MATERIALS-DESIGN.md`, `docs/CELLSTAGE-RECON.md` §4.
**Ownership:** this file only. No Rust was written, no build was run, nothing was committed.

---

## 0. Method, and what "measured" means here

Everything in §1 marked **OBSERVED** or **VERIFIED** was counted by me over the
installed corpus at `SPORE/Data/{Spore_Game,Spore_Content}.package`
(GOG digital 3.1.0.22), read-only, with the committed Python oracle
`tools/spore/dbpf/dbpf.py`. The scripts are throwaway; the counts are the
deliverable and each row says its own population. No Spore asset byte is
committed here: §1.1 states the schema as element paths and aggregate counts,
and the only "fragment" is a tag skeleton with placeholder values.

Evidence grades are the repository's canonical 7-rung `spore_core::EvidenceLevel`
(`crates/spore-core/src/evidence.rs:32`), not the coarser PROVEN/OBSERVED/INFERRED/
UNKNOWN labels of the brief:

| brief label | grades I actually use |
|---|---|
| PROVEN | `VERIFIED` (measured over a population, repeatable) or `CONFIRMED` |
| OBSERVED | `OBSERVED` (read straight out of real records, no inference) |
| INFERRED | `INFERRED` or `SUPPORTED` |
| UNKNOWN | `UNKNOWN` |

### 0.1 The load-bearing discovery

**Spore's own creator document is in the corpus, and it is XML.** 68 records
across **six** record type ids are `<sporemodel>` documents. This is not an
inference from the SDK: it is the byte content of the records, parsed and
census-counted. `docs/CELLSTAGE-RECON.md:71` already reported 62 of them
(47 `crt` + 13 `cll` + 1 `flr` + 1 `bld`) as "CONFIRMED: creatures are
block-assembled, not monolithic meshes"; a sweep of 28 648 records in the two
packages finds **two more carriers** the recon doc missed — `vcl`
(`0x24682294`, 4 records, group `0x40646200`) and `ufo` (`0x476a98c7`, 2 records,
group `0x40656200`).

| record type id | name | docs | group | blockrefs | `formatversion` |
|---|---|---:|---|---:|---|
| `0x2B978C46` | `crt` | 47 | `0x40626200` | 486 | 16 (×46), 11 (×1) |
| `0x3D97A8E4` | `cll` | 13 | `0x40616200` | 79 | 16 (×12), 15 (×1) |
| `0x24682294` | `vcl` | 4 | `0x40646200` | 57 | 15 |
| `0x476A98C7` | `ufo` | 2 | `0x40656200` | 47 | 15 |
| `0x2399BE55` | `bld` | 1 | `0x40636200` | 8 | 16 |
| `0x438F6347` | `flr` | 1 | `0x40666200` | 3 | 15 |
| **total** | | **68** | 6 families | **680** | 11, 15, 16 |

Document sizes run 2 667 – 28 525 bytes. Line endings are `\r\n`, indent is two
spaces, declaration is `<?xml version="1.0" encoding="UTF-8"?>` (**OBSERVED** —
relevant only to a byte-exact writer, which is out of scope, see §7).

**Consequence:** a creator-document model does not have to be invented from
nothing. Most of its shape is already in the corpus and can be graded
`VERIFIED` instead of `INFERRED`, which is the single strongest result in this
report.

---

## 1. The smallest honest authoring model

### 1.1 The corpus schema, as measured (VERIFIED, 68 docs / 680 blockrefs)

Element paths, with populations. This is the *entire* schema — there are no
other elements and no other attributes anywhere in the 68 documents.

| path | count | shape |
|---|---:|---|
| `/sporemodel` | 68 | root, always |
| `/sporemodel/formatversion` | 68 | integer; only `11`, `15`, `16` observed |
| `/sporemodel/properties` | 68 | always present |
| `/sporemodel/properties/modeltype` | 68 | opaque word; **one document writes it in decimal, 67 in hex** |
| `/sporemodel/properties/skineffect{1,2,3}` | 61 each | opaque word |
| `/sporemodel/properties/skineffectseed{1,2,3}` | 61 each | integer |
| `/sporemodel/properties/skincolor{1,2,3}` | 61 each | 3 comma-separated floats in `[0,1]` |
| `/sporemodel/properties/requiredpack` | 66 | word |
| `/sporemodel/properties/zcorpscore` | 59 | byte-ish integer |
| `/sporemodel/properties/name` | 9 | string |
| `/sporemodel/properties/description` | 9 | string (empty in all 9) |
| `/sporemodel/properties/hasthumbnail` | 1 | `false` |
| `/sporemodel/properties/cacheid` | 1 | integer |
| `/sporemodel/blocks` | 68 | `@count` **== `len(blockref)` in 68/68** |
| `/sporemodel/blocks/blockref` | 680 | one part instance |
| `…/blockref/blockid` | 680 | `"<group>, <instance>"`, two hex words, **no type word** |
| `…/blockref/transform/scale` | 680 | **one scalar**, range 0.28 – 2.254181, never negative |
| `…/blockref/transform/position` | 680 | 3 comma-separated floats |
| `…/blockref/transform/orientation/row{0,1,2}` | 680 each | 3×3; **proper rotation in 680/680** (orthonormal, `det = +1.0`) |
| `…/blockref/transform/triangledirection` | 680 | 3 floats; **equals `row1` in only 578/680** |
| `…/blockref/transform/trianglepickorigin` | 670 | 3 floats; **absent in 10/10 of the one v11 document's blockrefs** |
| `…/blockref/transform/userorientation/userorientation_row{0,1,2}` | 11 each | a second 3×3; present on 11 blockrefs only |
| `…/blockref/transform/blockbbox/blockbbox_{min,max}` | 10 each | 3 floats; present on 10 blockrefs only |
| `…/blockref/transform/basemusclescale` | 8 | scalar |
| `…/blockref/transform/musclescale` | 5 | scalar (3 distinct values, one of them `0`) |
| `…/blockref/transform/limbtype` | 5 | integer; **only value `0` observed** |
| `…/blockref/snapped` | 680 | `false` ×674, `true` ×6 |
| `…/blockref/handles` | 119 | `@count` == `len(weight)` in 119/119 |
| `…/blockref/handles/weight` | 252 | scalar value + `@channel` (opaque word; **14 distinct channels**) |
| `…/blockref/symmetric` | 84 | integer; **observed values 1…31** (a mask, not a flag) |
| `…/blockref/paintlist` | 80 | `@count` == `len(paint)` in 80/80 |
| `…/blockref/paintlist/paint/paintregion` | 195 | word; 7 distinct values |
| `…/blockref/paintlist/paint/paintid` | 195 | word; 5 distinct values |
| `…/blockref/paintlist/paint/color{1,2}` | 195 each | 3 floats in `[0,1]` |
| `…/blockref/childlist` | 520 | `@count` == `len(childid)` in 520/520 |
| `…/blockref/childlist/childid` | 601 | decimal index |

Attributes in the whole corpus: `count` on `blocks`/`childlist`/`handles`/
`paintlist`, and `channel` on `weight`. **That is the complete attribute set.**

### 1.2 The graph, as measured (VERIFIED)

The parent/child relation is not a guess; it is countable:

| measurement | value |
|---|---|
| child edges (`childid` elements) | **601** |
| edges whose target index is outside its document's `blockref` list | **0** |
| edges where a blockref is its own child | **0** |
| documents containing a cycle (proper DFS, back-edge detection) | **0** |
| edges that are not tree edges (target already has a parent) | **0** |
| maximum in-degree over all 680 blockrefs | **1** |
| blockrefs with in-degree 0 (roots) | **79** across 68 documents |
| roots per document | 1 (×62), 2 (×4), 3 (×1), 6 (×1) |
| blockrefs with no `childlist` element at all (leaves) | **160** |
| maximum children on one blockref | 23 |
| deepest root-to-leaf path | 20 |

So the corpus's own structure is a **forest of trees**, one to six roots per
document, up to 680 nodes, depth up to 20 — *not* a single tree, and not a flat
list. Two independent counting methods (in-degree histogram, DFS colouring)
agree, and `0` self-edges is confirmed after one of my own scripts leaked a loop
variable and briefly reported 108: the corrected figure is 0, and both methods
are reported here because the first one was wrong.

`childid` is therefore an **index into the document's own `blockref` list**, and
the list order *is* the index space. That is the only reading consistent with
601/601 in-range and 0 multi-parent nodes.

### 1.3 The proposed types

Stdlib only, no `serde`, no Bevy. Names chosen to match the corpus's vocabulary
so the mapping is checkable by eye.

```rust
// ── identity ────────────────────────────────────────────────────────────────

/// A `(group, instance)` pair naming one block, exactly as `<blockid>` writes it.
///
/// **No record type.** `<blockid>` stores two words and the corpus never states a
/// third. This is deliberately NOT `spore_core::ResourceKey`, which is a
/// `(type, group, instance)` triple: 60 of 60 distinct blockids in the corpus name
/// **zero** `gmdl` records (§3.2), so manufacturing a type word here would invent
/// the one field the format withholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockId {
    pub group: u32,
    pub instance: u32,
}

/// An index into `CreatorDocument::blocks`, which is also the corpus's `<childid>`
/// index space. Validated on construction of the graph, not trusted from the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BlockIndex(pub usize);

// ── transform ───────────────────────────────────────────────────────────────

/// Three rows of a 3×3 matrix, as `<orientation>` writes them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rows3 {
    pub row0: [f32; 3],
    pub row1: [f32; 3],
    pub row2: [f32; 3],
}

/// Everything `<transform>` carries, and nothing it does not.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockTransform {
    /// `<scale>`: a **single scalar** in 680/680 records. Never `[f32; 3]`.
    pub scale: f32,
    /// `<position>`.
    pub position: [f32; 3],
    /// `<orientation>`: a proper rotation in 680/680 measured records.
    pub orientation: Rows3,
    /// `<triangledirection>`. A field of its own, NOT an alias of `orientation.row1`:
    /// the two disagree in 102 of 680 records.
    pub triangle_direction: [f32; 3],
    /// `<trianglepickorigin>`. **`None` when the element is absent**, which happens
    /// for 10 of 10 blockrefs of the one `formatversion 11` document. `Some([0;3])`
    /// is a different statement and the corpus never makes it.
    pub triangle_pick_origin: Option<[f32; 3]>,
    /// `<userorientation>`: a second 3×3, on 11 of 680 blockrefs. Relationship to
    /// `orientation`: UNKNOWN.
    pub user_orientation: Option<Rows3>,
    /// `<blockbbox>`, on 10 of 680 blockrefs. Whose bounds: UNKNOWN.
    pub block_bbox: Option<BlockBounds>,
}

/// `<blockbbox_min>` / `<blockbbox_max>`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

// ── one placed part ─────────────────────────────────────────────────────────

/// One `<blockref>`: one placed instance of one block.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBlock {
    /// `<blockid>`. Which block this is.
    pub block: BlockId,
    /// `<transform>`.
    pub transform: BlockTransform,
    /// `<snapped>`. Present in 680/680; `true` in 6. **Meaning UNKNOWN** — the
    /// name suggests snapping to a socket and nothing in the corpus says so.
    pub snapped: bool,
    /// `<childlist>`: the indices of this block's children.
    ///
    /// **`None` for a leaf** — 160 of 680 blockrefs have no `childlist` element at
    /// all. `Some(vec![])` is a different statement; the corpus never makes it,
    /// and collapsing the two would make "no children element" indistinguishable
    /// from "an empty child list".
    pub children: Option<Vec<BlockIndex>>,
    /// `<handles>/<weight>`: named channels with a scalar each. `channel` is an
    /// opaque word (14 distinct); what a channel *is* — UNKNOWN.
    pub handles: Option<Vec<MuscleWeight>>,
    /// `<paintlist>/<paint>`. `paintid`/`paintregion` are opaque; `color1`/`color2`
    /// are two RGB triples. How any of it reaches a raster: UNKNOWN.
    pub paints: Option<Vec<Paint>>,
    /// `<symmetric>`: observed values 1…31, so a **mask**. Which bits mean what:
    /// UNKNOWN.
    pub symmetric: Option<u32>,
    /// `<limbtype>`, `<musclescale>`, `<basemusclescale>` — grouped because the
    /// corpus nests all three under `<transform>` and they appear together.
    /// `limbtype` has exactly one observed value (`0`), so the type is a bare
    /// integer and NOT a 2-variant enum.
    pub limb: Option<LimbFields>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MuscleWeight {
    /// `@channel`, verbatim. Never resolved to a name.
    pub channel: u32,
    pub weight: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paint {
    pub paint_id: u32,
    pub paint_region: u32,
    pub color1: [f32; 3],
    pub color2: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LimbFields {
    pub limb_type: u32,
    pub muscle_scale: Option<f32>,
    pub base_muscle_scale: Option<f32>,
}

// ── the document ────────────────────────────────────────────────────────────

/// One `<sporemodel>` document: a forest of placed blocks plus its properties.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatorDocument {
    /// Which record this was read from, when it was read from one. `None` for a
    /// document built in memory. This is the only link back to DBPF and it is
    /// deliberately optional: a creator document is a value, not a handle.
    pub source: Option<DocumentSource>,
    /// `<formatversion>`. Version-gated, and **necessary but not sufficient** as a
    /// gate: field presence varies *within* a version (§1.4).
    pub format_version: FormatVersion,
    /// `<properties>`.
    pub properties: DocumentProperties,
    /// `<blocks>`, in document order. **The order is load-bearing**: it is the
    /// index space `<childid>` refers to. Reordering is not a valid edit.
    pub blocks: Vec<PlacedBlock>,
    /// Every element and attribute the parser did not claim, verbatim, so a
    /// save→load round trip cannot lose a field this build has never heard of.
    pub unclaimed: Unclaimed,
}

/// Where a document came from, when it came from one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSource {
    /// The DBPF key of the record the bytes came from. **Not** the block
    /// identities: the document names blocks, it is not itself a block.
    pub key: spore_core::ResourceKey,
}

/// `<formatversion>`, a versioned enum with an `Unknown` arm that keeps the number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatVersion {
    V11,
    V15,
    V16,
    /// Any other integer, carried verbatim. Present so that a document this build
    /// has never seen round-trips as itself instead of as a parse error.
    Unknown(u32),
}
```

`DocumentProperties` is deliberately **every field optional**, including
`model_type` being a graded wrapper rather than a bare `u32`:

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DocumentProperties {
    /// `<modeltype>`, present in 68/68. **Meaning UNKNOWN**: 5 distinct values
    /// (`0x9ea3031a` ×46, `0xdfad9f51` ×13, and three singletons). Graded, because
    /// a bare word that reads like an id is exactly the claim that gets lost.
    pub model_type: Option<GradedWord>,
    /// `<skineffect{1,2,3}>` + `<skineffectseed{1,2,3}>`. All six are present
    /// together in 61/68 and absent together in 7/68 — grouped as one option
    /// because the corpus never shows a partial set. That is an observation about
    /// 68 documents, not a rule, so it is not enforced.
    pub skin_effects: Option<[SkinSlot; 3]>,
    /// `<skincolor{1,2,3}>`, present 61/68 — same all-or-nothing observation.
    pub skin_colors: Option<[[f32; 3]; 3]>,
    pub required_pack: Option<u32>,
    pub zcorp_score: Option<u32>,
    pub name: Option<String>,
    pub description: Option<String>,
    /// `Some(false)` in the one document that has it. `None` elsewhere is
    /// "not written", which is not `Some(false)`.
    pub has_thumbnail: Option<bool>,
    pub cache_id: Option<u32>,
}

/// An opaque word plus the grade of the claim that it means something.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GradedWord {
    pub value: u32,
    /// Always `EvidenceLevel::Unknown` for every field in this document. The type
    /// exists so that upgrading one word later is a data change, not an API change.
    pub level: spore_core::EvidenceLevel,
}

/// Elements and attributes this build does not model, kept verbatim and in
/// document order. This is the lossless-extension mechanism; §5 explains why the
/// alternative (a closed struct plus a version number) is not good enough.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Unclaimed {
    pub entries: Vec<UnclaimedEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnclaimedEntry {
    /// Dotted path from the root, e.g. `"blocks.blockref.3.newthing"`.
    pub path: String,
    /// `text` for an element, `attr:<name>` for an attribute.
    pub kind: UnclaimedKind,
    pub raw: String,
    /// Index of the owning block when the entry is inside one.
    pub owner: Option<BlockIndex>,
}
```

### 1.4 Which fields are which kind of claim

| kind | fields |
|---|---|
| **VERIFIED** (measured, repeatable) | the whole element set of §1.1; `blocks/@count == len(blockrefs)` (68/68); `orientation` is a proper rotation (680/680); `childid` is an in-document index (601/601); in-degree ≤ 1, 0 cycles, 0 self-edges, 79 roots/68 docs; leaves are expressed by *absence* of `childlist` (160); `scale` is one positive scalar (680/680); `triangle_pick_origin` absence is version-gated (10/10 v11) |
| **OBSERVED** (read from records, no population claim) | every value's *shape*: hex-vs-decimal spelling of `modeltype`, `symmetric` in 1…31, 14 distinct `weight` channels, 7 `paintregion` values, 5 `paintid` values, `limbtype == 0` only, `description` empty in 9/9 |
| **AUTHORING-ONLY** (we choose; the corpus neither requires nor forbids it) | the struct grouping; `BlockId`/`BlockIndex` newtypes; `Unclaimed`; `DocumentSource` being optional; the *policy* that unmodelled fields go to `Unclaimed` rather than being dropped; the projection's option set (§3) |
| **PLACEHOLDER, DELIBERATELY OPEN** | what `modeltype` means; what `skineffect*` + seeds mean; what `weight`'s `channel` identifies; what `paintregion`/`paintid` select; what `symmetric`'s bits mean; what `limbtype` means; what `zcorpscore` scores; whether `userorientation` overrides or informs `orientation`; whether `triangledirection` or `row1` is authoritative (they disagree 102/680); **what `snapped` means**; what `blockbbox` bounds |

Version is **necessary but not sufficient**, measured: `formatversion 16` spans
`skineffect*` present 58/59, `requiredpack` 58/59, `handles` 8/59, `paintlist`
8/59, `blockbbox` 0/556, `musclescale` 0/556. So **no field's presence may be
inferred from the version**, and every field above is `Option` for that reason
independently of the version being recorded at all.

---

## 2. Where it lives: a new `crates/spore-creator-core`

**Recommendation: a new crate. Do not put this in `spore-engine::domain`.**

### 2.1 Why not a `domain` module

1. **The crate would inherit Bevy.** `spore-engine` is the only crate that
   depends on `bevy` (`docs/RUST-ENGINE.md` §1). A creator document must be
   loadable, inspectable and testable by `osptool` and by any future headless
   tool; putting its parser inside the engine crate makes every such consumer
   compile Bevy to read a text file.
2. **It breaks the direction the dependency graph depends on.** The documented
   rule is that `spore-core` … `spore-material` are Bevy-free and that the
   renderer is downstream of data. A document *format* is data. `spore-engine`
   must be able to depend on the document model; the document model must never
   depend on the engine, or the projection in §3 becomes a cycle.
3. **It protects the `domain` API from churn.** The task requires
   `spore-engine`'s existing domain API not to break. A separate crate makes that
   structural: `spore-creator-core` cannot touch `Placement`,
   `ModelInstance`, `Creature`, `Scene` or `SceneObject`, because it does not
   depend on the crate that owns them.
4. **It matches the repo's own precedent.** `spore-semantic-bridge` exists
   (`crates/spore-semantic-bridge/Cargo.toml`) as a stdlib-plus-`spore-core`
   consumer of a committed artifact, and is not in `spore-engine`. A creator
   document is the same shape of thing: a committed/reproducible artifact with a
   strict reader.

### 2.2 Exact manifest

```toml
# crates/spore-creator-core/Cargo.toml
[package]
name = "spore-creator-core"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
description = "Spore creator documents (`<sporemodel>`): part instances, the block forest, and a failable projection seam. Renderer-agnostic, Bevy-free, stdlib-only."

[dependencies]
spore-core = { workspace = true }
thiserror = { workspace = true }
```

Plus two additive lines in the workspace root: `members` and
`[workspace.dependencies] spore-creator-core = { path = "crates/spore-creator-core", version = "0.1.0" }`.

**Dependency list is closed**: `spore-core` (for `ResourceKey`, `EvidenceLevel`,
`Fact`) and `thiserror` (already a workspace dependency, used by `spore-core`).
No `serde` — the workspace has none and adding one is a workspace-wide decision
that is not this model's to make. No `bevy`. No I/O: parsing takes `&str` or
`&[u8]`, so the crate is testable with no filesystem at all.

Resulting graph:

```text
spore-core ──> spore-creator-core ──> spore-engine (projection module only)
                                        └──> spore-tools  (document inspection)
```

### 2.3 Conventions it must follow

| convention | where it comes from |
|---|---|
| `#![forbid(unsafe_code)]` | six crates use the strong form; `spore-creator-core` needs no exception path and must not acquire one (`AGENTS.md`, `docs/RUST-ENGINE.md` §1) |
| `#![warn(missing_debug_implementations)]` | `crates/spore-core/src/lib.rs:29` |
| a `claims` module: `pub const SUBJECT: &str` names + `claim(subject) -> Option<Fact<&'static str>>` returning `None` for an unknown subject | `crates/spore-gmdl/src/claims.rs:37` + `:243`, `crates/spore-assets/src/graph.rs:100` |
| claims recorded for *every* §1.4 "placeholder" row, at `EvidenceLevel::Unknown` | `AGENTS.md` evidence discipline; a claim module is "a list of what it is guessing" |
| `Display`/`as_str` on every closed vocabulary, plus a `parse` that returns the `Unknown` arm rather than an error | `FormatVersion` follows `EvidenceLevel::parse` (`crates/spore-core/src/evidence.rs:66`) |
| no `unwrap`/`expect` on parsed input; parse errors carry the byte offset and the offending text | `AGENTS.md` trap list; `spore-assets` error style |
| every container ordered (`Vec`, never a hash map) so output is deterministic | `spore-assets/src/graph.rs` "Determinism" section |

Proposed `claims` subjects, all `UNKNOWN` today: `blockid_record_type`,
`block_geometry_resolution`, `modeltype_meaning`, `skineffect_meaning`,
`skineffect_seed_meaning`, `skincolor_meaning`, `weight_channel_meaning`,
`paintregion_meaning`, `paintid_meaning`, `symmetric_mask_bits`,
`limbtype_meaning`, `musclescale_meaning`, `blockbbox_owner`,
`userorientation_vs_orientation`, `triangledirection_vs_orientation_row1`,
`snapped_meaning`, `zcorpscore_meaning`, `transform_frame_composition`,
`blocks_count_attribute_meaning`, `unclaimed_field_policy`.

---

## 3. The projection seam

### 3.1 The hard finding that shapes it

**60 distinct `<blockid>` values across 680 placements. 0 of them name a `gmdl`
record.** Verified over the whole installed tree: 11 041 `gmdl` records across
33 distinct groups, and not one `(group, instance)` pair from a `<blockid>`
matches any of them. Each of the 60 resolves to exactly two records of exactly
two types:

| type id | name | records at each blockid |
|---|---|---|
| `0x00B1B104` | `prop` (`spore_core::record::type_id::PROP`) | 1 — a property container; one of them contains the ASCII string `creature_vertebrae` |
| `0x5CAA5E28` | **unnamed** — absent from `tools/spore/types/typenames.json` and from the entire repository | 1 — 36 bytes, two 12-byte halves differing in one bit each; **not decoded** |

The block's `prop` record names **no** `gmdl` either (no `0x00E6BCE5` word
anywhere in it), so the link from a block to geometry is **not in the data this
workspace can currently read**. `docs/CELLSTAGE-RECON.md:44` says the same in
different words: *"player-cell identity UNRESOLVED"*.

Therefore: **`blockid → ResourceKey` is a resolution step, not a conversion, and
it succeeds 0/60 on the real corpus today.** The model must make that a
first-class, failable, reported outcome rather than a `?`.

### 3.2 `BlockId` → `ResourceKey`: the resolver contract

In `spore-creator-core` (no engine types involved):

```rust
/// The outcome of turning one `<blockid>` into something loadable.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockResolution {
    pub block: BlockId,
    pub state: ResolutionState,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionState {
    /// Exactly one candidate, and this crate is willing to name it.
    Resolved { key: spore_core::ResourceKey, level: spore_core::EvidenceLevel },
    /// Candidates exist but none may be preferred. **`level` says how strong the
    /// adjacency is** — today the only adjacency available is
    /// `spore_assets::graph::EdgeKind::SiblingRecord`, which is graded `Inferred`.
    Ambiguous { candidates: Vec<spore_core::ResourceKey> },
    /// Looked, found nothing. Never a zero, never an empty "success".
    Unavailable { reason: UnavailableReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnavailableReason {
    /// No index row carries this `(group, instance)`.
    NoRecordAtIdentity,
    /// Rows exist, but none this workspace can decode into geometry.
    NoDecodableGeometry,
    /// The record exists and the caller forbade the `Inferred` adjacency route.
    InferredRouteRefused,
}
```

`spore-creator-core` does **not** depend on `spore-assets` — the resolver is
supplied by the caller as a trait so the document model stays a value model:

```rust
pub trait BlockResolver {
    /// `level` is the grade of the route used, so a `Resolved` at `Inferred` is
    /// visibly weaker than one at `Observed` and survives into the scene.
    fn resolve(&self, block: BlockId) -> BlockResolution;
}
```

### 3.3 The second failable decision: frame composition

Whether a `<blockref>`'s transform is **local to its parent** or already in
document space is **UNKNOWN**, and nothing in the corpus settles it (it would
need the game). The projection must not pick silently:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FrameComposition {
    /// No decision made. `ProjectOptions::default()` lands here, and projection
    /// then returns `Err(ProjectionError::FrameCompositionUndecided)` rather than
    /// composing transforms in an arbitrary order.
    #[default]
    Undecided,
    /// child ∘ parent, depth first.
    LocalToParent,
    /// Each blockref transform is already in document space.
    Absolute,
}
```

A `Default` that refuses is the point. It is the same discipline as
`Bounds = None` (`crates/spore-engine/src/domain/mod.rs:47`) and
`Placement::lifted_onto` returning `None` (`domain/model.rs:228`): the caller
states what it knows.

### 3.4 Conversion functions, and where each one lives

The split is forced by the boundary rule: **the document model may not know the
engine's types, so every conversion lives in `spore-engine`.**

In `spore-creator-core` (document-side, no engine types):

```rust
impl CreatorDocument {
    /// Index/edge view with the measured invariants checked. Returns findings,
    /// never a repaired graph.
    pub fn forest(&self) -> BlockForest;

    /// Roots in ascending index order. Deterministic.
    pub fn roots(&self) -> Vec<BlockIndex>;

    /// The blocks a caller must resolve, deduplicated and ascending. On the real
    /// corpus this is 60 entries for 680 placements — the count that decides how
    /// many loads are needed, which is `Creature::distinct_records`' job on the
    /// other side.
    pub fn distinct_blocks(&self) -> Vec<BlockId>;

    /// Every field this build does not model, for a report line.
    pub fn unclaimed_summary(&self) -> UnclaimedSummary;
}

/// What `forest()` found. A cycle or a dangling index is a FINDING, reported and
/// preserved — the policy `spore_assets::graph` states as "a dangling edge is a
/// finding, not an error". The document is never rewritten to make it valid.
pub struct BlockForest { /* … */ }

pub struct ForestReport {
    pub block_count: usize,
    pub edge_count: usize,
    pub roots: Vec<BlockIndex>,
    pub leaves: Vec<BlockIndex>,
    /// Indices a `<childid>` names that do not exist. 0 in 601/601 measured.
    pub dangling: Vec<(BlockIndex, BlockIndex)>,
    /// Non-tree edges (in-degree > 1). 0 in the measured corpus.
    pub multiply_parented: Vec<BlockIndex>,
    /// True when a cycle exists. Never observed; the type exists so that the
    /// first document with one is reported rather than hanging the loader.
    pub has_cycle: bool,
}
```

In `spore-engine` (the only crate that knows both), as a new module — say
`crates/spore-engine/src/creator.rs`, sibling of `loader.rs`, **not** inside
`domain/`, because it is the seam and the seam is allowed to know about
resolution:

```rust
/// The result of projecting a document. Nothing is dropped: what could not be
/// resolved is returned next to what could.
pub struct ProjectedScene {
    pub scene: crate::domain::Scene,
    pub unresolved: Vec<UnresolvedBlock>,
    /// Parts whose orientation the current `Placement` cannot express (§3.5).
    pub inexpressible: Vec<InexpressiblePlacement>,
}

pub struct UnresolvedBlock {
    pub index: crate::domain::…BlockIndex,
    pub block: BlockId,
    pub reason: UnavailableReason,
}

/// Projects a creator document onto the engine's scene value.
///
/// Total failure modes are named: no decision about frame composition, a
/// document with a cycle, an unresolvable block. It never returns a `Scene` that
/// silently omits a part.
pub fn project(
    document: &CreatorDocument,
    resolver: &dyn BlockResolver,
    options: ProjectOptions,
) -> Result<ProjectedScene, ProjectionError>;

/// The single-block case: one root, nothing nested, one resolved key.
pub fn project_single(
    document: &CreatorDocument,
    resolver: &dyn BlockResolver,
) -> Result<crate::domain::SceneObject, ProjectionError>;
```

Field-by-field mapping:

| creator | engine | lossless? |
|---|---|---|
| `CreatorDocument` | one `SceneObject` | — |
| `properties.name` | `Creature::label` / `ModelInstance::label` | yes, but see below |
| a **root** `PlacedBlock` + a `Resolved`/`Ambiguous` resolution | one `ModelInstance { key, label: None, placement }` | only if resolution succeeds — **0/60 today** |
| `transform.position` | `Placement.translation` | yes (`crates/spore-engine/src/domain/model.rs:197`) |
| `transform.scale` | `Placement.scale` | yes — both are one `f32`; measured range 0.28–2.254 |
| `transform.orientation` | `Placement.rotation_y_degrees` | **only for the 79 roots**, see §3.5 |
| `transform.triangledirection`, `triangle_pick_origin`, `user_orientation`, `block_bbox`, `snapped`, `symmetric`, `handles`, `paints`, `limb`, `unclaimed` | *nothing* | **not expressible**; they stay in the document and are reported, not invented in the scene |
| `children` | *nothing* | **the flat domain has no parent/child**; flattening is a loss unless composition is decided (§3.3) |
| `spore-semantic` `Fact` grades on a resolution | *nothing* | the `EvidenceLevel` rides in `ProjectedScene` so a report can print `route=inferred-sibling` |

**On `label`:** `ModelInstance::label` is documented as "**DBPF has no string
table**, so no name is ever derived from a record" (`domain/model.rs:243`). This
model must not break that sentence. `properties/name` is a string *written by the
author into the document* (present in 9 of 68 documents), so it is author-given
and legal as a label — but block names from `prop` records are **not** used as
instance labels, because that would be deriving a name from a record, which is
exactly what the existing comment forbids.

### 3.5 `Placement` cannot express a placed part — the minimal fix

`Placement` is `{ translation: [f32;3], rotation_y_degrees: f32, scale: f32 }`
(`crates/spore-engine/src/domain/model.rs:195-202`). Against the corpus:

| property | `Placement` | corpus | verdict |
|---|---|---|---|
| translation | `[f32;3]` | `<position>`, 3 floats | fits |
| scale | one `f32` | one scalar, 680/680 | fits exactly |
| rotation | one `f32` about **+Y only** | a full proper rotation, 680/680 | **does not fit** |

Measured split of the 680 orientations into classes (`row0 == 1,0,0` ⇒ a
rotation about Y; identity counted separately):

| class | all 680 blockrefs | the **79 roots** |
|---|---:|---:|
| identity | 90 | 32 |
| pure Y | 498 | 47 |
| general (neither) | 92 | **0** |

So **every root is expressible** in the current `Placement`, and **92 nested
parts are not**. The engine additionally has no way to say that one instance is
the child of another: `ModelInstance` is flat (`domain/model.rs:238`) and
`SceneObject::instances()` returns a `&[ModelInstance]`
(`domain/scene.rs:42`).

**Minimal extension — a sibling type, not a new field.** Adding a field to
`Placement` is *not* minimal: `Placement { … }` struct literals exist at
`crates/spore-engine/src/cli.rs:183`, `:192`, `:200` and
`crates/spore-engine/tests/material_wiring.rs:427`, so every one of them would
stop compiling. A sibling type is purely additive:

```rust
// crates/spore-engine/src/domain/model.rs  (additive; nothing above changes)

/// A placement that can express what a creator document's `<transform>` carries.
///
/// Separate from [`Placement`] because `Placement` is the *world* placement the
/// loader turns into a `Transform`, and this is the *authored* one. They are
/// related by a fallible conversion, never by inheritance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockPlacement {
    pub translation: [f32; 3],
    /// The full rotation, as the three rows `<orientation>` writes.
    pub orientation: Rows3,
    /// One scalar, because the corpus writes one scalar in 680/680 records.
    pub scale: f32,
}

impl BlockPlacement {
    /// The +Y angle this placement would need, or `None` when its rotation is not
    /// a rotation about +Y.
    ///
    /// `None` is the honest answer for 92 of the 680 measured blockrefs. It is not
    /// `0.0`: a general rotation silently reported as "no rotation" is the exact
    /// defect this repository keeps paying for.
    pub fn as_y_rotation_degrees(&self) -> Option<f32>;

    /// The world placement, when `as_y_rotation_degrees` is `Some`.
    ///
    /// The `atan2(row1[2], row1[1])` convention below is **ours, not measured**:
    /// the corpus's handedness cannot be settled without the game. The test in §6
    /// pins self-consistency (all 79 roots reconstruct `row0/row1/row2` exactly),
    /// not correctness against Spore.
    pub fn to_placement(&self) -> Option<Placement>;
}
```

This keeps the Bevy boundary exactly where it is: `loader.rs:597-602` still
builds the one and only `Transform`, from a `Placement`, and nothing above
`loader` learns that `BlockPlacement` exists. Nothing in `loader.rs` needs to
change until a caller chooses to project nested parts; `Placement`, `ModelInstance`,
`Creature`, `Scene`, `SceneObject` are untouched.

---

## 4. What must NOT be in the core

| rejected | one-line justification |
|---|---|
| ECS entities / `Entity` ids | an entity's identity is a renderer-lifetime fact; a document that survives a save must not, and `domain/mod.rs:10` already fixes "a scene is a value, not ECS". |
| any `bevy` type | the single load-bearing invariant (`AGENTS.md`, `docs/RUST-ENGINE.md` §1); a document format has no business naming `Transform`. |
| a texture/material registry handle | registry membership is `spore-material`'s job and it is `Inferred` at the edges (`graph.rs:29`); a handle in a saved document would freeze a resolution into the file. |
| a window, a surface, a viewport, camera | the consumer owns those; `EngineScene` owns the world and `SporeAssetPlugin` owns the content, one owner per entity kind (`loader.rs:5-15`). |
| selection / gizmo / drag / undo history | none of it is in the corpus, all of it is UI; `GUI-BOUNDARY.md`'s own conclusion is that general-purpose machinery is not justified — and an undo stack in a value type turns the document into a session. |
| I/O (file paths, `std::fs`) | keeps the parser testable with no filesystem and no install, like `spore-gmdl`. |
| a `serde` derive | no `serde` in the workspace; a serialization format decision is not this model's to make. |

---

## 5. Extensibility without speculation

For each concept, how an UNKNOWN survives:

| concept | mechanism | corpus instance that forces it |
|---|---|---|
| version | `enum FormatVersion { V11, V15, V16, Unknown(u32) }`; `parse` returns `Unknown(n)`, never an error | three versions already coexist; a fourth must not fail a load |
| a field the parser does not model | `Unclaimed { entries }` — path + raw text, document order | `userorientation` (11 records) and `blockbbox` (10) were both found by this route, not by a schema |
| a field that exists but has no meaning | `GradedWord { value, level: EvidenceLevel::Unknown }` | all 5 `modeltype` values, all 14 `weight` channels, all 5 `paintid` values |
| a field that exists but is absent in this record | `Option<T>`, with `None` meaning *not written* | `triangle_pick_origin`: 670/680 present; the 10 absences are exactly the v11 document's blockrefs. `children`: `None` on 160 leaves, never `Some(vec![])`. `has_thumbnail`: `None` in 67, `Some(false)` in 1 |
| a field we looked for and did not find | `Unavailable { reason }` in `ResolutionState` | 0/60 blockids resolve; `Unavailable` is a distinct state, never an empty `Resolved` |
| a set of sibling fields whose presence is correlated in this corpus | one `Option` holding the group, and a comment saying the correlation is observed over 68 documents and **not enforced** | `skineffect*`+seeds 61/68 together; `skincolor*` 61/68 together |
| a decision we must not make for the caller | an enum whose `Default` is the refusing variant (`FrameComposition::Undecided`) | local-vs-absolute frame is untestable from records |
| an ordering that must not be disturbed | `Vec` + a documented invariant that the order *is* the index space | `childid` is an index; reordering silently re-parents the document |
| a value that disagrees with a sibling value | two separate fields, never a reconciliation | `triangledirection` vs `orientation.row1` disagree in 102/680; picking one would delete a finding |
| a claim a caller wants to attach | reuse `spore_core::Fact` verbatim | the resolver's `Inferred` sibling route must stay visibly weaker than an observed one |

The single rule behind the table: **absence, disagreement and un-resolvability are
all values.** None of them is a default, a zero, an empty `Vec`, or `false`.

---

## 6. Testability contract

Hermetic means: no `SPORE/`, no GPU, no window, no filesystem, no network.
`cargo test -p spore-creator-core` and the projection tests in `spore-engine`
must pass with none of them present — which is what `AGENTS.md` demands of the
asset layer and what `domain/world.rs:122-137` already demonstrates with a
committed synthetic DBPF fixture.

### 6.1 Parser and model

| # | test | asserts |
|---|---|---|
| 1 | a hand-written 4-block document parses | 4 blocks; `blocks.len()` == the `count` attribute; `@count` mismatch is an **error**, not a repair (68/68 agree in the corpus, so a mismatch is corruption) |
| 2 | a document with `formatversion 17` | parses as `FormatVersion::Unknown(17)`; **not** an error; round-trips as 17 |
| 3 | a document with an unmodelled element and an unmodelled attribute | both land in `Unclaimed` with the right owner index, verbatim, in document order |
| 4 | a leaf blockref with no `<childlist>` | `children == None`; and a blockref with `<childlist count="0"/>` yields `Some(vec![])` — the two are **distinguishable** |
| 5 | a `<childid>` naming an index that does not exist | parse succeeds; `forest().dangling` reports it; nothing is dropped |
| 6 | a `<childid>` naming its own index | reported, not silently removed; **this input is synthetic on purpose** — 0/601 in the corpus |
| 7 | a two-node cycle | `forest().has_cycle == true`; `project` returns `Err`, and terminates |
| 8 | malformed XML, a truncated file, a non-numeric float, a `blockid` with one word | each is a typed error naming the offset and the offending text; no panic |
| 9 | decimal and hex spellings of the same word | both parse to the same `u32` — one corpus document writes `modeltype` in decimal and 67 in hex |
| 10 | determinism | parsing the same bytes twice yields `==` documents; iteration order is `Vec` order everywhere |

### 6.2 Projection

| # | test | asserts |
|---|---|---|
| 11 | default options | `Err(ProjectionError::FrameCompositionUndecided)` — **the refusing default is tested** |
| 12 | a resolver that resolves nothing | `Ok` with `scene.objects == []` **and** `unresolved.len() == n`; never an empty scene reported as success |
| 13 | two blockrefs with the same `BlockId` | one resolution, two instances — the analogue of `Creature::distinct_records` (`domain/creature.rs:70`) |
| 14 | a root with a general (non-Y) rotation | `as_y_rotation_degrees() == None`, the part lands in `inexpressible`, and the report says so; it is **not** silently rotated to 0° |
| 15 | all 79 measured root orientations | for each, `to_placement()` then reconstruct gives `row0/row1/row2` bit-identical — the §3.5 convention pinned by test, with its UNKNOWN status asserted in the doc comment |
| 16 | nested parts with `FrameComposition::LocalToParent` | composed positions are reproducible from a fixed synthetic document; the composition **order** is asserted, because that is the caller's decision |
| 17 | a document whose `properties/name` is absent | the resulting `ModelInstance.label == None`, preserving `domain/model.rs:243` |
| 18 | a document with 3 roots | 3 `SceneObject`s (or 1 `Creature` with 3 instances, per an explicit option), and `scene.records()` deduplicates as `Scene::records` already does (`domain/scene.rs:154`) |
| 19 | the projection never mutates the document | the input is `&CreatorDocument` and a clone-comparison test asserts it |

### 6.3 Round-trip invariants after save → load

1. **Field-for-field equality** for every modelled field, including `None`s.
2. **`Unclaimed` survives**: a document with an unknown element writes it back and
   reads it back with the same path, kind, raw text and owner. A round trip that
   silently drops an unknown field is a **failure**, not a partial success.
3. **Version survives**: `Unknown(17)` writes `17` and reads `Unknown(17)`.
4. **Index space survives**: `blocks` order is unchanged, so every `BlockIndex` in
   every `children` list still names the same block. Assert this by resolving
   indices to `BlockId` before and after.
5. **Absence survives**: a `None` field is *not written*, and reading the result
   yields `None` — not `Some(default)`.
6. **Unknown bit patterns survive**: a `symmetric` value of `31` and of `0x8000_0000`
   both round-trip (the corpus only reaches 31, so the second is synthetic on
   purpose).
7. **Byte-level stability is NOT an invariant.** The corpus writes `\r\n` and
   two-space indent (**OBSERVED**); a writer that emits `\n` has not corrupted the
   document. Round-trip equality is on the **value**, never on the bytes. Claiming
   byte-exactness with the original writer would be a claim with no measurement
   behind it.

---

## 7. What I rejected, and why

Recorded explicitly because the architecture reviewer will look for speculative
generalisation. Each of these is *plausible* and *unevidenced*.

| rejected concept | why |
|---|---|
| **skeleton / bones / joints / skinning weights on parts** | zero skeleton or bone data in 680 blockrefs. `handles/weight` has a `@channel` and a scalar and nothing else — no joint index, no matrix. `domain/creature.rs:11-15` already refuses this on the runtime side; the document side must agree. |
| **socket / attachment-point enum** (`Head`, `EyeSocket`, …) | across 60 distinct blocks, exactly **one** legible name exists in the whole corpus (`creature_vertebrae`, inside a `prop` record). There is no socket vocabulary to reproduce, and inventing one is the single most likely way this model rots. |
| **a "part kind" or "creature part category" enum** | same evidence, and it would have to be a *guess* dressed as a taxonomy. `modeltype` has 5 opaque values and names nothing. |
| **non-uniform scale (`scale: [f32;3]`)** | 680/680 records carry **one** scalar. A vec3 would be a plausible-looking field with no corpus behind it. If anisotropic scaling is ever established it is an *additive* change. |
| **deformation / muscle parameters as a typed struct** | `musclescale` has 3 distinct values across 5 records, one of which is `0`; `basemusclescale` 5 values across 8 records; `weight` has 14 opaque channel ids. Nothing states what a muscle is or what a channel indexes. Kept as an opaque weighted-channel list and nothing more. |
| **per-part material / paint → texture binding** | `paint` carries `paintid` (5 values) and `paintregion` (7 values) and two colours. `MATERIAL-ROLES.md` §4 already establishes that this corpus has **no** base-colour records; a paint→raster binding would be exactly the "diffuse map" invention that document refutes. |
| **an animation reference per part** | no animation element in any of the 68 documents, and `docs/SPORE-RECONSTRUCTION-ROADMAP.md` grades Anim `UNKNOWN — structs only`. |
| **a symmetric-mirror placement transform** | `symmetric` is an opaque mask with values 1…31. Exposing it as a mirror/duplicate feature would name 31 bits nothing has named. |
| **treating `<blocks>` as a single tree with a root at index 0** | true for 62 of 68 documents and **false for 6** (roots at 2, 3 and 6 per document). A `root: BlockIndex` field would be wrong 9% of the time and would fail silently. |
| **treating `<blocks count>` as the authoritative size** | it agrees in 68/68, so it is a *consistency check*, not a source of truth; reading it as the length would let a truncated file parse as a short document. |
| **`Snapped(bool)` named "auto-aligned-to-socket"** | `snapped` is `true` in 6 of 680 and nothing in the corpus says what it snapped to. Named for the element, meaning left `UNKNOWN`. |
| **byte-exact XML reproduction (declaration, `\r\n`, indent, attribute order)** | measurable but pointless for a value model, and it would make every future field a formatting decision. Recorded as an observation in §0.1, not a contract. |
| **a `serde`-derived wire format** | no `serde` in the workspace; picking one changes the workspace, not this model. |
| **writing the projection inside `spore-creator-core`** | it would need `spore-engine`'s types, which are behind the Bevy crate. That is the layering, and the seam belongs on the engine side (§3.4). |
| **a `part_id` / `name` on `PlacedBlock`** | the corpus has no per-blockref name. `name` exists only at document level, in 9 of 68. Per-block identity in the document is the `BlockIndex`, full stop. |
| **treating `triangledirection` as `orientation.row1`** | they disagree in 102 of 680 records. Merging them would delete 102 findings. |
| **treating `<childlist>` as "this block's parent"** | the index space is in-document and each node has at most one parent (601/601), so the direction is unambiguous — but the *element name* is the only evidence, and the corpus's own naming (`childlist`) plus the in-degree measurement are what settle it, not an assumption that lists are parents. |

---

## 8. Open questions this model deliberately does not answer

| # | question | grade | how it would be settled |
|---|---|---|---|
| 1 | what record type is `0x5CAA5E28`, and how does a `blockid` become geometry? | UNKNOWN | decode the 36-byte record; find the type in the SDK (`SporeGhidra_march2017.xml`); or trace the editor under Wine. **This blocks the whole projection** — 0/60 today. |
| 2 | is a `<blockref>` transform local to its parent or absolute? | UNKNOWN | only a runtime observation settles it; hence `FrameComposition::Undecided` |
| 3 | what are the 5 `modeltype` values? | UNKNOWN | SDK name for the record type, or the game's own loader |
| 4 | what does a `weight`'s `@channel` name? | UNKNOWN | correlate with animation state names (`docs/CELLSTAGE-RECON.md` §5 lists 73 observed state/channel names — a lead, not evidence) |
| 5 | why do `triangledirection` and `orientation.row1` disagree in 102 of 680? | UNKNOWN | a differential against the original's own transform composition |
| 6 | what do `symmetric`'s bits mean? | UNKNOWN | 31 values observed; no bit is named anywhere |
| 7 | are there creator documents in the 20 000+ records this pass did not census? | UNKNOWN | the sweep covered `Spore_Game` + `Spore_Content` (28 648 records) and found 6 XML carriers; `Spore_Graphics`, `PatchData` and the EP1 packages were not swept for XML |

---

## 9. Clean-room and scope statement

Nothing here derives from EA or Maxis source, code, data or assets. The schema in
§1.1 was obtained by reading the container index of a local GOG install with this
repository's committed Python DBPF reader and counting element paths; the values
were used to establish *shapes and populations*, not to reproduce any record. No
Spore asset byte is committed in this file: the table gives element paths and
counts, and the only structural illustration is the tag skeleton of §1.3, whose
values are types, not data. `SPORE/` stays git-ignored. No Rust was written, no
build was run, no file other than this one was touched.
