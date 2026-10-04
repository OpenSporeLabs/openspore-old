# The resource dependency graph

The engine resolves one record at a time. Real content is a graph. This document
is the measured record of the graph layer built in
`crates/spore-assets/src/graph.rs`: what edges exist, **how each kind was
established**, what each is graded, and which references dangle.

Every number below was measured over `SPORE/Data/Spore_Content.package` (17 119
index rows, 995 MB) and is asserted as an exact equality in
`crates/spore-assets/tests/graph_corpus.rs`, not as a tolerance.

```text
cargo test -p spore-assets --release --test graph_corpus -- --ignored --nocapture
```

The tests are `#[ignore]`d and skip cleanly with a printed `SKIP:` line when
`SPORE/` is absent. No Spore byte is committed anywhere: the hermetic tests build
their packages and records in memory.

---

## 1. The graph shape

```text
  package -> resource key -> model -> geometry -> material -> raster
                       \-> animation \-> metadata \-> dependencies
```

A node is a `spore_core::ResourceKey` — the `(type, group, instance)` triple, and
nothing else. An edge is `(kind, from, to)`. There is no blob struct, no global
registry, and no hash-map iteration anywhere: `outgoing` and `incoming` are
`BTreeMap<ResourceKey, Vec<Edge>>` and every `Vec` is sorted by `(kind, target)`
and de-duplicated, so two builds over the same packages produce byte-identical
traversal output.

The layer is Bevy-free and GPU-free by construction: it touches
`spore_dbpf`'s index, `spore_gmdl`'s walk, and `spore_core`'s evidence
vocabulary. It never decodes a raster and never opens an RW4 section payload.

## 2. Edge kinds and their grades

| kind | source → target | established by | grade | built by default |
|---|---|---|---|---|
| `gmdl->reference` | `gmdl` → `rw4` | the `refCount` key table, read verbatim | `VERIFIED` | yes |
| `gmdl->texture` | `gmdl` → `raster` | the `0x20D` texture-set entries | `OBSERVED` | yes |
| `model->texture-set` | `gmdl` → `raster` | category byte + instance co-membership | `INFERRED` | yes |
| `group-sibling` | any → any | group family + instance co-membership | `INFERRED` | **no** |

The two `gmdl` kinds are **references the record itself makes**: they are read
out of the record's bytes. The grade difference between them is deliberate.
`gmdl->texture` is `OBSERVED` because a texture-set entry stores only an
`(instance, group)` pair and the *type* is supplied by context. `gmdl->reference`
is `VERIFIED` because its entry stores a complete `(type, group, instance)` triple
that was read and then resolved against the real index — 7 989 of 7 989.

The bottom two kinds are **not references**. They are adjacency: two records that
share a naming convention, with no record anywhere naming the other. They stay
`INFERRED`, and `GraphOptions::default` leaves the weakest of them off.

## 3. `gmdl` → `rw4`: the reference-count key table

### The measurement that settles it

1510 of the 4209 `gmdl` records carry at least one key in the `refCount` table;
the other 2699 declare none. The `refCount` word itself is big-endian —
already established in this repository and the single most expensive bug in the
format's history. The **three words of every key behind it** are big-endian too.
That is a separate claim, and the corpus settles it outright:

| reading of the key table | keys that resolve to an index row |
|---|---|
| all three words **big-endian** | **7 989 / 7 989** |
| all three words little-endian (what `spore_gmdl` does) | **0 / 7 989** |

Three exact `(type, group, instance)` matches per entry, thousands of times,
against real rows, is not an inference. The on-disk field order is
`{instance, group, type}` — the same order `spore_gmdl` documents — so only the
byte order differs.

> **Open defect this crate does not own.** `spore_gmdl::GmdlModel::referenced_files`
> reads this table little-endian and therefore carries byte-swapped keys on all
> 1510 records that have one. `graph.rs` reads the table itself, big-endian, and
> records the divergence in `graph::claims::REFERENCE_TABLE_ENDIANNESS` rather than
> working around a neighbouring crate's bug in silence. The fix belongs in
> `crates/spore-gmdl/src/parse.rs::read_referenced_files`.

### Where the references go

| target group | declared keys | `rw4` records in the group |
|---|---|---|
| `0x41606100` | 7 449 | 591 |
| `0x40646200` (VehicleModels) | 285 | 285 |
| `0x40636200` (BuildingModels) | 164 | 164 |
| `0x40656200` (UfoModels) | 91 | 91 |
| **total** | **7 989** | 1 131 |

