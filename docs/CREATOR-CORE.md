# Creator Core — architecture

**Status:** design reviewed and corrected, implementation starting.
**Scope:** the reusable authoring/construction core, plus Creature Creator as its
first client. Not an editor UI.

This document is the synthesis of six independent investigations plus a direct
census of the real data, **reviewed adversarially** in
`docs/analysis/creator/07-architecture-review.md` (30 spot checks, 22 reproduced,
8 wrong — including a table in §0 whose hex ids matched no record in the tree;
all corrected in place and marked). Every structural claim below is either
measured here or cited to a report in `docs/analysis/creator/`. Where the corpus
is silent, the document says so and the data model preserves the silence.

---

## 0. The finding that reframes the work

**Spore's creator document is not hypothetical. It is in the game data, and it
is text.**

`Spore_Game.package` holds **68 creator documents** (and `PatchData.package`
carries the same 68 again, so **134 records in the install**) — verified by
direct census of the DBPF index, not inferred:

| type id | name | count | group | what it is |
|---|---|---|---|---|
| `0x2B978C46` | `crt` | 47 | `0x40626200` | creature |
| `0x3D97A8E4` | `cll` | 13 | `0x40616200` | cell |
| `0x24682294` | `vcl` | 4 | `0x40646200` | vehicle |
| `0x476A98C7` | `ufo` | 2 | `0x40656200` | UFO (a vehicle-family document) |
| `0x2399BE55` | `bld` | 1 | `0x40636200` | building |
| `0x438F6347` | `flr` | 1 | `0x40666200` | flora |

> Corrected after review: the first draft of this table carried hand-converted
> hex that matched **no record in the tree**. The ids above are the ones the
> index actually holds (`docs/analysis/creator/07-architecture-review.md` §1).

All 68 are XML with root `<sporemodel>`, and **all 68 parse cleanly** with a
stock XML parser. They carry **680 `<blockref>` part instances** in total.
`docs/analysis/creator/01-domain-model.md` and `02-parts-and-catalogs.md`
reached the same census independently; the grammar below is my own parse of the
real bytes.

```xml
<?xml version="1.0" encoding="UTF-8"?><sporemodel>
  <formatversion>16</formatversion>
  <properties>
    <modeltype>0x9ea3031a</modeltype>
    <skineffect1>0x791c8201</skineffect1>  <skineffectseed1>1234</skineffectseed1>
    <skincolor1>0.341176,0.360784,0.576470</skincolor1>
    <requiredpack>0x00000001</requiredpack>  <zcorpscore>255</zcorpscore>
  </properties>
  <blocks count="8">
    <blockref>
      <blockid>0x40626000, 0xd05c53a3</blockid>
      <transform>
        <scale>0.741465</scale>
        <position>0,0.0541105,0.622791</position>
        <triangledirection>0,0.809026,-0.587772</triangledirection>
        <trianglepickorigin>0,0,0</trianglepickorigin>
        <orientation><row0>1,0,0</row0><row1>0,0.809026,-0.587772</row1><row2>0,0.587772,0.809026</row2></orientation>
      </transform>
      <snapped>false</snapped>
      <childlist count="1"><childid>1</childid></childlist>
    </blockref>
    …
```

This changes the shape of the task in three ways:

1. **We do not invent a document format.** We decode the game's, and our own
   authoring format is a documented projection of it. The user's brief said "do
   not invent binary formats" — this goes further: we do not invent the
   *semantics* either, for everything the corpus already states.
2. **The shared-core thesis is now evidence-backed at the data layer.** Six
   record families across creature / cell / vehicle / UFO / building / flora
   share one grammar, one block structure, and one code type
   (`Editors::cEditorResourceBlock`, 472 B, named generically —
   `06-semantic-evidence.md`). That is what justifies a Creator Core.
3. **The shared *behaviour* thesis is not.** 469 `/Spore/Editors/*` types exist
   and **none** mention vehicle, building, UFO or terrain; no
   `cVehicleEditor` class-name string exists in the binary. The corpus justifies
   a shared **data** core only. Our Creator Core is therefore scoped to data +
   document mechanics, and the three editors remain separate clients.

---

## 1. The verified grammar

Censused over all 68 documents. Counts are element occurrences, not documents.

### 1.1 Document

| element | occurrences | notes |
|---|---|---|
| `formatversion` | 68 | `11` ×1, `15` ×8, `16` ×59. **A label, not a schema** — see below. |
| `properties` | 68 | see below |
| `blocks@count` | 68 | attribute equals the real child count in all 68 |

`properties` children: `modeltype` 68, `requiredpack` 66, `skineffect1..3` 61,
`skineffectseed1..3` 61, `skincolor1..3` 61, `zcorpscore` 59, `name` 9,
`description` 9, `hasthumbnail` 1, `cacheid` 1.

**`formatversion` does not gate fields.** It was the obvious candidate for a
version gate and the corpus refutes it: `v16` spans two presence classes
(`handles` present in some blocks and not others) and `v15` spans two. Every
field is therefore `Option`, keyed on its own presence and nothing else. Do not
write `if version >= 16 { … }`.

> **Trap, already live.** `modeltype` is written **both ways in the same
> corpus**: `<modeltype>0x9ea3031a</modeltype>` and
> `<modeltype>2661483290</modeltype>`. This is the AGENTS.md address-spelling
> trap in a new place, and it applies to every `u32` field in the grammar. The
> decoder must accept `0x`-prefixed hex **and** bare decimal and must never
> guess. (`tools/spore/…` and `spore_core::parse_id` exist for the id case.)

### 1.2 Block — `<blockref>`, 680 of them

| element | occurrences | meaning |
|---|---|---|
| `blockid` | 680 | `group, instance` of the part's **rigblock** record |
| `transform` | 680 | see §1.3 |
| `snapped` | 680 | boolean, present on every block |
| `childlist` | **520** | absent on 160 leaves — absence means "no children", never "empty list" |
| `childid` | 601 | decimal **index into this document's own `blockref` list** |
| `symmetric` | 84 | decimal index of the mirror partner block |
| `paintlist` | 80 | `bld` 8, `ufo` 15, `vcl` 57 — **never in `crt`** |
| `handles` | 119 | `weight@channel` entries, 252 total |
| `userorientation` | 11 | `vcl` only, inside `transform` |
| `blockbbox` | 10 | the `formatversion 11` document only, inside `transform` |
| `limbtype` | 5 | inside `<transform>`, `crt` + `flr` only — see below |
| `musclescale` | 5 | inside `<transform>`, `crt` + `flr` only |
| `basemusclescale` | 8 | inside `<transform>`, `crt` + `flr` only |

The last three were **missed by the first census** and by two of the six
reports: they are in the corpus, they are `crt`+`flr` only, and a model that
omits them silently drops 18 facts. A parent→child census over all 68 documents
also places them inside `<transform>`, **not** inside `<blockref>` — the first
draft of this table had them one level too high. Semantics UNKNOWN (a
limb/socket classification and two scalars); preserved as opaque values.

**The graph is measured, not assumed.** `childid` is an index into the same
document's block list. 601 edges, 0 dangling, 0 self-edges, 0 cycles, max
in-degree 1 → a **forest**: 79 roots over 68 documents (62 single-root, 4×2,
1×3, 1×6). Consequences we must honour:

- There is **no single root field.** A `root: BlockIndex` would be wrong for 9%
  of the corpus. Roots are derived: *a block no other block lists as a child*.
  This is a **method, not a struct field** — storing it would be a second source
  of truth by construction.
