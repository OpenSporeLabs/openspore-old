# 07 — Adversarial architecture review of `docs/CREATOR-CORE.md`

**Agent:** ARCHITECTURE (review only).
**Date:** 2026-10-04.
**Target:** `docs/CREATOR-CORE.md` (487 lines).
**Reads:** `AGENTS.md`, `docs/CREATOR-CORE.md`, `docs/analysis/creator/01`…`06`,
`crates/spore-engine/src/domain/{model,scene}.rs`, `crates/spore-assets/src/graph.rs`,
`crates/spore-tools/src/json.rs`, `crates/spore-semantic-bridge/src/json.rs`,
`crates/spore-core/src/key.rs`, `docs/RESOURCE-GRAPH.md`, `Cargo.lock`.
**Probed:** the real install at `./SPORE` (14 asset packages, 133 391 index rows,
every record decompressed) with the committed oracle `tools/spore/dbpf/dbpf.py`, plus a
standalone `rustc` float probe built outside the checkout.

**Mandate:** reject speculative generalisation, check the load-bearing boundaries, check
single-authority, and verify the numbers. Nothing was built, nothing was committed, the
shared target directory was not touched. This file is the only file created.

---

## 0. Verdict table

### 0.1 §2 — "what is shared, and what is not"

| # | item as written | verdict | one-line reason |
|---|---|---|---|
| 2.1 | part instance record — SHARED, CONFIRMED | **ACCEPT WITH CHANGE** | the 472 B `cEditorResourceBlock` is CONFIRMED, but the binary spells the graph as `parentIndex` and the XML as `<childlist>`; the proposal must record that this is one graph with two spellings, not one field |
| 2.2 | block transform `(Vec3, Matrix3, f32)` — SHARED, CONFIRMED | **ACCEPT** | 680/680 measured; `Row3` as a flat alias needs one wording change (§1.1) |
| 2.3 | parent/child forest — SHARED, CONFIRMED | **ACCEPT WITH CHANGE** | 601/520/79 all reproduce, but the proposal must record that it has **refuted** reports 02 and 06 on the direction of `childid`; it presents the number as agreement |
| 2.4 | symmetry as a partner index — SHARED, OBSERVED | **ACCEPT (upgrade)** | I measured **84 of 84** `symmetric` relations are reciprocal, which is decisive; the grade should rise from OBSERVED and reports 01/05's "mask" reading should be struck |
| 2.5 | paint regions + colours — PARTIAL | **ACCEPT WITH CHANGE** | the split (`bld`/`ufo`/`vcl` yes, `crt` no) reproduces exactly; `paintregion` is **23** distinct values corpus-wide, not "~15" |
| 2.6 | handles / named weight channels — SHARED, OBSERVED | **ACCEPT** | 119 blocks, 252 weights, 14 channels, top-3 frequencies all reproduce |
| 2.7 | snapping/sockets — NOT IN ANY DOCUMENT | **ACCEPT** | correct, and correctly kept out of the model |
| 2.8 | editor behaviour — NOT DEMONSTRABLE | **ACCEPT** | I attacked this hard (§6) and could not refute it; it is the strongest claim in the document |
| 2.9 | vehicle skinning differs | **ACCEPT (cited)** | not re-measured by me; it is a `02` figure and nothing in the proposal rests on it |
| 2.10 | UFO rigblocks borrowed | **ACCEPT** | independently confirmed: `0x40646000` ×104 = `vcl` 57 + `ufo` 47 |

### 0.2 §3 — crate layout

| # | item | verdict | one-line reason |
|---|---|---|---|
| 3.1 | new crate, `spore-core` + `thiserror` only, no Bevy | **ACCEPT** | correct reading of the load-bearing invariant |
| 3.2 | projection lives in `spore-engine` | **ACCEPT** | direction is right; the core must not know engine types |
| 3.3 | `src/history.rs` inside the core crate | **ACCEPT WITH CHANGE** | reports 01 §4 and 05 §7 both put undo in a **sibling** crate because a history stack inside a value type turns the document into a session |
| 3.4 | `spore-json` extraction | **REJECT (omission)** | §5 adopts a JSON-Lines codec whose reader/writer the proposal says already exist in two crates, and §3's file list contains neither the extraction nor a dependency on it |
| 3.5 | additive optional fields on `Placement` | **ACCEPT WITH CHANGE** | the decision is right; the *stated reason* it beat the sibling type is refuted (all four `Placement { … }` literals use `..Placement::default()`), and it introduces a two-answer rotation hazard with no guard |
| 3.6 | `rotation_y_degrees` "stays authoritative when `rotation` is `None`" | **REJECT** | that is half a rule: the loader reads `rotation_y_degrees` **unconditionally**, so a `Placement` with both set renders the wrong rotation silently; report 03 §7.3's `is_legacy_expressible()` / `effective_rotation_y_degrees()` guards are dropped and must be mandatory |

### 0.3 §4 — the domain model

| # | item | verdict | one-line reason |
|---|---|---|---|
| 4.1 | `blocks: Vec<Block>`, order **is** the index space | **ACCEPT** | correct, and it is the model's best decision |
| 4.2 | `roots: Vec<BlockIndex>` as a **struct field** | **REJECT** | a derived value stored in the value is a second source of truth by construction; 05 §3.1 forbids a redundant index for exactly this reason — make it a method |
| 4.3 | `Option<T>` = "not written", never empty/default | **ACCEPT** | the strongest single decision in the document; all five absences reproduce |
| 4.4 | `CreatorDocument` has no `unclaimed` / `claims` / `source` | **REJECT** | §5 adopts a four-record-kind format whose `unclaimed` and `claim` kinds have **no field to live in**; the document contradicts the format it adopts |
| 4.5 | `BlockId { group, instance }`, not `ResourceKey` | **ACCEPT WITH CHANGE** | correct not to invent a type word, but the proposal must state that group→type is an *inference from group occupancy*, grade it `INFERRED`, and note that `PatchData` puts a `prop` at those same keys |
| 4.6 | `snapped: Option<bool>` | **ACCEPT WITH CHANGE** | present in 680/680; the `Option` invents a third state the corpus never writes |
| 4.7 | `children: Option<Vec<BlockIndex>>` | **ACCEPT** | 160 leaves reproduce exactly |
| 4.8 | `symmetric: Option<BlockIndex>` | **ACCEPT (upgrade grade)** | 84/84 reciprocal ⇒ index, not mask |
| 4.9 | `paint` / `handles` as `Option<Vec<_>>` | **ACCEPT** | reproduces |
| 4.10 | `user_orientation` / `bbox` on `Block` | **ACCEPT WITH CHANGE** | the corpus nests both inside `<transform>`; flatten them in the type and the structure stops mirroring the document |
| 4.11 | `limbtype` / `musclescale` / `basemusclescale` | **REJECT (omission)** | measured present 5 / 5 / 8 times; §1.2's census table silently drops them and §4 has no field for them |
| 4.12 | `Vec3`, `Row3`, `BlockTransform` shapes | **ACCEPT WITH CHANGE** | `Row3` must be `pub type Row3 = [[f32; 3]; 3]` with the row order in the doc comment, not a three-field struct — a struct plus the engine's flat array is two representations of nine floats |
| 4.13 | `format_version: Option<u32>` | **ACCEPT** | simpler than 01's enum and equally lossless (`None` = absent, `Some(17)` = unknown version) |
| 4.14 | `properties` as a struct of ten independent `Option`s | **ACCEPT WITH CHANGE** | the corpus has exactly **five** presence classes and three perfect correlations; ten free options author 1016 states of which 5 occur |

