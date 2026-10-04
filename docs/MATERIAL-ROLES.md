# MATERIAL ROLES — what a Spore texture is, measured

**Agent:** MATERIALS. **Date:** 2026-10-04, **revised 2026-10-05 by MATFIX.**
**Crate:** `crates/spore-material` (Bevy-free, by construction).
**Corpus:** `SPORE/Data/Spore_Content.package` — 17 119 index rows,
2 954 `raster` records, 4 209 `gmdl` records, 995 MB, GOG digital build
3.1.0.22.

> ## ⚠ THE 2026-10-05 REVISION, IN ONE PARAGRAPH
>
> **§4 and §5 of this document were refuted, and their refutations are kept here
> rather than deleted.** A corpus measurement proved that Spore's raster framing
> is `header + level 0 + level 1 + (u32 length + level m) for m >= 2`, with
> **16 bytes per 4x4 block** for `DXT5` — and **one** chain of mip levels, with
> **no layer headers**. 2 954 of 2 954 raster records in
> `Spore_Content.package` and 4 386 of 4 386 across two packages satisfy that
> identity exactly. The previous rule charged **8 bytes** per block *and*
> divided the payload by `16 + chain`, which is a **double error that cancelled
> exactly**: a 16-byte block read at 8 bytes is two half-pictures, and two of
> them make one picture. It cancelled only when the geometry lined up, which is
> why 1 374 records decoded and 373 did not.
>
> Everything this document built on that rule is now false:
>
> | was | is | grade |
> |---|---|---|
> | `TextureRole::LayeredAlphaMask` | `TextureRole::ColourAndMask` | see §4 |
> | `ChannelRole::DegenerateColour` — "DXT5 carries no colour" | `ChannelRole::ColourPlane` — it does, and it is real | `INFERRED` (see §4.4) |
> | two layers per record | **one chain**, `layers.len() == 1` | `VERIFIED` over 4 386 records |
> | `dxt5_carries_no_colour` `VERIFIED` | `dxt5_carries_no_colour` **REFUTED**, same subject | `VERIFIED` for the falsity |
> | `dxt5_layers_are_distinct` `VERIFIED` | **REFUTED**, same subject | `VERIFIED` for the falsity |
> | 4 mask channels per texture | **4** channel bindings, one chain | — |
> | no `Diffuse` role because no record carries colour | no `Diffuse` role because **what colour is FOR is not established** | — |
>
> **What did NOT change, and is worth saying plainly: the sampler role is still
> unknown, and so is the purpose of every lane of a `DXT5` record.** The
> refutation moved the *content* boundary, not the *purpose* boundary. §7, §6 and
> §7.2a/§7.2b are untouched and still measured.

