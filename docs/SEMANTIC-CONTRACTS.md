# Semantic contracts — engine-level rules, with their evidence

What an OpenSpore engine author needs from `spore-semantic-bridge`, and what
each answer is worth.

The passport (`knowledge/semantic/function-passport-v1.jsonl`, 58 757 records,
`content_sha256` `5920dfa1827f55c6c7d09197d77cc3e5e989f1a441b58b341a4d9ec9410f3a81`)
answers *"what is at this address"*. It cannot answer *"what is true about the
renderer"*: nothing in the file states an ordering, a capability, a budget or a
dependency, and no field in the format could. Those answers exist — but they
exist in **runtime evidence**, produced by executing the binary, and they arrive
with a provenance and a grade attached.

This document states the rules that came out of that evidence, what each one is
worth, which engine component would assert it, and — in as much detail — what
the passport does **not** say.

---

## 1. The flow, and where the boundary is

```
spore-recomp  ──executes──▶  runtime evidence (its own reports)
                                    │
                                    │  imported, never re-implemented
                                    ▼
spore-semantic  ──joins──▶  a runtime overlay + the static snapshot
                                    │
                                    │  distilled by hand, graded, committed
                                    ▼
crates/spore-semantic-bridge/src/invariants.rs   <-- this document's rules
                                    │
                                    │  asserted by
                                    ▼
spore-engine / spore-assets (not wired yet — §8)
```

Three rules govern every byte of that chain:

* **No producer code is imported.** `spore-recomp`'s reports enter as a
  *portable artifact identifier* (`work/reports/startup-recovery84.json`), never as
  a parse and never as a build dependency. OpenSpore compiles, tests and ships
  with no `spore-recomp` checkout present — `crates/spore-semantic-bridge` depends
  on `spore-core` and `thiserror`, nothing else.
* **The join stays where it is.** Joining a static passport to a runtime overlay
  is `spore-semantic runtime`'s job, with `verdict_unchanged` on every output.
  Re-implementing that join in Rust would be a second answer to the same
  question, so this crate does not read overlays at all
  (`claims::RUNTIME_OVERLAYS_NOT_READ`).
* **A runtime observation never upgrades a static verdict.** Every rule carries
  two grades and the generalisation is bounded by the observation it came from
  (§2).

## 2. How to read a contract

Every entry in the catalog has the same envelope:

| field | meaning |
|---|---|
| `subject` | the engine-level concern (`adapter_selection`, `virtual_dispatch`, …) |
| `statement` | the rule, in engine vocabulary. Never a platform API name (§7) |
| `basis` | how well the **observation** underneath is supported |
| `level` | how well **this generalisation** is supported. Never above `basis` |
| `provenance` | this document (`committed_artifact`) plus, where one applies, the producer's report as an `imported` / `runtime` entry |
| `consumers` | the engine component that would assert it. Never empty |
| `anchors` | addresses the observation happened at, or `None` when the rule is about evidence rather than about a place in the program |
| `not_stated` | what the passport does not say about it, one clause per entry |
| `runtime` | what the producer observed, restated in engine terms, with a portable artifact id |

Three consequences worth stating plainly:

* `basis` and `level` are separate because an observation is not a requirement.
  "an adapter handle was opened" is `OBSERVED`; "adapter selection is a distinct
  step" is an `INFERRED` generalisation of it, and both rungs travel together.
* **Nothing here is `CONFIRMED` or `VERIFIED`.** `VERIFIED` means *established
  against the original binary by a repeatable differential*, and no differential
  was run for any rule here. Eleven entries are `OBSERVED` → `INFERRED`; one is
  `UNKNOWN` on both rungs, recorded precisely so that "we did not look" is in the
  catalog instead of being indistinguishable from an oversight.
* `verdict_unchanged()` is `true` for every contract this crate can build. It is a
  method returning a constant rather than a field, so there is no construction
  path that sets it wrongly.

This layer deliberately has **no** `reached`, `entry_count` or "observed" field of
its own. Those are the producer's tri-state vocabulary, they live in the runtime
overlay that `spore-semantic runtime` validates, and inventing a second spelling
here would be how a *not watched* observation turns into a *watched and saw
nothing*. A rule in this catalog therefore says what was seen and at which
addresses, and says nothing about what was not watched.

