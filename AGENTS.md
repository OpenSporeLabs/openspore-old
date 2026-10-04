# AGENTS.md — OpenSpore

Rules and durable context for any agent (opencode) working in this repo.

## What this is

A clean-room reimplementation of *Spore*, driven by a fully AI pipeline, in
**two coexisting halves**:

| Half | What it is | Status |
|---|---|---|
| `crates/` (Cargo workspace) | **Rust + Bevy engine.** The direction. Renders real Spore assets. | active, see below |
| `src/`, `tools/`, `knowledge*/`, `reconstruction/`, `docs/analysis/` | C++17/CMake prototype + Python oracles + the reverse-engineering corpus | still builds, still the reference |

Neither half replaces the other's knowledge. The C++ tree is the **differential
reference** and the Python tools under `tools/spore/` are the **independent
oracles** the Rust parsers are checked against — `docs/MIGRATION.md` classifies
every subsystem as KEEP / PORT / REIMPLEMENT / RESEARCH-ONLY / OBSOLETE.
**Do not delete the C++ tree to "finish" the migration**; it is load-bearing.

Start here: `README.md`, then `docs/RUST-ENGINE.md` (how the Rust workspace
works) and `docs/MIGRATION.md` (what became what).

## Hard rules (legal / ethics)
- No *Spore* assets, decompiled EA source, models, textures, sounds, scripts.
- Reimplement by analysis only (clean room). No DRM circumvention (use GOG).
- No trade secrets / leaked code. `SPORE/` is git-ignored — keep it that way.
- **OpenSpore must not depend on `spore-recomp`.** Consume semantic facts
  through the documented boundary (`tools/spore-semantic`,
  `knowledge/semantic/`), never its implementation. See
  "Read-only exchange" below — those rules are normative and still apply to
  any Rust code that touches the exchange.
- No *Spore* asset byte may be **committed**, in any form, including as a
  test fixture. Rust tests use synthetic fixtures or skip when `SPORE/` is
  absent. `/target/` is git-ignored; `SPORE/` and `*.package` must stay so.

## The Rust workspace

```
crates/spore-core        resource identity, record ids, evidence vocabulary
crates/spore-dbpf        DBPF v3 index, QFS/RefPack
crates/spore-rw4         RW4 container section directory
crates/spore-gmdl        gmdl decode, mesh extraction, bounds
crates/spore-texture     raster envelope, DXT5
crates/spore-assets      package priority, identity resolution, manifest
crates/spore-material    material + texture binding resolution
crates/spore-differential  Rust parsers vs the Python oracles, over real data
crates/spore-cellcontent    the 12 Cell Stage gameplay record layouts
crates/spore-semantic-bridge  read-only consumer of the spore-semantic passport
crates/spore-creator-core  the authoring core: <sporemodel> creator documents
crates/spore-rigblock  the part layer: rigblock bounds, RW4 skeleton body graph, bone names
crates/spore-morphology  composition: block -> part resolution with provenance + unresolved counts
crates/spore-engine      the Bevy runtime
crates/spore-tools       `osptool`, the inspection CLI
```

**The load-bearing invariant: everything above `loader` is renderer-agnostic
and Bevy-free.** `spore-core` … `spore-material`, `spore-creator-core`,
`spore-rigblock`, `spore-morphology` and
`spore-differential` must never gain a `bevy` dependency, and `spore-assets`
must never touch a GPU. That is what makes the asset layer testable with no
window and no game install, and it is why `--info` decodes a real record in
~140 ms. Keep it.

### Build / test (Rust)

```bash
cargo build --workspace
cargo test  --workspace                       # hermetic: no game, no GPU
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

Run the vertical slice (real asset, real window):

```bash
cargo run -p spore-engine -- --preset documented-asset \
    --package SPORE/Data/Spore_Content.package            # opens a window
cargo run -p spore-engine -- … --info                     # headless, exits
cargo run -p spore-tools  -- manifest SPORE/Data/Spore_Content.package --stats
```

Real-corpus differential (needs `SPORE/`, slow, `#[ignore]`d by default):
```bash
cargo test -p spore-differential --release -- --ignored --nocapture
```