This document is written in the order the work was done: **hypothesis →
measurement → evidence → conclusion**, with the corpus tables inline. Every
number here was measured; nothing is quoted from another document without
saying so, and where a document disagrees with the bytes the bytes win and the
disagreement is recorded in [§8](#8-refutations).

Reproduce with:

```bash
cargo test -p spore-material --release -- --ignored --nocapture --test-threads=1
```

which runs seven `#[ignore]`d census tests. They **re-measure with the Rust
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

**All three formats decode.** 2 954 of 2 954 records satisfy the framing
identity exactly.

| `fourcc` | records | shapes | status in `spore-texture` |
|---|---:|---|---|
| `0x35545844` `DXT5` | **1 747** | 64²×7 → 67, 256²×9 → 306, 512²×10 → 1 374 | decoded, **BC3** |
| `0x00000015` | **901** | 32²×6 → 67, 256²×9 → 294, 512²×10 → 540 | decoded, uncompressed |
| `0x31545844` `DXT1` | **306** | 256²×9 → 306 | decoded, **BC1** |

Envelope census, 2 954 of 2 954: `version == 1`, `field_10 == 8`. `field_10` is
read by **no** decoder and its meaning is `UNKNOWN`.

`field_18` is **the base level's own byte count in all three formats**, which
`docs/MATERIALS-DESIGN.md` §1 records as "unresolved" and the previous version of
this document recorded as format-specific:

| family | `field_18` | records |
|---|---|---:|
| `DXT5` | `ceil4(w)·ceil4(h)·16` | 512² → `0x00040000`, 256² → `0x00010000`, 64² → `0x00001000` | 1 747 / 1 747 |
| `0x15` | `w · h · 4` | the base level's payload size | 901 / 901 |
| `DXT1` | `ceil4(w)·ceil4(h)·8` | 256² → `0x00008000` | 306 / 306 |

It is not "never read" any more: the decoder **checks** it and refuses a record
whose header disagrees with its own geometry. It is a redundant restatement of
the geometry, which is exactly what makes it useful as a cross-check.

### 2.2 There is one chain, and the old layer split is a double error

**REFUTED 2026-10-05 — this subsection replaces the previous one in full.**

The previous rule was `n = (size − 32) / (16 + chain)` for `DXT5`, which had to
divide exactly. It "found" **two** layers on 1 374 records and refused 373.

The measured rule is an **exact-consume identity**:

```text
record_size == 32 + base_level_bytes
                     + SUM over m in 2..mip_count of (4 + level_m_bytes)
```

with `level_m_bytes == ceil4(w>>m) · ceil4(h>>m) · 16` for the block formats. It
closes with nothing left over on **2 954 of 2 954** records here and **4 386 of
4 386** across `Spore_Content.package` and `Spore_EP1_Content_01.package`.

Per shape:

| shape | records | record bytes | old `n` | measured chains |
|---|---:|---:|---:|---:|
| 512²×10 | 1 374 | 349 616 | 2 | **1** |
| 256²×9 | 306 | 87 468 | refused (short by 4) | **1** |
| 64²×7 | 67 | 5 540 | refused (short by 12) | **1** |

**Why the two errors cancelled.** A 16-byte block read at 8 bytes per block is
two half-pictures. Dividing the payload by `16 + chain` then finds "2 layers"
where there is one picture. The cancellation is exact only when the geometry
lines up — which is the 1 374 — and off by 4 or 12 bytes otherwise — which is the
373.

The 4 and 12 stray bytes are therefore **not a mystery any more**: they are the
remainder of reading a whole picture at half scale. The question the previous
§9.5 kept as `UNKNOWN` is answered; it is *this*.

**Consequences for this crate, all of them load-bearing:**

* `ResolvedTexture::layers.len() == 1` and `layer_count == 1`.
* `ResolvedTexture::channels().len() == 4`, **not 8**.
* `ResolvedTexture::layer(1)` is `None`. A renderer written against the
  two-layer reading got a second image; there is none.
* `inspect_envelope` reports `layer_count == 0`, not 1: a header read has looked
  at nothing that could answer the question.

The subject `layer_count_is_derived` is kept in the claims table **as a
refutation** under its original spelling, so the refutation is findable from the
name of the thing it refutes.

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

## 4. Hypothesis B, REVISED: a `DXT5` record **is** a colour image

> **This section replaces the previous §4 in full.** The previous one measured
> that a `DXT5` record "is not a colour map" and graded it `VERIFIED`. That
> measurement was real arithmetic over **this crate's own broken codec**, and the
> corpus refutes it. The old text is summarised in §4.1 so the refutation is
> readable against the claim it refutes; §4.2 is the new measurement; §4.4 is the
> grade, and it is the part the old section got wrong in the other direction.

### 4.1 What the previous version asserted, and why it was a tautology

The previous §4 measured, over 1 374 decodable `DXT5` records and
720 371 712 base-mip texels:

| statistic | result asserted |
|---|---|
| widest **green** span across any record | **0** |
| widest **blue** span across any record | **0** |
| largest red distinct-value count | **2** |
| exact distinct RGB triples over a 477 771-entry palette sample | **2**: `(0,0,0)` and `(8,0,0)` |

The old text called this **structural** and explained why:

> `tools/spore/dxt5/dxt5.py` reads the block's colour field as two 6-bit values
> (`c0 = b[2] & 0x3F`, `c1 = bits 6–11`) and expands them through `_rgb5`, whose
> green term is `(v >> 10) & 0x1F` and whose blue term is `(v >> 15) & 0x1F`. For
> any `v ≤ 0x3F` **both are identically zero.**

That explanation is **correct**, and it is the refutation. Green and blue read as
zero **by construction**, because a 6-bit value cannot set bits 10..19. The
measurement measured the codec's arithmetic, not any record. It is the classic
"observation and interpretation became indistinguishable" defect that
`spore_core::evidence` exists to prevent, and it was graded `VERIFIED`.

The second half of the old reasoning — "the block stride was 8 bytes" — is what
made the wrong codec the only one on offer, and §2.2 shows the records refute it.

### 4.2 What a real BC3 `DXT5` record's four lanes contain

Measured in Rust by `spore-material`'s own `MipStatistics` (exact occupancy sets,
exact integer cross-sums, **not** sampling), over the base mip of **all 1 747**
decodable `DXT5` records and 380 514 304 texels:

| statistic | red | green | blue | fourth plane |
|---|---:|---:|---:|---:|
| smallest distinct-value count, any record | **32** | **64** | **32** | **1** |
| largest distinct-value count | 235 | 255 | 235 | 256 |
| global minimum value | 0 | 0 | 0 | 0 |
| global maximum value | 255 | 255 | 255 | 255 |
| widest span, any record | 255 | 255 | 255 | 255 |
| records with **≤ 2** distinct values | **0** | **0** | **0** | 46 have **1** |

| statistic | result |
|---|---:|
| records the colour signature admits | **1 747 / 1 747** |
| records whose fourth plane is **constant** | **46** |
| mean of the three colour-pair correlations, median | **0.510** (min −0.078, max 0.712) |
| mean \|colour↔fourth-plane\| correlation, median | **0.067** (min 0.004, max 0.539) |
| records below a 0.30 mean colour correlation | **467** |
| distinct RGB triples in `0x2f4e681c:0x40632900:0x067a07f0` | **4 436** |

Per-document-asset records, slot by slot:

| record | distinct (r, g, b, a) | max fourth plane | mean (r, g, b, a) |
|---|---|---:|---|
| `0x40632900:0x067a07f0` | 92, 121, 73, 21 | 73 | 76.99, 80.45, 86.45, 0.151 |
| `0x40632901:0x067a07f0` | 184, 238, 198, 32 | 73 | 18.39, 31.91, 30.13, 0.559 |

**"DXT5 carries no colour" was a tautology of a broken codec.** It does carry
colour: three lanes that each take at least 32 distinct values, mutually
correlated at a median of 0.510, and **0 of 1 747** records have a colour lane of
two values or fewer. The single most direct statement of it is the 4 436
distinct RGB triples in a single 512² base mip of the documented asset's first
texture.

