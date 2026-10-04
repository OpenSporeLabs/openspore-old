# 05 — Creator document serialization

**Agent:** SERIALIZATION. **Date:** 2026-10-04. **Status:** research + design, nothing implemented, no build run against the shared target dir, no commits.
**Reads:** `AGENTS.md`, `docs/BOUNDARIES.md`, `docs/CELL-CONTRACT.md`, `docs/tooling/semantic-exchange.md`, `crates/spore-core/src/{evidence,key}.rs`, `crates/spore-engine/src/domain/model.rs`, `crates/spore-tools/src/json.rs`, `crates/spore-semantic-bridge/src/{json,sha256,envelope}.rs`, `src/apps/cell_stage.cpp:809`, and — written in parallel by other agents in this series — `01-domain-model.md`, `02-parts-and-catalogs.md`, `06-semantic-evidence.md`.
**Ownership:** this file only.

---

## 0. Verdict, in one line

**Canonical JSON Lines, `spore-creator-doc/1`, hand-written writer and fail-closed reader, no `serde` — plus a *separate*, CONFIRMED-layout binary export (`cEditorResource` v5) that is derived, lossy by construction, and read-only in v1.**

---

## 1. Measured facts this design rests on

Two of the brief's premises were wrong. Both corrections matter, so they are
first.

| # | brief said | measured | how |
|---:|---|---|---|
| F1 | "There is **NO `serde`** anywhere in the Rust workspace" | **`serde`, `serde_derive` and `ron` are already in `Cargo.lock`** and already compiled into `spore-engine`, transitively: `bevy_asset → ron → serde_derive`, and `bevy_ecs`/`bevy_reflect`/`bevy_asset → serde`. | `Cargo.lock` reverse-dependency scan; `bevy` is a direct dep of `spore-engine` only |
| F2 | "there is no `serde_json` in the tree today" | **correct.** `serde_json` is absent. Adding it costs exactly **3 new crates**: `serde_json`, `itoa`, `zmij` (`serde_core` and `memchr` already present). Note it is `zmij`, not `ryu`, as of `serde_json` 1.0.150. | extracted `~/.cargo/registry/cache/*/serde_json-1.0.150.crate`; `Cargo.toml` `[dependencies]` |

**F1 changes the argument, not the conclusion.** The marginal dependency cost of
serde is *one line in one `Cargo.toml`*, not a new subtree. So the case against it
has to be behavioural, and it is (§2.2).

What is actually new is this, and it is the single most important fact in this
document:

| # | fact | grade | evidence |
|---:|---|---|---|
| F3 | A hand-written, dependency-free JSON **writer** already exists: `Json::{Null,Bool,Int,Str,Arr,Obj}`, `Obj` is a `Vec<(String, Json)>` so key order is insertion order and a hash map is structurally impossible, `render()`/`to_line()` exist, and `render_json_lines` already emits JSON Lines for `osptool manifest`. Its module doc already carries a committed *"Why not serde"* rationale. | **VERIFIED** | `crates/spore-tools/src/json.rs:1-116`, `:125`, `:85`; `crates/spore-tools/src/commands/manifest.rs:125` |
| F4 | That writer has **no float variant** (`Int(i64)` only) and its parser **cannot read a float at all**: `fn number` consumes the `.` digits then calls `text.parse::<i64>()`, so `0.5` is a parse error. It also has **no exponent support**, **no duplicate-key rejection** (`pairs.push`, last wins) and **no depth cap**. | **VERIFIED** | `crates/spore-tools/src/json.rs:355-377`, `:455-482` |
| F5 | A **strict, schema-driven, fail-closed** JSON *reader* already exists in the other crate: unknown field → `Schema` error with a dotted path; missing required field → error; **repeated key → error**; wrong shape (incl. `null` where a value is required) → error; depth capped at 64, object keys at 1024; *"Nothing in this module panics on any input."* | **VERIFIED** | `crates/spore-semantic-bridge/src/json.rs:1-49`, `:564-578`, `:1010-1016` |
| F6 | That reader is `pub(crate)`, has **no writer**, and its number type is `Val::Int(i64)` — **no float variant**. A hand-rolled SHA-256 (checked against the FIPS vectors *and* against the committed snapshot's own declared digest) sits beside it, also `pub(crate)`. | **VERIFIED** | `crates/spore-semantic-bridge/src/{json.rs:96,sha256.rs:1-10}` |
| F7 | The repo already owns a **float round-trip contract**: *"float serialization \| `%.9g` decimal — 9 significant digits round-trip float32 exactly"*, and the cell contract test compares fixtures bit-exactly with no epsilon. | **VERIFIED** | `docs/CELL-CONTRACT.md:34`, `:122-129` |
| F8 | The C++ `CELLSTAGE-SIMMANIFEST v1` prints `%.3f`, which does **not** round-trip. It is a *report* two runs are compared on, never reloaded — so it is a precedent for **stdout reports**, not for a persisted artifact. Do not copy its float format. | **VERIFIED** | `src/apps/cell_stage.cpp:810-830` |
| F9 | Float determinism in Rust needs no care about locale: `core::fmt` has no locale path at all. The only source of cross-build divergence is a different `rustc` float algorithm. | **VERIFIED** | absence of any locale symbol in `std`; §3.2 measures the guarantee instead of hoping |
| F10 | The original's creator serialization is **binary, fixed-stride, and CONFIRMED**: `cEditorResource` = 172 B header (`mNameKey` @8, `cEditorResourceProperties` @24 = 128 B) + `count × 472 B` `cEditorResourceBlock` array at +152, read as one bulk run by `FUN_004bf9f0`. Magic `0x48657ed3`, version ∈ {3,4,5}, `count ≤ 1024`. Every one of the 24 block fields confirmed twice (SDK layout + loader offset match). | **CONFIRMED** (06-semantic-evidence §4.1-4.3) | as cited |
| F11 | The original's undo is **snapshot-based on a single monotone index with the redo tail truncated on commit**: `CommitEditHistory` @`0x00586410` pushes a whole freshly-built `cEditorResource` onto `mEditHistory` (+372) plus a 44-byte `EditorStateEditHistory` onto `mStateEditHistory` (+352), truncating when `index < count`, then increments `mEditHistoryIndex` (+392). | **CONFIRMED** (06-semantic-evidence §1.3) | as cited |
| F12 | The original splits value from session: live `Editors::EditorModel` (224 B, `vector<intrusive_ptr<EditorRigblock>>`) ⇄ saved `cEditorResource` (value array), with `cEditor` holding the history. | **SUPPORTED** (06-semantic-evidence §4.5) | as cited |
| F13 | Zero XML crates in `Cargo.lock`. | **VERIFIED** | `Cargo.lock` scan |
| F14 | The corpus's own creator document is **XML** (`<sporemodel>`, 68 records / 680 blockrefs), `\r\n`, two-space indent — and its block **order is the `<childid>` index space**; reordering is not a valid edit. Leaves are expressed by **absence** of `<childlist>` (160/680), never by an empty list. | **VERIFIED** (01-domain-model §1.1-1.2) | as cited |

---

## 2. Format decision

### 2.1 The candidates

**(a) Hand-written canonical JSON Lines** — F3/F5 mean both halves of the
machinery already exist in-tree, in two crates, and neither is complete.

**(b) `serde` + `serde_json`** — F1: `serde` is already compiled in. F2: three new
crates.

**(c) Binary** — two sub-cases, and they have opposite answers:

* **c1 — the original's format.** Not invented: F10 is CONFIRMED twice over. So
  this is a legitimate target, and §2.4 says exactly how far v1 goes.
* **c2 — a new binary format of our own.** **Forbidden** by `AGENTS.md`
  (*do not invent binary formats*). Not proposed.
* **c3 — a derived binary cache** (decoded mips, a parsed-document index).
  **Permitted, explicitly, as a cache and nothing else.** Rule: keyed by the
  document's `content_sha256`, regenerable from the document by construction,
  never committed, never read as an authority, and a cache miss is never an error.
  Stated here because the brief asks for it to be explicit.

**(d) XML, matching F14.** Rejected as the *working* format, for reasons that are
about determinism rather than taste:

* XML has no canonical form. `&#x41;` vs `A`, attribute order, self-closing vs
  paired tags, `\r\n` vs `\n`, whitespace significance — a byte-identical writer is
  a research project, and the corpus's `\r\n` + 2-space indent (F14) is an
  *observation*, not a specification we can reproduce from a spec.
* No XML tooling exists in the workspace at all (F13).
* `Unclaimed` (01-domain-model) carries **verbatim XML fragments**. In a binary
  container those become base64 and stop being diffable, which destroys the one
  property that makes `Unclaimed` reviewable.

But XML is **not** rejected as an *interop target* — see §2.4.

### 2.2 Why not `serde_json`, specifically

Not the dependency count (F1/F2). Four measured behaviours:

1. **It writes `null` for a non-finite float.**
   `serde_json-1.0.150/src/ser.rs:169-179`:
   ```rust
   fn serialize_f64(self, value: f64) -> Result<()> {
       match value.classify() {
           FpCategory::Nan | FpCategory::Infinite => self
               .formatter.write_null(&mut self.writer).map_err(Error::io),
           _ => self.formatter.write_f64(&mut self.writer, value).map_err(Error::io),
       }
   }
   ```
   A `NaN` becomes `null` on disk. `null` in this format means *"not written"*. So
   the library's own error path silently manufactures the exact substitution
   `AGENTS.md` calls the root failure of this repository: **a value that is not a
   value, written as absence, and read back as a statement.** (`serialize_f32` is
   the same. Note `Number::from_f64` returns `Option`, so `Value` refuses it — the
   hole is only in the direct `Serialize` path, which is the path a derive takes.)
2. **Key order is not the writer's.** `serde_json::Map` is a `BTreeMap` by default
   (`src/map.rs:3-4,33-34`), so keys come out **lexicographically sorted** unless
   the optional `preserve_order` feature is enabled. Our order is the domain
   model's declaration order (F14's corpus order), which is not lexicographic.
