# ANIMATION.md — skeleton, bind pose, animation

**Status:** the gmdl record's *bind side* is decoded and measured. The
*pose side* — joint transforms, hierarchy, timeline — is **not in the gmdl record
at all**, and is recorded here as a bounded UNKNOWN with the exact next step.

Every number below was measured over `SPORE/Data/Spore_Content.package` and is
re-asserted by `crates/spore-gmdl/tests/corpus_animation.rs` (nine `#[ignore]`d
tests, each skipping cleanly when `SPORE/` is absent). Reproduce with:

```text
cargo test -p spore-gmdl --release --test corpus_animation -- --ignored --nocapture
```

Corpus denominator: **17 119 index rows, 4 209 `gmdl` records, 5 945 vertex
descriptors.**

---

## 1. Summary of grades

| # | claim | grade | settled by |
|---|---|---|---|
| 1 | The `refCount` **count word and every key word are big-endian** | **PROVEN** | index resolution, §2 |
| 2 | The key word order is `{instance, group, type}`, uniquely | **PROVEN** | 6 permutations, §2.2 |
| 3 | Every declared key names an `rw4` container (`0x2F4E681B`) | **PROVEN** | 7 989/7 989 resolve, §3 |
| 4 | gmdl records carry `BLENDINDICES`/`BLENDWEIGHT` elements | **OBSERVED** | descriptor census, §4 |
| 5 | `BLENDWEIGHT` is always `UBYTE4N`, `BLENDINDICES` always `UBYTE4` | **OBSERVED** | 1 330 / 2 159 elements, §4 |
| 6 | Every `BLENDINDICES` slot is a multiple of 3 | **OBSERVED** | 2 109/2 109 records, §5 |
| 7 | `type_code` is a constant per `D3DDECLUSAGE` | **OBSERVED** | 5 945 descriptors, §4.3 |
| 8 | The joint table is strided by 3; `joint = slot / 3` | **SUPPORTED** | §5 |
| 9 | The trailer's first bone-range count bounds the joint id | **INFERRED** | 313/315, §6 |
| 10 | A gmdl record carries no joint transform | **SUPPORTED** | §7 |
| 11 | The references are the joint-transform containers | **SUPPORTED** | §3.2 |
| 12 | Why the stride is 3 | **UNKNOWN** | §8.1 |
| 13 | The RW4 section code *names* | **UNKNOWN** (tables disagree) | §3.3 |
| 14 | The bind-pose matrices | **UNKNOWN** | §8.2 |

---

## 2. The bug: the whole `refCount` table is big-endian

### 2.1 Hypothesis and measurement

The count word was already known to be big-endian — it is the trap in
`AGENTS.md`, and it explains 1 510 of the 4 209 walk failures. The **three words
of each key behind it were read little-endian**, which is a *separate* bug with a
much quieter symptom: the walk consumes the right number of bytes and yields
plausible `ResourceKey`s that address nothing.

The measurement that settles it is index resolution: each declared key is a
complete `(type, group, instance)` triple, so it either names a real DBPF row or
it does not.

| reading of the key table | keys that resolve to an index row |
|---|---|
| **big-endian** | **7 989 / 7 989** |
| little-endian | **0 / 7 989** |

Under the little-endian reading the count word is `N << 24`, so the declared
table only fits the record at all on **2 699 of 4 209** records — the 2 699 being
exactly the records that declare *no* references. Zero keys are readable.

### 2.2 The word order is unique

All six assignments of the three words to `(type, group, instance)`:

| permutation | resolves |
|---|---|
| `(w0, w1, w2)` → `(t, g, i)` | 0 / 7 989 |
| `(w0, w2, w1)` | 0 / 7 989 |
| `(w1, w0, w2)` | 0 / 7 989 |
| `(w1, w2, w0)` | 0 / 7 989 |
| `(w2, w0, w1)` | 0 / 7 989 |
| **`(w2, w1, w0)` → `(t, g, i)`** | **7 989 / 7 989** |

So the on-disk field order is `{instance, group, type}` — the order the crate
already documented — and only the *byte order* was wrong.