### 4.3 What the fourth lane is, and the 46 records that have none

The fourth plane is **statistically independent** of the colour triple: median
mean-absolute correlation **0.067**, max **0.539**, against a median mutual
correlation of **0.510** among the colour lanes themselves. It is *not* the fourth
colour channel, and it is *not* a function of the first three.

**46 of 1 747 records hold a constant, all-zero fourth plane.** That is a
measured population, and it has a direct consequence for the role signature:

> **The signature has no condition on the fourth plane at all.**

The previous signature required "alpha takes at least two distinct values". That
condition is refuted by 46 real records. A signature asserting it would be
claiming corpus support for a condition the corpus contradicts, so the condition
is **gone** and the per-record answer is published instead:
`ResolvedTexture::mask_plane_varies() -> Option<bool>` — `Some(true)` /
`Some(false)` for a `colour-and-mask` record, `None` for anything else, including
a record that was only inspected.

### 4.4 Grade, and the ceiling that produced it

| subject | grade | why |
|---|---|---|
| `colour_and_mask_content` | **`INFERRED`** | a statement about what a decoded BC3 texel *contains* cannot outrank the codec that decoded it, and `spore_texture::claims::BC3_CODEC` is `INFERRED` |
| `mask_plane_independent_of_colour` | **`INFERRED`** | comparing the colour lanes with the fourth lane depends on the colour half, whose codec is likewise `INFERRED` |
| `texture_content_role` | **`INFERRED`** | one subject records both content families, so it sits at the floor of the two; a *lowering*, not a convenience |
| `normal_map_content` | **`VERIFIED`** | the `0x15` family is uncompressed and its texel layout is graded `VERIFIED` upstream on an independent measurement (lane 0 tracks the derived `Z` at correlation 0.954) |
| `mask_layer_purpose` | **`UNKNOWN`**, non-finding | content known, purpose not |

`spore_material::claims::bc3_claim_ceiling()` returns the upstream codec's grade,
and a test pins every claim about decoded BC3 texels **at or below** it. That is
the rule that stops this crate from re-asserting a codec reading the crate that
wrote the decoder has not measured.

### 4.5 Conclusion

`TextureRole::ColourAndMask`. Three content classes, three grades, kept apart:

* `colour_and_mask_content` — **INFERRED**. The colour is real.
* `mask_plane_independent_of_colour` — **INFERRED**. The fourth plane is separate.
* `mask_layer_purpose` — **UNKNOWN**. What either is **for** is not established.

**There is still no `Diffuse` variant, and the reason has changed, which is the
point worth carrying forward.** It used to be "no raster record in this corpus
carries colour". That was false. It is now: *"what colour a record is for is not
established"* — no shader has been read, none of the six referenced group families
has a canonical name, and `ChannelRole::ColourPlane::purpose_established()` is
`false` for every lane of every `DXT5` record. `tests/honesty.rs` fails if a
recorded value names a purpose the corpus did not establish.

---

## 5. What replaced §5: one chain, two records, four channels

> The previous §5 was "Multi-layer DXT5 as distinct surfaces" and measured that
> layer 0 ≠ layer 1 byte-for-byte in **1 374 / 1 374** decodable records and
> **1 233 / 1 233** referenced record-instances, concluding that a three-entry
> model offers **five** surfaces (two records × two alpha planes, plus one normal
> map).

**Both numbers were real arithmetic over the wrong thing.** Layer 0 and layer 1
were the two *halves of one base mip*, which an 8-byte block stride had split in
two — which is exactly why they differed byte-for-byte. A correct 16-byte stride
finds one picture where the old rule found two.

What survives, and it is a different statement:

* A raster record is **one chain of mip levels**: `layers.len() == 1`,
  `layer_count == 1`, `channels().len() == 4`.
* A model's two `DXT5` references are **two distinct records**, and that is worth
  keeping — no 3-entry gmdl texture set names the same `DXT5` record twice
  (**0 / 1 922**), so a model's two `colour-and-mask` slots are always two
  different images. They are still **not** two layers of one image.
* A three-entry model therefore offers **three** surfaces — two colour images with
  a fourth plane each, plus one normal map — and **no** lane of any of them has an
  established purpose.

The old claim `dxt5_layers_are_distinct` is kept in the claims table **as a
refutation** under its original spelling, with the refuted numbers visible in the
recorded value.

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

`BindingScope::is_inferred()` returns `true` for **every** variant. There is no
proven binding in this build, and a scope that claimed otherwise would be the
failure this crate exists to prevent.

### 7.2a The list the division cuts is not always the record's whole list

**PERMESH, 2026-10-04.** The three measurements above are all taken over
records whose material-info table **closes**. It does not always close: the walk
stops on the undocumented shader-data id `0x218`, whose payload size is UNKNOWN
(`docs/COLOUR-SOURCES.md` §2.4 — 850 candidate sizes, all eliminated, so a size
is not guessed). Over all **4 209** gmdl records:

| | count |
|---|---:|
| material-info closes | **3 589** |
| walk stops on `0x218` | **619** |
| …of those, stopping **before the last declared block** | **100** |
| …of those, the truncated list still dividing evenly | **100** |
| …of those, the truncated list dividing evenly *by the mesh count* | **100** |