3. **Duplicate keys in the input silently last-wins.** `Map::insert` overwrites.
   F5's reader refuses them precisely because *"which duplicate wins must not
   decide a function's identity"* — a rule we would be opting out of.
4. **Fail-open unknown fields** without `#[serde(deny_unknown_fields)]` on every
   type, and the derive-generated field order is a macro implementation detail, not
   a documented wire contract — which is a poor foundation for a format whose
   selling point is byte-determinism you can *read*.

Also: `spore-doc` must be Bevy-free (the load-bearing invariant in `AGENTS.md`),
so it cannot reach Bevy's `serde` without declaring its own direct dependency
anyway — at which point "it's already in the tree" stops being an argument.

**`serde` derive is banned on domain types specifically**: a derive makes the
*type definition* the wire contract, so the byte order of a document becomes a
compile-time consequence of field order in a `struct`. The format spec in §4 is
the contract; the structs are an implementation of it.

### 2.3 Cost of the recommendation, measured

Not "some work" — a bounded list:

| item | cost | note |
|---|---|---|
| `Json::F32(f32)` variant + `From<f32>` + writer | ~40 lines | `format!("{:?}", v)`; panic on non-finite as a last-resort invariant |
| float parsing in the existing parser (`.`/exponent → `f32`) | ~50 lines | replaces F4's `parse::<i64>()` |
| duplicate-key rejection + depth cap in the existing parser | ~25 lines | port F5's rules |
| extract `spore-tools/src/json.rs` → `crates/spore-json` | mechanical move | `spore-tools` is `lib name = "spore_tools"` + `bin osptool`, so a tool crate must not be a library dependency |
| float + schema-driven strict mode in the bridge reader | ~60 lines | `pub(crate)` today; needs promoting |
| `spore-doc` codec (grammar, 4 record kinds, digest) | ~500 lines | §4 |
| SHA-256 | 0 | reuse F6's implementation; do not re-derive |

**Rejected alternative — a fourth JSON implementation inside `spore-doc`.** It is
the cheapest single move and it is wrong: it would leave three JSON
implementations in the tree, which is "the same record must never have two
different answers" in spirit. The extraction is the recommendation.

**Extraction trigger, named in advance:** `spore-json` earns its existence when it
has two consumers, which is `spore-tools` and `spore-doc` from day one, plus
`spore-semantic-bridge` when its reader is promoted. The freeze discipline: the
existing unit tests move with the code unchanged — `object_key_order_is_insertion_order_not_sorted`,
`escaping_covers_exactly_what_the_grammar_requires`, `to_line_appends_exactly_one_newline`,
`everything_written_parses_back_to_the_same_value`,
`the_parser_preserves_key_order_so_a_round_trip_is_comparable`,
`a_repeated_key_is_refused`, `escapes_and_surrogate_pairs_are_decoded` — and any
behavioural change to them is a separate, reviewable commit.

### 2.4 The three artifacts, and their directions

| artifact | schema | direction | lossless? | v1 status |
|---|---|---|---|---|
| working document | `spore-creator-doc/1` | read + write | **yes, by construction** | **the deliverable** |
| original-format export | `cEditorResource` v5 (F10) | **write only, derived** | **NO — see below** | **not built**; read-only interop only |
| derived cache | none (opaque, digest-keyed) | read + write | n/a | permitted, never committed |

The export is lossy *by construction*, and this is measurable rather than
speculative: the v5 block is **fixed-stride 472 B with every one of the 24 fields
named** (F10). There is no length prefix, no extension block and no reserved
region. So a document carrying `Unclaimed` residue — 10 `blockbbox`, 11
`userorientation`, and every field a future corpus sweep finds — **cannot** be
represented. A writer that silently dropped it would be the repo's signature
defect wearing a new hat.

Two further reasons the writer waits:

* Only the **reader** is decompiled (`FUN_004bf9f0`). The write path is not.
* 06-semantic-evidence **U10** measured that the SDK's nominal
  `cCreatureDataResource::Write` @`0x004bfcb0` resolves *inside* `0x004bfc40`,
  whose disassembly is an **EASTL allocator**, not a serializer. So there is no
  trustworthy symbol to work from.

