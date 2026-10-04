# VALIDATION.md — what the Rust half is actually checked against

**Agent VALIDATION, 2026-10-04.** This document records what is now protected by
tests, what is deliberately **not** protected and why, and the open risks. Every
number in it was measured on this checkout against
`SPORE/Data/Spore_Content.package` (995 268 083 bytes, 17 119 records, GOG
digital 3.1.0.22) unless it says otherwise.

**Revision 2 — Agent DIFFFIX, 2026-10-05.** A byte count over the raster corpus
refuted the two-layer framing this crate's raster tests asserted, and this
revision rewrites them to the new truth, extends the raster measurement to the
second content package, and records two findings that came out of it: the
**refuted raster oracle** (§1.3) and a **defect in `spore-texture`'s walk past
level 32** (§1.4). Sections touched: 1.1, new 1.3 and 1.4, 2.1, 2.3, 2.5, 3, 4, 5.

Read it as an inventory of *evidence*, not as a score. A test that re-asserts
the implementation is listed under "Deliberately not covered" even where it
exists.

---

## 1. The findings

All of them are real, all are measured over the whole corpus, and none is a
hypothetical. 1.1 and 1.2 are unchanged from the first revision; 1.3 and 1.4 are
new in this one; 1.5 is a stale test in another crate.

### 1.1 `0x218` truncates the reference list, and the per-mesh cut does not notice

**Where:** `crates/spore-differential/tests/material_block_binding.rs`.

The gmdl material-info walk stops on the undocumented shader-data id `0x218`.
`spore_gmdl::parse_recovering` records that as a typed `StopReason` and returns
the references it collected **so far**. `spore_material::partition_texture_refs`
then divides that list by the mesh count.

Measured over **all 4 209** gmdl records, with an independent material-info
walker in the test that is validated against `spore_gmdl` on every record
(4 209 of 4 209 collected reference lists identical, so the walker's block
boundaries are aligned to something):

| | count |
|---|---:|
| records whose material-info closes | **3 590** |
| records that stop on `0x218` | **619** |
| …of those, records whose walk stops **before the last declared block** | **100** |
| …of those, records where the partial list still divides evenly by the mesh count | **100** |
| records whose collected list divides evenly by the mesh count | **619 of 619** |
| …of those, records `partition_texture_refs` now refuses as `BindingScope::Undetermined` | **100** |
| records where the even cut would be *arithmetically* wrong (material-info closed) | **0** |

So:

* Among the 3 590 records whose material-info closes, the cut is sound:
  block count equals mesh count in 3 590 of 3 590, every block contributes the
  same number of references, and dividing by the mesh count reproduces the block
  boundaries. **The arithmetic is fine.**
* Among the 619 records that stop on `0x218`, the collected list **divides
  evenly by the mesh count in all 619**, so the division alone cannot tell the
  519 whose list is the record's whole list from the **100** whose list provably
  does not cover one entry per declared block.