### Rust traps (each one cost real debugging time)

- **`u32::from_str` rejects a `0x` prefix** while every other tool in this repo
  accepts one. Use `spore_core::parse_id`. Do not write another hex parser; the
  class of bug has already appeared twice.
- **`DefaultPlugins` builds a winit `EventLoop`, which panics off the main
  thread.** "The app assembles" is therefore NOT a headless-testable claim.
  The honest headless test is that *decoding* is Bevy-free. Use
  `MinimalPlugins` for anything headless.
- **`Commands::spawn` is deferred.** A `Query` in the same system that spawns an
  entity sees nothing. Camera framing runs in `PostStartup`, not `Startup`; this
  exact mistake looked like a framing bug.
- **One owner per entity kind.** `ScenePlugin` owns the world, `SporeAssetPlugin`
  owns the content. When both spawned a camera, Bevy warned "Camera order
  ambiguities" and the symptom was wrong framing — a duplicate spawn wearing a
  framing bug's clothes.
- **Bevy 0.19 renames**: `StandardMaterial::roughness` → `perceptual_roughness`;
  `DirectionalLight::shadows_enabled` → `shadow_maps_enabled`; `App::exit()` →
  an `AppExit` message; `WindowResolution::new` takes `u32`. The `zstd` feature
  needs an explicit backend — this workspace uses `zstd_rust` to avoid a C
  toolchain dependency.
- **wgpu has no triangle-fan topology.** GMDL `primType` 6 does, so a fan is
  expanded to a triangle list in `convert::to_buffers`, not at the call site.
- **`#![forbid(unsafe_code)]`** in every crate except `spore-assets`, which is
  `#![deny(unsafe_code)]` with exactly one documented `#[allow]` (memmap2 has
  no safe constructor). Do not add a second.
- **GMDL `refCount` is BIG-ENDIAN.** Every other word is little-endian. A
  little-endian read gives `N * 0x01000000` and walks off the end of the record.
- **Spore's creator documents are XML, and they spell numbers both ways.**
  68 `<sporemodel>` records live in `Spore_Game.package` (`crt` 47, `cll` 13,
  `vcl` 4, `ufo` 2, `bld` 1, `flr` 1). The same `u32` field appears as
  `0x9ea3031a` in 67 of them and `2661483290` in the one `formatversion 11`
  document, and floats appear as both `0.450979` and `7.26504e-9`. Parse both
  radices and both float spellings, and never gate a field on `formatversion` —
  `v16` documents both write and omit `<handles>`, and so do `v15` documents.
  See `docs/CREATOR-CORE.md`.

- **Bone identity in a part skeleton is FNV-1, not FNV-1a.** The `Skeleton
  (0x70002)` section's first array holds 2002 distinct 32-bit words that are
  **FNV-1** (multiply, then xor) hashes of a morph-parameter name — `joint1..9`,
  `base1..6`, `shaft1..5`, `hip`, `knee`, `iris`. It is *not* FNV-1a, which is
  what this repo uses for DBPF instance ids; swapping the two orderings gives
  **zero** matches and looks correct while doing it. 244 of 2002 are recovered,
  graded per name by what witnesses them, and an exhaustive 62M-string search is
  expected to produce **29 accidental collisions** — so a single unrepeated hit
  proves nothing. Never re-derive the hash; use `spore_rigblock::vocabulary`.

- **A prop record's FIRST container pointer is the part, not its own-instance
  entry.** A block's part lives at the instance named by the *first* validated
  `{instance, type, 0x2F4E681B, group, 0x00A00CE4}` entry in the `prop`, and that
  is frequently **not** the block's own instance (rigblock `0xc66a14b8` carries
  exactly one pointer, to `0x10fe9dc4`). Reading report 08 §5 as "the pointer names
  the rigblock's own instance" silently costs a body graph on 484 of 678 real
  parts. Measured by `spore-morphology`'s slice test; the resolver grades the hop
  `Inferred` when the two disagree (485 of 678 parts).