**So the honest v1 statement is: the working format is JSON Lines; the binary is a
future derived export whose lossiness must be reported as a diff, never as a
silent truncation.** When it is built, `osptool doc export --format spore-v5`
prints what it could not represent. `AGENTS.md`'s rule — *"do not relax a decoder
to make a test pass"* — has a writing-side twin: **do not relax an exporter to fit
a fixed stride.**

---

## 3. Determinism rules (normative)

### 3.1 Line discipline and field order

* **UTF-8, LF only.** A `\r` anywhere outside a JSON string escape is
  `DocError::CarriageReturn`. No BOM.
* One JSON value per line, newline-terminated, including the last. No blank
  lines. A line that is not exactly one value is an error, never skipped.
* **Field order is the order in §4's tables, full stop.** Objects are written in
  that order; the writer builds an ordered pair list (F3: a `Vec`, so no map
  iteration is even expressible). A reader **must not** reorder, must not sort,
  and must preserve input order so a re-render is comparable.
* **Record kinds appear in this order and no other:** `meta` (line 1), then all
  `part` lines, then all `unclaimed` lines, then all `claim` lines. A `meta` line
  anywhere but line 1 is an error.
* **`part` lines are in document order, and that order IS the `<childid>` index
  space** (F14, 01-domain-model). It is *not* sorted, because sorting would
  renumber the document. `BlockIndex` is therefore the line's 0-based offset among
  `part` lines, and **no `index` field is written** — a redundant index is a
  second source of truth that can disagree with the first.
* `unclaimed` and `claim` lines are in **document order** (the order the parser
  produced them). Same reason: order is data.

### 3.2 Float formatting — the exact algorithm, and the proof

**Rule.** Every float in the format is IEEE-754 **binary32**. It is written as
`format!("{:?}", value)` and nothing else. There is no precision parameter, no
`%.9g`, no exponent threshold, no special case.

**Why `{:?}` and not `{}`.** Both round-trip, but `{}` writes `0` for `0.0` and
`1` for `1.0` — a token that a reader cannot tell from an integer. `{:?}` always
writes a lexically-float token (`0.0`, `-0.0`, `1.0`), which is unambiguous
against the format's `Int` fields, and it switches to exponent form for extreme
magnitudes instead of emitting 39-digit runs (`1e-30`, not
`0.000000000000000000000000000001`).

**Proof that it round-trips — measured, exhaustively, on this machine:**

> For **every one** of the `2^32` `f32` bit patterns, excluding the `2 × 2^23`
> NaN encodings — **4,278,190,080 finite values** — the predicate
> `format!("{:?}", v).parse::<f32>().map(|b| b.to_bits()) == Some(v.to_bits())`
> holds. **0 failures.**
>
> Separately, **1,050,605** sampled `{:?}` outputs (stride 4093 across all bit
> patterns, plus dense coverage of the ±3 neighbourhood of every one of the 256
> exponents, plus both zeros and `f32::MIN_POSITIVE`) were checked against the
> strict RFC 8259 number grammar
> `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`. **0 violations** — no
> leading zeros, no bare `.5`, no trailing `1.`, no `inf`/`NaN`.

The full sweep is a `#[ignore]`d test (`float_round_trip_is_exhaustive_over_all_finite_f32`,
run with `--ignored`), because it takes minutes; the grammar check runs by default.

**Reader rule — a closed subset, not the JSON grammar.** A float token is accepted
iff it parses as a `f32` by `str::parse::<f32>()`, **and**:

* `NaN`, `nan`, `Infinity`, `inf`, `+inf`, `-inf` (any case) are **refused**
  (`float_not_finite`). A strict JSON reader already refuses them; assert it.
* a **non-zero** token that parses to `0.0` is **refused** (`float_underflow`).
  This is the cheap catch for a value written at more precision than binary32 can
  hold. A zero token that parses to `0.0` is fine.
* an integer-only token in a float field is accepted and narrowed. The writer never
  produces one, so this only fires on hand-written input.
* **`-0.0` is preserved.** The reader returns the sign bit, `save` re-emits `-0.0`,
  so the round-trip property holds bit-exactly through signed zero. A separate
  optional `canonicalize` *may* fold `-0.0 → 0.0`; it is **not** part of the
  round-trip property and `save` never calls it.

**Enforcement is doubled.** `spore_doc::validate(&Doc) -> Result<(), DocError>`
runs before any render and rejects non-finite; `Json::f32(v)` additionally
**panics** on non-finite as a last-resort invariant, exactly as `Json::push`
already panics on a non-object (`crates/spore-tools/src/json.rs:64-70`). A test
asserts `validate` catches it first.

**Cross-build.** `AGENTS.md` requires no timestamp, no PID, no absolute path, and
this format adds: **no toolchain version, no rustc version, no target triple.**
The reason is F9 plus the measurement above: a `rustc` whose float algorithm
differed would change the bytes, and the honest response to that is the
`content_sha256` field plus a version bump — not a compiler fingerprint smuggled
into the artifact.

### 3.3 Collections, ids, ordering

* Array order is data everywhere (§3.1). No array is sorted by the writer except
  where §4 says so explicitly.
* Where an order must be canonical rather than historical (currently: **none** in
  `/1`), the rule is *sort by the tuple, then by the id string, then by the
  document order* — three keys so the sort is total.
* **`content_sha256` is not self-referential**: SHA-256 over lines **2..N** only,
  each including its `\n`. The `meta` line is excluded. F10's `count × 472`
  analogue, and the passport's rule verbatim
  (`docs/tooling/semantic-exchange.md` §2).
* `counts` in `meta` is informational and **recomputed and cross-checked** on load.
* **Document identity is content, not a clock.** `meta.document_id` is a
  `doc:<16 lowercase hex>` value that the writer preserves if the input had one
  and otherwise mints once. It is never a hash of the content (that would make it
  change on every edit) and never a timestamp.

### 3.4 Explicitly forbidden in the artifact

| forbidden | why |
|---|---|
| timestamps, dates, "modified" fields | two saves of one state must be byte-identical |
| PIDs, host names, user names, `$HOME` | environment leakage; `AGENTS.md` |
| **absolute paths** | `spore-semantic` refuses them in `provenance.artifact` (`crates/spore-semantic-bridge/src/contracts.rs:240`); same rule, enforced on read |
| hash-map iteration order | structurally impossible with F3's `Vec`-of-pairs writer, and banned for any collection built by a caller |
| locale-dependent formatting | F9; and `LC_ALL` is never read |
| mesh/geometry payload | a creator document is *placement*, not geometry. Vertices, indices and texels are resolved from `(group, instance)` at load. This keeps a 680-block document in the tens of kilobytes. |
| a resolved texture or material **handle** | 01-domain-model §4 rejects it: a handle in a saved file freezes one resolution into the artifact |
| `NaN`, `Infinity` | §3.2 |
| a `null` where a value is required | F5's rule |
| toolchain / target identifiers | §3.2 |

---

## 4. The grammar

### 4.1 Common forms

