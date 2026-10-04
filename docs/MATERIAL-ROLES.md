# MATERIAL ROLES — what a Spore texture is, measured

**Agent:** MATERIALS. **Date:** 2026-10-04.
**Crate:** `crates/spore-material` (Bevy-free, by construction).
**Corpus:** `SPORE/Data/Spore_Content.package` — 17 119 index rows,
2 954 `raster` records, 4 209 `gmdl` records, 995 MB, GOG digital build
3.1.0.22.

This document is written in the order the work was done: **hypothesis →
measurement → evidence → conclusion**, with the corpus tables inline. Every
number here was measured; nothing is quoted from another document without
saying so, and where a document disagrees with the bytes the bytes win and the
disagreement is recorded in [§8](#8-refutations).

Reproduce with:

```bash
cargo test -p spore-material --release -- --ignored --nocapture --test-threads=1
```

which runs six `#[ignore]`d census tests. They **re-measure with the Rust
crates** what the tables below were first measured with, so the Python oracle
and the Rust port have to agree before a number is trusted.

---

## 1. The problem, stated as a question

The engine bound **one** texture to a whole model and used it as diffuse
colour. The documented asset `0x00e6bce5:0x40637e03:0x067a07f0` references
**three** textures; the creature preset `0x00e6bce5:0x40627100:0x067c79d2`
references more. So: *what is each of those textures, and which mesh does it
belong to?*

Three sub-questions, in increasing order of how hard they are:

| # | Question | Answerable from the corpus? |
|---|---|---|
| A | What are a record's **texels**? | **Yes** — and answered in §3–§5 |
| B | Which **shader stage** does the game bind it to? | **No** — §6 |
| C | Which **mesh** owns which texture? | **Partly** — §7, graded `INFERRED` |

`spore-material`'s previous answer to all three was "unknown", which was honest
and left the renderer with nothing. Two of the three are now real answers, and
the third is a bounded inference with its assumption written down.

---

## 2. The population

### 2.1 Raster records — 2 954, three formats

| `fourcc` | records | shapes | status in `spore-texture` |
|---|---:|---|---|
| `0x35545844` `DXT5` | **1 747** | 64²×7 → 67, 256²×9 → 306, 512²×10 → 1 374 | decoded |
| `0x00000015` | **901** | 32²×6 → 67, 256²×9 → 294, 512²×10 → 540 | decoded |
| `0x31545844` `DXT1` | **306** | 256²×9 → 306 | **refused by name** |

Envelope census, 2 954 of 2 954: `version == 1`, `field_10 == 8`. Neither is
read by any decoder.

`field_18` is **format-specific and exact**, which `docs/MATERIALS-DESIGN.md` §1
records as "unresolved":

| family | `field_18` | identity | holds |
|---|---|---|---|
| `DXT5` | `w · h` | 512² → `0x00040000`, 256² → `0x00010000`, 64² → `0x00001000` | 1 747 / 1 747 |
| `0x15` | `w · h · 4` | the base level's payload size | 901 / 901 |

It is still **never read** by a decoder — "derived, never read" stands, and now
there are two independent statements of the same quantity to check against.

### 2.2 The derived layer split, and 373 records it refuses

`n = (size − 32) / (16 + chain)` for DXT5, and it must divide exactly:

| shape | records | payload | per layer | shortfall vs 2 layers |
|---|---:|---:|---:|---:|
| 512²×10 | 1 374 | 349 584 | 174 792 | **0** |
| 256²×9 | 306 | 87 436 | 43 720 | **4** |
| 64²×7 | 67 | 5 508 | 2 760 | **12** |

**373 of 1 747 DXT5 records are 4 or 12 bytes short of two whole layers**, so
`spore-texture` refuses them with `PayloadNotMultiple`. That is correct
behaviour — rounding would invent a layer — but it means the DXT5 population
this document measures is the 1 374 that divide, and **any claim about "a DXT5
record" below is a claim about those 1 374**. What the 12 and 4 stray bytes are
is **UNKNOWN** and belongs to `spore-texture`, not here.

### 2.3 gmdl records

4 209 `gmdl` records exist. Two implementations walk different subsets of them:

| reader | records walked | why |
|---|---:|---|
| `tools/spore/gmdl` (Python oracle) | **1 819** | raises on 2 390; its `f32` unpacks run off the end of 2 390 records |
| `spore-gmdl` (Rust) | **3 590** | accepts records the oracle cannot reach |

Every structural figure below is quoted with the reader that produced it. Where
the two disagree the disagreement is a fact about the *readers*, not about the
data, and is stated as such.

---

## 3. Hypothesis A: the `0x15` family is a normal map

### Hypothesis

`spore-texture` established the family's **framing** over 901/901 records
(`docs/LUMINANCE-RASTER.md`). This document does **not** re-derive its
**content**; it delegates, because a restated fact is a second source of truth
and this workspace has already paid for that twice.

### Measurement (mine, per record, in Rust)

For every one of the 901 `0x15` records, over **every texel of the base mip**
(160 893 952 texels in total), the exact per-lane mean and distinct-value count:

| statistic | corpus extreme | threshold | margin |
|---|---|---|---|
| lane 0 mean (which is **Z**) | 232.51 … 255.00 | ≥ 224 | 8.51 |
| lane 0 mean − max(lane 1, lane 2) mean | ≥ **94.21** | ≥ 64 | 30.21 |
| \|lane 1 mean − 127.5\| | ≤ **10.80** | ≤ 16 | 5.20 |
| \|lane 2 mean − 127.5\| | ≤ **10.78** | ≤ 16 | 5.22 |
| records the check declined | **0** | — | — |
| records whose lanes 0–2 are **constant** | **7** | accepted | — |

The 7 constant records hold `(255, 128, 128)` in every texel, which is the
encoding of the flat normal `(0, 0, 1)`. They satisfy the check, because a
constant field is a *degenerate* normal map and not a failed one.

### Evidence

* **PROVEN (VERIFIED), delegated** — `spore_texture::claims::LUMINANCE_TEXEL_LAYOUT`:
  lanes 0–2 are one tangent-space normal map, 0..255 with 128 as zero, and **Z
  comes first on disk**. Upstream evidence: lane 0 tracks the derived
  `Z = sqrt(1 − nx² − ny²)` with correlation 0.954; lanes 1 and 2 are mutually
  uncorrelated (0.004), which two independent normal components are and two
  derived channels are not.
* **OBSERVED (mine)** — the signature holds in 901 of 901 records, and the
  *reordering* failure it exists to catch cannot pass it: a record rotated to
  `R,G,B` would have lane 0 averaging near 127 and lane 1 near 250, so the
  **lead** would be negative. That is why the lead condition exists and why the
  absolute floor alone is not the test.

### Conclusion

`TextureRole::TangentSpaceNormal`. **`Lane::Red` is `ChannelRole::NormalZ`.**
A renderer that reads file byte 0 as red gets a normal map with Z and X
swapped, which looks almost right and is not.

`spore-material` asserts the upstream grade on the way through the delegation
boundary, so a downgrade upstream cannot silently contradict this crate's table.

---

## 4. Hypothesis B: a DXT5 record is not a colour map

### Hypothesis

The engine was applying a DXT5 texture as diffuse colour. If DXT5 records
carried colour, that would be right. The hypothesis under test was therefore
the *opposite*: **no raster record in this corpus is a base-colour map**, and a
`Diffuse` role would be a claim about a record that does not exist.

### Measurement

**(a) Per-texel, over every decodable DXT5 record's every layer** — 1 374
records, 720 371 712 base-mip texels, measured in Rust by `spore-material`'s
own `MipStatistics` (exact occupancy sets, not sampling):

| statistic | result |
|---|---|
| widest **green** span across any record | **0** |
| widest **blue** span across any record | **0** |
| largest red distinct-value count | **2** |
| smallest alpha distinct-value count | **65** |
| records whose layer 0 equals layer 1 | **0** |
| records whose two layers differ | **1 374** |

**(b) Palette-level, over every DXT5 block in the corpus** — 46 293 760 blocks
examined for endpoint statistics; a stride sample of 477 771 palette entries
run through the repository's pinned Python codec:

> **exactly 2 distinct decoded RGB triples exist: `(0, 0, 0)` and `(8, 0, 0)`.**

**(c) Shape, over the 1 233 referenced DXT5 record-instances:**

| statistic | range |
|---|---|
| alpha distinct values per layer | 65 … 256 (never constant) |
| texels with alpha exactly 0 | 52.03 % … 81.46 % |
| mean alpha | 16.48 … 64.55 |
| mean \|neighbour delta\| in alpha | 9.99 … 39.27 |

### Evidence, and why this is structural rather than a sampling artefact

`tools/spore/dxt5/dxt5.py` reads the block's colour field as **two 6-bit
values** (`c0 = b[2] & 0x3F`, `c1 = bits 6–11`) and expands them through
`_rgb5`, whose green term is `(v >> 10) & 0x1F` and whose blue term is
`(v >> 15) & 0x1F`. For any `v ≤ 0x3F` **both are identically zero.** So the
degeneracy is a property of the *codec*, not of the data, and no input can
produce a third colour.

That cuts both ways and both directions are recorded here:

* What is **PROVEN**: the corpus contains no raster record that this repository
  can decode as a base-colour map. Every one of the 2 954 raster records is
  either a degenerate-colour layered mask (1 747) or a normal map (901), and the
  remaining 306 are refused by name.
* What is **UNKNOWN**: whether Spore's own shader sees colour in those blocks.
  The colour field carries **12 bits** of information per block and the pinned
  codec discards all but one bit of it. Whether that is Spore's intent or a
  shared defect in the C++ reference and the Python oracle — which were written
  to agree with each other — **cannot be settled from these records**, and the
  codec's four documented departures are byte-pinned and explicitly *not* to be
  "corrected" (`AGENTS.md`). This is recorded as an open question in §9, and it
  is `spore-texture`'s to answer, not this crate's.

### Conclusion

`TextureRole::LayeredAlphaMask`. Two claims, at different grades, and keeping
them apart is the whole exercise:

* `dxt5_carries_no_colour` — **VERIFIED**
* `dxt5_layers_are_distinct` — **VERIFIED**, 1 374 / 1 374 and 1 233 / 1 233
* `mask_layer_purpose` — **UNKNOWN**, a non-finding

The **alpha plane is the content**; what the game *does* with it is unknown.

---

## 5. Multi-layer DXT5 as distinct surfaces

`ResolvedTexture` now publishes `layers: Vec<MipImage>` — the base mip of
**every** layer — instead of layer 0 with the rest discarded. `image` remains as
layer 0 because existing callers read it.

Justification, measured:

* layer 0 ≠ layer 1 byte-for-byte in **1 374 / 1 374** decodable records and
  **1 233 / 1 233** referenced record-instances. Discarding layer 1 discarded a
  *different image*, not a duplicate.
* no 3-entry gmdl texture set names the same DXT5 record twice (**0 / 1 922**),
  so a model's two mask records are always two distinct records.

A model with the corpus's common three-entry shape therefore offers **five**
surfaces: two records × two alpha planes, plus one normal map. None of the four
alpha planes has an established purpose.

---

## 6. Hypothesis C: the 16 skipped bytes carry the sampler role

### Hypothesis

`spore-gmdl` steps over 16 bytes per `0x20D` texture-set entry before reading
`{instance, group}`, and `spore-material` recorded `sampler_role` as UNKNOWN
*because those bytes were not decoded*. If they decode to a role, the engine can
bind correctly.

### Measurement

Measured with the Python oracle over the 1 819 records it walks (2 381
material-info blocks, 6 684 references):

| question | answer |
|---|---|
| distinct values of the entry's **first** header word | **3**: `0`, `1`, `2` |
| does it equal the entry's **position** in its texture set | **6 684 / 6 684** |
| distinct 12-byte runs following it | **4**, across 6 684 references |
| do those 12 bytes vary with the entry's position | **no** — `…01020200521400` appears 1 919 times in slot 0, 1 919 in slot 1, 1 919 in slot 2 |
| the 12 bytes as three LE `u32` | word0 = `0x00000000` in 6 684 / 6 684; word1 ∈ {`0x02020107` (5 766), `0x0202010f` (459), `0x0202030f` (459)}; word2 ∈ {`0x00145200` (5 757), `0x00145201` (918), `0x08dc6d00` (9)} |

### Evidence

The header is **fully accounted for**: 4 bytes are the entry's own ordinal, and
the remaining 12 take four values in 6 684 references **without varying with the
ordinal**. There is no room in those 12 bytes for a per-texture role: they
encode the *block's* shape, not the entry's purpose.

### Conclusion — and a **refuted** claim

**REFUTED:** "`sampler_role` is unknown because the entry header was not
decoded." It was not decoded; now it is, and it holds no role field. The
non-finding **stands**, on a stronger footing: `SamplerRole` still has exactly
one variant, and its reason code changed from
`texture_entry_header_not_decoded` to `no_role_field_in_texture_entry`.

`AGENTS.md` forbids deleting a claim because it is inconvenient. This one was
**updated**, not deleted: the subject is still recorded, still `UNKNOWN`, still
`MISSING`, still with no provenance. What changed is the reason, and the old
reason is recorded here as refuted.

**What would change it:** reading a shader. Nothing in this repository has one.

---

## 7. Per-mesh bindings

### 7.1 The correspondence

`spore-gmdl` records one material id per mesh, in the mesh table's order —
**that much is structural**, an array immediately after the
`{index_buffer, vertex_buffer}` pairs. It then reads `material_info_count`
blocks and pushes every block's texture references into one flat list, losing
the block boundaries.

So two questions, with different answers:

| question | answer | grade |
|---|---|---|
| mesh *i* ↔ `material_ids[i]` | yes, by array position | **OBSERVED** |
| texture set *b* ↔ mesh *b* | **not established** | **UNKNOWN** |

### 7.2 The measurement that makes a partition *possible*

| measurement | reader | result |
|---|---|---|
| `mesh_count` == material-info block count | Python oracle | **1 819 / 1 819** |
| every block of a model contributed the same number of references | Python oracle | **2 381 / 2 381** |
| `texture_refs.len()` divides by `meshes.len()` | `spore-gmdl` | **3 589 / 3 589**, **0 uneven** |

Together these make `refs.len() / mesh_count` a sound cut: the total is a
multiple of the block count, and the block count equals the mesh count. The
remaining assumption — block *i* is mesh *i* — is a **naming-order inference**
and is recorded as the non-finding `material_block_is_mesh`.

`BindingScope::is_inferred()` returns `true` for **both** variants. There is no
proven binding in this build, and a scope that claimed otherwise would be the
failure this crate exists to prevent.

### 7.3 Two independent statements of each slot's ordinal

| statement | agreement |
|---|---|
| the entry's own first header word | equals its position, **6 684 / 6 684** |
| the referenced record's group id **low byte** | equals its position, **6 684 / 6 684** |

Two independent statements of one fact. `TextureSlot::group_stated_ordinal()`
and `TextureSlot::ordinal_agrees()` expose the cross-check, and
`ModelTextures::ordinal_agreement` reports the count for a given model. The group
id is a **convenient cross-check, never a source of truth**: a naming convention
would survive the records it describes being reordered.

### 7.4 The corpus regularity — and why it is not the basis of a role

Over the 1 819 records the oracle walks, a gmdl texture set occurs in exactly
**two** shapes:

| shape | sets |
|---|---:|
| `(DXT5, DXT5, 0x15)` | **1 922** |
| `(DXT5, 0x15)` | **459** |

The `0x15` record is always last, and slot 0 is always DXT5. Tempting — and
**not** how a role is assigned here, for a measured reason: **the second entry is
DXT5 in 1 922 sets and `0x15` in 459.** An ordinal does not determine the
content. The regularity is recorded as `texture_set_shapes` and re-checked per
model by `ModelTextures::observed_shape`, which reports the model's *own* roles
and has a `Divergent` variant for a model that does not fit.

### 7.5 Group families, and one population that names nothing

Measured with `spore-gmdl` over its 3 590 records — a wider sample than the
oracle's 1 819, and it finds two more families:

| family | models | references | refs per mesh |
|---|---:|---:|---:|
| `0x40612900` | 132 | 300 | 3 |
| `0x40632900` | 1 311 | 4 989 | 3 |
| `0x40642900` | 1 140 | 6 816 | 3 |
| `0x40652900` | 364 | 1 851 | 3 |
| `0x40662900` | 612 | 1 224 | **2** |
| `0x406b2900` | 30 | 60 | **2** |
| **group id `0`** | 132 | 150 | 3 |

Total 15 390 references; the per-mesh histogram is `{2: 642, 3: 2 947}`, i.e.
the two-entry shape occurs in `0x4066…` and `0x406b…` only.

**The group-0 population is a finding.** 150 references across 132 models — all
in model group `0x40616201` — name group id **zero**, and **no raster record
carries group 0** (0 of 150 resolve). This is an *absence of a name*, not a
missing file, and `spore-material` reports it as
`MaterialError::Lookup`, which is the honest variant. What those models intend
is **UNKNOWN**.

---

## 8. Refutations

Documents this work contradicts. Each is recorded rather than edited, because
none of those files is in this agent's ownership.

| # | source | said | the corpus says |
|---|---|---|---|
| 1 | `crates/spore-material/src/lib.rs` (its own prior text) | the documented asset's third texture is **1024×1024** and **refused** | it is **512×512**, 10 mips, 1 layer, `fourcc 0x15`, and it **decodes**. Fixed in this crate; recorded here because the old text was in the crate's own module documentation. |
| 2 | `docs/MATERIALS-DESIGN.md` §1 | "The 1024² records (`raster_1`/`_2`) are format 0x15 (luminance, not DXT5) — out of scope" | `Spore_Content.package` contains **no** 1024² raster of either format. The `0x15` family occurs at 32², 256² and 512² only. Either the records named are outside this package or the size is wrong. |
| 3 | `docs/MATERIALS-DESIGN.md` §1 | `field_1c` is "a float 1.0 in 0x15 records" | in a `0x15` record `field_1c` is **the first texel of the base level**: the documented asset's reads `0x008080ff`, which is bytes `ff 80 80 00` = the flat normal `(255,128,128)` with lane 3 = 0. Already corrected in `spore-texture`'s claims. |
| 4 | `docs/MATERIALS-DESIGN.md` §1 | `field_18` "**unresolved** (same in every sample)" | it is format-specific and exact: `w · h` for DXT5 (1 747/1 747), `w · h · 4` for `0x15` (901/901). Still never *read* by a decoder, so the crate's rule is unchanged — but "unresolved" is now wrong. |
| 5 | `docs/MATERIALS-DESIGN.md` header | "PNG32 (records are RW4 containers, §2 — not raw PNG)" | refuted before this work: a `png`-typed record (`0x2F7D0004`) begins `89 50 4E 47 0D 0A 1A 0A` and is **raw PNG**; an `rw4`-typed record (`0x2F4E681B`) begins `52 57 34 77 33 32 00` = `"RW4w32"`. Restated in `spore-material`'s crate docs. |
| 6 | this crate's own `sampler_role` basis | "unknown **because** the 16 bytes were not decoded" | the bytes are decoded now and carry **no role field** (§6). The non-finding stands; the reason is stronger. Reason code renamed; old one recorded as refuted. |

---

## 9. What is still UNKNOWN

Stated as a list, because "unavailable" is never zero, empty or `false`.

1. **The purpose of each mask plane.** Four alpha planes per three-entry model,
   none named. `mask_layer_purpose` — `UNKNOWN`.
2. **Which shader stage each texture feeds.** No shader has been read.
   `sampler_role` — `UNKNOWN`, `no_role_field_in_texture_entry`.
3. **Whether material-info block *i* is mesh *i*.** `material_block_is_mesh` —
   `UNKNOWN`, `material_info_block_order_not_established`.
4. **Whether the DXT5 colour field's 12 bits are meant to be visible.** §4.
   `spore-texture`'s question; unanswerable from these records.
5. **The 4 and 12 stray bytes** in the 373 DXT5 records that do not divide into
   layers (§2.2).
6. **The `0x15` family's fourth lane.** Statistically independent of the normal
   (correlation 0.032) and **binary in all 540 records of 512², non-binary in all
   294 of 256² and all 67 of 32²** — resolution separates them perfectly, slot
   does not (in the referenced population the two coincide). `INFERRED`
   upstream; **meaning UNKNOWN**. `ChannelRole::UnidentifiedLane`.
7. **What the 132 group-`0x40616201` models intend** (§7.5).
8. **DXT1** (`0x31545844`, 306 records, 256²×9): still refused by name, and
   whether it is a third role — or the missing colour family — is **UNKNOWN**.
9. **A material id's meaning.** `material_name` — `UNKNOWN`,
   `no_material_registry`, unchanged.
10. **The three documented-asset group names** (`0x40632900/01/02`) remain absent
    from `spore-core`'s table, so `resolved_group_name` is still a non-finding.
    §7 now explains what those ids *encode* without naming them.

---

## 10. Engine specification

**For the agent owning `crates/spore-engine`.** Bevy-free contract: everything
below is plain data from `spore-material`.

### 10.1 The call

```rust
let resolved: spore_material::ModelTextures =
    spore_material::resolve_model_record(&store, &model_key, spore_texture::RASTER_TYPE)?;
```

`resolve_model_record` replaces the old `resolve_model_textures` +
`model_materials` pair. `resolve_model_textures` still exists and still returns
one `Result` per reference; it is the subset of this you need if all you want is
pixels.

### 10.2 The data, field by field

```text
ModelTextures
  .textures : Vec<Result<ResolvedTexture, MaterialError>>   // flat, record order
  .meshes   : Vec<MeshMaterial>                            // one per mesh
  .scope    : BindingScope
  .observed_shape    : TexSetShape
  .ordinal_agreement : OrdinalAgreement { compared, agreed }
  .claims   : Vec<MaterialClaim>
  .failures() -> Vec<(GmdlTextureRef, &MaterialError)>

ResolvedTexture
  .role       : TextureRole        // CONTENT role — use this
  .statistics : MipStatistics      // the evidence behind .role
  .layers     : Vec<MipImage>      // base mip of EVERY layer
  .layer(i)   : Option<&MipImage>
  .image      : MipImage           // == layers[0]; kept for existing callers
  .channels() : Vec<ChannelBinding>// 4 per layer, file order
  .pixels_decoded : bool
  .claims     : Vec<TextureClaim>

MeshMaterial
  .mesh_index  : u32
  .material_id : u32      // no name, ever
  .slots       : Vec<TextureSlot>
  .scope       : BindingScope

TextureSlot
  .reference              : GmdlTextureRef
  .ordinal : Option<u32>  // None under WholeModel
  .group_stated_ordinal() : u32
  .ordinal_agrees()       : bool
```

### 10.3 The algorithm

**Step 1 — one material per mesh, always.**

```rust
for mesh in &resolved.meshes {
    // mesh.material_id is the id that mesh's material carries.
    // mesh.name is None, always. Do not substitute a colour for it.
}
```

Do **not** zip `resolved.textures` with `resolved.meshes`. `slots` is the
per-mesh slice; the flat vector is the model's whole set.

**Step 2 — bind the normal map where the role says so, not where the index is.**

```rust
for mesh in &resolved.meshes {
    for (within, slot) in mesh.slots.iter().enumerate() {
        let flat = match resolved.scope {
            BindingScope::PerMeshBlock { refs_per_mesh } =>
                mesh.mesh_index as usize * refs_per_mesh + within,
            BindingScope::WholeModel => within,
        };
        let role = resolved.role_of(flat);
        match role {
            TextureRole::TangentSpaceNormal => { /* bind as the normal map */ }
            TextureRole::LayeredAlphaMask   => { /* bind as a mask; see step 3 */ }
            TextureRole::Unidentified       => { /* no claim; see step 5 */ }
        }
    }
}
```

`ModelTextures::roles()` does exactly this and returns
`Vec<Vec<TextureRole>>` indexed by mesh then slot. Prefer it.

**Step 3 — a mask is FOUR surfaces, not one.**

A `DXT5` record has two layers and each layer's alpha plane is its own channel:

```rust
// one record -> two channels
let a = texture.layer(0).map(|layer| &layer.pixels); // channel A: unknown purpose
let b = texture.layer(1).unwrap().pixels.clone(); // channel B: unknown purpose
```

For the corpus's three-entry shape that is **four** mask channels plus one
normal map. Upload each as a single-channel (or RGBA-with-alpha-in-A) surface.
**Do not merge the two layers**, and do not treat their RGB planes as colour:
`channels()` reports them as `ChannelRole::DegenerateColour` because the pinned
codec cannot produce anything else.

**Step 4 — the normal map's channel order.**

File byte 0 is **Z**. To get a conventional `R=X, G=Y, B=Z` normal map:

```rust
// src.pixels is R,G,B,A in FILE order: R holds Z, G holds X, B holds Y.
let (z, x, y) = (px[0], px[1], px[2]);
let n = (x, y, z);          // <- the swizzle
```

Or use `channels()` and match on `ChannelRole::NormalX/NormalY/NormalZ`, which
makes the reindexing explicit and impossible to get wrong by accident. Bevy's
`NormalMap` texture format reinterprets the image as `[-1,1]` with `R=X`, so
**one of these two things must happen** — either swizzle the bytes on upload or
set the texture format so the sampler reads it correctly. Getting this wrong
produces lighting that looks plausible and is wrong; it is the single most
likely mistake in applying these roles.

**Step 5 — `Unidentified` is not an error and not a default.**

```rust
TextureRole::Unidentified => {
    // `texture.pixels_decoded` is false if the record failed to resolve;
    // `resolved.failures()` carries the reason. A slot whose record decoded but
    // matched no signature is a *finding*, and the statistics are on
    // `texture.statistics` if you want to see why.
}
```

The engine already has `MISSING_TEXTURE_TINT` for "no texture". Keep it for both
cases and keep them distinguishable in a log line via `role_of`.

### 10.4 What to log

```
model 0x…  meshes=2  scope=per-mesh-block(refs=3)  shape=(mask, mask, normal)
  mesh 0  material=0x407dfddb
    slot 0  0x40632900:0x067a07f0  layered-alpha-mask  layers=2  512x512x10
    slot 1  0x40632901:0x067a07f0  layered-alpha-mask  layers=2  512x512x10
    slot 2  0x40632902:0x067a07f0  tangent-space-normal layers=1 512x512x10
```

`ModelTextures::role_census()` produces the `layered-alpha-mask 2 /
tangent-space-normal 1` tail directly.

### 10.5 Do not do these

| do not | because |
|---|---|
| apply the first texture as diffuse colour | no raster record in the corpus decodes as one (§4) |
| read `texture.image` and call it the model's colour | it is layer 0 of layer 0 of slot 0 |
| assume `textures[i]` belongs to `meshes[i]` | only under `PerMeshBlock`, and the mapping is `i * refs_per_mesh + j` |
| treat `BindingScope::PerMeshBlock` as proven | `is_inferred()` is `true`; block *i* is not known to be mesh *i* |
| read file byte 0 of a normal map as R | it is **Z** |
| name a mask plane "specular" or "gloss" | UNKNOWN (§9.1); `tests/honesty.rs` fails if a recorded value does |
| treat `Unidentified` as a decode failure | the record may have decoded fine and matched no signature |
| add a `Diffuse` variant | a claim about a record that does not exist (§4) |

### 10.6 Expected results on the two documented assets

| asset | meshes | scope | shape | roles |
|---|---:|---|---|---|
| `…:0x40637e03:0x067a07f0` | 1 | `PerMeshBlock { refs_per_mesh: 3 }` | `MaskMaskNormal` | mask, mask, normal — `3/3` resolved, ordinals `3/3` agree |
| `…:0x40627100:0x067c79d2` | 2 (per the mission brief) | per the division | — | all DXT5 are `512²×10` × 2 layers |

The first row is asserted by `tests/real_corpus_roles.rs::the_documented_assets_roles`,
which prints every field above. Run it before trusting any of this.

---

## 11. Test inventory

| file | what it protects | count |
|---|---|---:|
| `src/role.rs` (unit) | signatures, statistics exactness, `Unidentified` landing | 18 |
| `src/material.rs` (unit) | the cut, ordinals, claim grades | 18 |
| `src/claims.rs` (unit) | grade/reason consistency, uniqueness | 10 |
| `src/resolve.rs` (unit) | refusals, assumed type, group-name non-finding | 7 |
| `tests/roles.rs` | role assignment from synthetic records; Z-first order; layer publication | 12 |
| `tests/bindings.rs` | per-mesh partition, fallbacks, cross-checks | 9 |
| `tests/honesty.rs` | non-findings, and the sharpened purpose sweep | 10 |
| `tests/errors.rs` | pre-existing, updated for the API change | 13 |
| `tests/resolve.rs` | pre-existing, updated for the API change | 13 |
| `tests/robustness.rs` | pre-existing, updated for the API change | 10 |
| `tests/real_asset.rs` | the documented asset end to end (`#[ignore]`d) | 1 |
| `tests/real_corpus_roles.rs` | **6 `#[ignore]`d corpus censuses** | 6 |

**122 hermetic tests, 7 corpus tests.** `cargo test -p spore-material` stays
hermetic and needs no game, no GPU and no network.

`tests/real_asset.rs` is worth a note: it used to assert that the documented
asset yields *two* images and *one* `UnsupportedFourcc` refusal, on the stated
grounds that its third reference is a *1024x1024* `0x15` record. **Both halves
were wrong** — the record is 512x512 and it decodes — so the test was updated
rather than deleted, and the old expectation is refutation #1 in §8. A test that
asserts a specification the real data refutes is the "stale claim contradicting
its own decoder" defect, and it is worse than no test at all.

`tests/honesty.rs`'s purpose sweep was **sharpened, not relaxed**: the original
forbade the words "diffuse"/"specular"/"normal_map"/"bump"/"emissive" outright,
which was right when this crate knew nothing about any texture's content. The
invariant now is that an **unestablished** purpose may never be asserted, and
naming one is allowed only where the same sentence marks it unknown. Two tests
enforce it at the type level and one over the claims table.

---

## 12. Clean-room

Nothing here is derived from EA or Maxis code, data or assets. The layouts were
read out of record bytes, out of this repository's own Python oracles under
`tools/spore/`, and out of the community SDK's transcribed type tables. No Spore
asset byte is committed: every fixture in `tests/` is synthesized in the test,
and the real-corpus tests read the git-ignored `SPORE/` tree at run time and
write nothing.