- **A `gmsh` record is big-endian everywhere except its 24-byte header.** Block
  `id`, `count` and `elemSize` are BE; `nv` is a `u16` BE inside header word
  `0x14`. Read LE, the ids come out as consecutive values increasing by one per
  block (they look like offsets) and the index block — id `0x00000001` — never
  matches at all. The chain length is `(recipe >> 8) & 0xFF`, verified for all 16
  recipe versions; without it a decoder either eats the bone tail or stops early.
  And the bone-range table is stored **unsorted in 599 of 1824 records**, so a
  tiling check written in file order fails on a third of the corpus.

- **A recovered name vocabulary is not a vocabulary in use.** The 2002 bone
  identity words in an EditorRigblock part skeleton are FNV-1 hashes of
  morph-parameter names, and 244 are recoverable — the set includes `hip`, `knee`,
  `eye`, `iris`, `jaw`, `shoulder` and four complete L/R pairs. But across the
  677 bones in the 680 `<blockref>` parts the creator core actually composes,
  **every recovered name is `joint1`** and nothing else: no anatomy, no laterality,
  no `attach`/`csnap`. A dictionary attack against a hash yields a *plausible,
  coherent* vocabulary, and coherent is not the same as used. Always check a
  recovered vocabulary against the population that matters before building on it.
  `spore_morphology`'s slice test asserts this in the negative on purpose.

### Evidence discipline (unchanged by the migration, now enforced in code)

- `spore_core::EvidenceLevel` is the canonical 7-rung scale. Attach a grade to
  every non-obvious claim; `Fact::unavailable` records a non-finding.
- **Unavailable is never a zero, an empty collection, or `false`.** Collapsing
  them turns "we did not look" into "we looked and found nothing".
- Each format crate has a `claims` module listing what it does *not* know
  (gmdl UBYTE4 normals: INFERRED; raster envelope fields 0x10/0x18/0x1c: UNKNOWN;
  material-id meaning: no registry; sampler roles: undecoded). Extend it when
  you learn something; do not delete a claim because it is inconvenient.
- **The corpus outranks the port.** When a new Rust file and an existing
  research document disagree, run a byte count over the real data — and until
  that count exists, the port is presumed wrong. This already happened once:
  three new doc comments claimed `png` records were RW4 containers, while
  `docs/CELLSTAGE-RECON.md` and `docs/MATERIALS-DESIGN.md` were right.

## Knowledge-graph architecture (do NOT reinvent)
- **Analysis graph** = **Ghidra** (via `ghidra` MCP). Source of truth for
  functions/classes/structures/xrefs/decompilation. Spore-ModAPI SDK symbols live here.
- **Source graph** = **codegraph** (MCP). Indexes this C++ repo for navigation.
- **Shared memory** = **SQLite** at `knowledgegraph/spore.db` (see `kg.py`).
  Cross-tool results only: test outcomes, decisions, asset-format mappings.
- No Neo4j / no general graph DB.

## Environment (CachyOS / Arch — use `pacman`, NOT `apt`)
- Ghidra 12.1.2 at `/opt/ghidra`. **User profile is `ghidra_12.1.2_DEV`**
  (not `_PUBLIC`). GhidraMCP extension already deployed there.
- ghidra-mcp cloned at `~/apps/ghidra-mcp` (uv venv synced). Bridge → `127.0.0.1:8089`.
  The MCP server runs **headless** (no GUI):
  `GHIDRA_MCP_ALLOW_SCRIPTS=1 java -Xmx8g -Dghidra.home=/opt/ghidra \
     -classpath ~/.config/ghidra/ghidra_12.1.2_DEV/Extensions/GhidraMCP/lib/GhidraMCP-7.0.0.jar:$(find /opt/ghidra/Ghidra -name '*.jar' | tr '\n' ':') \
     com.xebyte.headless.GhidraMCPHeadlessServer --port 8089 --bind 127.0.0.1 --project /home/juanr/ghidra-spore-project`
  then verify with `curl 127.0.0.1:8089/check_connection`.