```
f32        ::= <RFC8259 number>                  ; closed subset, §3.2
u32        ::= <RFC8259 integer> 0 .. 4294967295 ; JSON has no u32, range-checked
hexid      ::= "0x" %08x                          ; lowercase, zero-padded
tgi        ::= { "type": hexid, "group": hexid, "instance": hexid }
blockid    ::= { "group": hexid, "instance": hexid }   ; F14: <blockid> has NO type word
rows3      ::= [ f32, f32, f32 ]                 ; a ROW, flattened; see note
vec3       ::= [ f32, f32, f32 ]
rgba3      ::= [ f32, f32, f32 ]                 ; Spore ColorRGB, unit interval
index      ::= u32                                 ; into blocks, the childid space
```

`hexid` is the existing `hex_id` spelling
(`crates/spore-tools/src/json.rs:220`), which is also the passport's canonical VA
spelling. **A hex id is a JSON *string*, never a number** — same reason the
manifest gives: a decimal reader would have to be told the field was hex.

> **`rows3` flattening — a flagged divergence.** 01-domain-model models
> `Rows3 { row0, row1, row2 }`. Written flat as a 9-element array, `rows3` would
> be indistinguishable from a `vec3`-of-`vec3` and from nine loose scalars. I
> recommend instead:
> `"orientation": {"row0":[..],"row1":[..],"row2":[..]}`.
> **This is a request to CREATOR-DOMAIN, not a decision:** if the flat 9-array is
> kept, the field name must carry it (`"orientation_rows_flat"`), because a
> self-describing format that needs a comment to disambiguate a field is not
> self-describing. Everything else in §4 uses the nested form.

**The fact envelope** — the repository's own, reused verbatim
(`spore_core::Fact`, `crates/spore-core/src/evidence.rs:279-341`; wire shape from
`docs/tooling/semantic-exchange.md` §4):

```
fact      ::= { "state": "available",
                "evidence_level": <EvidenceLevel::as_str()>,
                "evidence_state": <EvidenceState::as_str()>,
                "provenance": [ prov, … ],       ; NON-EMPTY, required
                "value": <T> }
            | { "state": "unavailable",
                "evidence_level": "UNKNOWN",     ; forced
                "evidence_state": "MISSING",     ; forced
                "reason": <string> }             ; omitted `value`, no provenance
prov      ::= { "mode": "persisted"|"derived"|"live"|"imported",
                "reference": <string>,           ; portable id or repo-relative path
                "source_class": "generated_index"|"committed_artifact"|"ghidra"
                              |"derived"|"runtime" }
```

Enforced, not advisory (each is a named test):

* `available` with empty `provenance` → **refused**. `spore_core::Fact::new`
  already panics on this; the reader refuses it on disk too.
* `unavailable` carrying a `value`, a `provenance`, a non-`UNKNOWN` level or a
  non-`MISSING` state → **refused**.
* `unavailable` never becomes `0`, `false`, `""`, `[]` or `null`. This is the
  format's central prohibition and it is inherited, not reinvented.

**`origin` — the one new vocabulary, and why it is not an existing one.** A creator
document's provenance is not a reverse-engineering provenance. `EvidenceState` is
`LIVE|DERIVED|PERSISTED|MISSING` and `SourceClass` is
`generated_index|committed_artifact|ghidra|derived|runtime` — **none of which
describes a human clicking in a window.** So each envelope carries one extra
required key:

```
"origin" ::= "content_record"    ; read out of a Spore record
          |  "content_file"      ; read out of a package file on the author's disk
          |  "user_input"        ; the author typed or picked it here
          |  "engine_default"    ; a default this build supplies; claims nothing about Spore
          |  "derivation"        ; computed by OpenSpore from committed inputs
```

`evidence_level` keeps its meaning — **how strongly this value is known to match
the original game** — and is orthogonal to `origin`, which records *who supplied
it*, exactly as `EvidenceLevel` × `EvidenceState` already are. One enforceable
consequence: **`origin: "engine_default"` forces `evidence_level: "APPROXIMATION"`**.
A default nobody traced can never claim more. (ENGINEERING CHOICE, but it is the
one that stops an author from laundering an assumption into a claim.)

### 4.2 Record `meta` — line 1, always

Field order is normative.

| # | field | type | required | default / omission |
|--:|---|---|---|---|
| 1 | `record` | `"meta"` | yes | — |
| 2 | `schema` | `"spore-creator-doc/1"` | yes | — |
| 3 | `content_sha256` | 64 lowercase hex | yes | over lines 2..N; §3.3 |
| 4 | `document_id` | `"doc:<16 hex>"` | yes | preserved from input, else minted once |
| 5 | `written_by` | `"<tool>/<version>"` | yes | e.g. `"openspore/0.1.0"`. A name and a version, **not** a date |
| 6 | `formatversion` | `{"kind":"known","value":11\|15\|16}` or `{"kind":"unknown","value":<u32>}` | yes | tagged, so `Unknown(42)` is never an error and never a known version |
| 7 | `units` | `{"length":"meters"\|"spore_units","angle":"degrees"}` | no | omitted ⇒ `{"length":"meters","angle":"degrees"}`. Never inherited from anywhere |
| 8 | `source` | `tgi` | no | omitted iff `DocumentSource::source` is `None` |
| 9 | `properties` | object, §4.3 | no | omitted iff **every** property is `None` |
| 10 | `counts` | `{"blocks":u32,"roots":u32,"leaves":u32,"unclaimed":u32,"claims":u32}` | yes | recomputed and cross-checked on load |
| 11 | `digest_algorithm` | `"sha256"` | yes | literal, so the digest is never a guess |

**The omission rule, stated once and load-bearing:** *a field is omitted if and
only if its domain value is `None`.* Not "omitted when equal to the default". This
is what preserves the measured distinctions in F14 — `has_thumbnail` is `None` in
67 documents and `Some(false)` in 1, and `children` is `None` on 160 leaves and
never `Some(vec![])`. A "skip defaults" rule would collapse both.

### 4.3 `meta.properties`

Every field optional; omitted iff `None`. Order normative.

| # | field | type | notes |
|--:|---|---|---|
| 1 | `model_type` | `{"value":hexid,"evidence_level":<level>}` | present 68/68; **meaning UNKNOWN**. Graded, never a bare word — 01-domain-model's `GradedWord` |
| 2 | `skin_effects` | `[{"id":hexid,"seed":u32} × 3]` | one option for all six; all-or-nothing in 61/68, **observed, not enforced** |
| 3 | `skin_colors` | `[rgba3 × 3]` | one option for all three; same observation |
| 4 | `required_pack` | hexid | 66/68 |
| 5 | `zcorp_score` | u32 | 59/68. **What it scores: UNKNOWN** |
| 6 | `name` | string | 9/68 |
| 7 | `description` | string | 9/68, empty in 9/9 — written when `Some("")`, because that is what the record said |
| 8 | `has_thumbnail` | `true`\|`false` | 1/68, `false`. Omitted in the other 67 |
| 9 | `cache_id` | u32 | 1/68 |