Alongside the committed catalog, `ContractLedger` **projects** the snapshot into
the same shape for individual addresses: dispatch targets, ABI standing,
name-derived lifecycle roles, runtime gates, and the resolution of an address that
the function universe cannot place. It derives nothing: every value it reports
carries the grade and the provenance of the envelope it was read from.

## 3. The runtime evidence the catalog rests on

Stated in engine terms first, because that is what the rules are about. The
platform vocabulary the observation arrived in is in §3.1 and **nowhere else** —
the contract code refuses it mechanically (§7).

A runtime run of `SporeApp.exe` reached a renderer that no static analysis had
described, and it did so by *executing* the program:

1. **Three library entry points were resolved by name at run time.** The guest
   called `GetProcAddress` for `EnumDisplayDevicesA`, `D3DKMTEscape` and
   `D3DKMTOpenAdapterFromHdc`, and null-checked all three.
2. **Adapter selection is a separate step.** The enumeration ran, a device context
   was built from an enumerated device name, and an adapter handle was opened —
   all before any graphics device existed.
3. **Device creation is separate from the probe.** A real device interface was
   reached; its capability answers were written into the renderer's own state
   block; one query's status value was discarded and the renderer continued.
4. **A probed memory figure sizes renderer state.** A byte count of available
   graphics memory was read and consumed as MiB. In that environment the value was
   a saturated 32-bit wrap, and the producer deliberately did **not** copy it:
   adopting it would encode one machine's arithmetic as the program's behaviour.
   What the measurement established was the *shape*.
5. **Static analysis saw the sibling and missed the trio.** The one entry point
   reached through the import table is a six-byte thunk the passport holds; none
   of the three resolved by name appears anywhere in the file.
6. **An interface-slot ordering error survived until the calls were reached.** Four
   device methods sat one slot apart from the truth, twice in two places, and
   nothing noticed until execution reached them.
7. **A module-handle collision made one library answer under another's name**, and
   every entry point reached through it resolved to nothing — no fault, no crash.
8. **An address reached through a real virtual call could not be placed.**
   `0x00925050` sits between `FUN_00925000` (which ends at `0x0092504a`) and
   `FUN_009250c0`.

The architectural point is not any of those APIs. It is:

> **Static PE imports cannot describe dynamically resolved runtime behaviour, and
> executing the binary discovers latent structural errors that static
> reconstruction cannot distinguish from correct code.**

Items 6 and 7 are defects in the *producer*, and they are kept because they
establish engine-level rules (§4.4, §4.5) rather than because they are
interesting.

### 3.1 The observation's own vocabulary, kept here on purpose

| what the rule calls it | what the run called it |
|---|---|
| adapter selection | enumerating display devices and opening an adapter from a device context |
| capability probe | a device-interface slot query writing into renderer state |
| a probed memory figure | an available-texture-memory query, consumed as MiB |
| a dynamically resolved library entry point | `GetProcAddress("EnumDisplayDevicesA")` and two siblings |
| an import-table sibling | a six-byte thunk for `Direct3DCreate9` at `0x0120bb50` |

A rule that named the right-hand column would be an instruction to reproduce a
graphics API OpenSpore does not target. The catalog's coherence check greps every
statement and every anchor note for platform vocabulary
(`FORBIDDEN_IN_STATEMENTS`) and refuses the entry.

## 4. The catalog

Twelve entries: nine behavioural, one ordering, two probed-input. Every entry
names at least one consumer; `SemanticInvariant::coherence()` enforces that, plus
sourcing, legal grades, a non-empty `not_stated`, a portable runtime reference and
the vocabulary rule above.

### 4.1 Ordering — what must happen before what

**`capability_probe_precedes_resource_creation`** · subject `renderer_startup` ·
`OBSERVED` → `INFERRED` · consumers `renderer_startup`, `decode_policy`

Required chain, asserted by `LifecycleContract::check_order`:

| # | phase |
|---|---|
| 0 | `environment_probe` — find out what the environment offers |
| 1 | `adapter_selection` — choose one and open a handle to it |
| 2 | `device creation` — create the device that will do the drawing |
| 3 | `capability_probe` — ask what it can do and how much it has |
| 4 | `resource_creation` — create resources and the renderer state those answers shape |
| 5 | `teardown` — terminal; nothing follows |

Anchors: `0x011f9250` (queried the device from renderer setup, after a device
existed), `0x007cb590` (consumed a probed available-memory figure and sized state
from it).