- Spore-ModAPI SDK at `~/apps/Spore-ModAPI`. GOG/digital build →
  `SDKtoGhidra/SporeGhidra_march2017.xml` (+ `GhidraScript/ImportSporeSDK.java`).
- Game (GOG, no DRM) at `./SPORE/`; main exe `SPORE/SporeBin/SporeApp.exe` (+ `SporebinEP1/`).
- codegraph installed via npm (global). `uv` at `~/.local/bin/uv`, `mvn` present.
- Ghidra headless project: `~/ghidra-spore-project/SporeProject` (imported SporeApp.exe).
  Rebuilt 2026-09-20 (original was corrupt; backup in `/tmp/opencode/ghidra/SporeProject.broken-backup/`).
  Full binary map: `docs/RECON-3.1.0.22.md`.

## Working with the game / analysis
- Headless import (defensive — use the committed script, NOT the upstream one):
  `/opt/ghidra/support/analyzeHeadless ~/ghidra-spore-project SporeProject \
     -import SPORE/SporeBin/SporeApp.exe \
     -scriptPath tools/ghidra \
     -postScript ImportSporeSDK.java ~/apps/Spore-ModAPI/SDKtoGhidra/SporeGhidra_march2017.xml`
  `tools/ghidra/ImportSporeSDK.java` is the defensive fork (try/catch per address; the
  upstream `~/apps/Spore-ModAPI` one aborts the symbol pass on the first non-function
  address). Result on 3.1.0.22: 1670/1671 functions named, 1895 structures.
- x86:LE:32 cspec is `windows` (Ghidra 12 has no `msvc:LE:32:msvc`).
- **SporeApp.exe has no MSVC RTTI** — class hierarchy only via vtable data + SDK
  structures. Vtable detection never ran headless (0 vtable labels); run a vtable
  pass before relying on class structure.
- `run_script_inline` compiles Java against the Ghidra 12 OSGi classpath — API renames
  that break common assumptions: `Symbol.getName()`/`getAddress()` (no
  `getPrimaryName`/`getPrimaryAddress`); `getSymbols(String)` returns `SymbolIterator`
  (no `SymbolType` overload); no for-each on `Function` (use
  `program.getInstructionIterator(setView)`); `AddressSetView.getMaxAddress()` (no
  `getEnd()`); no `AddressSpace.translateFromInteger`. Failed scripts accumulate as
  `McpInline_*.java` in `~/ghidra_scripts/` — delete them to stop build warnings.
- Query sidecar: `python3 knowledgegraph/kg.py <init|add-node|add-edge|query|neighbors|record-test|dump>`.

## Reference exports (two files, one run)
- `tools/ghidra/ExportXrefs.java` takes an **optional 8th param `outData`**. Without
  it the edge file is byte-for-byte what it always was; with it the same run also
  writes a data-reference sidecar. It is a *separate* file because
  `tools/triage/export_xrefs.py` proves on the edge file that every non-EXT/non-VT
  endpoint is in the pinned universe, and a data address is not.
- Edge: `knowledgegraph/triage/xrefs-2540f2ca.tsv` (223,704 rows).
  Canonicalize: `tools/triage/export_xrefs.py`.