Every declared key names type `0x2F4E681B` (`rw4`). Nothing else appears.

**7 449 of 7 989 keys name 591 records**, an average of 12.6 times each, which is
why `0x41606100` is shared rather than per-model.

**What an `rw4` record contributes is `INFERRED`, and stays that way.**
`spore_rw4` decodes the section *directory* only and no section payload, so
nothing here claims a model is renderable from these edges, nor that the reverse
edge means anything. See `graph::claims::REFERENCE_TABLE_ROLE`.

### Declared entries versus stored edges

| | declared keys | distinct edges |
|---|---|---|
| `gmdl->reference` | 7 989 | 6 076 |

A record may name the same target twice. `ResourceGraph::declared_entry_count`
and `ResourceGraph::edge_count_of_kind` report the two numbers separately,
because reporting one where the record says the other is the same class of error
as reporting one graph node where the bytes hold two.

## 4. `gmdl` → `raster`: the `0x20D` texture sets

A texture-set entry is 16 skipped bytes (`{sampler, opaque}`) then an `instance`
word and a `group` word — **little-endian**, in deliberate contrast with the
reference table in §3. The asymmetry is measured, not explained, and is recorded
in `graph::claims::ENDIANNESS_ASYMMETRY`.

| | declared entries | distinct edges | resolve | dangle |
|---|---|---|---|---|
| `gmdl->texture` | 16 628 | 11 363 | 11 231 | **132** |

The type `0x2F4E681C` (`raster`) is **supplied**, not read: the entry has no type
word. It is supplied because 11 231 of 11 363 distinct edges resolve to an
existing raster row under that assumption and none resolve under another single
type. The grade is therefore `OBSERVED`, not `VERIFIED`
(`graph::claims::TEXTURE_ENTRY_TYPE`).

Distinct texture edges per model: **0** for one model, **2** for 1 261, **3** for
2 947. The 6- and 9-entry models in the raw entry counts are repeats.

### The 18 texture groups the corpus names

| group | stage byte | slot | declared refs | raster records | dangling edges |
|---|---|---|---|---|---|
| `0x00000000` | — | — | 150 | 0 | **132** |
| `0x40612900` | `0x29` | 0 | 150 | 44 | 0 |
| `0x40612901` | `0x29` | 1 | 150 | 44 | 0 |
| `0x40622900` | `0x29` | 0 | 582 | 97 | 0 |
| `0x40622901` | `0x29` | 1 | 582 | 97 | 0 |
| `0x40632900` | `0x29` | 0 | 1 663 | 164 | 0 |
| `0x40632901` | `0x29` | 1 | 1 663 | 164 | 0 |
| `0x40632902` | `0x29` | 2 | 1 663 | 164 | 0 |
| `0x40642900` | `0x29` | 0 | 2 272 | 285 | 0 |
| `0x40642901` | `0x29` | 1 | 2 272 | 285 | 0 |
| `0x40642902` | `0x29` | 2 | 2 272 | 285 | 0 |
| `0x40652900` | `0x29` | 0 | 617 | 91 | 0 |
| `0x40652901` | `0x29` | 1 | 617 | 91 | 0 |
| `0x40652902` | `0x29` | 2 | 617 | 91 | 0 |
| `0x40662900` | `0x29` | 0 | 612 | 153 | 0 |
| `0x40662901` | `0x29` | 1 | 612 | 153 | 0 |
| `0x406b2900` | `0x29` | 0 | 67 | 67 | 0 |
| `0x406b2901` | `0x29` | 1 | 67 | 67 | 0 |

The "declared refs" column counts raw texture-set entries and so includes repeats
within a model; it totals 16 628 where the distinct edges total 11 363. Group
`0x00000000` is the one case where the two differ sharply: 150 declared entries
against 132 distinct edges, because 18 further declarations of the same missing
texture come from models already in the set.

Two things fall out of this table and neither is used as an edge:

* **Every referenced group has stage byte `0x29`**, and the low byte is always
  `0`, `1` or `2` — three sub-blocks, matching "`0Y` is one of three slots per
  model". The 18th referenced group is `0x00000000`.