The missing part is *whole blocks*, so a truncated list frequently divides
evenly and the arithmetic succeeds on an incomplete input. Before the fix all
**619** reported `BindingScope::PerMeshBlock`. `0x218` is the **last** entry of
a block, so the other **519** stop *after* their last block's texture set was
read in full — their list is the record's whole list and the cut is sound, which
is why the fix does not refuse those.

`spore-material` cannot be told whether the table closed: `GmdlModel` carries no
such statement, and it is not derivable from the fields it does have —
**1 767** records whose material-info closed still leave bytes unread, so
`GmdlModel::fully_walked()` is a fact about bytes, not about the texture list.

So the cut is asked of the **record** instead, using §7.3's second statement:
`partition_texture_refs` offers the cut only when every slot's derived ordinal
equals the ordinal its group id states.

| scope | records (after) | before |
|---|---:|---:|
| `PerMeshBlock` | **4 108** | 4 208 |
| `WholeModel` | **0** | 0 |
| `Undetermined` | **100** | 0 |

The **100** match, one for one, the 100 the independent block walk in
`crates/spore-differential/tests/material_block_binding.rs` measures as stopping
before their last declared block — two implementations, agreeing on the same set.
**0** of the 16 428 slots in the 4 108 offered cuts are contradicted by the
record.

### 7.2b Block order is not merely unproven — it is unfalsifiable here

The one thing that could **refute** block-*i*-is-mesh-*i* is a record whose
blocks name **different** texture sets: then a permutation would move textures
between meshes and be visible. Measured over all 4 209 gmdl records:

| | count |
|---|---:|
| cut records with more than one mesh | **1 685** |
| …of those, **every** per-mesh slice naming the **same** texture set | **1 685** |
| …of those, slices that differ | **0** |

That matches the independent `(meshes, blocks)` census exactly: `(2, 2, 3)` in
1 615 records plus `(3, 3, 3)` in 70.

So permuting the blocks of any record in this corpus changes **no byte a
renderer can observe**. The ordering is not a gap a better partition function
could close: the evidence that would settle it is not in the texture references.
It would take the per-block `0x210` render parameters, the vertex buffers' UV
sets, or geometry — none of which this workspace has read. `material_block_is_mesh`
stays a non-finding, and `is_inferred()` stays `true` for all three scopes.

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

**One reference states nothing at all.** 150 references across 132 models name
group id **zero** (§7.5), which belongs to none of the six `0x40XX2900` families,
so its low byte is `0` by arithmetic rather than by statement.
`TextureSlot::group_states_an_ordinal()` says so, and both `ordinal_agrees()`
and the cut in `partition_texture_refs` **skip** such a slot rather than reading
it as "slot 0". Measured cost of that rule: **0** of the 132 records lose their
cut. Reading it as ordinal `0` instead would have refused **132** sound cuts —
the same mistake in the opposite direction, and the reason the census asserts
the number rather than the intent.

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
| 1 | `crates/spore-material/src/lib.rs` (its own prior text) | the documented asset's third texture is **1024×1024** and **refused** | it is **512×512**, 10 mips, 1 chain, `fourcc 0x15`, and it **decodes**. Fixed in this crate; recorded here because the old text was in the crate's own module documentation. |
| 2 | `docs/MATERIALS-DESIGN.md` §1 | "The 1024² records (`raster_1`/`_2`) are format 0x15 (luminance, not DXT5) — out of scope" | `Spore_Content.package` contains **no** 1024² raster of either format. The `0x15` family occurs at 32², 256² and 512² only. Either the records named are outside this package or the size is wrong. |
| 3 | `docs/MATERIALS-DESIGN.md` §1 | `field_1c` is "a float 1.0 in 0x15 records" | in a `0x15` record `field_1c` is **the first texel of the base level**: the documented asset's reads `0x008080ff`, which is bytes `ff 80 80 00` = the flat normal `(255,128,128)` with lane 3 = 0. Already corrected in `spore-texture`'s claims. |
| 4 | `docs/MATERIALS-DESIGN.md` §1 | `field_18` "**unresolved** (same in every sample)" | it is the base level's own byte count in **all three formats**, verified in 4 386 of 4 386 records, and the decoder **reads** it as a cross-check. |
| 5 | `docs/MATERIALS-DESIGN.md` header | "PNG32 (records are RW4 containers, §2 — not raw PNG)" | refuted before this work: a `png`-typed record (`0x2F7D0004`) begins `89 50 4E 47 0D 0A 1A 0A` and is **raw PNG**; an `rw4`-typed record (`0x2F4E681B`) begins `52 57 34 77 33 32 00` = `"RW4w32"`. Restated in `spore-material`'s crate docs. |
| 6 | this crate's own `sampler_role` basis | "unknown **because** the 16 bytes were not decoded" | the bytes are decoded now and carry **no role field** (§6). The non-finding stands; the reason is stronger. Reason code renamed; old one recorded as refuted. |
| **7** | **this document's §4** | "a `DXT5` record is not a colour map", `dxt5_carries_no_colour` **VERIFIED**, green and blue spans of **0** | **REFUTED.** "DXT5 carries no colour" was a **tautology of a broken codec**: green and blue read as zero under R5G5B5 on a 6-bit word *by construction*. Read as BC3, all 1 747 records carry ≥ 32 / ≥ 64 / ≥ 32 distinct values in lanes 0–2, **0** have a colour lane of ≤ 2 values, and the documented asset's first record holds **4 436** distinct RGB triples in one base mip. See §4. |
| **8** | **this document's §2.2 and §5** | `DXT5` records hold **2 layers**; 373 of 1 747 are 4 or 12 bytes short of two layers and are refused; `dxt5_layers_are_distinct` **VERIFIED** at 1 233 / 1 233 | **REFUTED.** The 8-byte block stride is wrong — every recorded level prefix is the 16-byte figure — and dividing the payload by `16 + chain` made one picture look like two. There is **one chain**; the "two layers" were its halves, which is why they differed byte-for-byte; and the 4/12 stray bytes are the remainder of reading a whole picture at half scale. See §2.2. |
| **9** | **this document's §10.3 step 3** | "a mask is FOUR surfaces, not one — a `DXT5` record has two layers and each layer's alpha plane is its own channel" | **REFUTED**, and this is the one that reached the renderer: `docs/ENGINE-SCENE.md` and `spore-engine` wired **four mask planes per three-entry model**. There is one record per slot, so a slot has **one** image and **four** lanes, of which the colour triple is real. See §10.3. |
| **10** | **this document's §10.5** | "do not treat their RGB planes as colour" | **REFUTED** with §7 above. The RGB planes *are* colour; what they colour is what is not established. |
| **11** | **this crate's `envelope_field_18_meaning` basis** | "re-published from spore_texture::claims; no decoder reads the word" | the decoder **reads** it, as the base level's byte count, in 4 386 of 4 386 records. Republished as a delegated `VERIFIED` value instead of a local non-finding — the second time this crate would have contradicted its own decoder. |