### 0.4 §5, §6, §7, §8, §9

| # | item | verdict | one-line reason |
|---|---|---|---|
| 5.1 | hand-rolled JSON Lines, no `serde` derive | **ACCEPT** | `Cargo.lock` confirms `serde`/`ron` present and `serde_json` absent; I confirmed the `null`-for-NaN behaviour in the crate source |
| 5.2 | round trip asserted on the value, `to_bits()` equality | **ACCEPT** | the sampled `{:?}` round trip is clean and `2^32 − 2·2^23 = 4 278 190 080` is arithmetically right |
| 5.3 | the game's binary container is CONFIRMED, read later, no writer in v1 | **ACCEPT WITH CHANGE** | no new binary format is invented (correct), but 1709 records in the corpus already carry that container's magic — "read later" is under-sold and under-specified (§7.2 below) |
| 5.4 | snapshot undo, two files, no coalescing | **ACCEPT** | matches the CONFIRMED `CommitEditHistory` shape; the index-space rule is right |
| 6.1 | the rigblock tail is "the one real blocker" | **ACCEPT WITH CHANGE** | it is the one blocker for *geometry*; two other unresolved joins (the per-block name, and `bem`) are missing from the list |
| 7.1 | build order 1–10 | **ACCEPT WITH CHANGE** | step 6 (engine projection) precedes step 7 (the resolver), which makes 05 §6.1's `P4` "render-plan identity" guarantee a comparison of two empty scenes |
| 7.2 | step 3 gate "version-gated fields" | **REJECT (category error)** | measured: `formatversion 16` spans **two** presence classes and `15` spans **two**; no field's presence is predicted by the version |
| 7.3 | step 8 gate "Bevy preview … headless via `MinimalPlugins`" | **ACCEPT WITH CHANGE** | a preview opens a window; `MinimalPlugins` describes a test. Split the deliverable, and note the AGENTS.md `EventLoop` trap |
| 8.1 | the eight refusals in §8 | **ACCEPT** | every one is justified by the corpus; nothing here is speculative |
| 8.2 | §8's refusal list | **ACCEPT WITH CHANGE** | add the two refusals the reports made and the proposal dropped: no free-form `param` bag (05 §4.7) and no per-block `local_id` (05 §7.5) |
| 9.1 | the §9 evidence index | **ACCEPT WITH CHANGE** | eight rows are wrong or ungraded; §0's "01 and 02 reached the same census independently" is false |
| 9.2 | §0's type-id table | **REJECT** | all six ids have **zero** records in the index; the correct ids are in 01/02 and in the in-game type dictionary |

---

## 1. Speculation hunt — engineering choices wearing corpus clothes

### 1.1 `Vec3` / `Row3` / `BlockTransform`

Mostly honest. `position`, `scale`, `orientation` are in 680/680 and
`trianglepickorigin` is `Option` for a *measured, version-correlated* reason — that is
corpus-forced. Two problems:

* `Row3 { row0, row1, row2 }` is a **struct** while `Placement.rotation` must be
  `Copy` and consumed by the engine's matrix code. That is two shapes for nine floats
  and therefore a conversion somebody will get wrong (transpose is the classic).
  The corpus names the rows, so the naming belongs in the **serialisation**
  (05 §4.4 already writes `{"row0":…,"row1":…,"row2":…}`) and in the doc comment, not in
  the Rust layout. Required: `pub type Row3 = [[f32; 3]; 3];` with
  "row-major; element `i` is the corpus's `row{i}`; column-vector convention".
* `triangle_direction: Option<Vec3>` in §4 contradicts §1.2's table, which says
  `triangledirection` is present in all 680. It must be a plain `Vec3`. A `None` here
  would be a value the corpus never writes, and the whole point of `Option` in this
  document is that `None` means "not written".

### 1.2 The derived `roots` decision — REJECT as a field

`roots: Vec<BlockIndex>  derived, never stored` sits **inside the struct sketch**, which
means it is a field. That is a second authority for the same fact: a stale `roots` is
representable, and nothing in the type system prevents a document carrying both a
`blocks` array and a `roots` array that disagree. 05 §3.1 reaches the identical
conclusion for the file format ("a redundant index is a second source of truth that can
disagree with the first") and 01 §3.4 makes it a method. Required: `roots()` as a method
over the derived forest, never stored, with the forest itself computed from `children`
at load and validated (`dangling`, `multiply_parented`, `has_cycle` as findings).

The underlying *reasoning* is right and should be kept: a `root: BlockIndex` field would
be wrong for 9 % of the corpus (I measured the distribution: 62 docs with 1 root, 4 with
2, 1 with 3, 1 with 6 — 79 roots over 68 documents).

### 1.3 `BlockId` as a 2-tuple rather than a `ResourceKey`

The decision is **correct** and I endorse it: `<blockid>` writes two words, 0 of 60
distinct values name a `gmdl`, and manufacturing a type word would invent the field the
format withholds. But the proposal states the consequence wrongly, in two places:

* §1.5: "those pairs name records of exactly one type, `0x5CAA5E28`". True **in
  `Spore_Game`** — I verified that `0x40616000 / 0x40626000 / 0x40636000 / 0x40646000 /
  0x40666000` contain *only* type `0x5CAA5E28` and nothing else. But **all 60 pairs also
  name a `prop` (`0x00B1B104`) at the same `(group, instance)` in `PatchData`**, whose
  first property key differs from the rigblock groups' (`0x00b2cccb` vs the rigblocks'
  neighbourhood) and which carries UTF-16 text. So "exactly one type" is a
  package-scoped statement, not a statement about the key.
* §9: "`blockid` names only `0x5CAA5E28` | VERIFIED". Same correction.

The real inference the resolver performs is: **group → type**, licensed by measured group
occupancy. That is `INFERRED`, not `VERIFIED`, and it is exactly the kind of thing the
proposal's own `claims` module exists to record. Required: a claims subject
`blockid_type_from_group_family` at `INFERRED`, plus a census assertion over the six
groups so a seventh family that breaks the rule is a finding rather than a surprise.

### 1.4 "Additive fields on `Placement`" versus a sibling type

The **decision** is right and I will not ask for it to be reopened. The **argument** is
wrong, and it is the argument a reviewer will be asked to defend:

> §3.1: *"`01-domain-model.md` prefers it because `Placement { … }` struct literals exist
> and a new field would touch four call sites."*

I read all four sites. Every one uses struct-update syntax:

```
crates/spore-engine/src/cli.rs:183          Placement { scale: self.scale, ..Placement::default() }
crates/spore-engine/src/cli.rs:192          Placement { scale: self.scale, ..Placement::default() }
crates/spore-engine/src/cli.rs:200          Placement { scale: self.scale, ..Placement::default() }
crates/spore-engine/tests/material_wiring.rs:427  Placement { translation: [5.0,0.0,0.0], ..Placement::default() }
```

Adding a `Copy` field touches **zero** call sites. Report 01 §3.5's claim that "every one
of them would stop compiling" is refuted by the source, and report 03 §7.1 got it right
(after compiling the shape). Strike the argument; keep the decision, and replace the
reason with the real one — a single placement type means the scene path and the compose
path cannot drift.

That still leaves a genuine defect, covered in 3.6 above: `loader.rs:598-602` reads
`rotation_y_degrees` unconditionally, so `rotation: Some(m)` + `rotation_y_degrees != 0`
renders the legacy value and drops the matrix with no error. Report 03 designed
`is_legacy_expressible()` and `effective_rotation_y_degrees()` for exactly this and
CREATOR-CORE drops both. Required: both methods, a test that a `Placement` with a
general rotation and a non-zero `rotation_y_degrees` is refused or routed to the full
form, and a claim that the two are never both authoritative.

### 1.5 The resolver enum's three arms

`Resolved | Ambiguous | Unavailable { reason }` — **ACCEPT, and it is corpus-forced, not
speculative.** Two measurements make it so:

* `Ambiguous` is real because 60 of 60 `blockid`s match **two** record types
  (`0x5CAA5E28` in `Spore_Game`, `0x00B1B104` in `PatchData`), so a resolver that
  returned one key without saying why would be hiding a choice. (Report 01 §3.1 got this
  right; the proposal dropped it and with it the justification for the middle arm.)
* `Unavailable` is real because 0 of 60 reach geometry today: the `0x5CAA5E28` tail is
  undecoded.

One change: `UnavailableReason`'s three variants should not be a closed set that
includes `NoDecodableGeometry` while the actual blocker is a **fourth** thing — "the tail
is not decoded". Add `TailNotDecoded` (or fold it into `NoDecodableGeometry` with the
reason string naming the tail). As written a caller cannot distinguish "no record" from
"record present, tail unknown", which are very different findings.

### 1.6 Should `properties` be a typed struct at all?

Every field is optional in a different subset of documents, so a ten-field struct of
independent `Option`s is the natural encoding — but it is *over-generous*, and the
corpus says so precisely. I measured the presence classes:

```
58  modeltype requiredpack skin* colors zcorp
 6  modeltype requiredpack name desc
 2  modeltype requiredpack skin* colors name desc
 1  modeltype zcorp
 1  modeltype skin* colors name desc hasthumbnail cacheid
=> exactly 5 presence classes in 68 documents
```

and three correlations hold in 68/68:

* `skineffect1..3` + `skineffectseed1..3` + `skincolor1..3` are all-or-nothing — never a
  partial set in any document;
* `name` ⟺ `description`;
* `hasthumbnail` ⟺ `cacheid`.

So: keep the typed struct (a bag of `Option` scalars would be worse — 05 §4.7 refused a
free-form `param` bag for exactly that reason, and I agree), but

* group the skin fields into **one** option per correlated group (`skin_effects`,
  `skin_colors`) as 01 does, with a comment that the correlation is observed over 68
  documents and **not enforced**;
* state the five observed classes in the doc comment and add a corpus test asserting the
  observed set is exactly those five, so a sixth class is reported rather than absorbed.

Do **not** introduce a `PropertyVariant` enum over the five classes: a new corpus family
would then fail to load, which is precisely the failure mode `FormatVersion::Unknown` and
`paintregion`'s open value set exist to avoid.

---

## 2. Boundary check

**Verdict: the boundaries are respected, with one placement error and one omission.**

| check | result |
|---|---|
| everything above `loader` Bevy-free | **PASS** — §3's dependency list is `spore-core` + `thiserror`; no `bevy` anywhere in the plan; the projection is placed in `spore-engine` |
| `spore-assets` never touches a GPU | **PASS, vacuously** — the core does not depend on `spore-assets` at all; 01's `BlockResolver` trait keeps the document model a value model, which is the right call |
| `#![forbid(unsafe_code)]` | **PASS** — stated in §3, and nothing in the plan needs an exception (the single existing exception is memmap2 in `spore-assets`) |
| a `claims` module | **PASS** — §3 lists `src/claims.rs`; 01 supplies 20 candidate subjects. Extend it with the four the proposal needs and does not list: `blockid_type_from_group_family`, `roots_are_derived`, `rotation_representation_coexistence`, `property_presence_classes` |
| core does not depend on `spore-engine` | **PASS** — and the stated reason ("the dependency has to point the other way") is the correct one |
| projection direction | **PASS** — document → engine, one direction, total; nothing in the resolved path writes back, matching the `spore-semantic` overlay discipline |
| no ECS / `Entity` / window in the core | **PASS** — nothing in §3–§5 names one |
| `history.rs` inside the core | **VIOLATION (soft)** — 01 §4 and 05 §7 both place undo in a **sibling** crate, because "an undo stack in a value type turns the document into a session". Either move it to `spore-creator-edit` or add an explicit invariant + test that `CreatorDocument` holds no history field. A separate file in the same crate is *probably* fine; "probably" is not a boundary |
| `spore-json` extraction | **OMISSION** — §5's format depends on a shared JSON writer/reader that §3 never creates and never depends on. 05 §2.3 prices the work (≈220 lines across five items plus a mechanical move). §3's file list must contain it or §5 is unbuildable as written |
| differential test against the Python oracle | **OMISSION** — `AGENTS.md` says "the same record must never have two different answers" and that `crates/spore-differential` exists to run that comparison. §7 step 2's gate is "68/68 real documents parse", which is a *count*, not a differential. There must be a `spore-differential` case comparing the Rust `<sporemodel>` decoder against the Python XML path field-for-field, including the `Option`-vs-absent distinction — that is the one place a port bug would be invisible |

One more boundary note: the proposal never mentions `spore-differential` at all, and the
XML decoder is precisely the kind of parser where "it parsed 68 documents" says nothing
about whether it read them correctly.

---

## 3. Second-source-of-truth check

`AGENTS.md`: *the same record must never have two different answers.* Five places where
the proposal could create one, judged one by one:

| # | candidate second authority | single authority? | verdict |
|---|---|---|---|
| 3.1 | `rotation_y_degrees` **vs** new `rotation: Option<Row3>` | **NO** | two fields in one struct, both readable, and `loader.rs` reads only the legacy one. The proposal says "`rotation_y_degrees` … stays authoritative when `rotation` is `None`" — that is a rule about *one* of the four states, not a single authority. **Must fix**: adopt 03 §7.3's `is_legacy_expressible()` + `effective_rotation_y_degrees()`, and make "both set and disagree" a reported finding, not a silent precedence |
| 3.2 | `roots` field **vs** derived-from-`children` forest | **NO** | a stored derived value. **Must fix**: method, never a field (§1.2) |
| 3.3 | block order **vs** a `BlockId`-keyed map | **YES** | the proposal explicitly refuses `BTreeMap<BlockId, Block>` and keeps `Vec` with order = index space, with a carried `block_id` on every long-lived reference so a stale index is *detectable*. This is exactly right and is the model I'd have asked for |
| 3.4 | `<blocks>/@count`, `<childlist>/@count`, `<handles>/@count`, `<paintlist>/@count` **vs** the real child counts | **YES, but unassigned** | I measured 68/68, 520/520, 119/119, 80/80 — 787 attributes, zero mismatches. The proposal mentions only the first. Required: state the policy once ("`@count` is a redundancy **check**, never stored state; a mismatch is a load-time `Invalid`, matching 05 §4.4"), and assert it in a test over all four attributes. Currently a reader can decide per-field |
| 3.5 | `symmetric` as index **vs** as mask | **YES** | resolved by measurement, and the proposal is on the right side: 84 of 84 relations are reciprocal, none is out of range, and 76 of the 84 sit on **leaves** (blocks with no `<childlist>`) — a 5-bit mask on a leaf is meaningless. This also **refutes** report 01 ("observed values 1…31 (a mask, not a flag)") and 05 §4.4 ("observed 1…31, so a **mask**"). The proposal should say so explicitly, because the wrong reading is already committed in two other files and someone will read those next |
| 3.6 | `format_version` as a gate **vs** field presence | **NO — the proposal asserts a gate that is not there** | §1.1 calls `formatversion` "a version gate" and §7 step 3 validates "version-gated fields". Measured: `formatversion 16` spans **two** presence classes (58 docs with `requiredpack`, 1 without), `15` spans **two** (2 with skin, 6 without). The only version-*correlated* absence in the corpus is `trianglepickorigin` (10/10 of the one v11 document) and `blockbbox` (10/10 of the same document) — everything else varies *within* a version. Fix the wording: the version is **recorded** and used for one documented absence, and is **not** a presence oracle |

No other candidate surfaced. In particular the project-vs-document direction is
one-way and total, and nothing in the plan writes a runtime fact back into a saved
document — which is the discipline `spore-semantic runtime` enforces and it is honoured
here by omission, correctly.

---

## 4. Evidence discipline — 30 spot checks against the real corpus

All commands below are reproducible read-only probes over `./SPORE` using
`tools/spore/dbpf/dbpf.py` (`read(pkg) -> (data, items)`, `getdata(data, rec_dict)`), plus
one standalone `rustc` float probe built in `/tmp`. Scripts are throwaway; the numbers
are the deliverable, exactly as the reports state.

### 4.1 Reproduced — the proposal is right

| # | §1/§9 claim | measured | verdict |
|---|---|---|---|
| 1 | 68 creator documents, 6 families, all `<sporemodel>`, all parse | 68 docs, 6 type ids, `root.tag == 'sporemodel'` for all | **VERIFIED** |
| 2 | 680 `<blockref>` | 680 | **VERIFIED** |
| 3 | `properties` census (`modeltype` 68, `requiredpack` 66, `skineffect*` 61, `seeds` 61, `skincolor*` 61, `zcorpscore` 59, `name` 9, `description` 9, `hasthumbnail` 1, `cacheid` 1) | every count identical | **VERIFIED** |
| 4 | `blocks/@count` equals the real child count in all 68 | 0 mismatches | **VERIFIED** |
| 5 | `blockid` 680, `transform` 680, `snapped` 680 | 680 / 680 / 680 | **VERIFIED** |
| 6 | `childlist` 520, `childid` 601, **160** blocks with no `<childlist>` | 520 / 601 / 160 | **VERIFIED** |
| 7 | 601 edges, 0 dangling, 0 self-edges, 0 cycles, max in-degree 1 | 0 / 0 / 0 / 1; also 0 duplicate `childid` targets and 0 lists with repeats | **VERIFIED** |
| 8 | 79 roots over 68 documents: 62×1, 4×2, 1×3, 1×6 | exactly that | **VERIFIED** |
| 9 | `symmetric` 84, families `cll`/`crt`/`ufo`/`vcl` | 84 = cll 2 + vcl 36 + ufo 40 + crt 6 | **VERIFIED** |
| 10 | `paintlist` 80 = `bld` 8 + `ufo` 15 + `vcl` 57, **never** in `crt` | exactly that, and zero in `cll`/`flr` too | **VERIFIED** |
| 11 | `paintid` — **6** distinct, `0xf5967f30` ×175 dominant | 6 distinct, `0xf5967f30` ×175 | **VERIFIED** |
| 12 | `handles` 119 blocks, 252 `weight` entries, **14** distinct channels, top-3 `0xd02751de` ×85 / `0x2e6521f4` ×56 / `0x7cc96c02` ×46 | 119 / 252 / 14, frequencies identical | **VERIFIED** |
| 13 | `scale` range **0.28 … 2.254181** over 680 blocks, never negative, never a vector | min 0.28, max 2.254181, n = 680 | **VERIFIED** |
| 14 | `trianglepickorigin` in 670; the 10 absences are exactly the one `formatversion 11` document | 670 present; missing 10, all in the single v11 `crt` record | **VERIFIED** |
| 15 | `blockbbox` 10, the `formatversion 11` document only | 10, all in that same record | **VERIFIED** |
| 16 | `userorientation` 11, `vcl` only | 11, all `vcl` | **VERIFIED** |
| 17 | `triangledirection` disagrees with `orientation` in **102** of 680 | 102 differ as written; of those 101 differ by more than 1e-6 and both are unit vectors pointing different ways; 1 differs only in written precision | **VERIFIED** — and this is the one number the proposal gets exactly right, including the reason it matters |
| 18 | `blockid` groups `0x40626000` ×487, `0x40646000` ×104, `0x40616000` ×79, `0x40636000` ×8, `0x40666000` ×2; **60** distinct pairs | identical | **VERIFIED** |
| 19 | **0 of 60** distinct `blockid`s name a `gmdl` | 60 ∩ 11 041 `gmdl` records across 33 groups = **0** | **VERIFIED** |
| 20 | 1549 `0x5CAA5E28` records; families 58 cell / 482 creature / 224 building / 540 vehicle / 107 flora / 138 `0x40686000` | 1549, per-group identical | **VERIFIED** |
| 21 | `plt` records number **0** | 0 records of `0x011989B7` in **all 14** installed packages (the proposal says 7 — see 4.3) | **VERIFIED as a negative** |
| 22 | `serde`, `serde_derive`, `ron` in `Cargo.lock`; `serde_json` absent | present / absent, as stated | **VERIFIED** |
| 23 | `serde_json` writes `null` for NaN/±Inf | `serde_json-1.0.150/src/ser.rs`: `serialize_f32` 155-165 and `serialize_f64` 168-178 both `classify()` → `write_null` | **VERIFIED** |
| 24 | the in-tree JSON writer has no float variant and cannot parse one | `crates/spore-tools/src/json.rs:23-34` has `Int(i64)` and no float arm; `:355-376` `number()` consumes `.` digits then calls `text.parse::<i64>()` | **VERIFIED** |
| 25 | the in-tree fail-closed reader rejects unknown fields / repeated keys, depth 64 | `crates/spore-semantic-bridge/src/json.rs:17, 25, 60, 564-578` | **VERIFIED** |
| 26 | f32 `{:?}` round-trips every finite pattern; 4 278 190 080 of them | `2^32 − 2·2^23 = 4 278 190 080` arithmetic confirmed; 1 050 605 strided + 2 000 000 pseudo-random + dense exponent coverage + `±0.0` + `MIN_POSITIVE` → **0 failures** (sample, not the full sweep) | **VERIFIED (sampled)** |
| 27 | `0x5CAA5E28` absent from `tools/spore/types/typenames.json` | absent | **VERIFIED** |
| 28 | the rigblock groups in `Spore_Game` contain only `0x5CAA5E28` | `0x40616000/0x40626000/0x40636000/0x40646000/0x40666000` → 58/482/224/540/107, no other type | **VERIFIED** (and see 4.3 for the `PatchData` caveat) |
| 29 | 68 is the **complete** set of creator documents | swept all 14 packages, every record decompressed: exactly 136 XML-leading records = 68 in `Spore_Game` + 68 in `PatchData`; the other 15 XML records are `0x0330A65E` audio (`<tracks>`, `<pattrstorage>`) and `0xCF6C21B8` UI definitions (`<Project>`, `<SporeUIFonts>`) | **VERIFIED** — this closes report 01 §8 Q7, which was listed as UNKNOWN. Caveat: the test is a prefix test, so a record with a BOM or leading whitespace would be missed |
| 30 | 68 documents → 5 property presence classes; skin fields all-or-nothing; `name`⟺`description`; `hasthumbnail`⟺`cacheid` | exactly as tabulated in §1.6; 68/68 for every correlation | **VERIFIED** |