* **The category byte is preserved.** A model group's category (`group_id >> 16`)
  equals its texture family's: `0x61` cell, `0x62` creature, `0x63` building,
  `0x64` vehicle, `0x65` ufo, `0x66` flora, `0x6b` palette. The stage byte is
  replaced by `0x29`. This is the structural rule behind §5.

**Four raster groups are never named by any model**: `0x40662800`,
`0x40662801` (stage `0x28`) and `0x40662c00`, `0x40662c01` (stage `0x2c`), 153
records each, all in the flora family. No `gmdl` texture-set entry anywhere in the
corpus carries one of those group ids. Four sub-blocks exist per flora instance
and only three slots are ever named, so **which one a model wants is not derivable
from the group id** — it comes from the texture-set entries, which is precisely why
those entries are the edge and the group arithmetic is not.

The co-membership edges of §5 do reach those four groups, because that rule pairs
on category and instance and ignores the group. That is a reason to prefer the
declared edges where they exist, not a reason to prefer the inferred ones.

## 5. `model` → `texture-set`: co-membership, `INFERRED`

A model at `(GMDL, G, I)` is linked to every `raster` record at
`(RASTER, G', I)` where `G'` shares `G`'s category byte.

The instance-set equality that motivates it, measured per category byte:

| category | model instances | raster instances | sets equal |
|---|---|---|---|
| `0x61` cell | 44 | 44 | **yes** |
| `0x62` creature | 97 | 97 | **yes** |
| `0x63` building | 164 | 164 | **yes** |
| `0x64` vehicle | 285 | 285 | **yes** |
| `0x65` ufo | 91 | 91 | **yes** |
| `0x66` flora | 153 | 153 | **yes** |
| `0x6b` palette | 67 | 67 | **yes** |
| **total** | **901** | **901** | **901 / 901** |

901 identity pairs, expanded over the raster sub-blocks each instance carries:
**13 682 edges, 0 dangling**.

**The equality is measured; the ownership is inferred.** No record names the
other — the link is a naming convention. And it is redundant for 4208 of the 4209
models, which carry explicit texture references anyway. It earns its place for the
one model that declares no texture set at all (§7), where it is the only
statement the graph can make.

Grade: `INFERRED`
(`graph::claims::CO_MEMBERSHIP_MEANS_OWNERSHIP`).

## 6. `group-sibling`: family adjacency, `INFERRED`, off by default

Two records are siblings when they share `group_id & 0xFFFF_FF00` **and** the
instance id **and** differ in type. Same-type pairs are excluded on purpose: two
`gmdl` records of one asset in sibling sub-blocks are variants of the same thing,
and pairing them tripled this kind on the real corpus — 91 740 edges instead of
77 130 — without adding a relationship anybody asked about.

| group family | sibling edges |
|---|---|
| `0x40646200` vehicle | 22 176 |
| `0x40626200` creature | 18 464 |
| `0x40636200` building | 14 472 |
| `0x40666200` flora | 10 098 |
| `0x40656200` ufo | 7 080 |
| `0x40616200` cell | 2 336 |
| `0x40637e00` building (second family) | 1 312 |
| `0x406b6200` palette | 804 |
| `0x41626200` creature (second family) | 388 |
| **total** | **77 130** |

All nine are model families; **no texture family (`…29xx`) appears**, which is the
check that makes this rule safe to leave off by default. Dangling count is
structurally zero — both endpoints came out of the index, so a dangling adjacency
would be a bug in the graph, not a finding about the content.

77 130 weak edges is why it is off by default. A family inspector wants it; a
loader does not.

## 7. Missing references, by edge kind

Dangling edges are the normal case, not an error case.

| kind | dangling | why |
|---|---|---|
| `gmdl->reference` | **0** | every key resolves |
| `gmdl->texture` | **132** | all the same target |
| `model->texture-set` | **0** | both endpoints are index rows |
| `group-sibling` | **0** | both endpoints are index rows |

**The 132 dangling texture references**, in full:

| field | value |
|---|---|
| source group | `0x40616201` (44), `0x40616202` (44), `0x40617100` (44) |
| source instance | one per model; 132 distinct models |
| target type | `raster` (supplied, §4) |
| target group | `0x00000000` |
| target instance | `0x552A5077` |
| occurrences in the index | **0**, across all 17 119 rows |

132 cell-stage models name the same group-zero instance, and that instance is in
no loaded package. That is the reference existing and its target not existing —
which is why the edge is kept, counted and listed by
`ResourceGraph::missing_references`, and why the group word is *not* normalised
to "absent". Collapsing `0x00000000` into "no group" would turn a measured
finding into a silent loss.