---

## 9. What is still UNKNOWN

Stated as a list, because "unavailable" is never zero, empty or `false`. Items
**7**–**9** are new or changed in the 2026-10-05 revision.

1. **What the fourth plane of a `DXT5` record is for.** Two per three-entry
   model, none named. `mask_layer_purpose` — `UNKNOWN`. *Its content is now
   measured* (§4.2): one 8-bit channel, statistically independent of the colour
   triple, and constant in 46 of 1 747 records.
2. **Which shader stage each texture feeds.** No shader has been read.
   `sampler_role` — `UNKNOWN`, `no_role_field_in_texture_entry`.
3. **Whether material-info block *i* is mesh *i*.** `material_block_is_mesh` —
   `UNKNOWN`, `material_info_block_order_not_established`. **And unfalsifiable
   from this corpus**: all 1 685 cut records with more than one mesh have every
   per-mesh slice naming the same texture set, so a permutation is invisible
   (§7.2b).
4. **What the `DXT5` colour is FOR.** §4.4. This is the question that keeps
   `TextureRole::Diffuse` out of the enum, and it is a different question from
   §4.2's "these records carry colour", which is answered.
5. **Whether Spore's own shader sees the colour these records carry, byte for
   byte.** The BC3 codec is `INFERRED`: the stride is a byte count and the BC4
   alpha half beats the alternative by a factor of 20–50 on every shape, but no
   decode of a real record has been compared against the game's own output.
   `spore-texture`'s question; it is what
   `spore_material::claims::bc3_claim_ceiling()` exists to keep this crate from
   answering on its behalf.
6. **The `0x15` family's fourth lane.** Statistically independent of the normal
   (correlation 0.032) and **binary in all 540 records of 512², non-binary in all
   294 of 256² and all 67 of 32²** — resolution separates them perfectly, slot
   does not (in the referenced population the two coincide). `INFERRED`
   upstream; **meaning UNKNOWN**. `ChannelRole::UnidentifiedLane`.
7. **Why the block formats do not prefix level 1 and the `0x15` family does.**
   New on this list. It is exact in every record and the reason is not
   established — the bytes where a level-1 prefix would sit in a DXT5 record
   hold block data. `spore_texture::claims::FIRST_PREFIXED_LEVEL`.
8. **What the 132 group-`0x40616201` models intend** (§7.5).
9. **`DXT1`** (`0x31545844`, 306 records, 256²×9): `spore-texture` **decodes** it
   as BC1, and `spore-material` therefore assigns it **no role** — a record whose
   `fourcc` this crate has no signature for is `TextureRole::Unidentified`, which
   is a *landing place*, not a claim that DXT1 has no content. Whether DXT1 is a
   third role is **UNKNOWN**.
10. **A material id's meaning.** `material_name` — `UNKNOWN`,
    `no_material_registry`, unchanged.
11. **The three documented-asset group names** (`0x40632900/01/02`) remain absent
    from `spore-core`'s table, so `resolved_group_name` is still a non-finding.
    §7 now explains what those ids *encode* without naming them.
12. **`0x218`'s on-disk size**, and therefore whether a record's flat reference
    list is its whole set. UNKNOWN and must stay that way: 850 candidates, all
    eliminated by a trailer that cannot close for this family
    (`docs/COLOUR-SOURCES.md` §2.4). `spore-material` does not guess it and does
    not need it — it asks the record's own ordinals instead (§7.2a) — but the
    question stays open.

**Closed in this revision:** the previous item 5, "the 4 and 12 stray bytes in the
373 `DXT5` records that do not divide into layers" — answered by §2.2; they are
the remainder of an 8-byte read of a whole picture. And the previous §4's
question "are the colour field's 12 bits meant to be visible" survives, renumbered
as item 5 above, but on a corrected footing: the field is **read** now, so the
question is about the codec rather than about a discarded field.