*Why it is a dependency and not a recipe*: a resource whose size or format depends
on a device answer cannot be built by a step that runs before the probe.

*Not stated*: the passport records no ordering between the two anchored addresses
and no capability value for either — neither is in the other's call list in either
direction; both carry `category: UNKNOWN`, so a subsystem query finds neither; the
chain has no recorded duration and no failure path, only the order.

### 4.2 Behavioural rules

**`adapter_selection_is_a_distinct_step`** · `adapter_selection` · `OBSERVED` →
`INFERRED` · `renderer_startup`
Choosing a display target is a step with its own failure modes and is not part of
creating a device. A run enumerated the display devices, built a context from one
of the enumerated names and opened an adapter handle — before any device existed.
Collapsing the two reports the wrong failure.
Anchors `0x011e1d50`, `0x011e1ba0`.
*Not stated*: no adapter, display device, monitor or handle is recorded at either
address, and the format has no field in which one could appear; neither address
carries a name, an ABI record, a table membership, a semantic record or a
reconstruction record; the `Runtime` subsystem both carry is a triage row's own
verdict (`INFERRED`, `APPROXIMATION`), not a renderer role.

**`capability_probe_is_advisory`** · `capability_probing` · `OBSERVED` →
`INFERRED` · `renderer_startup`, `decode_policy`
A capability probe is advisory: one query's answer was written into renderer state
and the next query's status was thrown away, so a failed or absent answer is a
degradation rather than a failure. An OpenSpore startup path must be able to
continue without one and must not make device capability a precondition for
standing a scene up.
Anchor `0x011f9250`.
*Not stated*: no capability value, no status value and no error handling; the
address's own record has `category: UNKNOWN` and the subsystem sentinel, so
nothing in the file routes a reader to it.

**`a_missing_dependency_is_a_distinct_outcome`** · `module_identity` · `OBSERVED`
→ `INFERRED` · `asset_resolution`
Every name the observed setup resolved was null-checked by its caller, so an absent
one selects a different path rather than falling back to a default. A resolver that
substitutes a default for an unresolved reference has collapsed two outcomes into
one, and the collapse is invisible in whatever it returns.
Anchor `0x011e1d50`.
*Not stated*: the passport records no optional-dependency resolution at all — it
has a field for a name imported at link time and none for a name resolved at run
time; its graph block distinguishes a canonical callee from an external callee
token, and an unresolved dependency is neither.

**`dynamic_resolution_is_invisible_to_import_analysis`** · `module_identity` ·
`OBSERVED` → `INFERRED` · `asset_resolution`, `evidence_pipeline`
A dependency set derived from a static import table is incomplete by construction.
Measured over all 58 757 records: the three library entry points the renderer
reached by name appear **nowhere** in the file — not as a name, not as an external
callee — while the one entry point the same code reaches through the import table,
the six-byte thunk at `0x0120bb50`, is present with its caller recorded. Static
analysis sees the imported sibling and is blind to the resolved trio, so no
completeness claim about a program's dependencies may rest on import analysis
alone.
Anchors `0x011e1d50`, `0x0120bb50`.
*Not stated*: the external-callee vocabulary stops at the import table, so a
run-time-resolved name cannot appear in it by construction; the single thunk record
is graded `IGNORE`, so a triage pass honouring the priority would skip the one
place the file *does* see the device surface; the 25 088 table memberships in the
file are about the program's own classes and none is a membership in a platform
interface.

**`module_identity_can_be_ambiguous`** · `module_identity` · `OBSERVED` →
`INFERRED` · `asset_resolution`
Identity resolved at run time can be ambiguous, and a wrong identity is worse than
an unresolved one: a handle-allocation collision made one library answer under
another's name and every entry point through it resolve to nothing, with no fault
and no counter moving. Any OpenSpore lookup that maps a runtime token onto a
module, package or record must be able to answer *unresolved*, and must never
continue on a guessed identity.
*Not stated*: this is a defect in the producer, not a property of the analysed
binary, and the passport describes the binary only — so it can neither corroborate
nor refute the rule; what the passport does hold is no record anywhere of an
identity that resolved wrongly.