### 2.3 The asymmetry, which is real and worth keeping

`refCount` (4 bytes) and every key word behind it (36 bytes) are big-endian; the
`0x20D` texture-set table **in the same record** is little-endian. Both were
settled independently by index resolution, and there is no marker in the file to
say which is which. `Reader` therefore keeps `read_u32` and `read_u32_be` as
separate methods, so getting it wrong requires typing the exception.

### 2.4 What this breaks outside this crate (ownership)

The fix is in `crates/spore-gmdl/src/parse.rs::read_referenced_files`. Three
things outside `spore-gmdl` read the old little-endian keys and are **not fixed
here** because they are not this crate's files:

| file | state at the time of writing |
|---|---|
| `crates/spore-tools/tests/describe.rs` | **already updated** in parallel; it documents the fixture divergence and no longer asserts the old identity. |
| `crates/spore-assets/src/graph.rs:58-70` and `crates/spore-assets/tests/graph.rs:343` | still to do. The module doc says this crate "carries byte-swapped keys" and `the_graph_reads_the_reference_table_big_endian_and_spore_gmdl_does_not` asserts the decoder returns three byte-reversed words. **That test now fails, by design** — the divergence it pins no longer exists. Its assertion inverts to "the two readers now agree". `graph.rs` already reads the table big-endian itself, so the two implementations agree again once the doc and test are updated. |
| `tools/spore/gmdl/gmdl.py:101` | still to do. `self.refs.append((u(b, o), u(b, o + 4), u(b, o + 8)))` reads the key words little-endian, so the Python oracle and this crate now disagree on every record that has a reference. `crates/spore-differential/src/compare/gmdl.rs:569` compares exactly this field. |
| `tests/fixtures/gen_fixtures.py:182` | still to do. `struct.pack('<III', ...)` packs the committed `mini.gmdl` ref key the wrong way round, so its type word decodes as `0xe5bce600`. `crates/spore-gmdl/tests/fixture_parse.rs` pins that divergence explicitly rather than hiding it, and says where the one-character fix belongs. |

---

## 3. What the references are

### 3.1 The table

| | value |
|---|---|
| records with at least one key | **1 510** of 4 209 |
| records declaring none | 2 699 |
| declared keys | **7 989** |
| distinct targets | **1 006** |
| distinct targets per declared key | 7.95 |
| largest `refCount` in the corpus | 24 |
| target type | `0x2F4E681B` (`rw4`) for **7 989 / 7 989** |

| target group | declared keys | distinct targets |
|---|---|---|
| `0x41606100` (unnamed; not in the canonical group table) | 7 449 | 466 |
| `0x40646200` (VehicleModels) | 285 | 285 |
| `0x40636200` (BuildingModels) | 164 | 164 |
| `0x40656200` (UfoModels) | 91 | 91 |

The three model-specific groups are **1:1** — each gmdl names its own companion
container. The unnamed shared group carries 7 449 keys over 466 records (16 per
record), so it is a library several models draw on, which is why
`docs/RESOURCE-GRAPH.md` graded its role `INFERRED` from adjacency alone. The
section census below replaces that adjacency argument with bytes.

### 3.2 The shared group carries skeleton and animation sections

All 1 006 targets are `fileType` `0xCAFED00D` and all 1 006 section directories
parse. Distinct section type codes, per group:

| group | section codes present (records carrying them) |
|---|---|
| `0x41606100` (466 records) | `0x70001` ×466, `0x70002` ×466, `0x70003` ×466, `0x7000C` ×466, `0x7000F` ×466, `0xFF0001` ×466 |
| `0x40636200` (164) | `0x80003` only |
| `0x40646200` (285) | `0x80003` only |
| `0x40656200` (91) | `0x80003` only |

A single shared-group container carries, typically, **five** `0x70001` sections
(one per animation clip), an `0xFF0001` index, a `0x70003` skin, and **two**
`0x70002` sections. That is the shape of an animation library entry: several
clips, one skin, a skeleton.