### 4.2 Wrong — with the correct value

| # | §1/§9 claim | measured | severity |
|---|---|---|---|
| **W1** | **§0 type ids**: `crt` `0x02B92A46`, `cll` `0x0AC5D2DC`, `vcl` `0x246C29D4`, `ufo` `0x47764BB7`, `bld` `0x238BC675`, `flr` `0x43854287` | **every one has 0 records in the index.** Correct ids, confirmed twice: `crt` `0x2B978C46` (47), `cll` `0x3D97A8E4` (13), `vcl` `0x24682294` (4), `ufo` `0x476A98C7` (2), `bld` `0x2399BE55` (1), `flr` `0x438F6347` (1). 02 §9's decode of the in-game type dictionary maps `0x2b978c46→crt`, `0x2399be55→bld`, `0x24682294→vcl`, `0x476a98c7→ufo`, `0x438f6347→flr` — independent byte evidence | **BLOCKER.** This is the document's headline table, it is captioned "verified by direct census of the DBPF index, not inferred", and it is wrong in all six rows. An implementer who types those constants gets a crate that finds nothing |
| **W2** | **§1.1 `formatversion`**: `11` ×1, `15` ×19, `16` ×58 | **`11` ×1, `15` ×8, `16` ×59** (sum 68). The published figures sum to **78** | **BLOCKER** — internally inconsistent with its own population, and both counts are wrong |
| **W3** | **§1.5**: `0x5CAA5E28` sizes "range 36 → **8631**" | min 36, **max 49 188** (vehicle family). Per-group maxima: cell 228, creature 804, building 3108, vehicle 49188, flora 804, `0x40686000` 84. 212 records are exactly 36 B | **MAJOR** — the 472-byte-block / 36-byte-floor argument in §5 and the "sparse/varint encoding" characterisation in §1.5 are both argued from a wrong ceiling |
| **W4** | **§1.4**: `paintregion` "~15 distinct values" | **23** distinct values corpus-wide (`0x01`…`0x09`, `0x0b`…`0x0e`, `0x10`…`0x15`, `0x18`, …) over 195 occurrences | **MAJOR** — and report 01's competing "7 distinct" is wrong too, so three files now carry three answers |
| **W5** | **§3.1**: "**92** of 680 blocks carry a general rotation" | tolerance-dependent and unstated: as-written **188**, at 1e-5 and 1e-4 **92**, at 1e-6 **140**, at 1e-3 **92**. Report 01's table (90/498/92) reproduces only at **1e-3** | **MAJOR** — the repo's own float tolerance is 1e-6, where the answer is 140. The companion claim "all 79 roots are identity-or-pure-Y" is true at 1e-3 (32 identity + 47 pureY) and **false at 1e-6** (8 roots fail) — and it fails by `1.0000000000287557e-06`, i.e. the `f32` rounding of `row0[0] = 0.999999`. The two sentences in §3.1 are mutually consistent only under a tolerance nobody wrote down |
| **W6** | **§1.2**: "`childlist` order in the corpus is **descending** (`8,7,6,4,3,2,1`)" | **507** of 520 lists have exactly **one** entry; only **13** have more, of which **12** are descending and **1** ascending | **MINOR but load-bearing on its own reasoning** — the claim is generalised from 12 lists. The *policy* it justifies ("preserve as written, never sort") is right and must stay |
| **W7** | **§0**: "`01-domain-model.md` and `02-parts-and-catalogs.md` reached the same census **independently**" | they did not. 02 §5.3 says `childlist` 437, `childid` 439, `weight` 128, `trianglepickorigin` 476; the true values are 520, 601, 252, 670. Four disagreements, none adjudicated | **MAJOR** — `AGENTS.md` requires the tie-break to be run and recorded; here the corpus settles it and the proposal asserts agreement instead |
| **W8** | **§1.1**: the `modeltype` spelling trap "applies to **every** `u32` field in the grammar" | measured: `modeltype` is the **only** mixed-spelling field (67 hex / 1 decimal). `blockid` 680/680 hex, `paintregion` 195/195 hex, `paintid` 195/195 hex, `@channel` 252/252 hex, `requiredpack` 66/66 hex, `skineffect*` 61/61 hex each; `zcorpscore` 59/59 decimal, `cacheid`/`hasthumbnail` decimal | **MINOR** — the decoder must still accept both spellings (`spore_core::parse_id` already does: `crates/spore-core/src/key.rs:116-131`, with both spellings asserted at `:251-254`), but "the corpus is inconsistent everywhere" is an engineering extrapolation |
| **W9** | **§9**: "`blockid` names only `0x5CAA5E28`" | true in `Spore_Game`; **all 60 pairs also name a `prop` (`0x00B1B104`) at the same `(group, instance)` in `PatchData`**, carrying UTF-16 text (I read 60/60: `fe_root_dummy`, `blade`, `Spherical|round.`, `square|win`, `triangular|point`, …) | **MAJOR** — and it refutes 02 §10.3 "**There is no name.** No record maps a rigblock … instance id to a string", which is exactly the premise 05 §7.5 uses to justify "no stable part id" |
| **W10** | **§1.2/§9**: `plt` "0 in **all 7** packages" | 0 in **all 14** installed packages | **MINOR** — the negative is stronger than stated |
| **W11** | **§1.2** omits `limbtype` (5), `musclescale` (5), `basemusclescale` (8) | present: `limbtype` crt 4 + flr 1; `musclescale` crt 4 + flr 1; `basemusclescale` crt 6 + flr 2 | **MAJOR** — §1.2 presents itself as the census of `<blockref>` elements; three measured elements are missing from it **and** from §4's model |
| **W12** | §1.3 `triangledirection` present in all 680 vs §4 `triangle_direction: Option<Vec3>` | the corpus has 680/680 | **MINOR** — see §1.1 |

