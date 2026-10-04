# Creator Core — architecture

**Status:** design agreed with the corpus, implementation starting.
**Scope:** the reusable authoring/construction core, plus Creature Creator as its
first client. Not an editor UI.

This document is the synthesis of six independent investigations plus a direct
census of the real data. Every structural claim below is either measured here or
cited to a report in `docs/analysis/creator/`. Where the corpus is silent, the
document says so and the data model preserves the silence.

---

## 0. The finding that reframes the work

**Spore's creator document is not hypothetical. It is in the game data, and it
is text.**

`Spore_Game.package` holds **68 creator documents** — verified by direct census
of the DBPF index, not inferred:

| type id | name | count | group | what it is |
|---|---|---|---|---|
| `0x02B92A46` | `crt` | 47 | `0x40626200` | creature |
| `0x0AC5D2DC` | `cll` | 13 | `0x40616200` | cell |
| `0x246C29D4` | `vcl` | 4 | `0x40646200` | vehicle |
| `0x47764BB7` | `ufo` | 2 | `0x40656200` | UFO (a vehicle-family document) |
| `0x238BC675` | `bld` | 1 | `0x40636200` | building |
| `0x43854287` | `flr` | 1 | `0x40666200` | flora |

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
| `formatversion` | 68 | `11` ×1, `15` ×19, `16` ×58. **A version gate, not a schema.** |
| `properties` | 68 | see below |
| `blocks@count` | 68 | attribute equals the real child count in all 68 |

`properties` children: `modeltype` 68, `requiredpack` 66, `skineffect1..3` 61,
`skineffectseed1..3` 61, `skincolor1..3` 61, `zcorpscore` 59, `name` 9,
`description` 9, `hasthumbnail` 1, `cacheid` 1.

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

**The graph is measured, not assumed.** `childid` is an index into the same
document's block list. 601 edges, 0 dangling, 0 self-edges, 0 cycles, max
in-degree 1 → a **forest**: 79 roots over 68 documents (62 single-root, 4×2,
1×3, 1×6). Consequences we must honour:

- There is **no single root field.** A `root: BlockIndex` would be wrong for 9%
  of the corpus. Roots are derived: *a block no other block lists as a child*.
- `childlist` order in the corpus is **descending** (`8,7,6,4,3,2,1`). Order is
  presentation, not topology; we preserve it as written and never sort it.
- **160 leaves carry no `<childlist>` element at all.** `Option<Vec<BlockIndex>>`
  where `None` = "not written" and `Some(vec![])` = "written empty". Collapsing
  them destroys 160 facts. (`05-serialization.md` makes the same point
  independently and calls the test
  `a_leaf_writes_no_children_key_and_an_empty_list_writes_an_empty_array`.)

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
- `triangledirection` / `trianglepickorigin` — picking aids. `triangledirection`
  is present in all 680. **They disagree with `orientation` in 102 of 680
  blocks**, so they are three separate facts and must stay three separate
  fields. Merging them deletes 102 findings.
- `handles/weight@channel` — 14 distinct channel words across the corpus
  (`0xd02751de` ×85, `0x2e6521f4` ×56, `0x7cc96c02` ×46, …). A named-channel
  weight vector, addressed by an opaque word. **What the channels mean is
  UNKNOWN**; the channel word is an opaque key, not an enum we may name.

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
- `paintregion` — small hex words, ~15 distinct values across the corpus
  (`0x01`, `0x04`…`0x09`, `0x10`, `0x11`, `0x13`, `0x20`, `0x21`). A region
  index or mask. **Meaning UNKNOWN.**
- `paintid` — an opaque word; only **6 distinct values** corpus-wide
  (`0xf5967f30` ×175 dominant). Looks like a swatch/pattern id.
  **Meaning UNKNOWN**, and `plt` records number **0 in all 7 packages**, so
  there is no palette record to join against.
- `color1`/`color2` are **two** RGB triples per region. Two colours per region
  is consistent with a two-layer skin (`skineffect1..3` in `properties`), but
  that is **INFERRED**; we store both, named by position, and claim nothing.
- **`crt` documents contain zero `paintlist`.** Creature appearance lives in
  `skincolor1..3` + `skineffect1..3` + `skineffectseed1..3` at document level,
  and per-part variation is therefore **UNKNOWN** for creatures. This is a real
  domain difference between creature and vehicle/building authoring, and it is
  the clearest example on the corpus of why the shared core must model
  *absences* rather than assume one shape.

### 1.5 Part identity, and the resolver that does not resolve yet

`blockid` is `group, instance` — no type. Measured across the corpus, those
pairs name records of exactly one type, **`0x5CAA5E28`**, in `Spore_Game`:

- 1549 such records; group families `0x40616000` cell 58, `0x40626000` creature
  482, `0x40636000` building 224, `0x40646000` vehicle 540, `0x40666000` flora
  107, `0x40686000` unknown 138.
