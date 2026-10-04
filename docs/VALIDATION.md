# VALIDATION.md — what the Rust half is actually checked against

**Agent VALIDATION, 2026-10-04.** This document records what is now protected by
tests, what is deliberately **not** protected and why, and the open risks. Every
number in it was measured on this checkout against
`SPORE/Data/Spore_Content.package` (995 268 083 bytes, 17 119 records, GOG
digital 3.1.0.22) unless it says otherwise.

Read it as an inventory of *evidence*, not as a score. A test that re-asserts
the implementation is listed under "Deliberately not covered" even where it
exists.

---

## 1. The two findings

Both are real, both are measured over the whole corpus, and neither is a
hypothetical.

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
| records that stopped early **and still report `BindingScope::PerMeshBlock`** | **619 of 619** |
| records where the even cut would be *arithmetically* wrong (material-info closed) | **0** |

So:

* Among the 3 590 records whose material-info closes, the cut is sound:
  block count equals mesh count in 3 590 of 3 590, every block contributes the
  same number of references, and dividing by the mesh count reproduces the block
  boundaries. **The arithmetic is fine.**
* Among the 619 records that stop on `0x218`, `partition_texture_refs` returns
  `Some` every time, so the scope is `PerMeshBlock` — never the `WholeModel`
  fallback that would have said "we could not cut this". For the **100** of them
  the reference list provably does not cover one entry per declared block, so
  the cut is being computed over references that do not include every block.
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

**Where the fix belongs.** `spore-material` (`partition_texture_refs`,
`model_materials`) or `spore-gmdl` (carry "this reference list is partial" on
`GmdlModel`). Neither crate is owned by this agent, so nothing was changed.

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

### 1.3 A stale test in another crate, now failing

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

### 2.1 The `0x15` raster family — a second implementation, in two languages

The `0x15` family grew by 901 real records this session and had **no oracle**:
`tools/spore/raster/raster.py::decode` refuses any `fourcc` other than `DXT5`.
Three implementations now exist and are compared.

| file | what it is | hermetic / corpus |
|---|---|---|
| `crates/spore-differential/tests/luminance_oracle.rs` | the framing identity and the decode contract, re-derived from the format description in a file that never calls the crate's helpers | 17 hermetic |
| `crates/spore-differential/src/oracle.rs` → `cmd_raster15` | the same identity in **Python**, inside the driver, with no call into `raster.py` or `dxt5.py` | — |
| `crates/spore-differential/tests/luminance_corpus.rs` | both implementations against all 901 real records | 3 corpus |

What the corpus run measures and pins:

* the identity holds **exactly** on 901 of 901 — nothing unconsumed, nothing
  padded — in three shapes: `32x32`/6 mips/5 508 bytes (67 records), `256x256`/9
  mips/349 584 bytes (294), `512x512`/10 mips/1 398 164 bytes (540);
* all 901 decode, and each decoded mip is byte-identical to the window the
  *other* implementation gives it;
* every length prefix equals its own mip's geometry;
* the raster census: `0x15` 901 records / 901 decoded, `DXT5` 1 747 / 1 374
  decoded (373 refused by the layer division), `DXT1` 306 / **0** decoded. Three
  format words, no fourth;
* the word at `0x1c` is the **first texel** of all 901 records — the 28-byte
  header is real, and `field_18` restates the base level's payload size in all
  901.

What the hermetic run pins, and why each is not a re-assertion:

| property | the failure it catches |
|---|---|
| the crate's `luminance_record_size` equals an independently written identity, over ~90 shapes | arithmetic drift; saturating vs wrapping |
| every single-byte flip in the 28-byte header, classified per word | a decoder reading a word it should derive (`field_18`), or a header that grew |
| truncation at **every** offset, and padding by 1..64 bytes plus "plausible" amounts | silent acceptance of a short or padded record — the failure mode that matters |
| every length prefix corrupted individually, refused **by name** at its own mip | a decoder that ignores the record's own statement of its framing |
| absurd counts (`u32::MAX` dimensions, `mip_count = 0xFFFFFFFF`) refused *before* the walk | a four-billion-iteration loop, i.e. a hang rather than a failure |
| DXT5 bytes with the format word set to `0x15`, and vice versa | "plausible garbage rather than an error" — the exact failure the format docs warn about, and nothing else in the tree checks it |
| 3 000 deterministic mutations (truncate / extend / flip 1–4 bytes) | any `Ok` whose pixels are not the record's own bytes at the independently derived offsets |
| `field_1c` reported bytes == first texel of the base mip | the 28-vs-32-byte confusion, which is where this format went wrong before |

One property was discovered while writing it and is now stated: `parse_envelope`
reads eight words and therefore needs **32** bytes, so the smallest decodable
`0x15` record is 32 bytes (a 1×1 base level with one mip) even though the
format's header is 28. A 28-byte record is refused as `TruncatedEnvelope`.

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
  walk contradicts it **0** times in either direction; raster is 2 275 decoded
  and 679 refused, every refusal being one of the conditions the decoder names;
  rw4 is **1 131 of 1 131** parsed.

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
| DXT5 texel **values** | `spore-texture`'s golden tests compare against hand-computed values from `tests/dxt5_golden.rs`, and `compare::raster` checks the decoded base mip against `dxt5.py` over real records. Here DXT5 bytes are used only as *framing*. |
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
cargo test -p spore-differential          # includes 17 new 0x15 property tests
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

Cost on this machine: the `0x15` corpus test is **54 s** (it decompresses 901
records and makes 901 driver round trips); the material-block corpus tests are
**14 s**; the reference-table tests are **1 s**; the manifest-vs-loader test is
**5 s**. Use `--release`: the DXT5 half of the raster census is allocator-bound.

---

## 5. Open risks

1. **The `0x218` truncation is unfixed** (§1.1). Until the walk can say "this
   list is partial", 619 records report a `PerMeshBlock` binding computed from a
   reference list the walk never finished reading, and 100 of those do not even
   cover one entry per block. The engine renders a two-mesh creature whose mesh 1
   is bound to a texture from mesh 0's block. Nothing crashes and nothing warns.
2. **The oracle's byte-order bug is unfixed** (§1.2). Every `compare::gmdl` run
   over a real corpus will report a `refs` divergence, and `tools/spore/gmdl`'s
   own analysis of any gmdl record's referenced files is wrong today. Anything
   derived from the oracle's reference table is suspect.
3. **A stale test in `spore-assets`** (§1.3) currently fails, which makes
   `cargo test -p spore-assets` red for reasons unrelated to any change under
   review. It needs one edit by its owner.
4. **`0x15` has no reference implementation, only a second one** written from the
   same document the crate was written from (`docs/LUMINANCE-RASTER.md`). Their
   agreement raises confidence in the *arithmetic*; it says nothing about whether
   the document describes Spore. The corpus is the only evidence for that — 901
   of 901 records consuming exactly, which is strong but is 901 records of one
   shape family.
5. **`cmd_raster15` is not an oracle** and is documented as such in
   `spore_differential::oracle`'s module docs. It exists because nothing else
   could, and it must never be described as a reference.
6. **The engine's report-line contract is only checked when someone points at a
   binary.** The hermetic half protects the parser, not the engine. If the field
   set changes, nothing fails until a run with `OPENSPORE_ENGINE_BIN` set.
7. **Not fuzzed:** `spore-dbpf`'s QFS decompression, `spore-rw4`'s section walk,
   and the twelve `spore-cellcontent` layouts. Each has an oracle the harness runs
   over the corpus, which catches a wrong answer but not a panic on a corrupt
   input. The `0x15` and raster sweeps in §2.1 are the shape those want.