- Sidecar: `knowledgegraph/triage/datarefs-2540f2ca.tsv` (157,633 rows).
  Canonicalize: `tools/triage/export_datarefs.py` (full-row dedupe key, NOT the
  edge file's triple — a read and a write of one address are two facts).
  Read by `tools/reconstruction_tooling/evidence_datarefs.py` for GLOBALS.
- Regenerate both: write `/tmp/opencode/xref-job/params.tsv` (the script's argless
  fallback) and run `ExportXrefs.java` through `ghidra_run_ghidra_script`. The
  `rangesTsv` column must be `<va8>\t<slot-count>`; `vtables.json`'s `slots` is a
  LIST, so `len()` it when converting.
- **The Ghidra DB drifts from the committed exports.** It now holds 83,820 live
  functions vs the 59,245 recorded at export time, so a re-export differs from
  `xrefs-2540f2ca.tsv` on its own (measured: 16 rows, from a body gap at
  `FUN_006ab120`). Always A/B an exporter change against a *same-DB* baseline,
  never against the committed file.

## Read-only exchange: `spore-semantic`
- `tools/spore-semantic/` is a standalone **Go 1.27.1** CLI, stdlib only (no
  `require` block), no network, no Python and no Ghidra at runtime. It is a
  *projection*, never a second source of truth: it copies facts from the
  artifacts listed in `docs/tooling/semantic-exchange.md` §1.1 with their
  provenance and derives **nothing**. Do not re-implement ABI inference,
  vftable detection or validation in it — if a fact is missing, it is missing.
- Export: `spore-semantic export --out knowledge/semantic/function-passport-v1.jsonl`
  (58,757 records, ~98 MB, ~1.0 s). Query: `lookup <VA>`, `lookup-symbol <NAME>`,
  `explain <VA>`, `stats`, `validate`; `--json` on each. Exit codes are stable
  (2 unknown function, 3 unknown symbol, 4 binary mismatch, 5 corrupt, 6 schema,
  7 unavailable, 8 ambiguous symbol). `OPENSPORE_REQUIRE_SHA` pins the binary;
  `OPENSPORE_ROOT` sets the checkout.
- JSON Lines: metadata on line 1, then one passport per line sorted ascending by
  `identity.canonical_va`. `metadata.content_sha256` covers the **record lines
  only**, so it is not self-referential. No timestamp, no PID, no absolute path.
  Two exports from the same state are byte-for-byte identical (tested).
- **Bare address digits are HEX everywhere in this repo**, and the CLI keeps
  that: decimal requires an explicit `dec:`/`0d` prefix and an RVA requires
  `rva:`. Reading `925050` as decimal would answer a different question
  silently. This is the AGENTS.md address-spelling trap in a new place.
- Optional fact groups are envelopes: `state: available|unavailable`,
  `evidence_level`, `evidence_state`, `provenance`, `value` (omitted when
  unavailable), and a **closed-vocabulary** `reason` code. An unavailable group
  is "we looked and found nothing", never a zero value and never a `false`.
  Inside `abi.receiver`, `present` is the *string* `"true"`/`"false"`/`"null"`
  and `"null"` is load-bearing (it makes `abi_infer` abstain).
- Interior VAs use the repo's existing rule
  (`reconstruction_knowledge.canonical_identity`: bisect on entries, then require
  `start <= a < start + size`), so padding resolves to nothing. Do not invent a
  second mapping algorithm.
- Consumers keep their own overlay keyed by `binary_sha256 + canonical_va` for
  runtime observations; OpenSpore source is never imported as production code.
  `spore-semantic runtime` now reads such an overlay (§"Runtime evidence bridge"
  below), but it never WRITES into one and never writes a runtime fact back into
  a static artifact.
- Installed at `~/.local/bin/spore-semantic` (on PATH). Rebuild with
  `cd tools/spore-semantic && CGO_ENABLED=0 go build -trimpath -ldflags '-s -w' -o ~/.local/bin/spore-semantic ./cmd/spore-semantic`
  — `-trimpath`/`-ldflags` are cosmetic and do not change the exported bytes.
- Verify with `cd tools/spore-semantic && gofmt -l . && go vet ./... && go test ./...`.
  Docs: `docs/tooling/semantic-exchange.md`, `docs/tooling/semantic-exchange-consumer.md`.

## Runtime evidence bridge: `spore-semantic runtime`
- `../spore-recomp` executes the binary independently and emits runtime
  observations. It is an **optional external artifact consumer**, not a
  dependency: it needs no OpenSpore checkout, no Python and no Ghidra, and the
  reverse dependency does not exist either. `spore-recomp` may stop emitting
  reports and nothing here breaks.
- Schema `spore-semantic-runtime-overlay-1` (`internal/runtime/schema.go`), JSON
  Lines mirroring the static snapshot: metadata on line 1, entry lines after,
  `content_sha256` over the entry lines only. 13 entry fields, 9 closed
  observation kinds. Docs: `docs/tooling/semantic-exchange.md` section 6.3.
- Commands: `runtime validate | lookup | frontier | stats | import-recomp |
  census`. **There is deliberately no `runtime merge`**: a command named "merge"
  invites a caller to produce a combined artifact and trust it as one source of
  truth. The join is a VIEW over two files that stay separate on disk.
- **Static and runtime facts have different provenance vocabularies.** The
  snapshot has `generated_index | committed_artifact | ghidra | derived`; an
  overlay may claim exactly one source class, `runtime`. A runtime artifact
  claiming `ghidra` provenance is REFUSED — that would be a static artifact
  wearing a runtime file's schema.
- **A runtime observation never upgrades a static verdict.** Every joined output
  carries `verdict_unchanged`. `runtime frontier`'s `runtime_evidence_relevant`
  means "the overlay holds observations in categories mapped to a recorded
  blocker", not "the blocker is settled". The dimension→observation-category
  table (`blockerRelevance`) is CLOSED, and an unlisted dimension yields NO
  relevance rather than a default — `ABI_RECORD` is deliberately unmapped.
- **A runtime-only address is first-class data, not a defect to repair.**
  `0x00925050` was reached by a real vtable indirect call in spore-recomp and
  OpenSpore cannot place it (it lies between `FUN_00925000`, ending `0x0092504a`,
  and `FUN_009250c0`). The bridge reports `canonical_va: null`,
  `static_status: static_non_function_entity`, `reached: "true"` and does NOT
  manufacture a passport, map padding to a function, or insert it into the
  function universe. That discrepancy is the deliverable.
- **Missing runtime data is NOT `observed: false`.** `reached: "null"` = the
  producer did not watch; `"false"` = watched and saw no entry; a missing entry =
  not observed at all. `entry_count: null` (not counted) is NOT `0` (observed
  zero). Collapsing these is how a runtime observation becomes a fabricated
  negative.
- **The adapter derives exactly one thing.** `runtime import-recomp` projects a
  spore-recomp report's address set and copies the producer's own vocabulary
  (event classes, import spellings, register dump). It leaves `canonical_va`
  **null on every entry** — identity is OpenSpore's to decide. It sets `reached`
  only where the report establishes it (a call-edge callsite was certainly passed
  through, which is not the same as entered); it emits **no**
  `stack_delta_bytes`, because the producer's register map is one CPU sample and
  a delta from one sample would read as "the callee restored the stack"; and an
  IAT cell check records what the cell *holds* with `reached_count: null`, because
  the report does not establish that the import was called.
- **The census exists because the Passport deliberately omits prose.**
  `index.json`'s `blockers` and `unresolved_questions` are the sentences that
  describe the remaining ceilings, and projecting them into the Passport would
  change the snapshot's `content_sha256`, which consumers pin. So
  `runtime census` writes a separate sidecar
  (`openspore-runtime-blocker-census-1`) and reads promotion from the **marker
  files**, per the marker-over-manifest trap above. Counts are computed over the
  FULL projection before any filter — computing them after the filter is a bug
  that reported 90 promoted VAs where the repository states 92.
- **Do not re-derive the frontier in a second language.**
  `tools/reconstruction_tooling/frontier.py` owns eligibility and scoring. The
  census and `runtime frontier` have no opinion about either. `--va` CHOOSES the
  targets and the census only ENRICHES them; letting `--census` contribute its
  whole 543-row set alongside ten named addresses buried the ten.
- Verified numbers on the current checkout (2026-10-01): `runtime census` gives
  618 index records, 89 markers, **92 promoted VAs**, 427 runtime-gated — each
  matching the repository's own statement, which is how the census is checked.
  `import-recomp` on `startup-recovery14.json` gives 281 addresses, 127
  runtime-only (87 inside the image), 0 promoted. The runtime namespace changes
  no static bytes: the new build and a `git archive HEAD` build produce
  byte-identical exports.
- **Traps.** (1) A call edge is a transfer FROM the callsite TO the target, so
  the target goes in the CALLER's `runtime_targets` and the callsite in the
  TARGET's `runtime_callers`; getting it backwards makes every address appear to
  call itself. (2) `Summarize`/`ComputeStats` merge every entry reachable from
  one VA, so counts must be deduplicated by RESOLVED canonical VA or a promoted
  function observed at both its entry and an interior address is counted twice.
  (3) An unknown JSON field is `ErrSchema` (exit 6), not corruption (exit 5) —
  wrap it separately or the exit code collapses. (4) Only lowercase `0x%08x` is
  accepted as a canonical VA, prefix case included; `"925050"` and
  `"0X00925050"` are refused so one address cannot become two entries.

## Build / test (once `src/` exists`)
- CMake + Clang/GCC. `mkdir -p build && cd build && cmake .. && make -j`.
- C++17, `clang-format` (modified Google), `fmt` for formatting.
- Differential tests run the original under Wine (11) as an oracle.
- `CTest` builds the 77 promoted model tests with `-m32`
  (`OPENSPORE_RECONSTRUCTION_M32=ON`). Pin the compiler:
  `cmake -DCMAKE_CXX_COMPILER=clang++` or `g++`. Six promoted packages currently
  fail clang-only on `-Wunused-variable`; they are pre-existing.

## Reusable traps (each cost real debugging time)
- **Address spelling must be canonicalised at every boundary.** A pack stores
  listing addresses bare (`00f9ff00`) while a decoded branch operand carries the
  prefix (`0x00f9ff00`). Comparing them as strings silently empties every set
  intersection. `evidence_returns._successors` had exactly this bug and it
  collapsed 0x00f9fef0's CFG to 8 of 112 instructions.
- **A duplicated literal is a bug waiting for the next edit.** `0x00f9fef0`'s
  model test sized its storage for 4 runs against 12 trials; the overrun clobbered
  a neighbouring table and the "guard band intact" check was the symptom.
- **Never subscript a vtable struct to reach a slot.** `table[1].slot1` advances
  by `sizeof(StateInterface)` — a WHOLE PAIR of slots — while `MOV EDX,[EAX+n]`
  advances by `n * kPointerSize`. They agree only at n == 0. `0x0068f9b0`'s body
  had exactly this bug: it read `table+0xC` where `MOV EDX,[EAX+0x4]` proves
  `table+0x4`. Reach a slot by BYTE displacement (`slot_at<kSlotIndex>(table)`),
  never by array subscript. The package header now says so.
- **A wrong offset that lands on a VALID neighbour is not caught by crashing.**
  That defect survived every case of `0x0068f9b0`'s test except one, because the
  fixture's two tables were adjacent and `plain+0xC` aliased `clobber+0x4`, a
  callable pointer. Put a *distinct, callable sentinel* immediately past the last
  modelled slot (or decoy values that differ at every candidate offset) so a wrong
  index is observable by VALUE. A crash is the weakest possible signal here.
- **A test defect and a reconstruction defect produce the same segfault.**
  `0x0068f9b0`'s prior investigation concluded "fixture bookkeeping"; it was
  actually the body. Reproduce with a minimal harness before attributing a crash
  to the test.
- **`evidence` WITHOUT `--live` overwrites a LIVE pack with a degraded one.**
  A plain `evidence <va>` silently drops `disassembly` (and everything that
  depends on it), turning 7 PASS checks into NOT_AVAILABLE while still exiting 0.
  Always use `recover <va> --live` before trusting a validation status.
- **The frontier's source of truth for "promoted" is the promotion marker, not
  the manifest.** `status_for` reads `body_status`/`integration_status` from the
  integrator-owned manifest, which lags promotion by design. 12 promoted VAs sat
  outside it and were re-offered, and 2 gated 4 more through the dependency path.
  Both are fixed by reading the markers via `build_gate.discover_packages` +
  `promotion_targets`. **Do not hand-write manifest state** — the marker is the
  authoritative, self-maintaining record.
- **Do NOT widen `evidence_dispatch.VIRTUAL_SOURCE_TOKENS` to a `slot\w*`
  pattern.** Measured: 40 promoted packages match a bare slot-ish identifier and
  25 of those have machine listings with NO indirect dispatch, so the
  `virtual_source and not sites -> FAIL` arm would fail 25 correct packages. The
  matches are `kReceiverSlot` (a receiver field), `PKG_..._THISCALL` (a convention
  macro) and `kStackArgumentSlots` (a stack-argument count) — none of which
  declares a vtable dispatch boundary. The token set is deliberately narrow.
- Python here is 3.14 with no system pytest: run the suite as
  `uv run --no-project --with pytest python3 -m pytest tests/ -q`.
- `test_agent_overlay` needs a real X display **and** the `Xlib` module. System
  `python3` has `Xlib`; the isolated `uv --no-project` env does not, so this one
  failure is ENVIRONMENTAL and provably passes under system Python. Report it as
  ENVIRONMENTAL, never as PASS. `test_stale_metadata_safety` and the two
  concurrency-isolation tests fail on this machine regardless of the change under
  test. Baseline is **1842 passed** (was 1771, then 1833, then 1841), not the
  headline count. Two failures are documented as pre-existing rather than
  regressions; verify before believing that list still holds.

## Genuine ceilings: do not "solve" these by inference
`0x0068f9b0` is CLOSED (promoted). The rest are machine properties, not defects:
- **`__thiscall` vs `__fastcall` is UNDECIDABLE** from one register argument and
  zero stack arguments — the two emit identical code. `0x01053db0`,
  `0x01053e00`, `0x00c70e00` and `0x00c71e30` all sit here, and the derived ABI
  record correctly abstains. Because `validate`'s ABI arm tests `ABI_UNKNOWN`
  *before* its "convention present in source" PASS arm, a genuinely ambiguous
  convention can never reach static PASS. **That ordering is correct policy, not a
  bug**: promoting an unproven ABI is worse than leaving it UNKNOWN.
- **A return width bounded only by a CALL result is undeterminable** by this
  listing. `0x00c70e00`, `0x00c71e30`, `0x00f9fef0`, `0x00f96840`. The callee's
  signature decides it, and that is another VA's evidence, which the dimension
  deliberately declines to read.
- `0x00fa5040`'s `LEA ESP,[ESP]` receiver/ESP heuristic stays rejected.

## Current status
- **Engine**: the vertical slice works end to end — `cargo run -p spore-engine
  -- --preset documented-asset --package SPORE/Data/Spore_Content.package`
  decodes a real Spore record and renders it through Bevy/wgpu/Vulkan. See
  `docs/RUST-ENGINE.md` for the crate graph, the commands and the gotchas.
- **C++/research corpus**: see `docs/STATE.md` for the reconstruction phases,
  `docs/MIGRATION.md` for what became what, and `docs/replacement-status.json`
  for per-subsystem status.

## Cross-cutting rules for the two halves

- **The same record must never have two different answers.** When a C++ file,
  a Python oracle and a Rust crate all claim to decode something, the tie-breaker
  is a byte count over the real data, and `crates/spore-differential` exists to
  run exactly that comparison on demand.
- **Do not relax a decoder to make a test pass.** The C++ and Python references
  both refuse gmdl versions other than 8, refuse undocumented shader-data ids,
  and refuse the `0x15xx` luminance raster family. Those refusals are the
  specification; the Rust ports preserve them, and their test counts are higher
  for it.
- A type-level classification that is looser than the loader is a second source
  of truth waiting to drift. `DecodeStatus::Ok` means exactly "a per-record
  decoder exists in this workspace", and a test asserts the manifest and the
  loader never disagree about that.