### 4.4 Record `part` — one line per block, in document order

| # | field | type | required | omission |
|--:|---|---|---|---|
| 1 | `record` | `"part"` | yes | — |
| 2 | `block` | `blockid` | yes | — |
| 3 | `scale` | f32 | yes | `<transform><scale>`, 680/680 one positive scalar |
| 4 | `position` | `vec3` | yes | |
| 5 | `orientation` | `rows3` (nested, §4.1) | yes | proper rotation in 680/680 |
| 6 | `triangle_direction` | `vec3` | yes | **its own field**: it disagrees with `orientation.row1` in 102/680 and picking one would delete a finding |
| 7 | `triangle_pick_origin` | `vec3` | **no** | omitted iff `None` — absent in 10/10 of the one v11 document |
| 8 | `user_orientation` | `rows3` | no | 11/680; relationship to `orientation` UNKNOWN |
| 9 | `block_bbox` | `{"min":vec3,"max":vec3}` | no | 10/680; **whose** bounds UNKNOWN |
| 10 | `snapped` | `true`\|`false` | yes | 680/680 present, `true` in 6. **Always written**, even when `false` — it is never absent in the corpus, so absence would be a claim the corpus does not support |
| 11 | `children` | `[index, …]` | **no** | **omitted for a leaf** (160/680). `Some(vec![])` is written as `[]`, which is a different statement |
| 12 | `handles` | `[{"channel":hexid,"weight":f32}, …]` | no | `@channel` is opaque; 14 distinct |
| 13 | `paints` | `[{"paint_id":hexid,"paint_region":hexid,"color1":rgba3,"color2":rgba3}, …]` | no | 80/80 `@count == len(paint)` |
| 14 | `symmetric` | u32 | no | observed 1…31, so a **mask**; whose bits mean what: UNKNOWN |
| 15 | `limb` | `{"limb_type":u32,"muscle_scale":f32,"base_muscle_scale":f32}` | no | one option for all three; both scales are individually optional *inside* it (5 and 8 of 680), so each is omitted iff `None` |

**There is no `parent` field.** Deliberate, and it is the one place where the
obvious design is wrong:

* `PlacedBlock` has `children: Option<Vec<BlockIndex>>`, and a leaf is `None`.
* Deriving `parent` from `children` is lossless; deriving `children` from `parent`
  would turn 160 measured leaves into `Some(vec![])` — **converting "there was no
  `<childlist>` element" into "there was an empty one"**, which is precisely the
  finding 01-domain-model §5 exists to prevent.
* So `children` is stored verbatim and `parent` is *derived at load*, then
  validated. The derived graph is checked against the measured corpus invariants,
  each of which is a **load-time refusal**, not a warning: index out of range
  (`dangling`), in-degree > 1 (`multiply_parented`), a cycle (`has_cycle`).
  Measured: 0/601 dangling, 0 multi-parented, 0 cycles, max in-degree 1.

### 4.5 Record `unclaimed` — one line per verbatim residue

| # | field | type | required |
|--:|---|---|---|
| 1 | `record` | `"unclaimed"` | yes |
| 2 | `path` | string | yes — dotted from the root, e.g. `"blocks.blockref.3.newthing"` |
| 3 | `kind` | `"text"` \| `"attr:<name>"` | yes |
| 4 | `raw` | string | yes — verbatim, never normalised, never re-escaped beyond JSON's requirements |
| 5 | `owner` | `index` | **no** — omitted iff the entry is document-level |

Lines in document order. **This record is what makes a save lossless**, and it is
also the reason the working format is text: `raw` is an XML fragment, and in a
binary container it would be base64.

### 4.6 Record `claim` — free-form, evidence-graded (opt-in)

**Flagged as a request to CREATOR-DOMAIN.** `CreatorDocument` (01-domain-model) has
no claims array, and its `Unclaimed` cannot express this: `Unclaimed` covers *a
field we do not model*, a claim is *an assertion about a field we do model*. They
do not overlap. If CREATOR-DOMAIN declines, this record kind is deleted from `/1`
and §4.6 goes away — the format does not depend on it.

| # | field | type | required | notes |
|--:|---|---|---|---|
| 1 | `record` | `"claim"` | yes | |
| 2 | `id` | `"c:<16 hex>"` | yes | stable within the document; **never reused** |
| 3 | `subject` | `"document"` \| `index` | yes | what the claim is about |
| 4 | `statement` | string | yes | prose; no structure is imposed on it |
| 5 | `evidence` | `fact` | yes | no claim without provenance |
| 6 | `supersedes` | `"c:<16 hex>"` | **no** | omitted iff none. A claim is **never deleted** — see §5.3 |

Lines in document order.

### 4.7 Record `param` — **reserved, not enabled in `/1`**

The brief asks for free-form evidence-graded parameter entries. I am **not**
enabling them, and the reason is that every scalar the corpus carries is already a
*named, typed, optional field* in §4.3/§4.4, and anything the corpus carries that
we do not model goes to `unclaimed` verbatim. A free-form `param` bag would be a
second, weaker place for the same values to live — which is how two answers to one
record get created.

**Grammar, specified now so it is reviewable, and enabled by a named condition:**

```
param line ::= { "record": "param",
                 "id":    "k:<16 hex>",              ; unique within its subject
                 "subject": "document" | index,
                 "name":  <snake_case>,             ; ^[a-z][a-z0-9_]{0,63}$
                 "type":  "bool"|"f32"|"i64"|"string"|"hexid"|"vec3"|"rgba3"|"enum",
                 "value": <per type>,
                 "unit":  <closed unit word> | null,  ; null ⇒ the unit itself is UNKNOWN
                 "evidence": fact }
```

The explicit `type` tag is not decoration: it is what keeps `false` distinguishable
from `0` and `0` from `0.0` in a document whose whole subject is that
distinctions are values. `unit: null` is the honest spelling for a number whose
unit nobody established.

**Enabling condition:** the first corpus field that cannot be expressed as a named
typed field in §4.3/§4.4. Not a date, not "when convenient".

### 4.8 A worked example

Two roots and a leaf, three blocks. Note the leaf's **absent** `children`, the
absent `triangle_pick_origin`, and that `snapped:false` is written anyway.