---

## 10. Engine specification

**For the agent owning `crates/spore-engine`.** Bevy-free contract: everything
below is plain data from `spore-material`.

> ### ⚠ WHAT CHANGED FOR THE ENGINE, 2026-10-05
>
> **The per-mesh normal-map and mask wiring changed, and a build break is the
> least of it.** Read this box before anything else in §10.
>
> | was | is |
> |---|---|
> | `TextureRole::LayeredAlphaMask` | `TextureRole::ColourAndMask` |
> | `ChannelRole::DegenerateColour` | `ChannelRole::ColourPlane` (`as_str` `"colour-plane"`) |
> | `ChannelRole::MaskChannel.as_str()` | `"mask-plane"` |
> | `TexSetShape::MaskNormal` | `TexSetShape::ColourAndMaskThenNormal` |
> | `TexSetShape::MaskMaskNormal` | `TexSetShape::ColourAndMaskTwiceThenNormal` |
> | `layer(1)` was a second mask channel | `layer(1)` is **`None`** |
> | **four** mask channels per three-entry model | **two** `colour-and-mask` textures, **four** lanes each |
> | `TexSetShape::as_str()` `"(mask, mask, normal)"` | `"(colour-and-mask, colour-and-mask, normal)"` |
> | `claims::ENVELOPE_FIELD_18_MEANING` was a non-finding | it is a delegated **`VERIFIED`** value |
>
> Three consequences a caller must handle, in order of how badly they hurt:
>
> 1. **`texture.layers` has length 1.** `layer_count == 1`, `layer(1)` is `None`,
>    `channels().len() == 4`. Any code that iterated `texture.layers` to build a
>    surface per layer now builds **one**. Any code that indexed `layer(1)` was
>    reading the top half of a base mip.
> 2. **The colour is real.** `resolved.textures[i].image` for a `colour-and-mask`
>    record is an RGBA8 image with genuine colour in RGB. It is no longer correct
>    to upload it as a single-channel mask, and no longer correct to zero out its
>    RGB "because the codec makes them degenerate".
> 3. **And the colour's purpose is still UNKNOWN.** Do not turn (2) into "so bind
>    it as the base colour". See §10.3 step 3.

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
  .layers     : Vec<MipImage>      // base mip of every chain: ALWAYS length 1
  .layer(i)   : Option<&MipImage>  // layer(1) is None — there is no second chain
  .image      : MipImage           // == layers[0]; kept for existing callers
  .layer_count : usize             // 1 for a decoded record, 0 from inspect_envelope
  .channels() : Vec<ChannelBinding>// 4 per chain — FOUR, not eight
  .mask_plane_varies() -> Option<bool>
  .pixels_decoded : bool
  .claims     : Vec<TextureClaim>

MipStatistics  // the evidence, all of it exact integers, none of it sampled
  .texels, .distinct[4], .min[4], .max[4], .sum[4], .squares[4], .cross[6]
  .mean(lane)                   -> Option<f64>   // None when texels == 0
  .is_constant(lane)            -> bool
  .correlation(a, b)            -> Option<f64>   // None when UNDEFINED, never 0.0
  .mean_colour_correlation()    -> Option<f64>   // median 0.510 over the corpus
  .mean_alpha_correlation_abs() -> Option<f64>   // median 0.067 over the corpus

MeshMaterial
  .mesh_index  : u32
  .material_id : u32      // no name, ever
  .slots       : Vec<TextureSlot>
  .scope       : BindingScope

TextureSlot
  .reference              : GmdlTextureRef
  .ordinal : Option<u32>  // None under WholeModel and Undetermined
  .group_stated_ordinal() : u32
  .ordinal_agrees()       : bool