### 4.3 Claims I could not overturn, and one lead nobody has opened

* `spore-assets/src/graph.rs:29` grades `EdgeKind::SiblingRecord` at `EvidenceLevel::Inferred`
  — the resolver's `Ambiguous` justification is real and correctly graded.
* `cEditorResourceBlock` = 472 B, `cEditorResource` = 172 B, magic `0x48657ed3`, versions
  3/4/5, `count ≤ 1024` — these come from 06's decompilation and I did not re-decompile.
  They are cited, not verified by me.
* **New lead, measured, and it belongs in §6's blocker list.** Type `0x1A99B06B`
  (`bem`), **1709 records in `Spore_Content`**, and **1709/1709** of them begin with the
  *same four bytes* as the confirmed `cEditorResource` magic (`d3 7e 65 48` = u32
  `0x48657ed3`), with the version word `5` in 1709/1709, a third word `< 0x401` in all,
  and a fourth word `0` in all — that is five of the six header constraints
  `FUN_004bf9f0` enforces. Their groups are exactly the seven model-slot groups
  (`0x40626200` 759, `0x40636200` 310, `0x40646200` 285, `0x40666200` 153, `0x40656200` 91,
  `0x406b6200` 67, `0x40616200` 44 = 1709), i.e. **one per model slot**. **But** 0/1709
  satisfies `len == 152 + 128 + count × 472`, so `bem` is **not** the documented
  `cEditorResource` body. Report 02 §3.4 saw the magic and attributed it to a shared
  `bem`/`world_object` sub-header without connecting it to the creator container.
  **Grade: UNKNOWN, and it is the cheapest unexplored thing on the page** — 1709 records
  with the creator container's magic, sitting exactly where models are.