- `<childlist>` order is **not** meaningfully descending: 507 of the 520 lists
  hold exactly one entry, so the apparent descending order in the samples is an
  artefact of the few multi-entry lists. We preserve written order and never
  sort it, and we make no claim about it.
- **160 leaves carry no `<childlist>` element at all.** `Option<Vec<BlockIndex>>`
  where `None` = "not written" and `Some(vec![])` = "written empty". Collapsing
  them destroys 160 facts. (`05-serialization.md` makes the same point
  independently and calls the test
  `a_leaf_writes_no_children_key_and_an_empty_list_writes_an_empty_array`.)
- **`<symmetric>` is reciprocal in 84 of 84 relations**, and 76 of the 84 sit on
  leaves. It is a partner index, and it is stored on *both* sides — so a
  document that recorded it on one side only would be a different, unobserved
  state. Preserve both and validate reciprocity as a check, not a rewrite.

### 1.3 Transform

Every block carries the same five fields. `position`, `scale`, `orientation` are
present in all 680; `trianglepickorigin` in **670** (absent in exactly the 10
blocks of the one `formatversion 11` document — a version-linked absence, so
`Option` is mandatory, not defensive).

- `position` — 3 floats, the block's offset. Local-to-parent vs absolute is
  **UNKNOWN** from the records (`07-editor-data.json` G-04 in
  `03-transforms.md`); we store local and compose at use.
- `scale` — a **scalar**. Measured range over 680 blocks: **0.28 … 2.254181**.
  There is no vector scale anywhere in the grammar, so **non-uniform scale is
  absent from the corpus** and stays out of the model.