```

**`correlation` returns `None` for three different reasons and all three are
real**: no texels were read, fewer than two were read, or one lane is constant
(zero variance, so no linear relationship exists). Do not substitute `0.0` —
"these lanes do not vary" and "these lanes are unrelated" are different findings,
and 46 corpus records take the `None` path.

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

**Step 2 — bind where the role says so, not where the index is.**

```rust
for mesh in &resolved.meshes {
    for (within, slot) in mesh.slots.iter().enumerate() {
        let flat = match resolved.scope {
            BindingScope::PerMeshBlock { refs_per_mesh } =>
                mesh.mesh_index as usize * refs_per_mesh + within,
            // A third arm. `Undetermined` means the flat list is SHORTER than
            // the record's whole texture set -- the gmdl walk stopped on the
            // undocumented shader-data id 0x218 before reading every block's
            // texture set -- so every mesh holds the whole short list and
            // `within` is the flat index. Falling through to the `PerMeshBlock`
            // arm here is exactly the bug that fix closed: it bound mesh 1 to a
            // texture out of mesh 0's block.
            // Prefer `resolved.roles()`, which already handles all three.
            BindingScope::WholeModel | BindingScope::Undetermined { .. } => within,
        };
        let role = resolved.role_of(flat);
        match role {
            TextureRole::TangentSpaceNormal => { /* step 4 */ }
            TextureRole::ColourAndMask     => { /* step 3 */ }
            TextureRole::Unidentified      => { /* no claim; see step 5 */ }
        }
    }
}
```

`ModelTextures::roles()` does exactly this and returns
`Vec<Vec<TextureRole>>` indexed by mesh then slot. Prefer it.

**Step 3 — a `colour-and-mask` record is ONE RGBA image with a fourth plane.**

```rust
// One record -> ONE surface. There is no layer 1; `layer(1)` is None.
let image = &texture.image;                       // 512*512*4 bytes of RGBA8
let planes = texture.channels();                  // exactly 4
assert_eq!(planes.len(), 4);
// planes[0..3] are Lane::Red/Green/Blue with ChannelRole::ColourPlane
// planes[3]    is Lane::Alpha     with ChannelRole::MaskChannel
```

**Upload it as one RGBA8 surface.** The RGB planes carry real colour and the
alpha plane is a separate channel; they belong to the same texture, not to two
textures or two layers.

What you may **not** do with them:

| do not | because |
|---|---|
| upload it as a single-channel mask | the RGB planes are a colour image (§4.2) |
| zero or flatten the RGB planes | the old `DegenerateColour` spelling measured a broken codec |
| call the alpha plane "specular", "gloss", "mask-weighted" | `mask_layer_purpose` is `UNKNOWN`; `tests/honesty.rs` fails if a recorded value names a purpose |
| bind it as the model's base colour | **the colour is real; what it colours is not established.** There is no `Diffuse` role, and the reason is §4.4, not §4.2 |
| build one surface per `texture.layers` entry | there is one |

If you need the alpha plane alone as a modulation channel,
`texture.statistics` and `texture.mask_plane_varies()` tell you whether it
varies at all — **46 of 1 747 real records carry an all-zero fourth plane**, so a
renderer must handle a `Some(false)` rather than assume a channel is present.

For the corpus's three-entry shape that is **two** such surfaces plus one normal
map — three surfaces, not five.

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
    // matched no signature for its fourcc is a *finding*, and the statistics are
    // on `texture.statistics` if you want to see why. A `DXT1` record lands here
    // too: spore-texture decodes it and this crate has no signature for it.
}
```

`Unidentified` means *this record's own format word's* signature declined — the
other family's signature may well hold. A `0x15` record whose bytes are a colour
field is `Unidentified` while satisfying the colour signature perfectly.

The engine already has `MISSING_TEXTURE_TINT` for "no texture". Keep it for both
cases and keep them distinguishable in a log line via `role_of`.

### 10.4 What to log

```
model 0x…  meshes=2  scope=per-mesh-block(refs=3)  shape=(colour-and-mask, colour-and-mask, normal)
  mesh 0  material=0x407dfddb
    slot 0  0x40632900:0x067a07f0  colour-and-mask  chains=1  512x512x10
    slot 1  0x40632901:0x067a07f0  colour-and-mask  chains=1  512x512x10
    slot 2  0x40632902:0x067a07f0  tangent-space-normal chains=1 512x512x10
```

`ModelTextures::role_census()` produces the `colour-and-mask 2 /
tangent-space-normal 1` tail directly.

### 10.5 Do not do these

| do not | because |
|---|---|
| apply the first texture as diffuse colour | **its colour is real, its purpose is not established** (§4.4) |
| read `texture.image` and call it the model's colour | it is the base mip of slot 0's chain; "the model's colour" is a claim §4.4 forbids |
| build one surface per `texture.layers` entry, or index `layer(1)` | there is **one chain**; `layer(1)` is `None` (§2.2) |
| upload a `colour-and-mask` record as a single-channel mask | the RGB planes are a colour image (§4.2) |
| assume `textures[i]` belongs to `meshes[i]` | only under `PerMeshBlock`, and the mapping is `i * refs_per_mesh + j` |
| match on `BindingScope` with only two arms | there are **three**; `Undetermined { collected, contradicted }` is the one where the reference list is short (§7.2a), and its slots are the whole collected list |
| assume `ordinal_conflicts == 0` means the ordinals agreed | `ModelTextures::ordinal_agreement.compared == 0` means **nothing was compared**; read `.checked()` |
| bind mesh *i* to `textures[i * refs_per_mesh]` without reading `scope` | under `Undetermined` there is no per-mesh width, and multiplying by one is the wrong binding |
| treat `BindingScope::PerMeshBlock` as proven | `is_inferred()` is `true`; block *i* is not known to be mesh *i* |
| read file byte 0 of a normal map as R | it is **Z** |
| name the alpha plane "specular" or "gloss" | UNKNOWN (§9.1); `tests/honesty.rs` fails if a recorded value does |
| treat `Unidentified` as a decode failure | the record may have decoded fine and matched no signature for its fourcc |
| treat `colour-and-mask` as `VERIFIED` | it is `INFERRED`, capped by the BC3 codec's own grade (`bc3_claim_ceiling()`) |
| substitute `0.0` for an undefined `MipStatistics::correlation` | `None` means "does not apply"; 46 records take that path |
| add a `Diffuse` variant | a claim about a **purpose** nobody established (§4.4) |

### 10.6 Expected results on the two documented assets