```jsonl
{"record":"meta","schema":"spore-creator-doc/1","content_sha256":"<64 hex>","document_id":"doc:9f2c41a07b6d4e15","written_by":"openspore/0.1.0","formatversion":{"kind":"known","value":16},"units":{"length":"spore_units","angle":"degrees"},"source":{"type":"0x2b978c46","group":"0x40626200","instance":"0x00000042"},"properties":{"model_type":{"value":"0x9ea3031a","evidence_level":"UNKNOWN"},"zcorp_score":0},"counts":{"blocks":3,"roots":2,"leaves":1,"unclaimed":1,"claims":0},"digest_algorithm":"sha256"}
{"record":"part","block":{"group":"0x40616100","instance":"0x00001122"},"scale":1.0,"position":[0.0,0.0,0.0],"orientation":{"row0":[1.0,0.0,0.0],"row1":[0.0,0.0,-1.0],"row2":[0.0,1.0,0.0]},"triangle_direction":[0.0,0.0,-1.0],"snapped":false,"children":[1,2]}
{"record":"part","block":{"group":"0x40616100","instance":"0x00001123"},"scale":0.75,"position":[0.5,0.0,0.0],"orientation":{"row0":[1.0,0.0,0.0],"row1":[0.0,0.0,-1.0],"row2":[0.0,1.0,0.0]},"triangle_direction":[0.0,0.0,-1.0],"snapped":true,"paints":[{"paint_id":"0x00000003","paint_region":"0x00000001","color1":[0.8,0.2,0.2],"color2":[0.1,0.1,0.1]}]}
{"record":"part","block":{"group":"0x40616100","instance":"0x00001124"},"scale":0.5,"position":[0.0,0.5,0.0],"orientation":{"row0":[1.0,0.0,0.0],"row1":[0.0,0.0,-1.0],"row2":[0.0,1.0,0.0]},"triangle_direction":[0.0,0.0,-1.0],"snapped":false,"symmetric":3}
{"record":"unclaimed","path":"blocks.blockref.1.transform.blockbbox","kind":"text","raw":"-0.25 0.0 -0.25 0.25 0.5 0.25","owner":1}
```

Two lines of it are **measurable claims** and are marked as such in the tests:
the leaf at line 4 has no `children` key at all, and line 5's `owner:1` is the
document index of the block it belongs to, not a line number.

### 4.9 Errors and exit codes

Mirroring the passport's discipline (`docs/tooling/semantic-exchange.md` §6.1) —
the same distinction, `Syntax` vs `Schema`, on a Rust boundary:

| `DocErrorKind` | exit | meaning |
|---|--:|---|
| `Io` | 2 | file missing or unreadable |
| `Schema` | 3 | wrong `schema`, unknown record kind, unknown field, wrong shape |
| `Invalid` | 4 | dangling/multiply-parented/cyclic index, duplicate `document_id`, digest missing, non-monotonic record kinds, `\r`, blank line, two values on a line |
| `Float` | 5 | `float_not_finite`, `float_underflow`, integer out of `u32` range |
| `Digest` | 6 | `content_sha256` mismatch, or `counts` disagreeing with the body |
| `Provenance` | 7 | absolute path in a provenance `reference`, empty `provenance` on an available fact, a violated envelope invariant |

Distinguishing 3 from 4 matters and is the passport's exit-5-vs-6 lesson: an
unknown JSON field is `Schema`, **not** corruption. Collapsing them makes a
version problem look like a broken file.

---

## 5. Migration and versioning

### 5.1 Because the reader refuses unknown fields, *every* additive change is a major bump

That is the honest price of fail-closed, and it should be stated rather than
discovered. `spore-creator-doc/1` accepts exactly `/1`; `/2` is refused with
`Schema`, never half-read. A reader must not read a future schema as if it were
today's (F5's own words).

The mitigation is §4.2's omission rule: a document that uses no new feature stays
byte-identical across versions, because absent-means-`None` means nothing new is
emitted. Old file + new reader: identical bytes. **New file using a new feature +
old reader: an explicit error, never a silent misread.** That is the correct
direction for the error to point.

Procedure for `/1` → `/2`:

1. `read_v1` and `read_v2` coexist; `read_v1` is not deleted while any `/1` file
   exists.
2. The writer targets the newest only.
3. `osptool doc migrate` emits `/2` from `/1` and prints what it changed.
4. Test: `read_v2(migrate(read_v1(x))).domain == read_v1(x).domain`. **The bytes
   change; the value must not.** A migration that alters a value is a bug, and this
   is the test that catches it.

### 5.2 What is *not* a version bump

Reordering two adjacent fields that no reader depends on, adding a `reason` code,
adding a `claim`, adding an `unclaimed` entry, and adding a new **observed value**
to an existing open vocabulary (`formatversion` `11 → 18`, a new `pain_id` hex, a
new `symmetric` mask bit). Note the last one: this is exactly why
`FormatVersion::Unknown(u32)` and `hexid` exist — a fourth version and a fifth
`modeltype` must not fail a load.

### 5.3 Preserving an UNKNOWN, and filling it later

This is where the repo's rule meets a file format.

| state | stored as | later finding |
|---|---|---|
| a field exists, **no meaning known** | the value plus `evidence_level: "UNKNOWN"` (`model_type`, `handles[].channel`, `paints[].paint_id`, `symmetric`, `zcorp_score`, `limb.limb_type`) | the author rewrites the level to `OBSERVED`/`VERIFIED`. **The value never moves.** |
| a field we **looked for and did not find** | the fact envelope, `state: "unavailable"` + `reason` | the envelope flips to `available` |
| a field the **parser does not model** | an `unclaimed` line, verbatim | a new named field is added *and* the `unclaimed` line is retired — both in one commit, with a test |
| a field **absent in this record** | the key is **omitted** | nothing; absence is a fact about the record |
| a claim that turned out **wrong** | a new `claim` line with `supersedes` pointing at the old one | — the old line **stays**. `AGENTS.md`: *never delete a claim because it is inconvenient* |

**So: "a future PROVEN finding fills an UNKNOWN without breaking old files" holds,
and it holds for a specific reason — the format's unknown-preserving slots are
additive and inert.** An old file's `UNKNOWN` stays `UNKNOWN`; the new finding
lives in the knowledge base (`index.json`, an evidence pack) and reaches a
document only when a human or a tool edits it, which is an ordinary document edit
with an ordinary diff. **No build retro-fits a document**, because a build that
rewrites saved files is a build that can corrupt them.

One load-time obligation follows: **an unrecognised `reason` string must still
load.** The passport's reason table grew after the fact and older snapshots carry
codes a current build does not have; a `/1` reader that refused unknown reasons
would break files it once wrote. `reason` is validated only as *non-empty when
present*.

---

## 6. Round-trip and reconstruction guarantees

### 6.1 The properties

```
P1  load(save(doc))            == doc            exactly, bitwise
P2  save(load(save(doc)))      == save(doc)      byte-for-byte   (idempotence)
P3  save is a pure function of the document value; no clock, no env, no map order
P4  project(load(save(doc)))   == project(doc)   bitwise
P5  an unparseable input is refused; never partially applied
```

`P1` is stated **bitwise**, and that requires one thing from the domain model:

> **`spore_creator_core::CreatorDocument` must implement `PartialEq` with
> `f32::to_bits()` comparison, not `==`.** `f32 == f32` makes `0.0 == -0.0` true,
> which would hide exactly the class of bug §3.2 works to preserve; and it makes
> `NaN != NaN`, which would make P1 untestable. Bits comparison makes both cases
> decidable. (Non-finite is banned outright, so `NaN == NaN` under bits is a
> non-issue.)
>
> **This is a request to CREATOR-DOMAIN**, and it is the only thing in this
> document that requires a change to a type they own.

`P2` is the cheap workhorse: it is one I/O round trip and it *implies* `P1` for
any writer that is a function of its input, so both are tested.

`P4` is the "renders identically" claim, stated honestly. See §6.3.

