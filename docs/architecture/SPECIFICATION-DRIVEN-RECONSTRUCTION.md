# Specification-driven reconstruction (OpenSpore methodology)

Owner: orchestrator · Status: active proposal · Date: 2026-10-16

The methodology OpenSpore should follow, adapted from PhotoCraft
(`../research/PHOTOCRAFT-METHODOLOGY-CASE-STUDY.md`) to Spore's reality — no
published format spec, a proprietary EXE, a rich simulation, and a decades-old
modding ecosystem (`../research/SPORE-SPECIFICATION-MAP.md`). It **reuses**
existing OpenSpore machinery and proposes only what is missing. It adds no
dependency and ports no code.

## 1. The one rule that carries over

**The corpus outranks the port.** PhotoCraft picks a spec, implements low, and
compares to an independent oracle at an explicit tolerance with a monotone floor.
Spore has no spec, so the oracle *is* the corpus and the community parsers. Same
shape, different oracle source: `observation -> contract -> implementation ->
differential -> grade`, with the real bytes as the arbiter.

## 2. Unified evidence model (extend, do not invent)

Everything already exists in `spore_core::evidence`
(`crates/spore-core/src/evidence.rs`). **Reuse it verbatim**; a competing scale is
forbidden.

- `EvidenceLevel` — the canonical 7 rungs: `UNKNOWN` < `APPROXIMATION` <
  `INFERRED` < `SUPPORTED` < `OBSERVED` < `CONFIRMED` < `VERIFIED`.
- `EvidenceState` (`LIVE|DERIVED|PERSISTED|MISSING`) — *how* obtained, orthogonal
  to strength.
- `SourceClass` (`generated_index|committed_artifact|ghidra|derived|runtime`) —
  closed; static excludes `runtime`.
- `Fact<T>` — `Available` (must carry provenance) or `Unavailable` (implies
  `Unknown`+`Missing`, carries a `reason`, **never** a zero/empty/`false`).

The per-crate `claims` modules (e.g. `spore-gmdl/src/claims.rs`) are the concrete
instance: `GmdlClaim { subject, statement, fact }`, one graded claim per open
question, `claim(subject) -> Option<Fact>` where **absence is not `UNKNOWN`**.
`docs/tooling/evidence-model.md` already defines the wire envelope
(`availability`, `evidence_state`, `evidence_level`, `provenance`, `value`,
`reason`).

### 2.1 The proposed claim-record schema (a superset, still `Fact`-backed)

A *specification claim* needs a few fields the VA-scoped evidence pack does not
carry. One record type, aligned to `Fact`, adaptable to JSON under schema id
`openspore-spec-claim-1`:

```jsonc
{
  "claim_id": "gmdl.vertex_normal_encoding",   // stable, subsystem-scoped
  "subsystem": "format.gmdl",
  "question": "Are NORMAL/UBYTE4 bytes signed, biased, or scaled?",
  "source_class": "derived",                    // SourceClass (closed)
  "provenance": ["docs/ASSET-PATH.md#..."],     // required when available
  "preconditions": "a v8 gmdl record with a NORMAL element",
  "expected": "3 floats per vertex in a documented space",
  "tested": "byte/255 copy reproduced over the corpus",
  "inferred": "space is unknown; no bias applied",
  "limitations": ["v9 refused", "shader ids undocumented"],
  "implementation": ["spore-gmdl/src/claims.rs:VERTEX_NORMAL_ENCODING"],
  "tests": ["spore-differential/tests/gmdl_reference_table.rs"],
  "level": "INFERRED",                          // EvidenceLevel
  "next_experiment": "corpus histogram of byte values vs a known-good render"
}
```

Populated **from** the existing `claims` modules + `replacement-status.json` +
differential test names — not from a new hand-authored taxonomy of the ecosystem.
Keep guest addresses and register snapshots **out**; a claim describes behavior
and *links* to the RE detail.

## 3. Coverage: two linked, separate views

### 3.1 Format & resource coverage (per family, 8 levels)

A family is graded against eight levels; **a parser returning `Ok` passes only the
first two.**

| # | Level | Evidence that proves it |
|---|---|---|
| 1 | Identified | the resource id/type is recognized |
| 2 | Parsed | framing walks end-to-end on the corpus |
| 3 | Structurally validated | offsets/sizes reconcile; refusals are explicit |
| 4 | Decoded to a documented IR | fields map to a format-agnostic model |
| 5 | Dependencies resolved | referenced resources resolve against the real index |
| 6 | Preserved / round-tripped | byte- or semantic-faithful where applicable |
| 7 | Converted to runtime assets | usable by the Bevy runtime |
| 8 | Validated vs independent evidence | agrees with a Python oracle / community tool |

Measured sources already exist: crate `claims` modules,
`spore-differential/src/compare/*`, and `docs/replacement-status.json` statuses
(`replaced-verified` / `replaced-stub` / …).

### 3.2 Behavioural coverage (game/editor, separate)

Behaviours are **not** formats. Per behaviour: what specification exists, how
strongly supported, is there an implementation, unit tests, original-program
validation, what remains unsupported. Because static != runtime
(`docs/tooling/validation-dimensions.md`), behaviour rows carry **two** axes —
static reconstruction verdict (`PASS/WARN/FAIL/UNKNOWN/NOT_AVAILABLE`) and runtime
(`GATED` / `PASS`) — never collapsed.

**Do not emit one percentage.** The repository already decided this for the binary
(`docs/tooling/reconstruction-coverage.md`: different universes, different
provenance classes, saturated dimensions -> a matrix plus a `cannot_determine`
entry literally named "a single overall completion percentage"). Format and
behaviour coverage extend that discipline; a headline number would be a
regression.

## 4. Provenance categories