`ResourceGraph::missing_reference_counts` distinguishes three states a plain `usize`
cannot:

* the kind was built and 0 references dangle → an entry with the value `0`;
* the kind was not built (`group-sibling` under default options) → **no entry at
  all**;
* `dangling_of_kind` returns an empty slice in both cases, so callers that need
  the difference ask `options().builds(kind)`.

## 8. Records that yielded no edges

| reason | records | by group |
|---|---|---|
| `PartialTextureSet` | **619** | `0x40626202` 97, `0x40626203` 97, `0x40627100` 97, `0x40627101` 97, `0x406b6200` 37, `0x41626202` 97, `0x41626203` 97 |
| `NoTextureSetDeclared` | **1** | `0x40637e03` |
| `WalkFailed` | **0** | — |
| `ReferenceTableUnreadable` | **0** | — |

**619 partial reads.** Every one stops at the undocumented shader-data id
`0x218`, whose payload size no record in this workspace has ever established.
The material-info table sits *after* the mesh table, so these records carry
complete, validated geometry with a texture-reference list that is a **prefix**.
The graph emits the prefix (each entry was bounds-checked out of the record) and
records `PartialTextureSet` rather than claiming the list is complete. All four
`0x406271xx` creature-model groups are 97 of 97, as the crate docs say.

**1 empty set.** Exactly one model walks to the record end and declares no texture
set. Because its walk closed, the empty set is *observed*, and it is recorded as
`NoTextureSetDeclared` — a finding, not a non-finding. This is the one model for
which the `model->texture-set` edges of §5 are the only thing the graph can say.

**0 walk failures.** No gmdl in this corpus stops at or before the mesh table,
which the recovering parse treats as a hard error. That is why
`spore_gmdl::parse` (strict) still refuses these 619 while `parse_recovering`
keeps their geometry, and why the graph uses the recovering parse.

## 9. Traversal

`ResourceGraph::traverse(root, depth, kinds)` is breadth-first, ascending by
`ResourceKey` within a depth level, with a visited set and a hard depth bound.
Depth `0` returns the root alone. An empty `kinds` follows every kind; a list
selects. Two runs produce identical output, asserted in both the hermetic and the
corpus test.

Cycles are real and are entered, not looped:

* sibling adjacency is symmetric by construction, so `a ↔ b` is a two-edge cycle;
* several `gmdl` variants of one asset share their texture family, so two nodes
  converge on the same raster.

Measured traversal from the first vehicle model
`0x00e6bce5:0x40646201:0x013f5ddf`, depth 6 — **9 records, all at depth 0 or 1**:

```text
depth 0  0x00e6bce5:0x40646201:0x013f5ddf   gmdl, VehicleModels
depth 1  0x2f4e681b:0x41606100:0x32a50e29   rw4
depth 1  0x2f4e681b:0x41606100:0x6a79e1df   rw4
depth 1  0x2f4e681b:0x41606100:0x7f16e85a   rw4
depth 1  0x2f4e681b:0x41606100:0x8fe5201c   rw4
depth 1  0x2f4e681b:0x41606100:0xf1f3146b   rw4
depth 1  0x2f4e681c:0x40642900:0x013f5ddf   raster
depth 1  0x2f4e681c:0x40642901:0x013f5ddf   raster
depth 1  0x2f4e681c:0x40642902:0x013f5ddf   raster
```

It stops there because `rw4` and `raster` records declare no edges: no RW4 section
payload is decoded and no raster references anything. That is a property of what
is understood, not an empty result — `spore_rw4::parse` reads a directory and
`spore-texture` reads pixels, and neither is a container with an outgoing
reference.

## 10. Considered and rejected

### `pollen_metadata` → `flr` — a self-identity, not a reference

153 of the 1641 `pollen_metadata` records — exactly those in group `0x40666200` —
carry a **big-endian** 12-byte triple at byte offset 12. Its type word is
`0x438F6347` (`flr`), and its group and instance words are **the record's own**.

**0 of the 153 resolve.** `Spore_Game.package` and `PatchData.package` each hold
exactly one `flr` record, at `0x40666200` / `0xDE727FA9`, and no `pollen_metadata`
record names it.