### 6.2 The tests — all hermetic, no game install, no GPU, no window

In `crates/spore-doc/tests/`:

| test | proves |
|---|---|
| `save_is_byte_identical_across_two_writes` | P3 |
| `load_save_then_save_is_byte_identical` | P2 |
| `load_save_is_the_identity_on_documents` | P1, with bitwise `PartialEq` |
| `float_round_trip_is_exhaustive_over_all_finite_f32` | §3.2, `#[ignore]`d, run with `--ignored` |
| `float_debug_output_is_always_a_json_number` | §3.2 grammar check, sampled, runs by default |
| `float_negative_zero_keeps_its_sign_bit` | §3.2 |
| `float_subnormal_one_round_trips` | the `1e-45` end of the range |
| `float_reader_refuses_nan_and_infinity_spellings` | `Float` |
| `float_reader_refuses_underflow_to_zero` | `Float` |
| `float_reader_refuses_leading_zero_padded_integers` | §3.2 |
| `save_never_writes_a_non_finite_float` | `validate` runs before `render` |
| `content_sha256_excludes_the_metadata_line` | §3.3 |
| `content_sha256_detects_a_single_flipped_bit` | integrity |
| `counts_disagreeing_with_the_body_is_refused` | `Digest` |
| `record_kinds_must_appear_in_order_with_meta_first` | §3.1 |
| `part_order_in_the_file_is_the_block_index_space` | §3.1 — rename a block, expect a refusal |
| `a_leaf_writes_no_children_key_and_an_empty_list_writes_an_empty_array` | the 160/680 distinction |
| `an_absent_field_and_a_default_valued_field_are_different_bytes` | §4.2's omission rule; `has_thumbnail` is the case |
| `an_unavailable_fact_is_never_rendered_as_zero_false_or_an_empty_list` | the central prohibition |
| `an_available_fact_without_provenance_is_refused` | `Provenance` |
| `an_unavailable_fact_carrying_a_value_is_refused` | `Provenance` |
| `engine_default_origin_forces_the_approximation_level` | §4.1 |
| `a_load_refuses_an_absolute_path_in_a_provenance_reference` | §3.4 |
| `a_load_refuses_an_unknown_field` | `Schema`, not `Invalid` |
| `a_load_refuses_a_repeated_key` | F5's rule, now that F4's gap is closed |
| `a_load_refuses_a_carriage_return` | §3.1 |
| `a_load_refuses_a_cyclic_or_multiply_parented_child_list` | §4.4 |
| `a_load_refuses_a_dangling_child_index` | §4.4 |
| `formatversion_unknown_is_carried_not_refused` | §5.2 |
| `an_unrecognised_unavailable_reason_still_loads` | §5.3 |
| `no_geometry_payload_is_ever_written` | §3.4 — the file has no vertex/index/texel key |
| `a_v1_document_loads_unchanged_and_migrates_without_changing_the_value` | §5.1 |

Golden fixtures are **synthetic strings committed under
`crates/spore-doc/tests/fixtures/`** — hand-built documents, never a Spore asset
byte (`AGENTS.md`). Regeneration is double-run and byte-identical before write, the
`docs/CELL-CONTRACT.md` §7 discipline.

### 6.3 `P4` — and what is *not* claimed

`P4` must be stated as a **render-plan** comparison, not a pixel comparison, and
the reason is `AGENTS.md`'s own: `DefaultPlugins` builds a winit `EventLoop` and
**panics off the main thread**, so "the app assembles" is not a headless-testable
claim. The honest headless claim is that the GPU-free half is identical:

```rust
// crates/spore-engine/tests/creator_round_trip.rs — MinimalPlugins, no window
#[test]
fn render_plan_is_identical_after_a_document_round_trip() { /* ... */ }
```

comparing, **bitwise**: the projected `Scene`, every `MeshBuffers` from the same
`convert::to_buffers` path the renderer uploads, and every `MeshMaterialPlan`. That
is the whole of the GPU-free pipeline.

**Not claimed, and stated as a non-finding:** that the *pixels* are identical.
There is no rendered-original frame to compare against and a headless pixel
comparison needs an adapter. Per `docs/CELLSTAGE-VALIDATION.md`, that claim
requires the oracle discipline, and until it runs it stays open rather than being
asserted from a buffer comparison.

---

## 7. Undo / redo