Per `../research/LICENSE-PROVENANCE.md`, every artifact records one of:
`independent-from-spec`, `independent-from-observation`, `informed-by-re`,
`ported-community`, `direct-dependency`, `blocked`. The crate `claims` `basis`
strings already are these citations; keep them.

## 5. Methodology-to-Spore mapping (every row: real evidence)

| PhotoCraft practice | Evidence inspected | Spore equivalent | Existing OpenSpore capability | Gap | Proposed next experiment |
|---|---|---|---|---|---|
| Generated, CI-checked parity/scorecard doc | `docs/scorecard.md`, `docs/parity-checklist.md`, `cargo xtask scorecard --check`, `xtask/src/layers.rs` | a format-family + behaviour coverage doc generated from the code | `coverage.py` (`docs/tooling/reconstruction-coverage.md`, refuses a single %); `docs/replacement-status.json` | coverage is **binary-scoped**, not per format family | `osptool format-coverage`, 8-level model, `--check` in CI |
| Monotone regression floor | `FLOOR = 631` (`crates/ui-egui/src/parity.rs`), `PASS_FLOOR = 5` | explicit per-family / per-differential floors | `spore-differential/tests/{gmdl_reference_table,luminance_corpus,real_corpus}.rs` | floors are implicit in test bodies | record explicit floor constants per differential |
| Independent oracle + comment-documented tolerance | CPU `flatten` vs GPU `parity.rs`, `TOL = 1/255+4eps`; `PASS_TOL = 2/255` | Python oracles vs Rust parsers | `spore-differential/src/{oracle,compare/*}.rs` | tolerances not documented per check | annotate each `compare/*` check with tolerance + reason |
| Never commit real data: pin + SHA-256 + fetch | `xtask/src/corpus_pins.rs`, `*.sha256`, per-corpus licence README | real `SPORE/` kept out of git; synthetic fixtures | `AGENTS.md` gitignore rule; tests skip when `SPORE/` absent | no pinned manifest of expected corpus byte-counts | pin a corpus manifest (path + sha256 + expected record counts) |
| Layering enforced by CI, not docs | `cargo xtask layers` run in `.github/workflows/ci.yml` | the Bevy-free-core invariant | documented in `AGENTS.md` + `spore-core` docs | **not enforced by any tool** | add a `cargo`/CI check: no `bevy` dep below `spore-engine` |
| Renderer-agnostic core, testable headlessly | `crates/doc` (renderer-agnostic) | `spore-core`..`spore-material` Bevy-free | true today; `cargo test --workspace` runs with no window/GPU | relies on review | the layering check above proves it |
| Separate measured vs estimated; self-revise loudly | `docs/target-app-parity.md` (each number dated + kind) | evidence grades + provenance classes | `EvidenceLevel`; `coverage.py` tracks `tracked/derived/untracked/machine_local` | behaviour coverage would mix measured+estimated | label every behaviour row measured vs estimated |
| Ranked, agent-actionable gap list | `docs/gaps.md` (G1..G17, "blocks beta") | the triage queue + actionable frontier | `coverage.py#dependencies.actionable_frontier` (231); `openspore frontier` | format/behaviour gaps not in the queue | seed a `docs/gaps`-style list from `claims` limitations |
| Never-crash as a testable invariant | `forbid(unsafe_code)` + `deny(clippy::unwrap_used, panic, …)`, `panic_hunt.rs`, 29/29 crates | crate lints | `#![forbid(unsafe_code)]` in every crate except `spore-assets` | `unwrap_used`/`panic` not denied | consider `deny(clippy::unwrap_used, expect_used)` in format crates |
| Multi-oracle corpora + honest skips | `crates/io/tests/corpus.rs`, GPU `parity.rs` skip *with a reason* | real-corpus `#[ignore]`d differentials | `spore-differential` real-corpus tests + synthetic envelopes | skips are silent `#[ignore]` not printed reasons | print the skip reason (corpus absent) |

## 6. What does not transfer

Covered in `../research/PHOTOCRAFT-METHODOLOGY-CASE-STUDY.md` §9. Short form: no
published spec; the reference app is a proprietary EXE rather than a re-runnable
oracle; the modding ecosystem is an oracle Spore has and Photoshop lacks;
simulation is far larger than stateless image ops; and no headline percentage is
acceptable.

## 7. Next experiment (single, chosen for leverage)

**`osptool format-coverage`** — a read-only generator in `spore-tools` that emits,
per resource family, which of the 8 levels (§3.1) are achieved, sourced from the
existing crate `claims` modules + `docs/replacement-status.json` +
`spore-differential` test names, under schema `openspore-format-coverage-1`. Ship
with a per-family monotone floor and a CI `--check` (mirroring
`reconstruction-coverage.md` and PhotoCraft's scorecard).

- **Why priority:** it is the cheapest way to turn scattered, already-proven
  knowledge into the *measured* view every subsequent decision needs, and it
  removes the real blocker that no one can currently say "how strong is family
  X". It adds **no** new format parsing.
- **Why not a parser:** §11 forbids implementing a subsystem; the property-list
  and animation gaps (§1 of the spec map) are real but are *implementation*, not
  this task.
- **How success is verified:** (1) the generator emits every family in the spec
  map with a level vector and no fabricated `100%`; (2) a family whose only
  evidence is a `claims` entry cannot be graded above `Supported`; (3) two runs
  over identical inputs are byte-identical (determinism, as `coverage.py` proves);
  (4) `--check` fails if the committed artifact drifts from the generator; (5) it
  runs with no `SPORE/`, no GPU and no network.
- **Cheap validation sub-step:** first serialize the existing `claims` modules to
  the `openspore-spec-claim-1` index (§2.1) and assert round-trip — proving the
  model fits the code before any coverage logic is written.