**`dispatch_targets_are_table_and_slot_addressed`** · `virtual_dispatch` ·
`OBSERVED` → `INFERRED` · `dispatch_table`
A virtual call target is addressed by a `(table, slot)` pair: the slot index is the
fact and the byte displacement is only the access. 6 712 of the snapshot's records
carry at least one membership — 25 088 memberships across 1 851 distinct tables, the
widest at slot 281 — and the sound scan that produces them is the only basis the
ABI engine accepts. Key on the slot index; reach the target by displacement from
the table base. Subscripting a table structure advances by a whole interface pair,
lands on a different slot, and on this repository's own history (`0x0068f9b0`)
produced a *callable* pointer and therefore a plausible wrong answer rather than a
crash.
*Not stated*: nothing about what any slot *means* — a membership names a table and
an index and no method, so an engine cannot learn an interface's shape from it; the
triage block records bare table addresses with no slot on some records, which is a
weaker, differently-sourced association and must not be read as a membership.

**`an_unplaced_runtime_address_is_data`** · `static_identity` · `OBSERVED` →
`INFERRED` · `dispatch_table`, `evidence_pipeline`
An address reached at run time that the function universe cannot place is data, not
a defect to repair. `0x00925050` was reached through a real virtual call and
resolves to no entry, so it is reported as a static non-function entity with no
canonical address. Snapping it to the nearest function, or modelling it as an empty
one, would destroy the only evidence the repository has that its universe is
incomplete.
*Not stated*: the passport holds no record for this address at all — not an empty
one, none — so nothing about it can be graded; the neighbours are `0x00925000`
(ending `0x0092504a`) and `0x009250c0`, which is why the canonical-identity rule
answers *non-function entity* rather than picking one.

**`runtime_gated_is_a_fourth_state`** · `evidence_debt` · `OBSERVED` → `INFERRED`
· `evidence_pipeline`
"Owes runtime evidence" is a fourth state, not a pending pass and not a failure. Of
the 727 records carrying a reconstruction record, 423 state an open runtime gate,
191 a closed one and 113 state nothing at all. Rendering those three as one boolean
loses the difference between *the gate is open, nothing was attempted, nothing
failed* and *this record says nothing* — the difference between work to do and no
answer at all.
*Not stated*: which runtime observation would settle a gated function, and who is
expected to produce it. The rule cannot be turned into a work item from the file
alone.

**`display_state_meaning_unknown`** · `adapter_selection` · **`UNKNOWN` on both
rungs** · `renderer_startup`
What an enumerated display device's state fields mean is **not established**, and
neither is whether a chosen adapter is ever re-selected once a device exists. The
run reached the enumeration and the adapter open and then continued past device
creation, so no observation bears on either question.
This entry exists to be `UNKNOWN`. An absent entry would say nothing; this one says
"we did not look", which is the useful statement.
*Not stated*: the passport has no field for a display device, a state flag or a
selection history, so this is unrecoverable from it at any grade.

### 4.3 Probed-input rules

**`device_memory_budget_is_a_policy_input`** · `resource_creation` ·
`device_memory_budget` → `gpu_allocation_budget` · `OBSERVED` → `INFERRED` ·
`decode_policy`, `renderer_startup`
An available-memory figure is a probed input to an allocation budget, never a
constant inside a decoder. The observed renderer read a byte count and consumed it
as MiB while sizing state; the value it got in that environment was a saturated
32-bit wrap, which the producer refused to copy for exactly this reason. What the
measurement established is the shape — a byte count, consumed as MiB, after the
device exists. OpenSpore must take its budget the same way: policy input, with the
probed value attached as evidence rather than as a default.
Anchor `0x007cb590`.
*Not stated*: no memory figure, no budget and no sizing decision for that address;
its body is 1 890 bytes with one external callee and no available fact group, so a
decode policy cannot be recovered from the file either.

**`device_capabilities_feed_renderer_state`** · `resource_creation` ·
`device_capabilities` → `resource_format_choice` · `OBSERVED` → `INFERRED` ·
`decode_policy`, `renderer_startup`
Device capability answers are written into renderer state and read back from there,
so a resource decision taken from them happens after that state exists rather than
at load time: a capability answer was written into the renderer's own state block
and a second query's parameters beside it. A format or budget choice derived from
capabilities is a renderer-startup decision with the probed answer attached — not a
property of the asset being loaded.
Anchor `0x011f9250`.
*Not stated*: no capability field, no renderer state block and no format decision
for that address; its only recorded caller is a single function, so even the call
context of the write is one address wide.