- `orientation` — 3 rows × 3 floats, a row-major 3×3. This is the
  `(Vector3, Matrix3, float)` triple `06-semantic-evidence.md` confirms in the
  binary's own part structures, byte-exact at 36-byte gaps. The game's
  *runtime* orientation is a quaternion; the save format is a matrix. We store
  the matrix and never a second representation (quaternion→matrix is lossy at
  1.131e-06, i.e. exactly at this repo's existing 1e-6 differential tolerance).
- How many blocks carry a *general* orientation is **tolerance-dependent and
  must not be quoted as a single number**: 92 at the tolerance
  `03-transforms.md` used, **140 at this repo's 1e-6**, 188 comparing the text
  as written. The first draft of this document quoted 92 flatly; that was
  wrong. What survives every tolerance is the qualitative fact: a general
  rotation is **not** expressible in the engine's current `rotation_y_degrees`,
  and all *roots* are identity-or-pure-Y (though that too fails at 1e-6 on one
  root, by 1.0e-06 — so treat "roots are simple" as a tendency, not a law).
- `triangledirection` / `trianglepickorigin` — picking aids. `triangledirection`
  is present in all 680. **They disagree with `orientation` in 102 of 680
  blocks**, so they are three separate facts and must stay three separate
  fields. Merging them deletes 102 findings.
- `handles/weight@channel` — 14 distinct channel words across the corpus
  (`0xd02751de` ×85, `0x2e6521f4` ×56, `0x7cc96c02` ×46, …). A named-channel
  weight vector, addressed by an opaque word. **What the channels mean is
  UNKNOWN**; the channel word is an opaque key, not an enum we may name.

> **Integration point, in flight elsewhere — and a collision.** Two things landed
> in `spore-assets` while this document was written, commit `5449fe3`
> "assets: where a Spore creature's colour actually is":
>
> * `crates/spore-assets/src/color.rs` **decodes the colour-bearing fields of
>   the same `<sporemodel>` grammar this crate decodes in full**. That is two
>   implementations of one record, which is the failure mode
>   `docs/ASSET-PATH.md`'s cross-cutting rule exists to prevent. It is not a
>   merge conflict; it is a design duplication, and it is worse because both
>   sides are correct today and will diverge on the next grammar change.
>   **Recommended resolution:** `spore-creator-core` becomes the single owner of
>   the document grammar, and `spore-assets::color` is re-expressed as a
>   projection over [`crate::document`] types (the dependency direction is
>   acyclic: `spore-assets` → `spore-creator-core` → `spore-core`). Not done
>   unilaterally — the other workstream owns that file and the decision is the
>   maintainer's.
> * `docs/COLOUR-SOURCES.md` and the shipped creature skin shader raise the
>   evidence on §1.4 from INFERRED to OBSERVED for the *mechanism*, and settle
>   the record count at **134** rather than 68: `Spore_Game.package` and
>   `PatchData.package` both carry the 68 documents (67 shared by identity, with
>   `cll`/`ufo`/`vcl` instances differing under the patch).

### 1.4 Paint — real, in-document, and creature-free

This is the most consequential correction to the prior plan, and it came out of
the corpus rather than out of a design:

```xml
<paintlist count="3">
  <paint>
    <paintregion>0x00000005</paintregion>
    <paintid>0xf5967f30</paintid>
    <color1>0.450979,0.450979,0.450979</color1>
    <color2>0.749020,0.749020,0.749020</color2>
  </paint>
```

- **Per-block colour exists in the authoring data.** 80 blocks carry it. So the
  blocker `04-materials-and-paint.md` identified in the *runtime* layer (one
  material id per record, `ModelDefinition.materials` hanging off the record)
  is real but it is a *projection* problem, not a reason to keep paint out of
  the document. Paint belongs in the document because the game puts it there.
- **195 `<paint>` entries** corpus-wide: `bld` 20, `ufo` 35, `vcl` 140.
- `paintregion` — small hex words, **23 distinct values** across the corpus
  (`0x01`, `0x04`…`0x09`, `0x10`, `0x11`, `0x13`, `0x20`, `0x21`, …). A region
  index or mask. **Meaning UNKNOWN.**
- `paintid` — an opaque word; only **6 distinct values** corpus-wide
  (`0xf5967f30` ×175 dominant). Looks like a swatch/pattern id.
  **Meaning UNKNOWN**, and `plt` records number **0 in all 14 packages** scanned,
  so there is no palette record to join against.
- `color1`/`color2` are **two** RGB triples per region. **OBSERVED, and the
  mechanism is settled**: the creature skin shader ships as readable source in
  the `smt` record type (`0x0469A3F7`) and consumes
  `struct { float4 mColor1; float4 mColor2; float4 mParams; } editorColors`,
  lerping `mColor1.rgb` over the diffuse by `mColor1.a` and `mColor2.rgb` by
  `mColor2.a * diffuseTexture.a`. So **the raster supplies WHERE each colour
  goes and the record supplies WHAT the colours are** — which is why the texture
  path alone renders every creature uniform grey, and it is not a texture bug.
  Two open pieces survive: the shader's **fourth** component has no XML field
  (the document writes three), and `docs/COLOUR-SOURCES.md` keeps
  `paintregion` bits and `paintid` resolution as `Fact::unavailable`.
- **`crt` documents contain zero `paintlist`.** Creature appearance lives in
  `skincolor1..3` + `skineffect1..3` + `skineffectseed1..3` at document level,
  and per-part variation is therefore **UNKNOWN** for creatures. This is a real
  domain difference between creature and vehicle/building authoring, and it is
  the clearest example on the corpus of why the shared core must model
  *absences* rather than assume one shape.

### 1.5 Part identity, and the resolver — **SOLVED after review**

`blockid` is `group, instance` — no type. Those pairs name records of exactly
one type, **`0x5CAA5E28`**, in `Spore_Game`: 1549 records in 8 distinct sizes
(`36` ×212, `48` ×330, `84` ×465, `228` ×369, `804` ×128, `3108` ×37, `12324`
×7, `49188` ×1) across group families `0x40616000` cell 58, `0x40626000`
creature 482, `0x40636000` building 224, `0x40646000` vehicle 540, `0x40666000`
flora 107, `0x40686000` unknown 138.

**The record is not a model reference, and my first reading of it was wrong.**
`08-rigblock-tail.md` resolved it: the layout is `size = 36 + 12·4^k`,
`k ∈ 0..7`, verified on 1549/1549. It is a **quantised spatial subdivision of a
bounding box** —

- `float[0..2]` = component-wise **MAX** of the per-cell low corners,
- `float[3..5]` = component-wise **MIN** of the per-cell high corners,
- `float[6]` = the quantisation step `s`,
- `u8[0x1c..0x24]` = `4` for `i < k` else `0`,
- then `4^k` cells × 3 bytes, `byte_j = (qLow[j] << 4) | qHigh[j]`,
  distances to the record's global extremes **in units of `s`**;
  `cellLow = f32[j] − qLow·s`, `cellHigh = f32[3+j] + qHigh·s`.

Field order was proved by falsification: the wrong axis order yields **234 428**
violations over 444 264 axis tests, the writer's own mapping yields **0**, and
`min(qLow) == 0` and `min(qHigh) == 0` on all three axes in **1337/1337**
records. The writers are `FUN_0041aaa0` (write) and `FUN_0041c1d0` (bake into
`Editors_BlockData.package` as `BlockData/0x%08x!0x%08x.blockdata`), with
`param_1[i+5] = 4` the hardcoded source of both the `4^k` scaling and the `0x04`
header bytes. **Caution:** Ghidra inverted two of the four leaf helpers —
`FUN_0041bfb0` is MAX and `FUN_0041c0c0` is MIN; the wrong version is
self-consistent and yields a negative scale.

So the mesh link is **not** in this record. Four hypotheses were tested and the
answer is elsewhere:

| hypothesis | verdict |
|---|---|
| (a) bbox pairing, sorted-order correspondence | **REFUTED** — 0 matches over 1 169 644 family-matched pairs; nearest residuals 0.24–4.3, no cluster |
| (b) arithmetic relation between rigblock and gmdl ids | **REFUTED** — best modular agreement 234/482 and 264/540, both *below* chance; instance-id intersection 0 |
| (c) the `prop` (`0x00B1B104`) at the rigblock's own key | **not a gmdl map** — it exists for 1531 of the ids and 0/1531 contain a gmdl type word |
| (d) a record that carries both a family group word and a gmdl type word | **HIT — and it is the link** |

**The real chain, SUPPORTED:**

```text
blockid (group, instance)
  → prop 0x00B1B104 @ the SAME (group, instance)     1531/1549 present
    → { instance = self, type = 0x2F4E681B, group = 0x40606000 }
      → group 0x40606000 holds 1824 × 0x01C135DA (skinned mesh: float3 vertex
        stream + bone block) and 1824 × 0x2F4E681B, identical id sets
        → 1316 / 1549 rigblock ids have a mesh counterpart
```

Method note worth keeping: the extraction form
`{instance, type, group, 0x00A00CE4}` was **validated first** against
`prop` @ `0x40626200`, where 97 records yielded 388 gmdl keys and **388/388
resolved** against the real `Spore_Content.package` index — a positive control
before trusting it anywhere else.

Open on this chain: level→parameter semantics, `0x01C135DA`'s vertex layout,
`0x2F4E681B`'s identity, the **233 rigblocks with no mesh counterpart**, and
flora as a category (107 flora rigblocks, **0** `0x40606000` counterparts). A
single Wine trace of one known `blockid` would settle the whole chain.

So the resolver stays a **failable** value, not a conversion — but it is now a
resolver with a measured mechanism and a measured coverage:

```text
Resolved(ResourceKey) | Ambiguous(Vec<ResourceKey>) | Unavailable { reason }
```

`Unavailable` carries a reason from a closed vocabulary, is never a zero, an
empty list, or `false`, and a block that does not resolve is **reported next to**
the composed result, never dropped — the policy `spore_assets::graph` already
applies to a dangling edge. **Not invented binary formats, no fabricated
attachment rules**: the two remaining decode targets (`0x01C135DA` skinned mesh,
`0x2F4E681B`) are *existing* record types to be read, not formats to be made up.

---

## 2. What is shared, and what is not

| concept | verdict | evidence |
|---|---|---|
| part instance record | **SHARED, CONFIRMED** | one XML grammar, one `cEditorResourceBlock` (472 B, generically named), six families |
| block transform `(Vec3, Matrix3, f32)` | **SHARED, CONFIRMED** | present in all 680 blocks of all 6 families |
| parent/child forest | **SHARED, CONFIRMED** | `childid` index in all 6 families |
| symmetry as a *partner index* | **SHARED, OBSERVED** | `<symmetric>` 84 (cll/crt/ufo/vcl); `mpSymmetricRigblock`/`mpAsymmetricRigblock` + `isAsymmetric` in the binary |
| paint regions + colours | **PARTIAL** | `bld`/`ufo`/`vcl` yes (195 `<paint>` entries); `crt` **no**. Creature appearance is document-level `skincolor1..3` instead — and the shipped skin shader reads both through one `editorColors` struct, so the *mechanism* is shared even though the *placement* is not. |
| handles / named weight channels | **SHARED, OBSERVED** | 119 blocks, 14 opaque channel words |
| snapping, sockets, magnet rules | **NOT IN ANY DOCUMENT** | nine `fixed_vector<ResourceKey,8>` snap/align/interact lists exist in `Editors::EditorRigblock` in **code only**; **zero bytes** in the corpus. UNKNOWN. |
| editor *behaviour*, undo policy, UI | **NOT DEMONSTRABLE** | 469 `/Spore/Editors/*` types, zero mention of vehicle/building/UFO |
| vehicle skinning | **differs** | vehicles: 268/285 blend indices, **0** bone ranges; creature gmdl: 0/582 decodable (619 refused for shader id `0x218`) |
| UFO rigblocks | **borrowed** | UFO documents use vehicle-family blockids (`0x40646000`) |

The rule that follows: **the core models what all six families do, and gives
every family-specific fact a place to live without pretending it is shared.**

The correct framing of the shared/separate verdict — the review's wording, which
is better than the first draft's "data vs behaviour" — is **"what the corpus can
read"**: the document grammar is legible and identical across all six families,
while editor *behaviour* is compiled into code that no symbol in the binary
connects to more than one domain. That is the boundary the Creator Core sits on,
and it survived four separate attempts to refute it.

---

## 3. Crate layout

New crate, because `spore-engine`'s domain module cannot be depended on without
dragging Bevy into every headless consumer, and because the dependency has to
point the other way (the core projects *into* engine types).

```text
crates/spore-creator-core/     docs-only + stdlib + spore-core.  No Bevy, no GPU.
  src/document.rs              the domain model (plain Rust)
  src/sporemodel.rs            the corpus `<sporemodel>` decoder
  src/resolve.rs               blockid -> geometry, the failable resolver
  src/validate.rs              structural checks
  src/edit.rs                  the six authoring operations + the index-space remap
  src/history.rs               undo/redo
  src/codec.rs                 our own save format
  src/claims.rs                what we do NOT know
  src/error.rs
```

Dependency list: `spore-core` only (plus `thiserror`, matching the other
crates). `#![forbid(unsafe_code)]`. Bevy-free and GPU-free by construction, like
`spore-core` … `spore-material`, and like them it gets a `claims` module.

The projection into the engine lives in **`spore-engine`**, never in the core:
the core may not know engine types. `spore-engine::domain` gains one module and
one additive change (below). Nothing above `loader.rs` changes.

### 3.1 The one additive change to the existing domain

`Placement` (`crates/spore-engine/src/domain/model.rs:195`) is
`translation + rotation_y_degrees + uniform scale`. It cannot express a general
block orientation: **92 of 680** blocks carry a general rotation, while all 79
roots are identity-or-pure-Y.

Options considered:

- *narrow the existing field* — rejected: `rotation_y_degrees` is read by
  `cli.rs`, `loader.rs` and `domain/scene.rs`; changing its meaning changes
  rendered output for existing callers.
- *add fields to `Placement`* — viable, and `03-transforms.md` compiled and ran
  exactly this shape against every existing construction and read site
  (`cli.rs:183/192/200`, `tests/material_wiring.rs:427`,
  `domain/scene.rs:223-229`, `loader.rs:99-102/598-602`) with all of them
  passing unchanged.
- *a sibling `BlockPlacement` type* — also viable; `01-domain-model.md`
  prefers it because `Placement { … }` literals exist.

**Chosen: additive optional fields on `Placement`** (`rotation: Option<Row3>`,
`pivot`, `mirror: Option<Symmetry>`), because one placement type means the
engine's scene path and the creator's compose path cannot drift. The review
found the decisive fact that settles it: **all four `Placement { … }` literals
use `..Placement::default()`**, so adding a field touches **zero** call sites,
`Default` stays identity, and every existing read compiles.

**And it must not become a second answer inside `Placement`.** `loader.rs` reads
`rotation_y_degrees` unconditionally, so `rotation: Some(m)` alongside a
non-zero legacy value would silently render the legacy value. Two guards are
mandatory, both from `03-transforms.md`, and the first draft of this document
dropped them:

- an `is_legacy_expressible()` predicate — false when a general `rotation` is
  present;
- an `effective_rotation_y_degrees()` accessor — so exactly **one** field is
  authoritative for any given placement and the other is provably unused.

`rotation_y_degrees` is retained for the engine's own convention; `rotation` is
additive and wins whenever present. Never both.

---

## 4. The domain model

Sketch; the code is the authority. Vocabulary follows the corpus so a reviewer
can grep one name across data, code and binary.

```text
CreatorDocument
  format_version: Option<u32>            absent = not written
  properties:     DocumentProperties     model_type, name, description, skin effects/colours/seeds,
                                            required_pack, zcorp_score, has_thumbnail, cache_id
  blocks:         Vec<Block>             ORDER IS the childid index space
  // roots is a METHOD over `blocks`, never a field

Block
  block_id:  BlockId                     (group, instance) of the rigblock record
  transform: BlockTransform
  snapped:   Option<bool>
  children:  Option<Vec<BlockIndex>>     None = not written; Some([]) = written empty
  symmetric: Option<BlockIndex>          mirror partner index
  paint:     Option<Vec<PaintRegion>>    absent in all 47 crt documents
  handles:   Option<Vec<ChannelWeight>>  opaque channel word + f32
  limb:      Option<Limb>                limbtype/musclescale/basemusclescale, crt+flr only
  user_orientation: Option<Row3>          vcl only
  bbox:      Option<(Vec3, Vec3)>        formatversion 11 only

BlockTransform
  position: Vec3, scale: f32, orientation: Row3,
  triangle_direction: Option<Vec3>, triangle_pick_origin: Option<Vec3>
```

Every value that carries an evidence grade in the corpus is stored with its
grade, so a later finding raises the grade instead of changing the bytes:

```text
Claim<T>  = { value: T, evidence: EvidenceLevel }      // VERIFIED … UNKNOWN
Claim<Unset> = absent                                   // "not written"
```

Three decisions worth defending:

- **`blocks: Vec<Block>` and "block order IS the index space."** The corpus
  addresses blocks positionally. A `BTreeMap<BlockId, Block>` would be a second
  identity scheme; `05-serialization.md` independently declined to invent a
  stable part id for the same reason. A manipulation command therefore carries
  `Vec<BlockIndex>` **plus** the `block_id`, so a stale index is *detectable*
  rather than silently wrong.
- **`Option<T>` means "not written", never "empty" and never "default".**
  Verified cases: 160 blocks with no `childlist`; 10 with no
  `trianglepickorigin`; 10 with `blockbbox`; 11 with `userorientation`; 47
  documents with no `paintlist` anywhere. All five absences are load-bearing.

---

## 5. Serialization, undo/redo, and the save→load guarantee

Adopted from `05-serialization.md`, which corrected two of my premises:

- **`serde` is already in `Cargo.lock`** (via `bevy_asset→ron`,
  `bevy_ecs`/`bevy_reflect`), so the argument against it is behavioural, not
  about dependency count. `serde_json` is genuinely absent and costs 3 crates —
  and `serde_json` writes **`null` for NaN/±Inf** (`ser.rs:169-179`), which in
  this format means *not written*. The library's error path would manufacture
  the exact substitution this repo exists to prevent.
- Both halves of a hand-written format **already exist in-tree**:
  `crates/spore-tools/src/json.rs` is a writer that cannot emit hash order, and
  `crates/spore-semantic-bridge/src/json.rs` is a fail-closed reader that
  rejects unknown fields, missing required fields and repeated keys.
- Format: canonical JSON Lines, `spore-creator-doc/1`. Writer and reader
  hand-rolled. Floats via `format!("{:?}", v)`, f32 only — **all 4 278 190 080
  finite f32 bit patterns** verified to round-trip write→parse, 0 failures.
- The game's own binary container (magic `0x48657ed3`, versions 3/4/5, 472-byte
  block stride, reader `FUN_004bf9f0`) is **CONFIRMED** and may be read later.
  It is not writable in v1: a fixed 472-byte stride has no room for an
  `Unclaimed` residue field, so an export would be lossy by construction. **No
  new binary format is invented.**