| asset | meshes | scope | shape | roles |
|---|---:|---|---|---|
| `…:0x40637e03:0x067a07f0` | 1 | `PerMeshBlock { refs_per_mesh: 3 }` | `ColourAndMaskTwiceThenNormal` | colour-and-mask, colour-and-mask, normal — `3/3` resolved, ordinals `3/3` agree, **`layers` 1 / 1 / 1** |
| `…:0x40627100:0x067c79d2` | 2 | **`Undetermined { collected: 2, contradicted: 1 }`** — was `PerMeshBlock { refs_per_mesh: 1 }`, which was wrong (§7.2a) | `Unavailable` | both meshes are handed the **whole** collected list: colour-and-mask, normal. `ordinal_agreement.compared == 0` — nothing was compared, so read `.checked()` |

The first row is asserted by `tests/real_corpus_roles.rs::the_documented_assets_roles`
and `tests/real_asset.rs`, which print every field above. Run them before
trusting any of this.

---

## 11. Test inventory

| file | what it protects | count |
|---|---|---:|
| `src/{material,claims,resolve,role,error}.rs` (unit) | the cut and the record cross-check, ordinals, claim grades, **both** content signatures, the correlations, the refutation set, refusals | **69** |
| `tests/roles.rs` | role assignment from synthetic records; Z-first order; the one-chain model; the refutations as recorded values | 12 |
| `tests/resolve.rs` | the resolve surface, strict-gmdl refusals, assumed type, the inspect-vs-decode split | 13 |
| `tests/errors.rs` | typed refusals, each named | 13 |
| `tests/robustness.rs` | empty and corrupt inputs answer rather than panic | 10 |
| `tests/honesty.rs` | non-findings, and the purpose sweep sharpened **twice** | 10 |
| `tests/bindings.rs` | per-mesh partition, fallbacks, cross-checks | 9 |
| `tests/reference_completeness.rs` | **the truncated-list fix**: 5 hermetic, 2 `#[ignore]`d corpus censuses | **7** |
| `tests/real_asset.rs` | the documented asset end to end (`#[ignore]`d) | 1 |
| `tests/real_corpus_roles.rs` | **7 `#[ignore]`d corpus censuses** | 7 |

**141 hermetic tests, 10 corpus tests.** `cargo test -p spore-material` stays
hermetic and needs no game, no GPU and no network.

`tests/reference_completeness.rs` is the file that pins §7.2a and §7.2b. Its
fixture `truncated_material_gmdl` reproduces the documented creature's material
info byte for byte — `0x210` (20 bytes), then a two-entry `0x20D`, then `0x218`,
with **two** declared blocks — so the hermetic tests drive the **real**
`spore_gmdl::parse_recovering` and observe the real typed `StopReason`. It
asserts that `2 / 2 == 1` divides, that the cut is refused anyway, and that
every mesh is handed the whole collected list.

### 11.1 The 2026-10-05 rewrite of `tests/support/mod.rs`

The synthetic fixtures were themselves built from the refuted layout, and that
is worth recording because it is the most transferable thing in this section:
**a test suite built on a wrong format fails for the wrong reason.** Every
`DXT5` fixture wrote **8-byte blocks and 16-byte layer headers**, so the real
decoder refused every one of them and all twenty-odd role assertions downstream
were failing on the fixture rather than on the code under test. The fixtures are
now built from the measured framing identity — 16 bytes per block, `field_18` =
the base level's byte count, a `u32` before every level from 2 on, no layer
headers, one chain — with a hand-written `bc3_block` and an `r5g6b5` packer
written from the published block layout and *not* shared with the decoder.

`colour_block(k)` is chosen so the fixture **mirrors the corpus on the axis the
role check rests on**: every colour channel rises with the block index, so the
three lanes come out mutually correlated (measured 0.76 on a 16×16 fixture,
against the corpus's 0.510 median), and the fourth plane is *scrambled* across
the index rather than monotone, so it does not track the colour (measured 0.089,
against the corpus's 0.067 median). A fixture whose three colour lanes moved
independently would decode to something no Spore record contains, and every
statistic taken from it would be a statistic of the fixture.

`tests/real_asset.rs` is worth a note: it used to assert that the documented
asset yields *two* images and *one* `UnsupportedFourcc` refusal, on the stated
grounds that its third reference is a *1024x1024* `0x15` record. **All three
halves of that were wrong** — the record is 512x512, it decodes, and the two
`DXT5` records hold one chain each — so the test was updated rather than deleted,
and the old expectations are refutations #1, #7 and #8 in §8. It now also pins
the 4 436 distinct RGB triples, because that is the most direct statement that
"these records carry no colour" was a property of a codec.

`tests/honesty.rs`'s purpose sweep was **sharpened twice, not relaxed**. The
original forbade the words "diffuse"/"specular"/"normal_map"/"bump"/"emissive"
outright, which was right when this crate knew nothing about any texture's
content. It then knew two things and permitted naming a normal map. The
refutation took away the second of those, and the invariant is now sharper than
at any point before: **not one lane of a `DXT5` record has an established
purpose**, where the refuted model let its alpha plane look like a mask with a
job. The sweep's positive half was extended too — it now asserts that the
recorded values mention "colour" and "refuted", so the suite cannot pass by
denying that these records carry colour or by staying quiet about what changed.

---

## 12. Clean-room

Nothing here is derived from EA or Maxis code, data or assets. The layouts were
read out of record bytes, out of this repository's own Python oracles under
`tools/spore/`, and out of the community SDK's transcribed type tables. No Spore
asset byte is committed: every fixture in `tests/` is synthesized in the test,
and the real-corpus tests read the git-ignored `SPORE/` tree at run time and
write nothing.