The `0x80003`-only containers are the per-model companions: `objCount` 1, one
small section. Nothing about them is animation-shaped.

### 3.3 The names are contested, and are not resolved here

| code | `spore_rw4::cpp_type_name` | `spore_rw4::python_type_name` |
|---|---|---|
| `0x70001` | `KeyframeAnim` | `SkinsInK` |
| `0x70002` | `Skeleton` | *(no entry)* |
| `0x70003` | `AnimationSkin` | *(no entry)* |
| `0x80003` | `TriangleKDTreeProcedural` | `BBox` |
| `0xFF0001` | `Animations` | *(no entry)* |

The two tables disagree, and neither is verified by this crate. The *codes* are
`OBSERVED`; the *names* are recorded as `INFERRED`
(`claims::RW4_SECTION_CODE_NAMES`). One measurement favours the C++ table on
`0x80003`: the per-model companions carry `objCount` 1 and one small section,
which is not the shape of a per-triangle KD-tree and is exactly the shape of a
bounding box — but a size argument is not a decoding, so it stays a note.

---

## 4. The gmdl record's own skinning: the descriptor census

### 4.1 What is there

| | count |
|---|---|
| vertex descriptors | 5 945 |
| `BLENDWEIGHT` (`usage` 1) elements | **1 330** |
| `BLENDINDICES` (`usage` 2) elements | **2 159** |
| descriptors with **both** | 1 330 |
| descriptors with indices and **no** weights | **829** |
| descriptors with weights and no indices | **0** |
| records declaring `BLENDINDICES` | **2 109** of 4 209 |
| records declaring indices and no weight | 812 |

So **just over half the corpus is skinned**, and the binding is the Direct3D 9
one: `BLENDWEIGHT` is always `UBYTE4N` (code 8) and `BLENDINDICES` always
`UBYTE4` (code 5). One vertex names **four** joints with four `u8` slots.

Every skinning element in the corpus is `stream` 0, `declMethod` 0, `usageIndex`
0. There is no second influence set and no per-vertex transform method anywhere
in these 4 209 records.

### 4.2 The eight descriptor shapes, in full

```
  2991  POSITION@0:2 NORMAL@12:5 TEXCOORD@16:1
   795  POSITION@0:2 NORMAL@12:5 TANGENT@16:5 TEXCOORD@20:1
   546  POSITION@0:2 NORMAL@12:5 TEXCOORD@16:1 TANGENT@24:5 BLENDINDICES@28:5 BLENDWEIGHT@32:8
   480  POSITION@0:2 NORMAL@12:5 TANGENT@16:5 BLENDINDICES@20:5 BLENDWEIGHT@24:8 TEXCOORD@28:1
   441  POSITION@0:2 NORMAL@12:5 BLENDINDICES@16:5 TEXCOORD@20:1
   388  POSITION@0:2 NORMAL@12:5 TEXCOORD@16:1 BLENDINDICES@24:5
   284  POSITION@0:2 NORMAL@12:5 BLENDINDICES@16:5 BLENDWEIGHT@20:8 TEXCOORD@24:1
    20  POSITION@0:2 NORMAL@12:5 BLENDINDICES@16:5 BLENDWEIGHT@20:8 TANGENT@24:5 TEXCOORD@28:1
```

Strides follow: 24, 24, 36, 32, 24, 28, 28, 32 bytes.

### 4.3 `type_code` is a constant per usage

| usage | `type_code` | elements |
|---|---|---|
| `POSITION` | 0 | 5 945 |
| `NORMAL` | 2 | 5 945 |
| `TEXCOORD` | 6 | 5 945 |
| `BLENDINDICES` | 14 (0x0E) | 2 159 |
| `BLENDWEIGHT` | 15 (0x0F) | 1 330 |
| `TANGENT` | 19 (0x13) | 1 841 |

One code per usage with no exceptions across 5 945 descriptors. The crate still
carries the word verbatim and never interprets it
(`claims::VERTEX_ELEMENT_TYPE_CODE`, `OBSERVED`): knowing it is a per-usage
constant says nothing about what the numbers *mean*.