- Round trip is asserted on the **value**, never the bytes: the corpus writes
  `\r\n` and 2-space indent, which is observed, not a contract. `PartialEq`
  compares `f32` by `to_bits()`, or the property is untestable.
- Determinism: no timestamp, no PID, no absolute path, no hash-map iteration.
- **Undo/redo is snapshot-based**, before/after whole-field snapshots — which is
  what the original does (two parallel stacks plus a monotone index with
  redo-tail truncation, CONFIRMED in `06-semantic-evidence.md`). Inversion is
  total and free. Any transaction that changes the block *count* carries
  whole-array snapshots, because block order is the index space.
- Document and history serialise to **two files**, so undoing cannot change the
  document's bytes.

### 5.1 The six authoring operations, and the index-space policy

`src/edit.rs`. Six methods on `CreatorDocument`, each of which either does what
it says or returns a typed `EditError` and leaves the document **byte-identical**.
There is no repair path, no clamping, and no `MaybeUninit`-shaped hole: an editor
that silently fixes a bad request cannot answer the only question an undo stack
and a corpus test both need answered, which is what the tool actually did.

| op | signature | policy |
|---|---|---|
| add | `add_block(&mut self, Block, Option<BlockIndex>) -> EditResult<BlockIndex>` | **appends**; index is `blocks.len()` |
| remove | `remove_block(&mut self, BlockIndex, OnLastChild) -> EditResult<IndexRemap>` | **compacts** + full remap |
| replace | `replace_block(&mut self, BlockIndex, BlockId) -> EditResult<()>` | one field, everything else bit-for-bit |
| reparent | `reparent_block(&mut self, BlockIndex, Option<BlockIndex>, OnLastChild) -> EditResult<()>` | moves the edge; refuses a cycle |
| transform | `set_block_transform(&mut self, BlockIndex, BlockTransform) -> EditResult<()>` | whole transform, never derived |
| paint | `set_paint(&mut self, BlockIndex, Option<Vec<PaintRegion>>) -> EditResult<()>` | `None` omits, `Some(vec![])` writes |