- The `blockid` groups actually used by the 68 documents: `0x40626000` ×487,
  `0x40646000` ×104, `0x40616000` ×79, `0x40636000` ×8, `0x40666000` ×2.
- **0 of 60 distinct `blockid`s name a `gmdl`.** The chain is
  `blockid → 0x5CAA5E28 record → (encoded tail) → gmdl`, and the tail is not
  decoded.

I sampled that record myself. It is **not** a fixed 36 bytes: sizes range 36 →
8631, with a distinct shape per part kind. The 36-byte minimum is
`6 floats + 12 zero bytes` — a plausible AABB plus an empty tail. Larger records
carry what is plainly a **compact encoding**: repeated `0x10`/`0x20`…`0xf0`
high-nibble rows crossed with `0x11`…`0x55` low-nibble columns, then sparse
`15 00 00 1a 00 00 1f 00 00` triples. That is nibble-packed structured data, not
a plain field table. **Decoding it is a research project of its own and this
document does not pretend otherwise.**

**Therefore:** `blockid → geometry` is modelled as a **failable resolver**, not a
conversion:

```text
Resolved(ResourceKey) | Ambiguous(Vec<ResourceKey>) | Unavailable { reason }
```

`Unavailable` carries a reason from a closed vocabulary. It is never a zero, an
empty list, or `false`. A block that does not resolve is **reported next to**
the composed result, never dropped — the same policy `spore_assets::graph`
already applies to a dangling edge.

This is the one hard blocker on "assemble creature → render", and it is named
rather than papered over. See §6.

---

## 2. What is shared, and what is not

| concept | verdict | evidence |
|---|---|---|
| part instance record | **SHARED, CONFIRMED** | one XML grammar, one `cEditorResourceBlock` (472 B, generically named), six families |
| block transform `(Vec3, Matrix3, f32)` | **SHARED, CONFIRMED** | present in all 680 blocks of all 6 families |
| parent/child forest | **SHARED, CONFIRMED** | `childid` index in all 6 families |
| symmetry as a *partner index* | **SHARED, OBSERVED** | `<symmetric>` 84 (cll/crt/ufo/vcl); `mpSymmetricRigblock`/`mpAsymmetricRigblock` + `isAsymmetric` in the binary |
| paint regions + colours | **PARTIAL** | `bld`/`ufo`/`vcl` yes; `crt` **no**. Creature appearance is document-level `skincolor*` instead. |
| handles / named weight channels | **SHARED, OBSERVED** | 119 blocks, 14 opaque channel words |
| snapping, sockets, magnet rules | **NOT IN ANY DOCUMENT** | nine `fixed_vector<ResourceKey,8>` snap/align/interact lists exist in `Editors::EditorRigblock` in **code only**; **zero bytes** in the corpus. UNKNOWN. |
| editor *behaviour*, undo policy, UI | **NOT DEMONSTRABLE** | 469 `/Spore/Editors/*` types, zero mention of vehicle/building/UFO |
| vehicle skinning | **differs** | vehicles: 268/285 blend indices, **0** bone ranges; creature gmdl: 0/582 decodable (619 refused for shader id `0x218`) |
| UFO rigblocks | **borrowed** | UFO documents use vehicle-family blockids (`0x40646000`) |

The rule that follows: **the core models what all six families do, and gives
every family-specific fact a place to live without pretending it is shared.**

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
  prefers it because `Placement { … }` struct literals exist and a new field
  would touch four call sites.

**Chosen: additive optional fields on `Placement`** (`rotation: Option<Row3> →
matrix`, `pivot`, `mirror: Option<Symmetry>`), because one placement type
means the engine's scene path and the creator's compose path cannot drift, and
because `Default` stays identity so every existing literal keeps compiling.
`rotation_y_degrees` is retained and stays authoritative when `rotation` is
`None` — the engine's own convention, not a second answer.

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
  roots:          Vec<BlockIndex>        derived, never stored

Block
  block_id:  BlockId                     (group, instance) of the rigblock record
  transform: BlockTransform
  snapped:   Option<bool>
  children:  Option<Vec<BlockIndex>>     None = not written; Some([]) = written empty
  symmetric: Option<BlockIndex>          mirror partner index
  paint:     Option<Vec<PaintRegion>>    absent in all 47 crt documents
  handles:   Option<Vec<ChannelWeight>>  opaque channel word + f32
  user_orientation: Option<Row3>          vcl only
  bbox:      Option<(Vec3, Vec3)>        formatversion 11 only

BlockTransform
  position: Vec3, scale: f32, orientation: Row3,
  triangle_direction: Option<Vec3>, triangle_pick_origin: Option<Vec3>