* **Post-fix, for the record:** `partition_texture_refs` no longer divides and
  trusts the result. It asks the record — each slot's derived ordinal against the
  ordinal its group id states — and returns
  `BindingScope::Undetermined { collected, contradicted }` for exactly those
  **100** (`PerMeshBlock` for the other 519, `WholeModel` for none; commit
  228a701, after this document's first revision). The test in this crate measures
  the **arithmetic**, which is unchanged, and its two FINDING messages now say
  that rather than claiming a scope the function no longer returns. Nothing in
  the numbers moved; only the description of what the caller does with them.
* The documented creature `0x00e6bce5:0x40627100:0x067c79d2` is one of the 100.
  Its record declares 2 material-info blocks; the walk covers block 0, collects
  **2** references from it, and stops. `refs_per_mesh` is therefore
  `2 / 2 = 1`, so mesh 0 gets block 0's first reference and **mesh 1 gets block
  0's second reference**, while mesh 1's own block contributes nothing to the
  list at all. The engine's reported `ordinal_conflicts=1` for that record is a
  symptom of the same fact seen from another angle.

**What this is not.** It is not a decoder bug: refusing to guess `0x218`'s size
is correct, and `parse_recovering` reports the stop rather than hiding it. It is
a gap between two honest pieces: a walk that stops, and a partition that cannot
know it stopped.

**Where the fix belonged.** `spore-material` (`partition_texture_refs`,
`model_materials`) or `spore-gmdl` (carry "this reference list is partial" on
`GmdlModel`). Neither crate is owned by this agent, so nothing was changed here;
`spore-material` took the first route and this crate's messages were updated to
match.

**What would detect a regression:** `every_record_whose_material_info_closes_has_blocks_equal_to_its_mesh_count`
asserts all six numbers above. If `0x218` is ever sized, `stopped_early` drops
and the test says so rather than quietly passing.

### 1.2 `tools/spore/gmdl/gmdl.py` reads the reference-table keys little-endian

**Where:** `crates/spore-differential/tests/hermetic.rs` and
`crates/spore-differential/tests/gmdl_reference_table.rs`.

The oracle's own docstring states the rule it then breaks:

```text
refCount         u32  (BIG-endian; every other word is little-endian)
  refCount *     { instanceID, groupID, typeID } u32 x3
```

`u32be` is applied to the count and plain little-endian `u` to the three words
of every key behind it. The corpus settles which side is right — a reference key
either names a row in the package's own index or it does not:

| reading | keys declared | keys that resolve |
|---|---:|---:|
| big-endian count, **big-endian** keys | 7 989 | **7 989** |
| big-endian count, **little-endian** keys | 7 989 | **0** |
| little-endian count, big-endian keys | 0 (1 510 records overrun) | 0 |

`spore_gmdl` now reads them big-endian and resolves 7 989 of 7 989. The oracle
resolves 0.

Two consequences:

* `hermetic.rs::a_fixture_run_produces_no_divergence_except_the_one_the_oracle_gets_wrong`
  now asserts **exactly one** divergence, on the `refs` field, with both values
  quoted. It is not deleted and it is not tolerated silently: a harness that
  stopped reporting it would report "clean" the moment either side was "fixed" in
  the wrong direction.
* **The committed fixture `tests/fixtures/mini.gmdl` cannot settle this.** Its
  reference key was *generated* from the oracle's assumption, so its bytes are
  little-endian too, and the little-endian reading is the one that names the gmdl
  type id. That is asserted explicitly, so nobody later uses the fixture as
  evidence for a byte order.

**Where the fix belongs.** `tools/spore/gmdl/gmdl.py`, which this agent does not
own and must not edit ("the Python oracles are the specification; they are never
modified to make them agree"). Until it is fixed, `compare::gmdl`'s `refs` field
will keep reporting one divergence over the fixture and — presumably — over the
corpus.

### 1.3 The raster oracle on this side of the boundary is refuted

**Where:** `crates/spore-differential/src/compare/raster.rs`,
`crates/spore-differential/src/oracle.rs`, `tests/real_corpus.rs`.

The raster comparisons in this crate were built on `tools/spore/raster/raster.py`
and `tools/spore/dxt5/dxt5.py`. Both implement a framing a byte count over the
corpus **refuted**: a derived layer count dividing `(record_size - 32)` by
`16 + chain` where `chain` charges **8** bytes per 4x4 block, a 16-byte "layer
header" in front of every layer, and an 8-byte "Spore block" codec. A 256² `DXT5`
base level occupies 65 536 bytes — 4 096 blocks of **16** — and every recorded
level prefix says 16 (`docs/RASTER-FRAMING.md` §2). Two errors cancelled exactly:
a 16-byte block read at 8 bytes per block is two half-pictures, and two of them
make one picture.

What this crate now measures, over the deterministic 150-record sample of
`Spore_Content.package` (`--release`):

| arm | measured |
|---|---:|
| envelope, eight words, 150 sampled | **150 agree, 0 disagree** |
| layer count | **68 disagreements, 82 refusals by the oracle's own division, 0 agreements** |
| base mip digest, 12 decoded | **8 disagreements, 4 one-sided, 0 agreements** |

Every disagreement is the oracle's rule, not a port defect, and each one says so:
a layer-count divergence quotes the oracle's own `payload` and `stride` next to
the record's real length and the length its own envelope implies, and a digest
divergence states that the block codec is still `INFERRED` and that the byte
count, not the pixels, is what settles the framing. The comparison was renamed
`raster-envelope-layer-count-and-base-mip` because the old name promised agreement
with `dxt5.py`.

Two things follow, and both are in the code:

* **The oracle refuses 82 of 150 sampled raster records with its own division**
  ("1398132 payload bytes not a multiple of 174792"). A refusal is recorded as a
  refusal — never as a layer count, and never as agreement.
* **There is no longer any Python left to check the framing against.** That is
  why `cmd_raster15` became `cmd_rasterframe`, generalised from the `0x15` family
  to all three format words, and why it deliberately imports neither `raster.py`
  nor `dxt5.py`: calling the refuted rule would have made the cross-language check
  vacuous. §2.1.

**Where the fix belongs.** `tools/spore/raster/raster.py`, `tools/spore/dxt5/dxt5.py`
and `src/assets/Dxt5.cpp`, none of which this agent owns and none of which may be
edited to agree. Until they are re-derived against `docs/RASTER-FRAMING.md` §2,
the raster halves of this crate's comparison report a measured disagreement, which
is the correct outcome.

### 1.4 `spore-texture`'s walk mis-places levels past 32 — found here, owned elsewhere

**Where:** `crates/spore-differential/tests/luminance_oracle.rs` →
`the_walk_past_level_32_disagrees_with_the_identity_it_reports`.

`spore_texture::framing::record_size` charges every level past `MIP_FLOOR` (32) a
`u32` prefix plus a 1×1 payload, and `chain_size` agrees with it.
`framing::level_offset` — which `decode_raster` walks with — accumulates only
`0..mip.min(MIP_FLOOR)`, so it omits one prefix plus one 1×1 payload for every
level past the floor. From `mip_count == 34` the tail of a chain is therefore
read from the wrong offsets:

```text
walk past level 32: 6 shape(s) decoded Ok and wrong, 0 refused by name, 0 exact
  1x1 mips=34: level 33 read 8 bytes early (SILENT)
  8x8 mips=40: level 39 read 56 bytes early (SILENT)
```

Six shapes (1×1, 4×4, 8×8 at 34 and 40 levels) all **decode as `Ok`** with the
last level's texels read from another level's bytes, because every level past the
geometry is 1×1 and so every `u32` prefix at the mis-placed offset happens to hold
the right length. Nothing in the corpus has more than ten levels, so no real data
is affected: this is an internal inconsistency between two functions of one
module, reachable only from a synthetic envelope.

It is **pinned, not worked around**, with the shortfall number a fix must make
zero. Deleting the shapes would have left a green suite and no idea.

**Where the fix belongs:** `crates/spore-texture`, whose owner is another agent.

### 1.5 A stale test in another crate, now failing

`crates/spore-assets/tests/graph.rs:344`,
`the_graph_reads_the_reference_table_big_endian_and_spore_gmdl_does_not`,
asserts that `spore_gmdl` reads the table little-endian. That was true when it
was written and is false now:

```text
test the_graph_reads_the_reference_table_big_endian_and_spore_gmdl_does_not ... FAILED
  left:  [ResourceKey(0xa1b2c3d4:0x11223344:0xaabbccdd)]
  right: [ResourceKey(0xd4c3b2a1:0x44332211:0xddccbbaa)]
```

The test's own comment says the divergence "is pinned here" because that crate
could not fix `spore-gmdl`. `spore-gmdl` has now been fixed, so the pin is a
lie. **It needs updating by whoever owns `spore-assets/tests/graph.rs`** — it
should now assert that the two agree. Not edited here: it is another agent's
existing test.

---

## 2. What is now covered

### 2.1 The raster framing — one chain, and three transcriptions of it

Every raster record in the two content packages is **one mip chain** with a
header, the base level bare, and a little-endian `u32` byte count in front of
every level past the family's first prefixed one. The per-format differences that
remain are three numbers and one unknown:

| `fourcc` | header | bytes per 4x4 block | first prefixed level | records (both packages) |
|---|---|---|---|---:|
| `0x35545844` `DXT5` | 32 | 16 (BC3) | 2 | 2 478 |
| `0x31545844` `DXT1` | 32 | 8 (BC1) | 2 | 634 |
| `0x00000015` | **28** | 4 per *texel* | **1** | 1 274 |

*Why* the block formats leave level 1 bare is **UNKNOWN** and is recorded as such
in `spore-texture`'s claims; what is asserted everywhere below is the measurement.

`tools/spore/raster/raster.py::decode` refuses any `fourcc` other than `DXT5`, and
its layer count divides by an 8-byte block — so after the framing was re-measured
there was **no Python left to check the Rust against** (§1.3). Three independent
transcriptions now exist and are compared:

| file | what it is | hermetic / corpus |
|---|---|---|
| `crates/spore-differential/tests/luminance_oracle.rs` | the frame table and the decode contract, re-derived in a file that never calls `spore_texture::framing` or the `luminance_*` helpers, for **all three** format words | 21 hermetic |
| `crates/spore-differential/src/oracle.rs` → `cmd_rasterframe` | the same table in **Python**, inside the driver, sharing no constant with the crate and importing neither `raster.py` nor `dxt5.py` | — |
| `crates/spore-differential/tests/luminance_corpus.rs` | all three transcriptions against **every** raster record of **both** content packages | 4 corpus |

Three transcriptions is not redundancy for its own sake: the point of the third is
that it was written from the measured table without reading either of the other
two, so a transcription slip is findable.

What the corpus run measures and pins — `cargo test --release -p
spore-differential --test luminance_corpus -- --ignored --nocapture`, **108 s**:

```text
raster records: 4386
decoded by the crate: 4386
cross-checked against the driver: 4386
field_18 read as the base level's size: 4386
  0x00000015 32x32   mips=6   67 record(s),    5508 bytes
  0x00000015 256x256 mips=9   637 record(s), 349584 bytes
  0x00000015 512x512 mips=10  570 record(s), 1398164 bytes
  0x31545844 256x256 mips=9   634 record(s),  43764 bytes
  0x35545844 64x64   mips=7    67 record(s),    5540 bytes
  0x35545844 256x256 mips=9   634 record(s),   87468 bytes
  0x35545844 512x512 mips=10 1777 record(s), 349616 bytes
```

That is:

* the identity closes **exactly** on 4 386 of 4 386 records — nothing unconsumed,
  nothing padded — in seven shapes across three formats;
* **all 4 386 decode, and `TextureError` is unreachable from `decode_raster`** over
  the measured corpus. The census is asserted per `fourcc` and per package, so a
  new refusal cannot appear without the test failing;
* for the `0x15` family, every level is byte-identical to the window the *other*
  transcriptions give it, because that family's codec is a copy; for the block
  formats the geometry, the offsets and the RGBA8 output shape are compared and
  the codec is not (§3);
* every level at or past the first prefixed one states its own size in a `u32` at
  the offset the identity derives, and the levels below it carry none — checked on
  both sides, for all three formats;
* `field_18` states the base level's byte count in **4 386 of 4 386** records, in
  all three formats. It is now **read** by the decoder as a cross-check and a
  disagreement is refused by name (`Field18Mismatch`) rather than ignored;
* the word at `0x1c` is the **first texel** of all 1 274 records of the `0x15`
  family — the 28-byte header is real.

What the hermetic run pins, and why each is not a re-assertion:

| property | the failure it catches |
|---|---|
| the crate's `framing::record_size` equals an independently written identity, over ~90 shapes × 3 formats | arithmetic drift; saturating vs wrapping |
| a well-formed record of every shape decodes to **its own bytes** at the independently derived offsets (`0x15`), or to correctly shaped RGBA8 that provably came from each level's own window (block formats) | a decoder reading the right length of the wrong bytes |
| every single-byte flip in the header, classified **per word** | a decoder reading a word it should derive, or one obeying `field_18` instead of the geometry |
| truncation at **every** offset, and padding by 1..64 bytes plus "plausible" amounts | silent acceptance of a short or padded record — the failure mode that matters |
| every length prefix corrupted individually, refused **by name** at its own level | a decoder that ignores the record's own statement of its framing |
| a prefix that agrees with the bytes present, on a record one byte short | one check substituted for the other |
| absurd counts (`u32::MAX` dimensions, `mip_count = 0xFFFFFFFF`) refused *before* the walk | a four-billion-iteration loop, i.e. a hang rather than a failure |
| a record that is not exactly the framed size refused **with both numbers**, 1..8 bytes over and under, and the old `PayloadNotMultiple`/`PayloadTooSmall` unreachable | "rounding" a remainder into a partial image — the old rule's documented refusal, now an exact-size refusal |
| every format relabelled with another's format word, refused by the length identity | "plausible garbage rather than an error" — the exact failure the format docs warn about |
| `DXT1` decodes; every *other* format word is refused by name while `parse_envelope` still reads it | the reverse error: refusing a format the corpus says exists, which is what the previous version of this file asserted |
| the bytes where a level-1 prefix would sit in a block format are block data, and editing them moves texels instead of being refused | a decoder that grew a level-1 prefix the records do not have |
| 3 000 deterministic mutations per format (truncate / extend / flip 1–4 bytes) | any `Ok` inconsistent with the independently derived layout |
| `field_1c` reported bytes == first texel of the base mip | the 28-vs-32-byte confusion, which is where this format went wrong before |

One property is a **defect in the crate under test**, pinned rather than hidden:
§1.4.

Two properties this section used to have, and what replaced them:

* *"a `DXT5` record of this geometry derives two layers"* and *"a layer count that
  does not divide is refused rather than rounded"* — both asserted the refuted
  division. They are now `every_raster_record_is_one_chain_and_no_count_is_derived_from_a_division`
  (which also computes the old rule's arithmetic and shows it dividing **exactly**
  on the 512² shape, which is *why* it looked right for 1 374 records) and
  `a_record_that_is_not_exactly_the_framed_size_is_refused_with_both_numbers`.
* *"`DXT1` is refused by name"* — the corpus has 634 of them and all 634 decode.

One property was discovered while writing it and is now stated: `parse_envelope`
reads eight words and therefore needs **32** bytes, so the smallest decodable
`0x15` record is 32 bytes (a 1×1 base level with one mip) even though the format's
header is 28. A 28-byte record is refused as `TruncatedEnvelope`.

### 2.2 The gmdl reference table's byte order

`crates/spore-differential/tests/gmdl_reference_table.rs`: 2 hermetic + 2 corpus.
The corpus half is the pin on `spore_gmdl` (§1.2); the hermetic half pins the
walker's refusal behaviour — a table one byte short, a count implausible under
either byte order, and the observation that zeroing the count word on a real
record shifts the rest of its layout so the record must stop decoding.

### 2.3 Material-info block structure, and how much work the inference does

`crates/spore-differential/tests/material_block_binding.rs`: 3 corpus tests, two
independent sources.

* Against the **Python oracle** (`cmd_gmdl`, extended with `block_tex_counts` and
  `block_tex_sets` so block boundaries can be expressed at all): a 1 082-record
  deterministic sample, of which the oracle walks 487 — block count equals mesh
  count in 487 of 487, every block uniform in 487 of 487, and **0** of the 133
  multi-block records have blocks that name *different* texture sets. So for
  every sampled multi-block record the block order carries no information and a
  permutation would be unobservable.
* Against an **independent walker** over **all 4 209** records, which has no
  ceiling of its own: the numbers in §1.1.
* The documented creature on its own, with the `ordinal_conflicts=1` named.

Note the two halves are complementary, not redundant: the oracle sample excludes
the `0x218` records (its refusal is in the anim-data section, after material-info)
and the independent walk includes them.

**Revision 2 changed no number in this section and reworded two messages.** The
two FINDING assertions describe what `partition_texture_refs` did *before* commit
228a701 — "every record whose walk stops on `0x218` still reports a
`PerMeshBlock` scope" — and the function now refuses the 100 contradicted records
as `Undetermined` instead. The counts are computed here from the test's own walk
(an even division of a possibly partial list), which is exactly the hazard, so
the assertions stay and the wording now says which function turned it into a
refusal. The stale sentence was a claim about another crate's behaviour that had
become false.

### 2.4 Package priority and the two-revision record

`crates/spore-assets/tests/priority.rs`: 6 hermetic + 1 corpus.

* `push` appends, `push_front` prepends, and `package_names()` is exactly the
  order asked for — including **two** `push_front` calls, where the later one must
  outrank the earlier. An `insert` that reordered, or a `push_front` that
  appended, passes a single-patch test and fails this one.
* Two revisions of one identity: the order alone decides, reversing it flips the
  answer, and `read_from` reaches the shadowed revision without changing anything.
* Resolution is deterministic across 64 repeated queries.
* `identities_of_type` is in resolution order with duplicates attributed to the
  higher-priority package, and `find` agrees with it.
* The real record: `0x2A3CE5B7:0:0xA426730B` is **264** bytes in
  `Spore_Game.package` and **276** in `PatchData.package`; base-first resolves to
  264, patch-first to 276. A two-cursor word alignment matches **66 of the older
  record's 66 words** and the newer record skips exactly **three**, at words 0,
  14 and 53 (bytes `0x0000`, `0x0038`, `0x00d4`). Two of those three read zero
  in the patched revision, which is exactly why the alignment is necessary rather
  than a value comparison.

### 2.5 Manifest classification vs the loader

`crates/spore-assets/tests/manifest_vs_loader.rs`: 4 hermetic + 1 corpus.

The existing tests call the classifier and assert what it returns, which proves
the table says what it says. This file closes the loop:

* every type id with a decoder in this workspace is classified `Ok`
  (`gmdl`, `rw4`, `raster`), and eleven ids without one are not — including
  `png`, `jpeg`, `plt` and `gmsh`, each of which this repository has already got
  wrong once;
* one-bit neighbours of all three decodable ids are **not** classified `Ok`.
  (`RW4 + 1` would have been a poor choice to write down: RW4 is `0x2f4e681b` and
  raster is `0x2f4e681c`, so adjacency in the number line means nothing here.)
* the four `DecodeStatus` values render as four distinct labels, so a report can
  tell them apart;
* corpus: the manifest calls **exactly** `{gmdl, rw4, raster}` decodable over all
  17 119 records; the header probe accepts 4 209 of 4 209 gmdl records and the
  walk contradicts it **0** times in either direction; rw4 is **1 131 of 1 131**
  parsed.
* **Stale in another crate, for the record:** that test's raster arm said "raster
  is 2 275 decoded and 679 refused, every refusal being one of the conditions the
  decoder names". After the framing was re-measured the figure is **2 954 of 2 954
  decoded and 0 refused** (§2.1), so that assertion now under-counts what the
  decoder does and needs one edit by whoever owns
  `crates/spore-assets/tests/manifest_vs_loader.rs`. Not edited here: not this
  agent's crate, and under-counting is not a false claim about a defect.

### 2.6 The engine's published report line

`crates/spore-differential/tests/engine_report_line.rs`: 4 hermetic + 1 corpus.

`OPENSPORE-STAGED v1` is the only machine-readable thing the engine emits, and
this file is a **consumer** of it: a strict parser plus 19 malformations it must
refuse (version bump, renamed field, missing field, appended field without a
bump, swapped fields, duplicated field, bare token, empty value, non-integer or
negative counts, a two-component or non-numeric or non-finite bound, `textures`
without its pair, `textures` resolved above attempted, banner alone, unrelated
text). The version token, the exact ten-field set **and its order** are the
contract; the numbers are not asserted.

The corpus half runs the real binary and requires `OPENSPORE_ENGINE_BIN` to be
set. That is deliberate: a binary discovered under `target/` is a trap, because a
stale one still prints a line and the test would report a field-set failure that
is really a build-order problem. (There *is* a stale `openspore` in this
checkout's `target/release`, predating the current `decode=` field — which is
what prompted the rule.)

`ContentStore` resolution determinism is covered in §2.4.

---

## 3. Deliberately not covered, and why

| not covered | why |
|---|---|
| What the four `0x15` lanes *mean* | `spore-material` measures it over the same records. A second statement here would be a second source of truth for a fact that already has one. |
| What the bytes inside a block **mean** (BC3, BC1) | `spore-texture` grades both codecs `Inferred`: the 16-byte stride is a byte count, and the codec inside it is the published reading of a record that declares `DXT5`, never compared against what the game draws. `compare::raster` used to check the decoded base mip against `dxt5.py`; that comparison now reports the oracle's refutation instead (§1.3), so **no oracle in this tree can grade the codec**. The test that would is: decode a real record and compare it against the game's own output. |
| The **decoded** `0x15` level bytes of the block formats | Only geometry, offsets and output shape are compared against the other transcriptions; the codec's texel values are not, because the only other decoder in the tree implements the refuted 8-byte block (§1.3). |
| Levels past 31 in a **real** record | No record in either package has more than ten levels, so the arithmetic past `MIP_FLOOR` is untested by the corpus. What *is* tested is that it is **inconsistent inside the crate** (§1.4) — which is a stronger statement than "unverified". |
| `0x218`'s size | UNKNOWN, and must stay that way. The tests pin **that** 619 records stop there and what the consequence is; a test that guessed a size would make the guess load-bearing. |
| Whether material-info block *i* is mesh *i* | Nothing in any record states it. §2.3 measures what *can* be measured (block count, uniformity, how often the order is observable) and leaves the inference labelled. Asserting it either way would be inventing evidence. |
| The engine's window, camera, lighting, or draw path | Bevy, GPU, window. `spore-engine`'s own headless tests cover the wiring; this agent owns no Bevy code. |
| The engine binary's report line without `OPENSPORE_ENGINE_BIN` | A discovered `target/release/openspore` may be stale and would report a build-order problem as a contract failure. |
| The oracle-side byte-order bug (§1.2) as a *fixed* condition | `tools/spore/**` is off limits and the oracles are the specification. The divergence is asserted as present, with both values quoted, so it cannot be forgotten or silently absorbed. |
| `spore-texture`'s own error variants and helpers | Each crate tests its own errors. What was missing was the *second implementation*, and that is §2.1. |
| `spore-assets/src/graph.rs` internals | `crates/spore-assets/tests/graph.rs` and `graph_corpus.rs` cover it. §2.4 adds the priority policy it depends on. One of its tests is now stale — §1.3. |
| A property-based fuzz of `spore-dbpf` / `spore-rw4` / `spore-cellcontent` | Real gaps, but not this session's regression surface, and each already has an oracle the differential harness runs over the whole corpus. Listed in §5. |

---

## 4. How to run

Hermetic (no game, no GPU, no window) — this is what CI runs:

```bash
cargo test -p spore-differential          # includes 21 raster framing property tests
cargo test -p spore-assets --test priority --test manifest_vs_loader
cargo clippy -p spore-differential -p spore-assets --all-targets -- -D warnings
cargo fmt --all --check
```

Real corpus (needs the git-ignored `SPORE/` tree; every one skips cleanly with a
`SKIP:` line when it is absent):

```bash
cargo test -p spore-differential --release -- --ignored --nocapture
cargo test -p spore-assets        --release -- --ignored --nocapture
OPENSPORE_ENGINE_BIN=target/release/openspore \
  cargo test -p spore-differential --release --test engine_report_line -- --ignored --nocapture
```

Cost on this machine, `--release`: the raster corpus test is **108 s** (4 386
records, 4 386 driver round trips, and two 1 GB package images read one at a time);
the material-block corpus tests are **14 s**; the reference-table tests are
**1 s**; the whole `spore-differential` ignored suite is **155 s**; the
manifest-vs-loader test is **5 s**. Use `--release`: decoding 2 478 `DXT5` records
and 4 386 mip chains is allocator-bound, and a debug build turns the raster half
into minutes.

Full real-corpus differential, all seven comparisons, as measured on this checkout:

```text
dbpf-index      walked 17119  equal 17119  diverged 0
record-bytes-qfs walked  200  equal   200  diverged 0
rw4-describe    walked 1131  equal  1131  diverged 0
gmdl-walk       walked  293  equal   134  rust-only 123  both-failed 36  diverged 0
raster-envelope-layer-count-and-base-mip  walked 230  equal 150  diverged 80
                (68 structural layer-count + 8 value-mismatch digest + 4 one-sided;
                 every one is the refuted oracle, §1.3)
manifest-rows   walked    2  equal     1  diverged 1 (definitional)
worldobj-oracle-only  walked 1023  oracle-only 1022  diverged 1 (definitional)
```

---

## 5. Open risks

1. **`spore-texture` mis-places levels past 32** (§1.4). `decode_raster` returns
   `Ok` with the tail of a chain read from another level's bytes for any envelope
   with more than 33 levels. No real record has more than ten, so nothing renders
   wrong today, but the crate's own two framing functions disagree with each other
   and one of them is used for the walk. The owner's fix is one loop bound in
   `framing::level_offset`.
2. **The raster oracles are refuted and unfixed** (§1.3). `tools/spore/raster/raster.py`,
   `tools/spore/dxt5/dxt5.py` and `src/assets/Dxt5.cpp` still divide a payload by
   an 8-byte-block stride, so every `compare::raster` run over a real corpus
   reports 68 layer-count disagreements and 8 digest disagreements, and the oracle
   refuses 82 of 150 sampled records. Nothing downstream of those files — including
   `src/` — can be trusted about raster framing until they are re-derived against
   `docs/RASTER-FRAMING.md` §2. This crate reports the disagreement rather than
   absorbing it.
3. **The block codecs are `Inferred`, and there is no oracle left to grade them.**
   The framing is `Verified` over 4 386 of 4 386 records; the 16-byte stride and
   the `field_18` cross-check are `Verified`; what a BC3/BC1 block *means* is the
   published reading of a measured stride. The test that would settle it — decode
   a real record and compare it against what the game draws — has not been run, and
   the only Python decoder in the tree decodes the wrong bytes. `spore-texture`
   fails its own build if either codec is raised to `Verified`.
4. **The oracle's byte-order bug is unfixed** (§1.2). Every `compare::gmdl` run
   over a real corpus will report a `refs` divergence, and `tools/spore/gmdl`'s
   own analysis of any gmdl record's referenced files is wrong today. Anything
   derived from the oracle's reference table is suspect. Measured over the corpus:
   big-endian resolves 7 989 of 7 989 keys, little-endian 0 of 7 989.
5. **The `0x218` truncation**: `spore-material` now refuses the 100 contradicted
   records as `Undetermined` rather than cutting them (§1.1), and this crate
   measures the arithmetic that made them dangerous. `GmdlModel` still carries no
   statement that its material-info table closed, so a *different* truncation — one
   whose truncated list does not divide evenly and is not contradicted by an
   ordinal — would still be cut as if it were complete. Nothing crashes and nothing
   warns.
6. **`cmd_rasterframe` is not an oracle** and is documented as such in
   `spore_differential::oracle`'s module docs. It is a second transcription of the
   measured table in another language, sharing no constant with the crate; it
   exists because the real oracles were refuted (§1.3), and it must never be
   described as a reference. Its agreement raises confidence in the *arithmetic of
   two transcriptions*; the corpus is the only evidence that the table describes
   Spore, and that is 4 386 records of seven shapes.
7. **A stale test in `spore-assets`** (§1.5) currently fails, which makes
   `cargo test -p spore-assets` red for reasons unrelated to any change under
   review. It needs one edit by its owner. A second stale figure — the raster
   census in the same file — is recorded in §2.5.
8. **The engine's report-line contract is only checked when someone points at a
   binary.** The hermetic half protects the parser, not the engine. If the field
   set changes, nothing fails until a run with `OPENSPORE_ENGINE_BIN` set.
9. **Not fuzzed:** `spore-dbpf`'s QFS decompression, `spore-rw4`'s section walk,
   and the twelve `spore-cellcontent` layouts. Each has an oracle the harness runs
   over the corpus, which catches a wrong answer but not a panic on a corrupt
   input. The raster sweeps in §2.1 are the shape those want: 9 000 deterministic
   mutations across the three formats, and truncation at every offset of a record
   of each format.

---

## 6. What changed in revision 2, in one list

* `tests/luminance_oracle.rs`: **rewritten**, 17 tests → 21. The frame table is
  re-derived for all three format words instead of one, and every assertion that
  encoded the two-layer reading was replaced by one that states the new truth and
  says which measurement replaced it (§2.1's last two paragraphs). The properties —
  exact consumption, truncation at every offset, padding, per-word header-flip
  classification, per-level prefix refusal by name, absurd counts before the walk,
  no cross-format acceptance, a 9 000-mutation sweep — are all still here and now
  run over three formats rather than one.
* `tests/luminance_corpus.rs`: **rewritten**, 3 tests → 4. One package → **both**,
  `0x15` only → **all three formats**, and the census arm now asserts the *absence*
  of refusals where it used to assert the presence of them.
* `src/oracle.rs`: `cmd_raster15` → `cmd_rasterframe`, generalised to all three
  format words, with the refuted oracle's arithmetic reported (`payload`, `stride`)
  so a disagreement can quote it.
* `src/compare/raster.rs`: a third comparison arm (layer count), the refutation
  stated per divergence, `is_luminance`'s refuted `0x1500..=0x15FF` range
  deleted, and the comparison renamed.
* `tests/real_corpus.rs`: the raster test's `diverged == 0` replaced by the
  measured disagreement (§1.3's table), asserted exactly.
* `tests/material_block_binding.rs`: two assertion **messages** reworded; no count
  or assertion changed (§2.3).