A triple that identifies the record carrying it is not a reference. Deriving 153
dangling `flr` edges from it would be a fabricated negative — the exact failure
this repository's evidence rules exist to prevent. Recorded as
`graph::claims::POLLEN_METADATA_SELF_IDENTITY`, graded `OBSERVED`.

### `prop` — layout unresolved, so no edges

The 1162 `prop` records are counted and known, and contribute nothing.

A 20-byte-entry reading of the key table fits 694 of the 1162 records and resolves
**0 of 1388** candidate keys. The families the candidate entries land in
(`0x4284b800`, `0x4284a700`, `0x4284a100` …) do not exist as groups in the same
package — which is what a wrong stride looks like, not what a reference table
looks like. In `Spore_Game.package` the same reading fits 7030 records and
resolves 3 of 2338; in `PatchData.package`, 83 of 4529.

No `prop` layout is established, so no reference is read out of one. No `prop`
payload is walked at all. Graded `UNKNOWN`
(`graph::claims::PROP_LAYOUT_UNRESOLVED`) — an explicit `UNKNOWN`, which is a
different statement from "there are no references in there".

### `bem` — nothing established

1709 `bem` records sit in the model families beside the models. Their leading word
is `0x048565D3`, a plausible magic, and nothing past it has been decoded. No edge.

### `rw4` section references

`spore_rw4` decodes the section directory and no payload. A `BaseResource` section's
`data_pointer` is arena-relative and every other section's is absolute, so a
section "reference" is an offset inside one record, not a resource identity. There
is no cross-record edge to extract and none is invented.

## 11. Remaining `UNKNOWN`

| subject | why it is still unknown |
|---|---|
| what an `rw4` record contributes to a model | section payloads undecoded; the graph proves the model *names* it and nothing more |
| whether co-membership means ownership | the identity equality is measured; the intent is a naming convention |
| the `0x20D` entry's 16 skipped bytes | `sampler` equals the entry ordinal on the corpus; the other 12 bytes are opaque |
| the `prop` record layout | §10 |
| the `bem` record layout | §10 |
| the texture-set target type | supplied as `raster` from an 11 231-of-11 363 resolution, not read from the record |
| which flora raster sub-block a model wants | four exist per instance, three slots are named, and the choice lives in the record |
| animation and the `0xEE17C6AD` configs | 2091 records in `Spore_Game.package`, none of which carries a key this workspace can read |

## 12. API summary

```rust
use spore_assets::{ContentStore, Package};
use spore_assets::graph::{EdgeKind, GraphOptions, ResourceGraph};

let mut store = ContentStore::new();
store.push(Package::open("Spore_Content", "SPORE/Data/Spore_Content.package")?);

let graph = ResourceGraph::build(&store);
for row in graph.edge_summary() {
    println!("{row:?}");
}

// Neighbours, sorted by (kind, target).
for edge in graph.neighbours(&key) { /* … */ }
let textures: Vec<ResourceKey> = graph
    .neighbours_of_kind(&key, EdgeKind::GmdlTexture)
    .into_iter()
    .map(|edge| edge.to)
    .collect();

// Bounded, cycle-safe, deterministic.
let reachable = graph.traverse(&key, 6, &[EdgeKind::GmdlTexture, EdgeKind::GmdlReference]);

// Findings, not errors.
for missing in graph.missing_references() {
    println!("{} -> {} unresolved", missing.from, missing.to);
}
```

`ResourceGraph::build_with(store, GraphOptions::all())` additionally builds the
77 130 sibling edges. `GraphOptions::none()` builds nothing and reports nothing,
which is a different answer from "looked and found none".

## 13. Claim registry

Every non-obvious statement above is registered in
`crates/spore-assets/src/graph.rs::claims` with an `EvidenceLevel`, an
`EvidenceState` and provenance, and is queried through
`graph::claims::claim(subject)`:

| subject | grade |
|---|---|
| `reference_table_endianness` | `VERIFIED` |
| `endianness_asymmetry` | `VERIFIED` |
| `reference_table_role` | `INFERRED` |
| `co_membership_means_ownership` | `INFERRED` |
| `texture_entry_type` | `SUPPORTED` |
| `pollen_metadata_self_identity` | `OBSERVED` |
| `prop_layout_unresolved` | `UNKNOWN` |

`Unknown` is a legitimate grade for a registered claim here — the `prop` entry
records that the format is *not* understood, which is a fact about this repository
rather than a guess about the data.