```

Two decisions worth defending:

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

---

## 6. The one real blocker, named

`assemble → render` needs a mesh per block. Today:

```text
blockid (group, instance)
  → 0x5CAA5E28 record            RESOLVED, 1549 records, 6 families
    → encoded tail               UNKNOWN — nibble-packed, sizes 36…8631
      → gmdl ResourceKey         UNREACHED
```

Consequences for the first vertical slice, stated plainly:

1. **The authoring loop is buildable now.** Catalog the 68 real documents, parse
   them, compose the forest, compute world transforms, save, load, verify
   equality. All of that is corpus-verified and needs no rigblock decode.
2. **Rendering a block needs the resolver.** Until the tail is decoded, a
   composed creature renders as *resolved blocks + an explicit unresolved
   report*, never as a silently missing part and never as a placeholder mesh
   pretending to be a body part.
3. **The rigblock tail is the next research target**, and it is worth a dedicated
   package in the existing `src/reconstruction/pkg_*` style: a bounded,
   test-fixtured decoder for `0x5CAA5E28`, graded against the 1549 real records,
   in the spirit of the 77 promoted model tests. It is deliberately *not* part
   of the Creator Core — the core must be correct while this is unknown.

Note for the reviewer: `target/release/osptool` predates the uncommitted
`spore-tools` source and has no `stats`/`graph`/`texture` subcommands. Figures
above come from the committed Python oracle
(`tools/spore/dbpf/dbpf.py`) and from my own XML/DBPF census, not from that
stale binary.

---

## 7. Build order

| # | Deliverable | Gate |
|---|---|---|
| 1 | `spore-creator-core`: domain types + `claims` + hermetic tests | `cargo test -p spore-creator-core` green, no Bevy in the tree |
| 2 | `<sporemodel>` decoder for the verified grammar | 68/68 real documents parse; `#[ignore]`d corpus test, clean skip without `SPORE/` |
| 3 | structural validation (index range, cycles, in-degree, version-gated fields) | hermetic, synthetic fixtures only |
| 4 | `spore-creator-doc/1` writer + fail-closed reader | `save→load == doc` by `to_bits()` |
| 5 | snapshot undo/redo | invertibility + no-op-undo tests |
| 6 | `spore-engine`: additive `Placement` fields + projection into `Scene`/`Creature` | existing engine tests unchanged |
| 7 | the failable resolver, returning `Unavailable { reason }` | reports 0/60 today and says so |
| 8 | Bevy preview of a *composed* document | headless via `MinimalPlugins` only |
| 9 | rigblock tail research package (separate workstream) | graded against 1549 records |
| 10 | interactive editing (selection, gizmo, snapping) | **not started, by design** |

Steps 1–5 are pure data work, fully hermetic, and are the deliverable this
milestone. Step 6 is the invariant the brief demands: creator output and runtime
input share one semantic representation, because the projection target is the
*same* `domain::Placement` the engine already consumes.

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
| 68 creator documents, 6 types, 680 blocks, all parse | **VERIFIED** | my DBPF+XML census over `Spore_Game.package`; independently `01`, `02` |
| 601 edges, 0 dangling/cyclic, max in-degree 1, 79 roots | **VERIFIED** | `01-domain-model.md` (two independent methods) |
| 160 blocks have no `childlist` element | **VERIFIED** | my census; `01`, `05` |
| grammar census (§1 tables) | **VERIFIED** | my census |
| paint is in-document, per block, `crt`-free | **VERIFIED** | my census; `04` |
| `cEditorResourceBlock` 472 B, `cEditorResource` 172 B | **CONFIRMED** | `06-semantic-evidence.md` |
| authored part transform is `(Vec3, Matrix3, f32)` | **CONFIRMED** | `06` + my grammar |
| symmetry is a partner index, not a transform | **OBSERVED** | `03`, `06`, my census |
| one material id per record; 4208/4208 single-id | **VERIFIED** | `04` |
| `RegisterShaderData` has 72 sites, `0x218` in none | **VERIFIED** | `04` — `spore-gmdl`'s refusal is faithful |
| 1549 rigblock records; `blockid` names only `0x5CAA5E28` | **VERIFIED** | my census; `02` |
| rigblock tail encoding | **UNKNOWN** | my sampling: nibble-packed, sizes 36…8631 |
| snap/align/interact lists | **UNKNOWN (code only)** | `02`, `06` — zero bytes in the corpus |
| `paintregion` / `paintid` / weight-channel meaning | **UNKNOWN** | `04`, my census |
| shared editor base class | **NOT DEMONSTRABLE** | `06`: 469 types, zero cross-domain mentions |
| local-vs-absolute frame | **UNKNOWN** | `03` (G-04) |
| quaternion↔matrix loss 1.131e-06 | measured | `03` |
| f32 `{:?}` round-trip, all finite patterns | measured | `05` |
| `serde_json` emits `null` for NaN | measured | `05`, `serde_json-1.0.150/src/ser.rs:169-179` |