**The index-space policy: a complete remap, not a tombstone.** F14 makes block
order *be* the index space and reordering not a valid edit, so removal renumbers
everything after the hole. `remove_block` compacts the array, rewrites **every**
index-valued reference in the document — each `<childid>` naming the removed
block is dropped, each `<childid>` *above* it shifts down by one, each
`<symmetric>` naming it is cleared and each above it shifts — and returns an
`IndexRemap` with an entry for **every** index of the pre-removal document, so
the remap cannot have a gap. `Remap::Moved(n)` and `Remap::Removed` are two
answers rather than an index and a sentinel, because "this names nothing now"
must never be spelled as a number.

A tombstone (a dead slot, nothing shifts) was **not** chosen, and the difference
is pinned by a test rather than left to prose. Three reasons: it needs a second
meaning for a slot `Block` does not have, which leaks into `roots()`,
`parent_of()` and `validate` — the query layer shared with the decoder; it
produces a `<blockref>` with no `<blockid>`, a shape the corpus never shows, so
the eventual writer would need a second answer; and it moves the cost instead of
removing it, because every caller-held index would still need to know which slots
are dead. `add_block` needs no remap precisely because it appends: insertion at a
slot *is* an ordering change, so `insert_at` does not exist and its absence is
the policy.

**The two `None`/`Some` edges stay distinct, in the signature rather than in a
default.** Exactly one op may create a `<childlist>` — `add_block`, always with
at least one child in it. Exactly one thing empties one — a removal or a
reparent — and because the corpus expresses "leaf" by *omitting* the element
(160 of 680), both take an explicit `OnLastChild` (`KeepWrittenList` → the
`Some(vec![])` shape `validate` reports as `empty_child_list`; `OmitElement` →
the corpus's leaf shape). `set_paint` takes `Option` for the same reason.

**What the ops refuse to do.** They do not touch a transform on a reparent:
whether `<position>` is parent-local or absolute is still the open
`transform_frame` question, so a rescale would be an answer invented in a
function with no evidence for one (`reparent_transform_consequence` in
`claims.rs`). They do not repair — pre-existing problems are `validate`'s to
report. They do not re-derive `symmetric`; reciprocity is a check, not a
rewrite, and the one place removal *must* touch it (a partner pointing at a block
that no longer exists) clears the reference and **reports which blocks it
cleared** via `IndexRemap::severed_symmetry`. And a reparent **moves** a block
that already has a parent rather than refusing: max in-degree is 1 corpus-wide,
so the old edge is removed and the new one added in one call and the invariant
never transiently breaks.

`EditError` lives beside `CreatorError` in `error.rs` and is a **separate type**:
a decode failure is a finding about bytes on disk, an edit refusal is a finding
about what the caller asked for, and they must not share a `Display`.

**Measured over the real corpus** (68 documents, 680 blocks, `cargo test -p
spore-creator-core --release -- --ignored`): all 680 blocks are removed once per
`OnLastChild` policy (1360 removals), and each is checked for the exact edge
delta, a complete remap, and that every surviving block equals the original with
each reference carried by the remap. All 680 have their part swapped with the
other nine fields asserted equal; all 680 have their transform rewritten; every
document has a block appended and a subtree reparented; and a full
add → replace → transform → paint → reparent → remove session returns all 68
documents to their original block count, edge count and root set. Zero
`Severity::Error` issues after any op.

---

## 6. Status of the vertical slice

```text
blockid → rigblock 0x5CAA5E28                     RESOLVED (bounding subdivision)
       → prop 0x00B1B104 @ same key                RESOLVED (1531/1549)
       → 0x2F4E681B / 0x01C135DA @ 0x40606000      RESOLVED as a reference (1316/1549)
       → skinned mesh vertices + bones              NOT DECODED  ← the remaining work
```

So the authoring loop is buildable now and the render path is one decode away:

1. **Buildable now.** Catalog the 68 real documents, parse them, compose the
   forest, compute world transforms, save, load, verify equality. All
   corpus-verified, no rigblock decode required.
2. **One decode away.** Rendering a block needs `0x01C135DA` (skinned mesh:
   float3 vertex stream + bone block) and `0x2F4E681B`. Both are **existing
   record types to be read**, in the spirit of the 77 promoted `pkg_*` model
   tests — graded against the 1824 real records, not invented. Until then a
   composed creature renders as *resolved blocks + an explicit unresolved
   report*, never as a silently missing part and never as a placeholder mesh
   pretending to be a body part.
3. **Known coverage gaps to report, not hide:** 233 rigblocks with no mesh
   counterpart, and flora at 0/107.

Note for the reader: `target/release/osptool` predates the uncommitted
`spore-tools` source and has no `stats`/`graph`/`texture` subcommands. Figures
above come from the committed Python oracle (`tools/spore/dbpf/dbpf.py`), from
my own XML/DBPF census, and from the cited reports — not from that stale binary.
One further correction from the review: `AGENTS.md`'s `~/apps/Spore-ModAPI` path
does not exist on this machine.

---

## 7. Build order

Reordered by the review: the **resolver precedes the engine projection**,
because projecting two empty scenes into each other proves nothing.

| # | Deliverable | Gate |
|---|---|---|
| 0 | `spore-json`: the in-tree JSON writer + strict reader **lifted** out of `spore-tools` / `spore-semantic-bridge` | one extraction, not a fourth implementation; the two existing users keep their byte output |
| 1 | `spore-creator-core`: domain types + `claims` + hermetic tests | `cargo test -p spore-creator-core` green, no Bevy in the tree |
| 2 | `<sporemodel>` decoder for the verified grammar | 68/68 real documents parse; `#[ignore]`d corpus test, clean skip without `SPORE/` |
| 3 | structural validation: index range, cycles, in-degree, symmetry reciprocity, per-field presence | hermetic, synthetic fixtures; **no version gate** (§1.1) |
| 4 | the failable resolver, `Unavailable { reason }` closed vocabulary | reports coverage honestly: 1316/1549 have a mesh counterpart, 233 do not, flora 0/107 |
| 5 | `spore-creator-doc/1` writer + fail-closed reader | `save→load == doc` by `to_bits()` |
| 6 | snapshot undo/redo | invertibility + no-op-undo tests |
| 7 | `spore-engine`: additive `Placement` fields + guards + projection into `Scene`/`Creature` | existing engine tests unchanged; `effective_rotation_y_degrees` is the only reader |
| 8 | `spore-differential`: Rust creator document vs the Python oracle over all 68 records | 68/68 identical documents |
| 9 | decode `0x01C135DA` skinned mesh + `0x2F4E681B` (separate workstream, own package) | graded against the 1824 records |
| 10 | Bevy preview of a *composed* document | headless via `MinimalPlugins` only |
| 11 | interactive editing (selection, gizmo, snapping) | **not started, by design** |

Steps 1–6 are pure data work, fully hermetic, and are the deliverable this
milestone. Step 7 is the invariant the brief demands: creator output and runtime
input share one semantic representation, because the projection target is the
*same* `domain::Placement` the engine already consumes — and by then it is
projecting a non-empty document, which is the whole point of doing the resolver
first.

---

## 8. Explicitly not built yet

Not because they are hard, but because the model behind them is not understood,
and each was explicitly rejected by the research as speculative:

skeleton / bones / animation binding · socket and snap-point enums · a part-kind
taxonomy · non-uniform scale · negative scale for mirroring · deformation
parameter semantics beyond the opaque channel weights · creature per-part paint
(the corpus has none) · undo coalescing · a second representation of rotation ·
any claim about `paintregion` or `paintid` meaning · any binary writer · any
editor UI, gizmo or selection model.

---

## 9. Evidence index

| claim | grade | source |
|---|---|---|
| 68 creator documents, 6 types, 680 blocks, all parse | **VERIFIED** | my DBPF+XML census over `Spore_Game.package`; `01`, `02` |
| the 68 documents are **corpus-complete** | **VERIFIED** | `07` §10 — this closes `01` §8 Q7 |
| 601 edges, 0 dangling/cyclic, max in-degree 1, 79 roots | **VERIFIED** | `01` (two independent methods) |
| 160 blocks have no `childlist` element | **VERIFIED** | my census; `01`, `05` |
| grammar census (§1 tables) | **VERIFIED** | my census; `07` spot-checked 30 items |
| `formatversion` does **not** gate field presence | **VERIFIED** | `07` §9.3 — v16 and v15 each span two presence classes |
| `<symmetric>` reciprocal 84/84; 76/84 on leaves | **VERIFIED** | `07` §6 |
| `limbtype` 5, `musclescale` 5, `basemusclescale` 8 | **VERIFIED** | `07` §5 — missed by the first census and by `02` |
| paint is in-document, per block, `crt`-free | **VERIFIED** | my census; `04` |
| all 60 `blockid`s also name a `prop` with UTF-16 text | **OBSERVED** | `07` §7 — refutes "there is no name anywhere" |
| `cEditorResourceBlock` 472 B, `cEditorResource` 172 B | **CONFIRMED** | `06-semantic-evidence.md` |
| authored part transform is `(Vec3, Matrix3, f32)` | **CONFIRMED** | `06` + my grammar |
| symmetry is a partner index, not a transform | **OBSERVED** | `03`, `06`, my census, `07` |
| one material id per record; 4208/4208 single-id | **VERIFIED** | `04` |
| `RegisterShaderData` has 72 sites, `0x218` in none | **VERIFIED** | `04` — `spore-gmdl`'s refusal is faithful |
| 1549 rigblock records, 8 sizes, `size = 36 + 12·4^k` | **VERIFIED** | `08` on 1549/1549 |
| rigblock = quantised bounding subdivision, field order proved | **VERIFIED** | `08`: 234 428 violations for the wrong order, 0 for the right one |
| `blockid → prop → 0x40606000 mesh`, 1316/1549 | **SUPPORTED** | `08` §5, with a 388/388 positive control |
| bbox-pairing and arithmetic-relation hypotheses | **REFUTED** | `08`: 0/1 169 644; 234/482 below chance |
| `0x01C135DA` skinned-mesh vertex layout, `0x2F4E681B` identity | **UNKNOWN** | `08` §6 — the remaining decode |
| 233 rigblocks with no mesh counterpart; flora 0/107 | **OBSERVED** | `08` |
| type `0x1A99B06B` (`bem`) carries the confirmed `cEditorResource` magic at v5 | **OBSERVED** | `07` §7 — 1709/1709, but 0/1709 fit the body layout. Cheapest unexplored lead. |
| snap/align/interact lists | **UNKNOWN (code only)** | `02`, `06` — zero bytes in the corpus |
| `paintregion` / `paintid` / weight-channel meaning | **UNKNOWN** | `04`, `docs/COLOUR-SOURCES.md`, my census (195 `<paint>` entries) |
| the shader's 4th colour component has no XML field | **UNKNOWN** | `docs/COLOUR-SOURCES.md`, `smt` shader source |
| a Spore creature renders uniform grey from its textures alone | **CONFIRMED** | `5449fe3`: colour lives in the record, not the raster |
| shared editor base class | **NOT DEMONSTRABLE** | `06`: 469 types, zero cross-domain mentions; survived 4 refutation attempts (`07`) |
| local-vs-absolute frame | **UNKNOWN** | `03` (G-04) |
| quaternion↔matrix loss 1.131e-06 | measured | `03` |
| f32 `{:?}` round-trip, all finite patterns | measured | `05` |
| `serde_json` emits `null` for NaN | measured | `05`, `serde_json-1.0.150/src/ser.rs:169-179` |
| no UI is a **scope** decision, not a knowledge ceiling | **VERIFIED** | `07` §11 — type `0xCF6C21B8` ships the game's own UI-definition XML |

---

## 10. The body graph is recovered — and it is not in the mesh

Added 2026-10-04, from `docs/analysis/creator/09`, `10` and `11`. This is the
expedition's main result and it changes the architecture above, so it is stated
here rather than only in the reports.

### 10.1 A part is a PAIR, not a file

The chain now closes one hop further than §3 described. A part is two records at
group `0x40606000` sharing one instance id:

| type | name | holds | crate |
|---|---|---|---|
| `0x2F4E681B` | `rw4` | a RenderWare 4 container; **6270 `Skeleton (0x70002)` sections** in 1568 of 1824 records | `spore-rigblock::skeleton` |
| `0x01C135DA` | `gmsh` | the geometry, whose bones are an **anonymous local ordinal list** | not yet decoded in Rust |

Neither record contains the other's type id (0 hits for `0x01C135DA`, 0 for the
`0x00A00CE4` key tag), so **the join is by instance id alone** and it is the load
that binds anonymous geometry to named bones. Decoding a mesh without its sibling
container yields half a part.