Plain Rust data. **No ECS, no `bevy`, no `Entity`.** Compiles and tests with no
window and no game install. Placed in `spore-creator-edit`, a **sibling** of
`spore-creator-core`, never inside it — 01-domain-model §4 rejects an undo stack
inside the value type ("an undo stack in a value type turns the document into a
session"), and F12 shows the original makes the same split: history lives in
`cEditor`, the value in `EditorModel`/`cEditorResource`.

### 7.1 What an operation contains

```rust
// crates/spore-creator-edit/src/lib.rs
pub struct Op {
    /// Closed vocabulary: "set_transform" | "set_paints" | "set_snapped"
    ///                 | "set_label" | "set_child_list" | "insert_blocks"
    ///                 | "remove_blocks" | "set_property" | "add_claim".
    pub label: OpLabel,
    /// Targets, as document indices. See §7.5 — there is no stable part id.
    pub targets: Vec<BlockIndex>,
    /// The touched fields, before and after. A snapshot, not a delta.
    pub before: Vec<FieldSnapshot>,
    pub after:  Vec<FieldSnapshot>,
}

/// One captured field. A closed enum over the *mutable* fields only, so an
/// op can never name a field the format does not have.
pub enum FieldSnapshot {
    Transform(BlockTransform),
    Snapped(bool),
    Children(Option<Vec<BlockIndex>>),
    Paints(Option<Vec<Paint>>),
    Handles(Option<Vec<MuscleWeight>>),
    Symmetric(Option<u32>),
    Limb(Option<LimbFields>),
    Property(DocumentPropertyKey, Option<PropertyValue>),
    /// Whole-array snapshots, for the ops that renumber the index space (§7.4).
    WholeBlocks(Option<Vec<PlacedBlock>>),
    WholeUnclaimed(Option<Vec<UnclaimedEntry>>),
}
```

**Why before/after snapshots rather than a delta algebra.** Three reasons, in
order of weight:

1. **Inversion is total and free.** `fn invert(op) -> Op` swaps the two vectors and
   the label becomes `<label>_inv`. There is no inverse function to get wrong, no
   partial inverse, and no "this op is not invertible" case.
2. **It is the mechanism the original uses** (F11: snapshot-based, CONFIRMED). We
   are not importing an untested idea; we are declining to copy one detail of a
   known one.
3. **A delta scheme needs its own inverse algebra and its own tests**, which is the
   expensive part. `before`/`after` needs one test.

**How it applies:** `apply(&mut Doc, &Op)` writes the `after` snapshots, or for
`WholeBlocks` splices the array. **How it inverts:** `apply(&mut Doc, &op.invert())`.
That is the whole mechanism.

### 7.2 Transaction and coalescing boundary

```rust
pub struct History { undo: Vec<Txn>, redo: Vec<Txn>, index: usize }  // F11's shape
pub struct Txn { pub label: OpLabel, pub ops: Vec<Op> }
```

* `History::begin(label)` opens a transaction; ops append to it; `commit()` pushes
  **one** undo entry and truncates the redo tail (F11: the original truncates when
  `index < count`, then increments — same order, and the order matters).
* `abort()` replays the inverses in reverse order. This is only correct *because
  inversion is exact*, and it is the honest cost of applying eagerly to the live
  document so rendering stays live.
* **Coalescing.** Two consecutive ops join the same transaction iff **all** hold:
  1. same `label`;
  2. same `targets` and same touched-field set (the *coalesce key*);
  3. the newer op's `before` is **bit-equal** to the older op's `after`.

  Condition 3 is what makes coalescing safe rather than merely convenient: it is
  the continuity test, it is checkable, and it is what stops a slider drag that
  jumped back to its start from being merged into one entry. A drag becomes one
  undo step; a second drag of a different field does not.

### 7.3 Document and history serialise **separately**

* `doc.spcdoc` — `spore-creator-doc/1`. The artifact a human diffs and commits.
* `doc.spcundo` — `spore-creator-history/1`, **optional**, written only on
  explicit request, same JSON-Lines discipline (§3 applies to it unchanged).
* `osptool doc replay --history doc.spcundo --out replayed.spcdoc` applies the
  log and writes a document, so history is **verifiable** rather than trusted.

The reason for two files is not tidiness: if history lived inside the document,
then **undoing would change the document's bytes**, and P2/P3 would become
conditional on session state. The byte-identical-save property is worth more than
the convenience of one file. The reason the history schema is separate is that it
will churn far faster than the document, and a churning schema inside the artifact
forces version bumps on the artifact.

### 7.4 The index-space problem, and the one structural rule

F14: **block order is the index space.** So a `Vec<BlockIndex>` target is only
meaningful against a document state, and any op that inserts or removes a block
renumbers every index after it.

**Rule: a transaction that changes the number of blocks carries
`FieldSnapshot::WholeBlocks` for every op in it.** A transaction that does not may
use index + field snapshots. There is no third case and no "probably fine" path.

Two corollaries:

* Undo of a `remove_blocks` restores the exact array **with the exact indices**,
  so any op recorded before the removal stays valid. This is why the split exists.
* Reordering blocks is **not a valid edit** (01-domain-model), so index drift comes
  only from structural ops — precisely the ops that snapshot wholesale.

### 7.5 There is no stable part id, and inventing one would be a second identity scheme

F14: a `<blockref>` names `group, instance` — and that pair is shared by many
placed instances (the corpus has 680 blockrefs over far fewer distinct blocks). The
document therefore has **no per-instance identity**, and `BlockIndex` is the only
handle.

So the minimum a manipulation command needs is:

```rust
pub struct Selection { pub indices: Vec<BlockIndex> }   // sorted, deduped
```

with three rules:

* `Selection` is **session state and is never serialised.** It is not part of the
  document; a saved document has no selection, and loading one does not invent
  one.
* Any **long-lived** reference (an `Op` target, a `claim.subject`) additionally
  carries the part's `blockid`, so a stale index is **detectable** rather than
  silently pointing at a different part after an edit. That is the whole reason
  `blockid` is cheap to carry and worth carrying twice.
* Optional, **not** implemented in v1: a `local_id` on `PlacedBlock`, written only
  when set. **A request to CREATOR-DOMAIN**, and only worth making if a real
  workflow needs a reference that outlives an index-space edit.

### 7.6 What I would NOT build yet

Named, so they are decisions rather than oversights:

* **A patch/diff format.** `before`/`after` snapshots are the format. A third
  representation of the same change is a third answer.
* **Redo-tree branching** (keep-alternative history). One index and a truncation,
  like the original (F11). Branching is only justified by real usage.
* **Collaborative / OT / CRDT merging.** A merge format for two independently
  edited documents is a research project with a hostile-input surface, and there is
  no second author.
* **Undo across a schema migration.** Refuse it: the inverse of an op is only
  meaningful against the document version it was recorded against. Migrate, then
  the history is stale — say so rather than guessing.
* **Undo of a load.** Loading is not an edit.
* **Persisting history across sessions by default.** v1: per-session, saved only
  when asked.
* **A `Diff` view for the UI.** Not serialization's business.

---

## 8. Assumptions on the domain model (CREATOR-DOMAIN)

Named explicitly, because a serialization format that guesses at the value model
becomes a second source of truth for it.

| # | I assume | why it matters to serialization | status |
|--:|---|---|---|
| A1 | `CreatorDocument`'s field set is `source`, `format_version`, `properties`, `blocks`, `unclaimed` | it *is* the record-kind list in §4 | read from 01-domain-model |
| A2 | `PlacedBlock`'s field set and order | it *is* §4.4's field order | read from 01-domain-model |
| A3 | `Option<T>` means *not written*; `Some(vec![])` ≠ `None` | the omission rule (§4.2) depends on it | read from 01-domain-model §5 |
| A4 | Block order is the index space and reordering is invalid | §3.1, §7.4 | read from 01-domain-model |
| A5 | `spore_creator_core` has **no** `serde` derive and **no** `bevy` | §2.2 | agrees with 01-domain-model §4 |
| **A6** | **`CreatorDocument: PartialEq` compares `f32` by `to_bits()`** | P1 is untestable otherwise (§6.1) | **REQUEST** |
| **A7** | `Rows3` serializes **nested** (`{row0,row1,row2}`), not as a flat 9-array | self-description (§4.1) | **REQUEST** — flat is acceptable if the field is renamed |
| **A8** | `claims` is **not** in `CreatorDocument` | §4.6 is opt-in and deletable | **REQUEST** — `unclaimed` cannot cover it |
| **A9** | no `local_id` on `PlacedBlock` | §7.5 | **REQUEST**, optional, not needed in v1 |
| A10 | `DocumentSource.key` is a full `ResourceKey` (three components) | `source` is a `tgi`, whereas a block is a `blockid` with no type word — the asymmetry is F14's, and it is preserved | read from 01-domain-model |

**What I need from 02 and 06:** nothing further. F10 and F11 are quoted from 06
and F13/F14 from 01/02, all with their grades attached.

---

## 9. What this document does not decide

* **The domain model.** Read, cited, and asked three questions (§8). Not designed
  here.
* **The `cEditorResource` v5 writer.** Deferred with reasons in §2.4, the first of
  which is that only the reader is decompiled and the nominal writer symbol
  resolves into an EASTL allocator (06's U10).
* **XML export.** Acknowledged as the interop target the corpus actually uses
  (F14) and rejected as the working format in §2.1. Whichever agent takes it must
  first answer "what is a canonical `<sporemodel>` byte sequence?", which is a real
  question with no answer in this repository yet.
* **Whether `param` is ever enabled** (§4.7) — one named condition, not a date.
* **Whether the four record kinds are the right four.** `meta`/`part`/`unclaimed`
  follow the domain model exactly; `claim` is the only addition, and it is the only
  one that can be deleted without touching anything else.