* **New negative, recorded so it is not re-run:** the UI definition records
  (`0xCF6C21B8`, `<Project>` with `<Class>`, `<Interface>`, `<Enum>`, `<EnumVal>`,
  `<Property editorhint=… browsable=…>`, `<RecordType typeid=…>`) do **not** contain the
  string `5caa5e28` in any form. That lead (02 §9's "cheapest open question") is closed
  as a negative for this file family.

---

## 5. Overreach hunt

**Nothing in the plan invents a binary format, fabricates an attachment or deformation
rule, or builds UI.** Those three prohibitions are honoured: §5 defers the binary writer
with a *reason* (fixed 472 B stride, no room for `Unclaimed`, writer not decompiled, and
06 U10's finding that the nominal writer symbol resolves into an EASTL allocator), §8
refuses sockets, part-kind taxonomies, non-uniform and negative scale, deformation
semantics, and any claim about `paintregion`/`paintid` meaning, and no step in §7 is a UI
step. On the explicit prohibitions: **clean.**

What *is* built before its model is understood:

1. **§7 step 6 (engine projection) before step 7 (the resolver).** With the resolver at
   0/60, the only reachable projection path is the failure path. This makes 05 §6.1's
   `P4  project(load(save(doc))) == project(doc)` a comparison of two empty scenes — a
   green test that proves nothing, presented as the render-identity guarantee. Fix: either
   move the resolver ahead of the projection, or scope step 6 to "additive fields +
   guards + synthetic-fixture tests, no projection", and do not let `P4` be claimed until
   a real document resolves at least one block.
2. **§7 step 5 (undo/redo) before any client exists.** Undo is only exercised by a caller.
   Its index-space rule (05 §7.4) is right and worth writing down, but building it before
   step 6 means building it against a model nothing consumes. Move after 6/7.
3. **§7 step 8's gate is incoherent**: "Bevy preview of a *composed* document | headless
   via `MinimalPlugins` only". A preview is a window; `MinimalPlugins` is a test
   configuration. `AGENTS.md` also records that `DefaultPlugins` builds a winit
   `EventLoop` and panics off the main thread. Split into "composed-scene assembly under
   `MinimalPlugins`" (the test) and "windowed preview" (the deliverable), and say which
   one the gate means.
4. **§7 step 3's gate "version-gated fields"** is a category error (§3.6 above).
5. **The `spore-json` extraction is scheduled nowhere** yet §5 depends on it (§2 above).
6. **No differential test is scheduled** against the Python XML path, which is the one
   mechanism this repository has for the rule "the same record must never have two
   different answers" (§2 above).

One thing I will *not* call overreach, because it is asked for and earned: keeping the
authoring loop buildable before the geometry blocker is solved. Cataloguing, parsing,
composing, saving, loading and verifying 68 real documents is the right first milestone,
and §6's refusal to render a placeholder mesh "pretending to be a body part" is exactly
the discipline `AGENTS.md` asks for.

---

## 6. The unresolved verdict, attacked

> §0.3: *the shared-**behaviour** thesis is not [justified]. 469 `/Spore/Editors/*` types
> exist and none mention vehicle, building, UFO or terrain; no `cVehicleEditor`
> class-name string exists in the binary. The corpus justifies a shared **data** core
> only.*

I tried four routes to refute it.

**Route A — the discriminator lives in the data, so the behaviour is data too.** 06 U3
says `mModelType` is "almost certainly the creature/vehicle/building discriminator". If a
`modeltype` value → family map existed, one would have a cross-family discriminator in
the *document*, which would move behaviour into the shared layer. **Measured: the map is
in the document but is not usable as one.** The ten distinct `modeltype` values partition
as `crt → 0x9ea3031a` (46 + the same value written in decimal), `cll → 0xdfad9f51` (13 —
which is *also* the `cell` record type id), `ufo → 0x98e03c0d` (2), `vcl →` **four**
distinct values across 4 documents, `bld → 0x47c10953` (1), `flr → 0xbcd73e89` (1). One
value per family for four families, **four values for the vehicle family**, and the cell
value is a *different record type's* id. A discriminator with four values in one family
and a borrowed id in another is not a behaviour boundary; it is an opaque word with a
per-family convention. **Route fails** — and it is a warning against ever promoting
`modeltype` from `UNKNOWN` on the strength of "it looks like a discriminator".

**Route B — `kEditorType` is one enum on one class, so the editors are one class.** 06
§2.3 establishes `kEditorType = {Cell, Creature,Building, Vehicle, Accessories}` with
five per-editor-type slots registered on a single class. That is real. But the same report
records that the owning class is **UNKNOWN** (`PTRREF_00584230` resolves into a code
region, not a vtable), and that the enum is "evidence of a shared enumeration, **not**
evidence of a shared base class". A shared enum is a shared *vocabulary*; the vehicle
editor could be a subclass, a configured instance, or a reimplementation that borrows the
enum. **Route fails.**

**Route C — the asset layer is shared, therefore the tool that edits it is shared.** This
is the strongest form of the argument and it is genuinely seductive: one record type,
one grammar, six families, `bem` sharing the container magic. But 02 §10.1 draws exactly
the line the proposal needs: **a part is not a mesh** and **a part's geometry is not in
the data** (1 549 rigblocks, 6 floats of bounds plus an undecoded tail). An editor's
behaviour is about sockets, snap rules, magnet compatibility and limits — nine
`fixed_vector<ResourceKey,8>` lists on `EditorRigblock`, all of which 02 §6.4 and the
proposal's own §2.7 record as **code-only, zero bytes in the corpus**. So the shared part
of the domain is precisely the part with no behaviour, and the part with behaviour is
precisely the part the corpus cannot see. **Route fails** — and it is the strongest
statement in favour of the split, stronger than the proposal makes it.

**Route D — the string evidence is thin enough to be a naming artefact.** Five editor
display names (`CellEditor`, `CreatureEditor`, `HutEditor`, `BuildingEditor`,
`PlanetEditor`) exist, but 06 §2.2 shows they sit in a shader/thread-name LUT, are
labels rather than classes, and that `BuildingEditor`/`PlanetEditor` occur exactly once
each with no class behind them. `cAppModeEditorBase::Save` is one string with one
reference and no structure. So even the *positive* hints for a shared base are
non-evidence by 06's own standard. **Route fails.**

**Verdict: the split survives all four attacks, and it is the load-bearing judgement of
the whole proposal.** I would strengthen it in one direction and weaken it in none. The
strengthening: the split is not "data vs behaviour", it is **"what the corpus can see vs
what it cannot"**. Every shared concept is shared because it is *serialised*; every
behavioural concept is unshared because it is *code*. That reframing has a practical
consequence the proposal should adopt: **a new shared concept may only enter the core if
it is bytes we can read**, and the test for that is not "is it shared across families" but
"is it in the 68 documents or in the 472-byte record". `snapping` fails the second test
even though it is obviously shared in intent.

One caveat I will not let pass: 06's negative is a **CONFIRMED negative for the *named*
SDK surface and UNKNOWN for the unnamed one**, and 04 §7.3 records that
`~/apps/Spore-ModAPI` **does not exist on this machine** (I checked — `AGENTS.md`'s claim
that the SDK is at that path is stale), so every SDK-derived name reached the reports
through the committed `sdk_functions.tsv` and the passport. The negative is therefore
"the corpus has exactly one **named** creator implementation", not "the corpus has one
creator implementation". The proposal should say the former. It currently implies the
latter by citing "469 types" without the qualifier.

---

## 7. Missing concepts

Genuinely shared across the six families that §4 does **not** model, each with evidence
or an explicit UNKNOWN:

| # | concept | evidence | what to do |
|---|---|---|---|
| M1 | **The binary face of the same documents.** `bem` = 1709 records, one per model slot, in the seven model-slot groups, carrying the confirmed `cEditorResource` magic in 1709/1709 and version word 5 in 1709/1709, but **not** fitting the confirmed body layout (0/1709 on `152 + 128 + count × 472`) | measured (§4.3) | **UNKNOWN.** Add to §6's blocker list as a second, cheaper attack on the rigblock tail and as the reason "read later" is not a research project but a decode. Do **not** model it |
| M2 | **A per-block display name.** All 60 `blockid`s name a `prop` at the same `(group, instance)` in `PatchData`, carrying UTF-16 text; 0/60 in `Spore_Game` | measured (§4.3, W9) | Meaning UNKNOWN (prop layout is a standing `NonFinding`), and it is **package-order-dependent** because `PatchData` overrides every shared `prop`. Model as `Option<BlockLabel>` with grade `Unknown` **or** record explicitly that the core declines to read names and why — but do not inherit 02 §10.3's "there is no name", which is false |
| M3 | **The family partition.** The upper 16 bits of the group are the *only* cross-family discriminator (02 §8), and group→type is the inference the resolver performs | 02 §8 + my group census (§4.1 #28) | Claims subject `blockid_type_from_group_family` at `INFERRED`, with a test over the six groups. This is the load-bearing inference of the resolver and §1.5 states it as `VERIFIED` |
| M4 | **`limbtype` / `musclescale` / `basemusclescale`** | 5 / 5 / 8 occurrences, in `crt` and `flr` | Model them (`Option<Limb>` as 01 does) or route them to `unclaimed`. Dropping measured elements from a census table is the one failure mode §4's own `Option` discipline exists to prevent |
| M5 | **The `@count` redundancy family** | 787 attributes across four elements, 0 mismatches | Policy + test (§3.6 above). It is the cheapest possible loader tripwire and it is currently unassigned |
| M6 | **The five property presence classes, and the fact that the version predicts none of them** | §1.6 / §3.6 | Doc comment + a census assertion. Cheap, and it stops a future implementer "optimising" `Option`s into defaults |
| M7 | **Package priority as part of block identity** | 02 §7.2: 21.5 % of keys live in more than one package and all 1162 shared `prop` records differ in bytes | Anything the creator reads out of a `ContentStore` must record the answering package, or the same document yields different bytes on a different machine. This is a determinism requirement, not a note |
| M8 | **The corpus's UI definition language** | `0xCF6C21B8`, `<Project>`/`<Class>`/`<Interface>`/`<Enum>`/`<Property editorhint=…>` records in `Spore_Graphics` and `Spore_EP1_Data` | **Scope note only.** §8's "no editor UI" is a *scope* decision, not a knowledge ceiling — the game's own UI definitions are readable. Say so, so nobody later files it as an impossibility |
| M9 | **Spore_UIFonts / audio XML coexist with the creator XML** | 10 audio XML records (`<tracks>`, `<pattrstorage>`) in the same corpus | Not a creator concept. Recorded only because it proves the corpus uses XML for authoring data generally — which slightly strengthens §0's "we do not invent a document format" and slightly weakens "XML is not canonical" |

---

## 8. Required changes before implementation starts

Ordered. Items 1–5 are correctness of the document itself and cost nothing but edits;
6–11 change the plan; 12–14 are additions that the corpus has already paid for.

1. **Fix the six type ids in §0 (W1).** `0x02B92A46 / 0x0AC5D2DC / 0x246C29D4 /
   0x47764BB7 / 0x238BC675 / 0x43854287` have zero records. Use `0x2B978C46 / 0x3D97A8E4 /
   0x24682294 / 0x476A98C7 / 0x2399BE55 / 0x438F6347`, and cite 02 §9's in-game type
   dictionary as the independent confirmation. Add a corpus test that each of the six ids
   resolves to 47/13/4/2/1/1 records.
2. **Fix the `formatversion` row (W2)** to `11 ×1, 15 ×8, 16 ×59`, and add an arithmetic
   assertion to the census that the three counts sum to 68.
3. **Fix the rigblock size ceiling (W3)** to 36 … 49 188 with the per-group maxima, and
   re-check every argument in §1.5 and §5 that used "8631".
4. **Remove `roots` from the struct; make it a method** (§1.2), with the derived forest
   validated on load (`dangling` / `multiply_parented` / `has_cycle` as findings, never
   repairs) and the roots distribution (62×1, 4×2, 1×3, 1×6) asserted over the corpus.
5. **Reconcile §4 with the format §5 adopts (0.3 #4.4).** Add `unclaimed` and
   `claims` to `CreatorDocument` (and `source`) or delete the `unclaimed`/`claim` record
   kinds from the format. Right now the two halves of the document cannot both be built.
6. **Close the rotation two-answer hole (3.1).** Adopt `is_legacy_expressible()` and
   `effective_rotation_y_degrees()` from 03 §7.3; make "`rotation` set **and**
   `rotation_y_degrees` non-zero **and** they disagree" a reported finding; add a test
   that the loader cannot be reached in that state. Also strike 01's refuted
   "four call sites" argument (§1.4) and replace it with the struct-update fact.
7. **Re-grade the orientation claim (W5).** State the tolerance next to every count:
   188 as-written, 140 at 1e-6, 92 at 1e-3; and state that 8 of the 79 roots miss 1e-6 by
   `1.0000000000287557e-06`, so "all roots are identity-or-pure-Y" holds at 1e-5 and not
   at the repo's own tolerance.
8. **Fix `paintregion` to 23 distinct (W4)** and adjudicate 01's "7" and 02's
   "childlist 437 / childid 439 / weight 128 / trianglepickorigin 476" as **refuted by the
   corpus**, in one place (W7). The `AGENTS.md` tie-break has to be written down, not
   assumed.
9. **Record the `childid`-direction adjudication.** The arithmetic (601 edges, 79
   components, 160 blocks with no `<childlist>`) excludes reports 02 and 06's
   "childid is the parent ordinal": a forest with 79 components has 79 roots, and the
   parent reading requires 160. State that `childid` is a **child** index and that the
   binary record's `parentIndex @8` is the *same graph spelled the other way*.
10. **Restore the three measured limb fields (W11)** as `Option<Limb>` or `unclaimed`,
    and put `limbtype` / `musclescale` / `basemusclescale` back into §1.2's census.
11. **Move `history.rs` out of the core (0.2 #3.3)** to `spore-creator-edit`, or state
    and test the invariant that keeps the document a value.
12. **Add `spore-json` to §3** (0.2 #3.4) with 05 §2.3's costed line items, or §5 is
    unbuildable as written.
13. **Add a `spore-differential` case** comparing the Rust `<sporemodel>` decoder against
    the Python XML path field-for-field over the 68 real documents, with the
    `None`-vs-`Some(vec![])` distinction among the assertions (§2).
14. **Extend §6's blocker list** with `bem` (M1) and the per-block `prop` (M2), and add
    the `UnavailableReason` variant that distinguishes "no record" from "tail not
    decoded" (§1.5). Add the claims subjects listed in §2 and M3, and add the `@count`
    policy + test (M5) and the property-presence-class assertion (M6).
15. **Reorder §7**: resolver before engine projection, undo after both; split step 8 into
    a headless assembly test and a windowed preview; relabel step 3's gate away from
    "version-gated fields"; schedule the differential test.
16. **Correct the small ones**: `childlist` order (W6 — 507 of 520 lists are single-entry;
    keep the never-sort policy, drop the "descending" claim), the `modeltype` trap's scope
    (W8 — one field, not every `u32`), `plt` "14 packages" (W10), `triangle_direction` to
    a plain `Vec3` (§1.1), `userorientation`/`bbox` under `BlockTransform` (0.3 #4.10),
    `Row3` as a flat alias (0.3 #4.12), `symmetric`'s grade raised with the 84/84
    reciprocity cited and 01/05's mask reading struck (§3 #3.5), and §8's refusal list
    extended with "no free-form `param` bag" and "no per-block `local_id`".
17. **Qualify 06's negative** as "one *named* creator implementation", and note that
    `~/apps/Spore-ModAPI` does not exist on this machine, so `AGENTS.md`'s SDK path is
    stale and every SDK-derived name came through the committed `sdk_functions.tsv`.

---

## 9. What is right and must not be lost

Not padding — these are the parts I tried hardest to break:

* **`Option<T>` = "not written", never empty, never default.** All five absences
  reproduce (160 leaves, 10 `trianglepickorigin`, 10 `blockbbox`, 11 `userorientation`,
  47 documents with no `paintlist`), and the distinction survives into the format's
  omission rule. This is the document's best idea.
* **`blocks: Vec<Block>` with order as the index space**, refusing a `BlockId`-keyed map,
  and carrying the `block_id` alongside every long-lived index reference so a stale index
  is *detectable*. Single authority, correctly assigned.
* **The failable resolver with a closed `Unavailable` vocabulary**, reported next to the
  composed result rather than dropping the block — and explicitly **no placeholder mesh
  pretending to be a body part**.
* **Refusing a second rotation representation**, negative scale for mirroring, non-uniform
  scale, sockets, a part-kind taxonomy, and any claim about `paintregion`/`paintid`
  meaning. All eight §8 refusals are earned, and I found no speculative generalisation
  among them.
* **No new binary format, and the export deferred with a stated reason** rather than
  "later".
* **`rotation: Option<Row3> → matrix` rather than a quaternion**, with the measured
  `1.131e-06` loss quoted against this repository's own `1e-6` tolerance. Correct, and it
  is the reason the model is right to store a matrix.
* **The boundary placement**: a new Bevy-free crate depending on `spore-core` only, with
  the projection in `spore-engine`. Right direction, right reason.
* **"we do not invent the semantics either, for everything the corpus already states"**
  — the ambition is correct and, once the eight corrections above are made, the document
  is close to earning it. `AGENTS.md`'s standard is that a new Rust file and an existing
  research document may not disagree without a byte count. This proposal had eight
  numbers that did not survive one.

---

## 10. Method, reproduction, and clean-room

Everything above came from read-only probes over `./SPORE` using the committed oracle
`tools/spore/dbpf/dbpf.py`, plus one standalone `rustc` binary built and run in
`/tmp/opencode/archrev`. Nothing under `SPORE/` was written, copied, or modified; no
Spore asset byte appears in this document (ids, counts, sizes, element names and UTF-16
decodings quoted from `prop` records are the same categories the six reports already
publish). No Rust in this repository was written, no build was run against the shared
target directory, and this file is the only file created. The `Spore_Game.package` /
`Spore_Content.package` figures come from my own census, not from `target/release/osptool`
— which §6 already notes predates the committed `spore-tools` source.