### 4.4 What the two producer defects establish

Neither defect is a fact about the analysed binary, so neither is a claim about it.
Both are rules about how *evidence* is produced, and both belong to a consumer
that resolves identities:

* the slot off-by-one (written twice, in a model and in a skip list) says a
  structural ordering error can survive every static check and every earlier
  execution, because it is only observable when the affected code is reached →
  `capability_probe_precedes_resource_creation` and the `INFERRED` rung on it;
* the handle collision says an identity that resolves *wrongly* is worse than one
  that does not resolve, because nothing downstream can tell the difference →
  `module_identity_can_be_ambiguous`, and `a_missing_dependency_is_a_distinct_outcome`
  from the same run.

## 5. What the passport does not say — the whole list

Computed, not asserted: `ContractLedger::audit` recomputes the stance of every
anchored address against the file, and `tests/contracts_real_snapshot.rs` asserts
the result.

1. **It does not know the renderer exists.** Of the four addresses the run stood on
   — `0x011e1d50` (579 bytes), `0x011e1ba0` (250), `0x011f9250` (592), `0x007cb590`
   (1 890) — every one is *placed* in the function universe and every one is
   **explicitly silent**: a record exists and all five groups a rule could read
   (name, ABI, table membership, semantic record, reconstruction record) are
   explicit absences. Consequently `ContractLedger::for_va` on any of them returns
   **an empty contract set**, which is the honest answer and not a gap to be filled.
2. **It does not know any run-time-resolved name exists.** `D3DKMT`,
   `EnumDisplayDevices`, `gdi32`, `shell32` and `user32` occur **zero** times in
   98 080 685 bytes — not as an SDK name, not as an external callee, not in prose.
   (The six `d3d9` matches in the file are all hexadecimal coincidences — three
   inside SHA-256 digests and three inside plain hex addresses such as
   `0x00d3d930`. Not one is the word.)
3. **Its only graphics-device surface is a six-byte thunk** at `0x0120bb50`,
   graded `IGNORE`, whose single recorded caller `0x011f84b0` is itself
   `category: THIRD_PARTY_OR_RUNTIME`, `evidence: APPROX`, `subsystem: Runtime`.
4. **It states no ordering, no phase, no capability, no budget and no dependency.**
   Not one field in the format could.
5. **It says nothing about slot semantics.** 25 088 memberships name a table and an
   index; none names a method.
6. **It holds no record for `0x00925050`** — not an empty one, none — so the one
   address the runtime proved real and the universe cannot place is ungradeable.
   `canonical_va` stays `null` and no passport is manufactured.
7. **It does not say what would settle a runtime-gated function**, or who should.
8. **It does not record the producer's own defects** (items 6–7 of §3), because it
   describes `SporeApp.exe` and not the tool that ran it.
9. **Two of its own fields are known-wrong and are carried, not repaired**: the
   literal `00000000` directory on all 92 promotion-record paths
   (`claims::PROMOTION_RECORD_ZERO_PATH`) and the `APPROX` spelling on 1 500
   classification rows (`claims::CLASSIFICATION_EVIDENCE_VOCABULARY`).
10. **`fan_in` / `fan_out` are absent from all 58 757 records**, and the triage
    counters the upstream docs mention may legitimately differ from the xref
    counts beside them (`claims::GRAPH_FAN_COUNTERS_ABSENT`).

## 6. What was refused, and why

| refused | why |
|---|---|
| naming a platform API or interface anywhere in a contract | OpenSpore is Rust + Bevy + wgpu. Reproducing a graphics API is the wrong goal, and a rule that named one would be an instruction to do it. Enforced by `FORBIDDEN_IN_STATEMENTS` + `SemanticInvariant::coherence`. |
| reading a runtime overlay in this crate | the join is `spore-semantic runtime`'s, with `verdict_unchanged` on every output; a second join would be a second answer |
| re-deriving ABI inference, table detection or validation | those live upstream, and the passport's pinned `content_sha256` is what makes consumers able to pin anything at all |
| repairing `0x00925050` into a function | that discrepancy *is* the deliverable |
| copying the measured memory value | a saturated wrap in one environment is not the program's behaviour; the shape is evidence, the number is not |
| prefix-matching lifecycle tokens | `Create\w*` would add 38 records including `CreateSpaceCommEvent` and `CreateCivCommEvent`, which create nothing; the closed exact set matches 55 |
| an absolute path or a program image in a provenance entry | a committed contract must not pin a machine or a binary |
| grading a lifecycle role above `INFERRED` | 1 167 of 1 171 SDK names are `CONFIRMED`; a confirmed *name* is not a confirmed *role* |
| any `CONFIRMED` or `VERIFIED` contract | no rule here came from a repeatable differential against the original binary |