### 4.4 The half-bindings

829 descriptors (812 records) declare bone indices and **no weights**. Three
readings are consistent with the bytes and this crate picks none:

* the mesh is rigidly bound and one joint suffices, the extra slots being
  padding;
* weights come from a stream this walk does not reach;
* equal weights are implied.

No weights are synthesised. `GmdlSkinBinding::weights` is `None` there, and
`weight_slots()` is `None` rather than `0` — an absent width and a width of zero
are different statements.

---

## 5. The stride of three

Over **all 2 109 records** that declare `BLENDINDICES`, reading the four index
bytes of every vertex of every vertex buffer:

> **not one slot value is anything other than a multiple of 3.**

and the gap between two consecutive *distinct* values, pooled over the corpus:

| gap | occurrences |
|---|---|
| **3** | **48 623** |
| 6 | 836 |
| 9 | 204 |
| 12 | 90 |
| 15 | 32 |
| 18 | 13 |
| 21 | 4 |
| 24 | 2 |
| 27 | 1 |
| 39 | 2 |
| 57 | 1 |
| 66 | 1 |

The highest slot value anywhere in the corpus is **189**, i.e. joint 63.

Two consequences, graded separately:

* **OBSERVED** — the values are multiples of 3, and 3 is overwhelmingly the
  spacing.
* **SUPPORTED** (`claims::BLEND_INDEX_STRIDE`) — the joint table is addressed
  with a stride of three, so the joint id is `slot / 3`.
* **UNKNOWN** — *why*. Three 4-byte words per joint, three copies of a skeleton,
  or a reserved slot per joint all fit the same measurement.

This is the single most useful number found: it turns a raw byte into a joint id.

---

## 6. The trailer's bone ranges

315 of the 4 209 records declare a non-empty bone-range table. 333 ranges in
total, at most 3 per record, and only **9 distinct `start` values** across the
whole corpus. Every one has `start == 0` or `start == end of the previous range`.

The trailer walk over the whole corpus:

| outcome | records |
|---|---|
| `TrailerWalk::Complete` | 2 438 |
| `Truncated` at `BakedDeform` | 1 719 |
| `Truncated` at `AnimData` | 52 |

### 6.1 How the range count relates to the joint ids

Let `c` be the **first** range's count (not the sum — a record with ranges
`[(0, 62), (62, 22)]` bounds its joints by 62, not 84), and `h` the highest slot
value the vertex stream names:

| relation | records |
|---|---|
| every `slot / 3 < c` | **313** |
| some `slot / 3 >= c` | **2** — `(c = 60, joint 63)` and `(c = 62, joint 63)` |
| `h == 3 * (c - 1)` — the ceiling | **285** |
| `h == 3 * (c - 2)` — one triple below | 27 |
| neither | 3 |

Both exceptions overshoot by four joints. The exceptions are why this is
`INFERRED` (`claims::BONE_RANGE_MEANING`) and not stronger: the record is
consistent with the count being a joint count and two records say otherwise.

### 6.2 The bone table is not a pointer into the reference table

Only **44 of the 315** records that declare bone ranges reference any file at
all. Whatever the bone table is, it is not an index into the `refCount` keys.

---

## 7. There is no joint transform in a gmdl record

A gmdl record holds:

* geometry (index buffers, interleaved vertices, meshes);
* materials and textures;
* **which joints each vertex names** (the `BLENDINDICES` slots);
* **a bound on how many joints there are** (the trailer's bone ranges);
* names of `rw4` containers.

It does **not** hold a single joint matrix, a parent index, or a time. A bind
pose is `joint_inverse_bind_matrices × joint_matrices`, and neither factor is
here. So:

> A skeleton can be **counted** from a gmdl record and cannot be **posed** from
> one. `crates/spore-gmdl` deliberately exposes no `Joint`, no `Transform` and no
> `Skeleton` type: a `Joint` with a zeroed matrix would read as "the skeleton is
> at the origin", which is the precise failure this repository is built to avoid.

`GmdlModel::bone_range_span()` is exposed because a renderer needs it to *size* a
joint table even though it cannot fill one, and because the number is measured.

---

## 8. The explicit UNKNOWN list

### 8.1 Why the joint-table stride is three

Blocked on: a decoded `0x70002` section. With one in hand, a 3-word-per-joint
layout and a 1-word-per-joint layout are distinguishable by size alone.

### 8.2 Where the bind-pose matrices are

Not in the gmdl record (§7). The candidates, in order:

1. the `0x70002` sections of the `refCount`-referenced `rw4` containers — 466
   records, two sections each, sizes 36/48/60/…/168 bytes. **`size − 16` is always
   a multiple of 4** across every measured pair, which is consistent with a
   16-byte header and a 4-byte-per-entry array, but the entry size does **not**
   correlate with the gmdl's own bone-range count (97 distinct
   `(section size, range count)` pairs, no relation), so nothing here is claimed;
2. the `0x7000F` (256 B) or `0x70003` (336 B) sections of the same containers;
3. `Spore_Graphics.package`, which holds 9 368 `rw4` records of its own — 1 244
   carrying section code `0x20005` and 1 256 carrying `0x80005`, in the
   `0x40606000` *EditorRigblocks* group and in group `0x00000000`. **No gmdl in
   `Spore_Content.package` names any of them**, so if the poses live here they
   are reached by something other than the `refCount` table.

Next step, in order: (a) decode one `0x70002` section and establish its header
and entry size; (b) check whether its entry count matches the gmdl that names it;
(c) only then look at `Spore_Graphics.package`.

### 8.3 The RW4 section code names

`spore-rw4` holds two tables that disagree (§3.3). Resolving them needs either a
byte-level demonstration (`0x70002` entries that are provably joint transforms)
or a `spore-semantic` fact about the class that writes the section. Until then
both spellings are reproduced and neither is promoted.

### 8.4 The `0x218` shader-data id, still unmeasured

619 gmdl records — including **all 97** records of the creature group
`0x40627100` — stop at `PartialTextureSet` because `0x218` has no known payload
size. Nothing in this document depends on it, and nothing guesses at it.

### 8.5 `0xEE17C6AD` "animation" records: a lead, not decoded

2 091 records in `Spore_Game.package` and 2 223 in `PatchData.package`, **all in
group `0x00000000`**, none in `Spore_Content.package`. The canonical type table
calls the id `animation`. Undecoded, and not a gmdl concern: it is a different
record type in a different package. It is the obvious next place to look for a
*clip list* if the `rw4` route in §8.2 dead-ends.

### 8.6 The 829 half-bindings (§4.4)

### 8.7 What `bone_ranges` counts, beyond "it bounds the joint ids" (§6.1)

### 8.8 Base colour for a skinned creature

Unrelated to animation but load-bearing for it: `0x15`-family rasters are
tangent-space normal maps, Z first, and DXT5 records carry no colour. The
creature `0x00e6bce5:0x40627100:0x067c79d2` (2 meshes, 47 016 triangles) renders
with a normal map and no base colour, because its base colour is not in the
rasters this crate can read.

---

## 9. What was changed in `spore-gmdl`

| change | file |
|---|---|
| the three key words are read big-endian (the fix) | `src/parse.rs::read_referenced_files` |
| `Reader::read_u32_be` doc: 4 bytes → 52 bytes in this format | `src/reader.rs` |
| new: `GmdlSkinBinding`, `skin_bindings()`, `uses_skinning()`, `bone_range_span()`, `declares_bone_ranges()` | `src/skin.rs` |
| six new claims; `REFERENCED_FILES_ROLE` regraded `INFERRED` → `SUPPORTED` | `src/claims.rs` |
| nine corpus tests, all `#[ignore]`d, all skipping cleanly without `SPORE/` | `tests/corpus_animation.rs` |
| eight hermetic tests over synthetic records | `tests/skinning.rs` |
| the shared fixture's little-endian key divergence pinned, not hidden | `tests/fixture_parse.rs` |
| the test-only builder writes key words big-endian | `tests/common/mod.rs` |