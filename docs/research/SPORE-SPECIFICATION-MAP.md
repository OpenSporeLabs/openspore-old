# Spore specification & evidence map

Owner: orchestrator · Status: research · Date: 2026-10-16

For each format/behavior family: the **strongest evidence we actually have** (per
the categories in the task — published spec / community documentation / existing
parser / known-good real files / differential vs an independent implementation /
direct observation / untested inference), the source, the OpenSpore crate, and
what remains unproven. Evidence grades are `spore_core::EvidenceLevel`.

## 0. The headline fact

**There is no published Maxis/EA Spore format specification.** Every entry below
rests on community reverse-engineering, real files, or original-binary
observation. The closest thing to a primary source is SporeModder-FX's code +
`Documentation/` + `reg_*.txt` (GPL-3.0; treat as a *knowledge* oracle, see
`LICENSE-PROVENANCE.md`). Therefore PhotoCraft's "spec-first, then reference
behavior" split collapses for Spore into **"the corpus outranks the port."**

## 1. Format & resource families

| Family | Strongest evidence | Source (verified) | OpenSpore crate | Grade now | Remains unproven |
|---|---|---|---|---|---|
| DBPF v3 index | community parser + **differential** over real files | SporeModder-FX `file/dbpf`; `SPORE/Data/Spore_Content.package`; `docs/ASSET-PATH.md` | `spore-dbpf` | **Verified** (`replacement-status.json` dbpf-index) | compressed-index edge cases |
| QFS / RefPack | community parser + real files | SporeModder-FX; `docs/ASSET-PATH.md` | `spore-dbpf` | **Verified** (qfs-decompress) | — |
| Resource identity `type:group:instance`; name→id hash | **algorithm fact** + community registries + name/layout match | `HashManager.java` `fnvHash` proves **FNV-1** (offset `0x811C9DC5`, prime `0x1000193`, multiply-then-xor, lowercased); `reg_type.txt`/`reg_file.txt` | `spore-core` (`key.rs`), `spore-assets` | **Confirmed** | coverage of the name universe |
| Property lists (PROP / `<sporemodel>`) | community documentation + parser | SporeModder-FX `Documentation/properties.txt`, `file/prop` | **(no dedicated crate — gap)** | Supported | typed field semantics beyond framing |
| Creator creation doc (`<sporemodel>` in PNG) | **known-good real creations** + research | authored `.png` creation files; `docs/analysis/creator/*` | `spore-creator-core` | Observed/Supported | both number spellings; catalog semantics |
| GMDL (baked model) | **two independent impls + differential** | `tools/spore/gmdl/gmdl.py` (oracle) vs `src/assets/Gmdl.cpp`; `spore-gmdl/claims.rs` | `spore-gmdl` | **Verified for v8** | v9+ **refused**; shader-data ids; trailer framing |
| RW4 container | community parsers + real files | SporeModder-FX `file/rw4`; Blender addons; two name tables **disagree** (`spore-rw4::cpp_type_name` vs `python_type_name`) | `spore-rw4` | Supported | payloads out of scope; section-name truth |
| Raster / textures (DXT5) | community parser + real files + differential | `tools/spore` raster; `docs/VALIDATION.md` (refuted 0x218 oracle; 0x15xx luminance) | `spore-texture` | Supported | `0x15xx` luminance **refused**; level-32 walk defect |
| Materials | research docs | `docs/MATERIALS-DESIGN.md`, `docs/MATERIAL-ROLES.md` | `spore-material` | Inferred/Supported | binding rules vs runtime |
| Rigblock / skeleton / bone names | hash vocabulary + real files | `docs/analysis/creator/11-bone-identity-vocabulary.md`; `spore_rigblock::vocabulary` | `spore-rigblock` | Supported | every composed part witnesses only `joint1` |
| Animation (`.tl`/`.sa`) | community parser | SporeModder-FX `file/anim`,`tlsa`; `docs/ANIMATION.md` | **none (gap)** | Inferred | keyframe semantics; joint transforms |
| Cell-stage records | original-program observation + passports | `docs/CELLSTAGE*.md`, `CELL-RUNTIME-OBSERVATION.md` | `spore-cellcontent` | Observed | 12 layouts; runtime state |

## 2. Community sources as oracles (not as interchangeable truth)

- **SporeModder-FX** (GPL-3.0): the richest knowledge + parser corpus. Use as
  *knowledge* and, where the copyleft is accepted, as an implementation to
  differential against. **NB**: a working community parser does not prove every
  decoded field is *correctly understood* — it proves the framing round-trips.
- **SporeModder-Blender-Addons** (GPL-3.0): RW4/GMDL import↔export round-trips
  are a ready-made **independent conformance oracle**.
- **Spore-ModAPI** (no license → BLOCKED code): its `SDKtoGhidra/` +
  `SporeGhidra_*.xml` are a **readable knowledge oracle** for class/function
  identity, not code to copy.
- **SporeTools / ModBrowser / SporeHash-VSCode** (MIT): FNV tools corroborate the
  hash; reusable with notice.

## 3. Original-program observation (static ≠ runtime)

Sources: Ghidra, the behaviour passports, stable `spore-recomp` reports
(read-only). The project's own rule holds: **static decompilation is not runtime
proof, and a startup milestone is not editor/simulation/rendering semantics.**
`docs/tooling/validation-dimensions.md` encodes this as two axes — a static
`PASS` is never a runtime claim, and runtime `GATED` is an *open gate*, not a
failure. The whole reconstruction coverage matrix already refuses a single score
(`docs/tooling/reconstruction-coverage.md`).

## 4. What still requires the original executable

- **Game simulation semantics** — creature/part attachment rules, editor state
  transitions, animation playback, cell-stage state machines. None is in any
  format; all need observation (`docs/analysis/gameplay-*.md`, `runtime-oracle*`).
- **Rendering semantics** — how a decoded material/geometry is actually shaded.
- **Undocumented/ambiguous feature behavior** — the Spore equivalent of
  PhotoCraft's "what should this do?" has **no reference-app authoring path**; it
  must come from controlled GOG runs + `spore-recomp` reports.

## 5. Differential & synthetic testing already present

`crates/spore-differential` (`src/oracle.rs`, `src/compare/{dbpf_index,gmdl,rw4,
raster,record_bytes,manifest,worldobj}.rs`) is the existing spec-to-test harness:
Rust parsers vs the Python oracles in `tools/spore`, plus synthetic fixtures and
`#[ignore]`d real-corpus tests. **Improve its index and provenance before adding
new tests** — several families above already have a differential; do not
duplicate them.