Note also that the two halves live in **different packages**: the rigblock bounds
records are in `Spore_Game.package`, the part pair in `Spore_Graphics.package`,
and the `prop` pointers in between in `Spore_Graphics.package` too. A reader who
opens one file finds half the answer.

### 10.2 The skeleton IS the body graph

`Skeleton (0x70002)` is `0x18 + 12·N` bytes: three `u32[N]` arrays and nothing
else, with **zero unaccounted bytes** in 6270 of 6270.

* array 1 — bone identity
* array 2 — a field whose values lie in `{0,1,2,3}` only (**meaning unknown**)
* array 3 — **parent index**, every value in `[-1, N-1]`, **exactly one `-1` per
  skeleton**, every parent chain acyclic

Measured over the corpus with the Rust decoder: **6270 sections, 1568 records,
15 763 bones, at most 25 bones in a skeleton, and a deepest parent chain of 24.**

That last number is the interesting one. A depth-24 chain under a single root is
a **serial chain**, which is what the SIGGRAPH paper describes; it is not proof
that the chain is a *spine*, and the distinction is recorded rather than glossed.

### 10.3 Bone identity is a hash of a name — FNV-1, not FNV-1a

The 2002 distinct identity words are **FNV-1 hashes of a morph-lattice parameter
name**: multiply then xor, basis `0x811C9DC5`, prime `0x01000193`. It is *not*
FNV-1a, which is how this repository hashes DBPF ids, and swapping the two orderings
yields zero matches where the correct one yields hundreds.

244 identities are recovered, covering **43 % of all bone occurrences**, graded
per name by what actually witnesses them:

| grade | witness | names | bone occurrences |
|---|---|---:|---:|
| `Family` | ≥3 siblings of an indexed family recovered | 95 | 38.0 % |
| `BinaryLiteral` | exact delimited bytes in a shipped `.exe`/`.dll` | 56 | 4.6 % |
| `RecordLiteral` | exact delimited bytes in a decompressed record | 19 | 0.6 % |
| `Unconfirmed` | a single exhaustive hit, nothing else | 74 | 1.4 % |

Chance is accounted for: the exhaustive search covered 62 193 780 candidate
strings against 2002 targets, i.e. **29.0 expected accidental collisions** against
244 recovered. `BoneName::name_if_certain()` returns a name only for the three
witnessed grades, so a caller cannot accidentally print an unproven word as
anatomy — which is exactly how `kiomq` stayed out of a creature's skeleton.

The recovered vocabulary *available* is anatomical (`root`, `body`, `head`, `eye`,
`pupil`, `iris`, `jaw1..3`, `neck1..2`, `shoulder`, `arm1..3`, `hand`, `hip`,
`knee`, `ankle`, `elbow`, `tail`, `wing`, `fin`, `toe`, `weapon`, `mast`),
mechanical (`joint1..9`, `joint20..29`, `base1..6`, `node1..6`, `stem0..5`,
`shaft1..5`, `bolt1..4`, `bulb1..4`, `ring1..3`, `bounce`, `rotate`, `spin`,
`radius`), and positional (`xpos`, `xneg`, `ypos`, `yneg`, `z_up`, `j_up`,
`mid`, `tip`, `top1..4`).

> **CORRECTION (2026-10-04).** The paragraph above describes the vocabulary the
> **search** recovered. It is *not* the vocabulary the creator core **uses**.
> Measured over the 677 bones in the 680 composed `<blockref>` parts, the only
> recovered name that occurs is **`joint1`** — 557 of 677 bones, and nothing else.
> No `hip`, `knee`, `eye`, `body`, `root` or `wing`; no bilateral L/R pair; no
> `attach` or `csnap` in any composed part. The anatomy reading of §10.3 is
> therefore **REFUTED for this layer**: these parts are simple articulated blocks
> named `joint<N>`, and the anatomical vocabulary belongs to a different record.
> See `docs/analysis/creator/11` §11 for the correction and its reasoning.

### 10.4 Two questions the corpus answered, against history

**Symmetry is laterality in the vocabulary, but the creator core does not use
it.** There is no symmetry flag, mirror node or reflection relationship anywhere:
the skeleton payload is three arrays with zero slack. Complete left/right name
pairs *are* in the vocabulary — `wingL`/`wingR`, `sideL`/`sideR`, `hipL`/`hipR`,
`kneeL`/`kneeR` — but **no composed part contains a bilateral pair**, because no
composed part contains any name but `joint1`. So for the creator core, symmetry is
**UNRESOLVED** rather than established; the laterality evidence is real but
belongs to parts outside the 680 the documents place. A "mirror this part" rule
would be invented behaviour either way.

**Attachment is still open, and precisely so.** `attach` (36 bones) and `csnap`
(46) are recovered names — but note that **neither occurs in any of the 677
composed bones**, so they are properties of the wider vocabulary, not of the
creator core's parts. Both would live *inside* one part's skeleton anyway. The other
candidate is the `<blockref>` childlist one level up. No array in `0x70002`
stores a part-to-part reference, so the honest answer today is: the minimal
persisted relationship for composing a creature is the **`<blockref>` childlist
plus the local transform**, and whether a part's own `attach` bone participates is
UNKNOWN. That is the highest-value open question in the whole subsystem, because
it decides whether attachment is data or an editor convenience.

### 10.5 What the paper predicted, and what it did not

`docs/analysis/creator/11` §3 carries the full correspondence table. Summary of the
rows that moved:

| historical concept | corpus | verdict |
|---|---|---|
| body | one `Skeleton` per part container, 1–25 bones | **confirmed** |
| parent | parent array, one root, acyclic, 6270/6270 | **confirmed** |
| serial chain at the root | deepest chain measured at **24** | **strongly supported** |
| caps | bone names recovered as a morph-parameter vocabulary | **confirmed as naming, not as caps** |
| `grasper` | no token hashes to it | **unresolved** |
| rest pose | 10 859 orthonormal transforms, none labelled | **unresolved** |
| 20–80 bodies per character | a part holds ≤9 skeletons; a creature assembles up to 680 `<blockref>` | **consistent, not confirmed** |

The one that must not be over-read: a paper "body" and a rigblock container's
skeleton are **not shown to be the same unit**, and the table says so.

---

## 11. The vertical slice, measured end to end

Added 2026-10-04. New crate `spore-morphology` closes the loop from a shipped
`<sporemodel>` record to a named body graph, with a resolver that reports its own
coverage instead of implying it.

### 11.1 What it runs

```text
Spore_Game.package ─DBPF─> <sporemodel> ─> spore-creator-core
                                                    │ canonical authoring data
                                                    ▼
                                       spore-morphology ─ block resolution + provenance
                                                    │
                                                    ▼
                                       spore-rigblock ─ part pair, skeleton, bone names
```

### 11.2 Measured over the real corpus

From `cargo test -p spore-morphology --release --test slice -- --ignored`:

```
68 documents, 680 blocks -> 678 resolved / 2 unresolved
677 resolved parts carry a body graph; 677 bones, 557 named

  Creature  blocks 486  resolved 486  unresolved 0
  Cell      blocks  79  resolved  79  unresolved 0
  Vehicle   blocks  57  resolved  57  unresolved 0
  Ufo       blocks  47  resolved  47  unresolved 0
  Building  blocks   8  resolved   8  unresolved 0
  Flora     blocks   3  resolved   1  unresolved 2

  stopped at part_pair because no_such_record: 1
  stopped at prop      because wrong_structure: 1
```

Two things a reader must not misread:

* **678/680 at the block layer is not 1316/1549 at the rigblock layer.** The
  680 `<blockref>` blocks name fewer distinct rigblocks than the 1549 records
  exist, and two denominators must not be compared. A test asserts both counts so
  a future change cannot quietly conflate them.
* **Flora's gap is a rigblock-layer fact, not a block-layer one.** Report 08
  measured 0 of 107 flora rigblocks reaching a part; the documents place only 3
  flora blocks, and 1 of them does resolve. "Flora has no parts" is true of one
  layer and false of the other, and the slice reports what it measured.

### 11.3 The resolver's selection rule, measured not assumed

Which instance does a block's part live at? The obvious reading of report 08 §5 —
the `prop` pointer naming the rigblock's own instance — is **wrong for most
parts**, and finding that out cost a body graph on 484 of 678.

Measured over 370 sampled rigblocks: the **first** validated
`{instance, type, group, 0x00A00CE4}` entry pointing at `0x2F4E681B` is the part,
and it frequently is *not* the block's own instance. Example: rigblock
`0xc66a14b8` carries exactly one pointer, to `0x10fe9dc4`, and that is the one
whose container holds a skeleton.

The resolver therefore:

* selects the **first** validated container entry, and
* grades the hop `Measured` only when that entry also equals the block's own
  instance — two signals agreeing — and `Inferred` otherwise.

**485 of 678 real parts report an inferred hop**, and that is the honest number.
It says the selection rule is measured but the choice is unconfirmed per record,
not that 485 parts are questionable.

### 11.4 What the API makes impossible

* **Silently skipping a block.** Every block gets a slot; `resolved +
  unresolved == blocks` is asserted exactly, so a future "skip the unresolvable
  ones" fails the test rather than shipping.