## 7. The machine-checked rules

`SemanticInvariant::coherence()` refuses an entry that has no id, no statement, an
illegal grade (`level > basis`), no consumer, no provenance, no committed
reference, a non-portable runtime reference, a runtime observation missing from its
own provenance, `Some(&[])` anchors, an empty `not_stated`, or platform vocabulary
in a statement or an anchor note. It runs over the whole catalog from
`tests/contracts.rs` and `src/invariants.rs`, so an entry that breaks any of them
fails the suite rather than shipping.

`SemanticContract::verdict_unchanged()` is `true` for every contract, asserted over
the catalog and over every contract a ledger produces.

## 8. What an engine author would write — and what is not wired

**Nothing in this chain is wired yet.** `crates/spore-semantic-bridge` is
Bevy-free and depends on nothing but `spore-core`, so it can name where an assertion
belongs without installing it. `EngineComponent` is an identifier for exactly that
reason, and `contract_consumers_are_not_yet_wired` is a standing claim that this is
not oversold. What each consumer would do:

| consumer | the assertion | where it belongs |
|---|---|---|
| `renderer_startup` | `LifecycleContract::check_order(&[phases])` on the phases the startup actually performed; `Err(PhaseOrderViolation)` names the pair and the index. `Err` is a startup error, not a warning: creating resources before the probe is the defect the rule exists for. | `spore_engine::prepare` / `SporeAssetPlugin`, `Startup` |
| `asset_resolution` | keep "unresolved" as its own outcome end to end: a reference that resolves to nothing is reported, never defaulted (`spore_assets::ResourceGraph::unresolved` already exists). Manifest completeness may not be claimed from import- or index-derived evidence alone. | `spore_assets::{ContentStore, ResourceGraph}` |
| `decode_policy` | the GPU allocation budget is a **resource** with a policy default and, when available, a probed value attached as evidence; a decoder may not hold a memory constant. | `spore-texture` / `spore-gmdl` policy surface |
| `evidence_pipeline` | four gate states, not a boolean; an unplaced runtime address is emitted as `static_non_function_entity` with `canonical_va: null` and never repaired. | `spore-semantic export`, `reconstruction/evidence` |
| `dispatch_table` | key on `(table_va, slot)`; reach a slot by `slot * 4` from the table base, never by subscripting a table structure; an address with no entry is an explicit unplaced target, not the nearest function. | the generated handler/trampoline registry |

## 9. Verification

```sh
cargo fmt --all
cargo test  -p spore-semantic-bridge                                        # hermetic
cargo test  -p spore-semantic-bridge --test contracts_real_snapshot -- --ignored
cargo clippy -p spore-semantic-bridge --all-targets -- -D warnings
```

Measured on the committed artifact (`--release`): open 0.85 s, projecting all
58 757 records 4.5 s. The `#[ignore]`d cases are pinned to these counts, all
measured over the whole file rather than sampled:

| quantity | value |
|---|---|
| records | 58 757 |
| records with an SDK name | 1 171 (1 167 `CONFIRMED`) |
| records whose name states a lifecycle role | 55 |
| records with ≥1 table membership | 6 712 (25 088 memberships, 1 851 tables, max slot 281) |
| records with a determined convention | 407 (377 `__thiscall`, 18 `__stdcall`, 11 `__cdecl`, 1 `__fastcall`) |
| records with an ABI record that declines | 233 |
| records with no ABI group | 58 117 |
| open / closed / unstated runtime gates | 423 / 191 / 113 |
| catalog entries | 12 (9 behavioural, 1 ordering, 2 probed-input) |
| grades in the catalog | 11 × `OBSERVED`→`INFERRED`, 1 × `UNKNOWN`→`UNKNOWN` |
| entries backed by a runtime observation | 10 of 12 (the other two are counted from the artifact itself) |

A re-export that changes any of these fails the ignored suite rather than silently
rewriting this document's claims.