* **Inventing a part.** There is no default mesh, no placeholder body, and no
  substitution path. `UnresolvedPart` carries the block, the block id, the stage
  reached, the reason, and the key that was tried.
* **Confusing "absent" with "undecodable".** `StopReason` separates
  `no_such_record`, `wrong_structure`, `undecodable` and `not_applicable`.
* **Presenting an inferred hop as measured.** `Trail::weakest_grade` is a `max`
  over a rank where `Inferred` outranks `Measured`, so one inferred hop makes the
  whole trail inferred.
* **Debug strings as provenance.** `Trail` is typed fields; `as_provenance()`
  derives `spore_core::Provenance` from the same values the resolver used.

### 11.5 What is still not established

* The 2 unresolved blocks are counted, not diagnosed: one flora block has no part
  pair, one block's `prop` record holds no validated pointer.
* Which of a part's transforms is its **rest pose** — UNKNOWN.
* Whether `attach` / `csnap` bones cross part boundaries — UNKNOWN, and it is the
  highest-value open question (§10.4).
* The `gmsh` geometry is now decoded too — see §12.

---

## 12. The geometry layer, decoded — the part is complete

Added 2026-10-04. `spore_rigblock::gmsh` reads the `0x01C135DA` half of the
pair, so a resolved part now carries vertices as well as a body graph.

### 12.1 Measured over all 1824 records

```
1824 meshes: 1222646 vertices, 1368951 triangles, 973 single-bone, 16 recipes
max per mesh: 18800 vertices, 16578 triangles
46 distinct channel block ids
```

Every figure matches report 09's independent Python measurement exactly, which is
the point of running both.

### 12.2 Three things that are easy to get wrong

**Every block field is big-endian; only the 24-byte header is little-endian.**
`id`, `count` and `elemSize` are all BE. Read LE, the ids come out as a run of
consecutive values increasing by one per block — they look like offsets, not
identifiers — and the index block (id `0x00000001`) never matches at all. The
discriminator is `elemSize = 48`: LE reads 12 288 and overruns immediately.

**The chain length is a formula.** `(recipe >> 8) & 0xFF` matches all 16 observed
recipe versions (3 … 35 blocks) with zero exceptions. This matters because the
tail follows the last block: a decoder that walks "until the bytes run out" eats
the bone tail, and one that trusts a stale block count stops early. Deriving the
count is what makes the boundary knowable.

**The bone-range table is not sorted.** 599 of 1824 records store the ranges out
of order, and only 1225 happen to be ascending. Validated in file order, a tiling
check fails on a third of the corpus while looking entirely reasonable; the
`ranges_by_start()` accessor exists because of that. The `bone` word *is* the
entry index in file order (1824/1824) — so file order indexes bones, and sorting
reorders ranges. Conflating those two is the trap.

### 12.3 What the geometry says about skinning

**Rigid, per triangle, with no weights anywhere.** The bone table is the literal
list `0 … nBones-1` — anonymous local ordinals — and influence is decided by a
half-open index range per bone, which tiles `[0, nIndices)` exactly. 973 of 1824
meshes are bound to a single bone.

This is why the pair is not optional: the geometry record cannot name a bone, and
the container record has no vertices. A part is only complete when both are read,
and the geometry's `bone_at(index)` is what binds a triangle to the *ordinal*,
which the skeleton then resolves to a *name*.

### 12.4 Still not decoded, deliberately

The remaining 46−3 channel blocks are exposed as **opaque blocks with their id and
element size** and no interpretation. Report 09 grades most of them INFERRED
("packed bytes, not float" / "2 floats per vertex" / "float3 channel"), and this
crate does not promote an inference to a decode. `Block::as_f32` returns `None`
for an element size that is not a multiple of 4, and its documentation says
plainly that a 4-byte block is float-*shaped* without being float-*correct* — the
1232 records whose 4-byte blocks are implausible as floats are exactly why.

The fifth word of a bone-range entry is also not a constant: `4` in most entries,
`0x02000000` and `0x03000000` elsewhere. It is carried verbatim as `tag` and
claimed nothing, correcting report 09's "constant == 4, 1824/1824".

---

## 13. Where the anatomy is NOT — and the next question, posed properly

Added 2026-10-04, after §12. This section exists because §11 refuted the anatomy
reading, and a refutation without a direction is just a dead end.

### 13.1 What was ruled out, with the counts

The obvious candidate for "where the anatomy lives" is the whole-body `gmdl`
layer. It is **not** the answer, and the reason is specific:

* **All 4209 `gmdl` records are in `Spore_Content.package`** — none in
  `Spore_Graphics`, `Spore_Game` or `PatchData`.
* **A `gmdl` record is not an RW4 container.** 0 of 4209 begin with `RW4w32`, and
  therefore **0 of 4209 carry a `0x70002` Skeleton section**. The bone graph
  recovered in §10.2 is a property of the *part* layer's `rw4` containers, and the
  whole-body layer does not share it.
* `spore-gmdl` already parses 4209/4209 records and exposes `bone_ranges` from the
  trailer, but **no bone identities**. Those would sit in the anim data or the
  baked deforms, which that crate reads best-effort and does not yet interpret.

So the creature's anatomy is in neither the `<sporemodel>` block layer (only
`joint1`) nor the `gmdl` Skeleton layer (no such sections). It is somewhere this
expedition has not looked.

### 13.2 The three candidates that remain, in order

1. **`gmdl`'s anim data / baked deforms.** The most likely. `spore-gmdl` reaches
   the trailer; the bone identities are probably in the `GameModelAnimData` it
   currently treats as opaque. This is *known work on existing code*, not new
   research — the crate parses 4209/4209 already.
2. **A `BlendShape` / `MorphHandle` pairing.** `docs/RENDERWARE-RESEARCH.md` §11
   records `rw4_11` samples carrying `BlendShape (0xff0002)` +
   `BlendShapeBuffer (0x200af)` and calls them "a morph-heavy creature/creature-part
   model". Morph parameters are the vocabulary the anatomy names come from, and a
   blend-shape set is where per-creature parameter channels would live.
3. **The rigblock's own 4^k lattice.** Report 08 §7 item 1 asks what each
   subdivision level drives. If the levels index morph parameters, then the
   rigblock — not the skeleton — is where a part's parameters are enumerated, and
   the recovered vocabulary would key them. That would make the anatomy a property
   of the *bounds record* rather than the body graph, which would be a genuinely
   surprising and very testable claim.

### 13.3 What would settle each, cheaply

| candidate | the single observation that settles it |
|---|---|
| gmdl anim data | histogram the 32-bit words in the anim-data section of a creature `gmdl` against the 2002-token vocabulary; a hit rate far above the ~29 expected chance collisions answers it |
| blend shapes | read a `0xff0002` section's parameter name table and hash its entries |
| rigblock lattice | correlate a rigblock's `k` and its cell occupancy against the number of morph parameters its part's `gmsh` channels imply |

The first is a few dozen lines against a crate that already parses the records, and
it is the obvious next move.

### 13.4 One thing that is now settled independently of any of this

The **authoring** graph is a chain, measured over the corpus: 520 of 680 blocks
carry a `<childlist>`, 79 derived roots over 68 documents (so some documents are
multi-rooted forests), maximum depth 19. That is consistent with the serial-chain
description in the SIGGRAPH paper at the *authoring* layer, and it depends on no
bone name at all — which is what makes it trustworthy while the anatomical
question is open.
