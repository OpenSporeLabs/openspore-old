# RUST-ENGINE — the OpenSpore Rust + Bevy workspace

The developer-facing guide to `Cargo.toml` and `crates/`. For what the C++/Python
half of this repository is and how the two halves relate, read
[`MIGRATION.md`](MIGRATION.md). For the clean-room rules, read `AGENTS.md`.

A Bevy application whose asset path starts at a real Spore `.package` file and
ends at pixels:

```text
  .package on disk
    -> spore_dbpf     DBPF v3 index, QFS decompress          (no Bevy)
    -> spore_gmdl     gmdl decode, mesh extraction           (no Bevy)
    -> spore_assets   package priority, identity resolution   (no Bevy)
    -> convert        vertex buffers, normal policy          (no GPU)
    -> loader         Assets<Mesh> handles, entity spawn     (Bevy)
    -> bevy_render    wgpu -> Vulkan / Metal / DX12
```

---

## 1. The crate dependency graph, and the one rule that makes it work

```text
                spore-core            (ResourceKey, RecordType, evidence vocabulary)
                     |
     +---------------+---------------+---------------+---------------+
     |               |               |               |               |
 spore-dbpf      spore-rw4       spore-gmdl     spore-texture    (all leaf decoders)
     |               |               |               |
     +---------------+---------------+---------------+
                     |
                spore-assets        ContentStore, priority, identity, canonical manifest
                     |
                spore-material       material id -> texture ref -> RGBA8 + graded claims
                     |
        +------------+------------+
        |                         |
   spore-engine               spore-tools  ->  osptool
   (Bevy)                     (no Bevy)

                spore-core
                     |
            spore-creator-core    the authoring model: <sporemodel> documents,
                     ^             part blocks, transforms, paint, validation
                     |
             (projection lives in spore-engine, never here)
```

> **The rule: everything above `loader` is renderer-agnostic and Bevy-free.**

`spore-dbpf`, `spore-rw4`, `spore-gmdl`, `spore-texture`, `spore-core`,
`spore-assets`, `spore-material`, `spore-creator-core` and `spore-tools` have
**no `bevy` dependency at all**. You can confirm it by reading their
`Cargo.toml` — none of the nine lists one. They speak in `spore_gmdl::Mesh` and
`spore_texture::RasterImage`, never in Bevy assets, components or GPU handles.

That is not tidiness; it is what makes the asset layer testable. `spore-assets`
puts it plainly: *"Everything above this crate is renderer-agnostic… That is the
whole design intent, and it is why this crate can be tested with no window, no
GPU and no game installed."*

`spore-engine` is the only crate that depends on Bevy, and even inside it the
split holds: `spore_engine::convert` is deliberately free of windowing,
rendering and asset lifetimes so it can be tested with no GPU, and
`spore_engine::prepare()` decodes a scene request **without touching Bevy, the
GPU or the filesystem beyond the packages named**.

All nine crates carry a crate-level `unsafe` lint, and the three that need an
exception path carry the weaker form on purpose:

* six crates use `#![forbid(unsafe_code)]` — `spore-core`, `spore-dbpf`,
  `spore-rw4`, `spore-gmdl`, `spore-texture`, `spore-material`;
* `spore-assets`, `spore-engine` and `spore-tools` use `#![deny(unsafe_code)]`, so
  that a single, greppable `#[allow]` with a written safety argument stays
  possible rather than requiring a blanket hole in the policy. `spore-assets`
  says exactly this at the lint.

### What each crate is for

| crate | one line |
|---|---|
| `spore-core` | Resource identity (`ResourceKey`), record type ids, and the evidence/provenance vocabulary. Dependency-free by design; must stay cheap to compile. |
| `spore-dbpf` | DBPF v3 package index, QFS/RefPack decompression, record extraction. |
| `spore-rw4` | RenderWare 4 **section directory** walker. Section *payloads* are deliberately out of scope. |
| `spore-gmdl` | GMDL (`GameModel`, type id `0x00E6BCE5`) record decoder and static-mesh extractor. |
| `spore-texture` | Raster envelope parsing and the DXT5/BC3 block codec. |
| `spore-assets` | The boundary where Spore data enters the engine: package priority, identity resolution, typed decode, the canonical manifest. |
| `spore-material` | What a gmdl material id names — and what this build does not know. The narrowest crate in the workspace. |
| `spore-engine` | The runtime: a Bevy application that loads and renders real Spore assets. |
| `spore-tools` | `osptool`, an offline inspector. Library-first, binary-as-shim, so every command is drivable from a test with an in-memory sink. |
| `spore-creator-core` | The authoring core: Spore's own `<sporemodel>` creator documents (68 of them, six record families), decoded into plain Rust. Depends on `spore-core` and nothing else. See `docs/CREATOR-CORE.md`. |

---

## 2. Build, test, lint

```bash
cargo build --workspace
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

### Measured timings (16-core machine, `nproc` → 16, 2026-10-04)

**These are warm-cache numbers.** The `target/` tree was already populated when
they were taken, so they measure *re-running* each command, not a cold build. A
cold build was not measured — a clean rebuild would have destroyed the shared
`target/` directory other work depends on. Treat the cold figures as
**not measured** and discover them yourself once.

| command | measured | note |
|---|---|---|
| `cargo build --workspace` | `Finished ... in 0.23s` | nothing to do; no-op re-check |
| `cargo test --workspace` | `0.85s` wall for the whole run, 696 passed / 0 failed / 10 ignored | tests themselves take ~0.1 s |
| `cargo clippy --workspace --all-targets -- -D warnings` | `Finished ... in 0.31s`, **clean, zero warnings** | no-op re-check; `-D warnings` passes |
| `cargo fmt --all --check` | **not run here** | it was explicitly out of scope for this pass |

The number that actually matters is the test count, because it is the one you
can check:

```
$ cargo test --workspace
# 696 passed; 0 failed; 10 ignored  (48 suites)
```

Per crate, from `cargo test -p <crate>`:

| crate | passed | ignored |
|---|---|---|
| `spore-core` | 24 | 0 |
| `spore-dbpf` | 109 | 0 |
| `spore-rw4` | 45 | 0 |
| `spore-gmdl` | 121 | 0 |
| `spore-texture` | 71 | 0 |
| `spore-assets` | 22 | 0 |
| `spore-material` | 61 | 1 |
| `spore-engine` | 40 | 0 |
| `spore-tools` | 203 | 9 |

All **10 ignored tests** need the git-ignored `SPORE/` tree and say so in their
own `#[ignore]` note (`needs SPORE/Data/Spore_Content.package; run with
--ignored`). On a machine with the game installed, run `cargo test --workspace
-- --ignored` to include them.

---

## 3. Running the vertical slice

The engine **opens a window** unless you pass `--info`. Never run it without
`--info` on a headless machine.

```bash
# headless: decode, report, exit. This is the CI gate.
cargo run -q -p spore-engine -- --preset documented-asset \
    --package SPORE/Data/Spore_Content.package --info

# a window showing a real decoded Spore asset (needs a display)
cargo run -q -p spore-engine -- --preset documented-asset \
    --package SPORE/Data/Spore_Content.package

# a window showing a built-in mesh, no game data at all
cargo run -q -p spore-engine -- --placeholder
```

### The `OPENSPORE-STAGED v1` report line

`--info` prints exactly one machine-readable line. Actual output from the command
above:

```
OPENSPORE-STAGED v1 key=0x00e6bce5:0x40637e03:0x067a07f0 package=Spore_Content format=gmdl meshes=1 triangles=20 bounds_min=-2.070057,0.139144,0.081329 bounds_max=-0.371524,2.164293,5.326164 normals=derived
```

The format is versioned (`v1`) with a **fixed field set in a fixed order**,
because the point of a machine-readable line is that a consumer can rely on it —
the same discipline as the repository's existing `CELLSTAGE-MANIFEST v1`.

| field | meaning |
|---|---|
| `v1` | format version. Bump on any field change. |
| `key=` | the `(type, group, instance)` identity that was loaded |
| `package=` | which package answered (its file stem) |
| `format=` | the model format: `gmdl` or `rw4` |
| `meshes=` | mesh count in the decoded model |
| `triangles=` | triangles actually staged for the GPU, after fan expansion |
| `bounds_min=` / `bounds_max=` | the mesh's own bounding box, 6 decimals |
| `normals=` | `derived` — see §6. Never `raw`, and never a lie. |

On failure the same version tag carries the reason instead of the fields:

```
OPENSPORE-STAGED v1 failed: could not open package `/nope.package`: No such file or directory (os error 2)
```

and the process exits **3**.

### Engine exit codes

| code | meaning |
|---|---|
| 0 | ok |
| 2 | bad command line |
| 3 | asset unavailable |

Verified: `--bogus` → 2; `--package /nope.package --info` → 3.

### The other modes

* `--placeholder` — prints `OPENSPORE-MODE v1 placeholder` and opens a window on
  a built-in mesh. Proves the renderer with no game data involved.
* `--record T:G:I --package ...` — the explicit-identity form of the vertical
  slice. `--record` and `--preset` both accepted; a later one of the two wins and
  claims the origin.
* `--scale`, `--width`, `--height`, `--title` — window and model controls.

---

## 3a. Textures on the vertical slice

`--texture` resolves the model's texture references through `spore-material`
and applies the **first that decodes** as the diffuse colour:

```
$ openspore --preset documented-asset --package SPORE/Data/Spore_Content.package \
      --texture --info
OPENSPORE-STAGED v1 key=0x00e6bce5:0x40637e03:0x067a07f0 package=Spore_Content \
  format=gmdl meshes=1 triangles=20 ... normals=derived textures=2/3
```

Two of three resolve; the third is refused **by name** for carrying fourcc
`0x15` -- the luminance family `spore-texture` declines. That refusal is the
correct outcome, and it is reported rather than worked around.

Which reference is *diffuse* is **not known**: the 16 header bytes of a
texture-set entry are undecoded, which is why `spore-material` exposes
`SamplerRole::Unresolved` as its single variant. Applying the first resolved
reference is a stated visual choice, not a claim about the record.

Expect the model to render **near-black** with `--texture`. That is faithful,
not a bug: `docs/BOUNDARIES.md` records the same record (`0x40662900`) as "a
near-black alpha mask", and the DXT5 codec's colour handling is deliberately
non-standard (see `spore_texture::claims::dxt5_spec_deviations`).

The assumed record type is a visible flag rather than a constant, because a
gmdl texture-set entry stores `{instance, group}` with **no type word** and a
wrong assumption yields a confidently wrong texture instead of an error:

```
--assumed-texture-type 0x2f4e681c   (default)
```

## 4. `osptool`

```bash
cargo run -q -p spore-tools -- --help
```

```
osptool - inspect Spore DBPF packages offline
```

| subcommand | what it does |
|---|---|
| `list <package>` | index rows in DBPF file order. `--type`, `--group`, `--limit N` (default 50, 0 = unlimited), `--json`, `--names` |
| `find <package> <T:G:I>` | resolve one identity: which package answered, its extent, and whether it decodes |
| `describe <package> <T:G:I>` | decode a record according to its type id and print a structural summary |
| `extract <package> <T:G:I> --out <dir>` | write one record's bytes to `<dir>/<sanitised name>`. The package is only ever read. |
| `manifest <package...>` | canonical manifest over one or more packages, rows ascending by (type, group, instance). `--out`, `--json`, `--stats` |
| `verify <package>` | walk every record and count decoded / container / failed / no-decoder. Streams; nothing accumulates. |
| `types` | the canonical type table. `--group` adds the group table, `--package <p>` adds a per-package histogram merged with that package's own prop directory |
| `-h`, `--help` | usage and exit codes |

### Exit codes

| code | meaning |
|---|---|
| 0 | ok |
| 2 | bad command line |
| 3 | record not found, or no package loaded |
| 4 | a record or package was found but the container layer refused it |
| 5 | an I/O failure |

Verified on this machine: `find` on an absent identity → **3**; an unknown
subcommand → **2**; a missing `.package` file → **5**.

Two exit-0 cases are deliberate and will surprise you:

* **`describe` on a type with no decoder exits 0.** The record was found and its
  extent read — a complete answer to the question asked. It prints the extent and
  says it has no decoder; it never prints a summary that *looks* decoded.
* **`verify` exits 0 even when records fail**, because it is a measurement: the
  failures are its output.

Use `describe` when a decode failure should be fatal.

### The rule every human-readable line obeys

**A statement about a record is either `decoded` or `type-level`, and it says
which.** "This build has a decoder family for `gmdl`" is a fact about the *type
table*; "this record decoded to one mesh of 32 vertices" is a fact about 1 156
bytes. Collapsing the two is how an asset tool reports a decoder's *existence* as
if it had reported a record's *contents*.

### Determinism

`list`, `find`, `describe`, `manifest` and `types` are byte-for-byte reproducible
for the same input, and the test suite asserts it. A report that reorders itself
between runs cannot be diffed, and a report that cannot be diffed cannot be used
to review a change.

Worked example — the type census over the content package:

```
$ cargo run -q -p spore-tools -- manifest SPORE/Data/Spore_Content.package --stats
rows              17119
by decode status  (type-level classification, not a per-record decode)
  container-undecoded  1162
  ok                   8294
  undecoded            7663
top 20 type names
       4209  gmdl
       2954  raster
       1709  bem
       1642  png
       1641  pollen_metadata
       1631  summary
       1162  prop
       1131  rw4
       1022  world_object
         18  summary_pill
```

And the per-record view of the vertical slice's identity:

```
$ cargo run -q -p spore-tools -- describe SPORE/Data/Spore_Content.package \
    0x00e6bce5:0x40637e03:0x067a07f0
record      0x00e6bce5:0x40637e03:0x067a07f0
package     Spore_Content
type        0x00e6bce5 (gmdl)
...
index extent            0x09c2f5a2+794 (794 bytes)
index memory size       1156 bytes
index compression       qfs

decoder: gmdl (decoded — every number below came out of this record's 1156 bytes)
  version            8
  mesh count         1
  index buffers      1
    [0] prim=4 (TRIANGLELIST) indices=60 bits=16 payload=120 bytes
  vertex descriptors 1
    [0] 3 element(s), stride 24
  vertex buffers     1
    [0] descriptor=0 vertices=32 payload=768 bytes
  material ids       1
    [0] 0x407dfddb
  texture refs       3
  trailer            complete (1156 of 1156 bytes validated)
```

Note the last line: `trailer complete (1156 of 1156 bytes validated)`. Strict
walk, then a best-effort trailer whose boundary is **reported** rather than
hidden — `strict_consumed` is how far the walk actually read, `consumed` is
always the input length.

---

## 3b. A real creature, and the honest reason its tail is missing

```
$ openspore --preset creature --package SPORE/Data/Spore_Content.package --info
OPENSPORE-STAGED v1 key=0x00e6bce5:0x40627100:0x067c79d2 package=Spore_Content
  format=gmdl meshes=2 triangles=47016
  bounds_min=-0.776000,-0.772762,-0.010258 bounds_max=0.776000,0.441339,2.098687
  normals=derived textures=0/0
  decode=material-info: gmdl: undocumented shader-data id 0x218 in material info
```

47 016 triangles, rendered. Two things about that line are worth reading.

**Where creature models actually live.** Group `0x40627100`, which
`tools/spore/types/groupnames.json` calls `CreatureModelsHQ`. Group
`0x40626200` (`CreatureModels`) holds **no gmdl records at all** -- it carries
`pollen_metadata`, `bem`, `png`, `summary` and `prop`. So a creature model has to
be looked for in the HQ group, which is not obvious from the names and cost a
search to establish. Measured: 97 gmdl records in `0x40627100`.

**Why `decode=` is not `complete`.** Those records name shader-data id `0x218`,
whose byte size is not in the table, so the material-info walk cannot continue
past it. `spore_gmdl` therefore has **two** entry points with deliberately
different contracts:

| function | contract | used by |
|---|---|---|
| `parse` | the whole record or an error. No partial model. | `osptool describe`, `spore-differential` |
| `parse_recovering` | geometry-complete-or-error, plus a typed `StopReason` | the renderer |

The geometry is *behind* the mesh table, so a record that stops at the
material-info table has already-validated index buffers, vertex descriptors,
vertex buffers, the mesh table and the per-mesh material ids. `parse_recovering`
never invents a size, a count or a value -- it stops and reports where. A test
pins that `parse` still refuses `0x218`, because if it ever accepted it the
differential harness would be comparing two partial decodes and calling it a
match.

**Deriving `0x218`'s size was attempted and failed**, and that is recorded too:
sweeping candidate sizes 0..2048 against all 97 creature records, requiring the
bone-range/anim-data trailer to close *exactly* on the record end, produced
**zero** candidates. The creature trailer is a different shape from the framing
the C++ reference documents, which it already noted for the cell-stage models.
So `0x218` stays unmeasured rather than guessed.

## 3c. Texture groups, and what `png` records actually are

Measured over a 60-record sample of `Spore_Content` gmdl records, 209 texture-set
references:

| referenced group | refs | types present in that group |
|---|---|---|
| `0x40642900/01/02` | 34 each | `raster` **only** |
| `0x40632900/01/02` | 22 each | `raster` **only** |
| `0x40652900/01/02` | 8 each | `raster` **only** |
| `0x40662900/01` | 6 each | `raster` **only** |
| `0x40612900/01` | 1 each | `raster` **only** |

Three consequences:

1. **`--assumed-texture-type` defaulting to `raster` is empirically right.** Every
   one of the 209 sampled references points at a group that contains *only*
   `raster` records. The reference still carries no type word, so the assumption
   stays an assumption -- but it is now an assumption measured over a sample
   rather than a guess, and `spore-material` grades it `INFERRED`.
2. **The texture-group layout is regular**: `0x40 6X 29 0Y`, where `X` is the
   stage category (`1` cell, `2` creature, `3` building, `4` vehicle, `5` ufo,
   `6` flora, `b` palette) and `0Y` is one of three slots per model. Three
   textures per model, which matches the three texture-set entries the
   documented asset references.
3. **`png` records are NOT textures.** They live in the *model* groups --
   `0x40616200`, `0x40626200`, `0x40636200`, `0x40646200`, `0x40656200`,
   `0x40666200` -- one preview image alongside each model's geometry, traits and
   summary. There is **zero** overlap with any referenced texture group. So a PNG
   decoder is a Sporepedia/editor-preview feature, not a blocker for creature
   rendering, and it should not be built before something that is.

## 4a. Package priority is a product decision, and it is load-bearing

`ContentStore` resolves **first match wins** in the order packages were
pushed. Nothing sorts them, nothing deduplicates, nothing guesses. The order
you pass is the order that is honoured, which is what makes it auditable.

For most records the choice does not matter. For at least one it is decisive.
The Cell Stage globals record `0x2A3CE5B7:0:0xA426730B` exists in **one stock
install with two byte-distinct revisions**:

| package | length |
|---|---|
| `Spore/Data/Spore_Game.package` | **264 bytes** |
| `Spore/Data/PatchData.package` | **276 bytes** |
| `SPORE/DataEP1/Spore_EP1_Data.package` | **276 bytes** |

A word-level alignment shows the 264 is not a truncation and not a shift: the
276-byte revision is the same record with exactly **three fields absent** --
`gameMode` @0, `startingCellKey` @56, `controlMethod` @212 -- and all 63 other
words are byte-identical. `cCellGlobalsResource` grew by three fields after the
base game, and the three it gained are all mode/cell/control selectors.

So loading base content *before* a patch resolves this record to the 264-byte
copy and the decoder returns a typed `ExtentMismatch`. **That refusal is
correct** -- padding to 276 would fabricate three fields -- and the fix is the
package order:

```rust
let mut store = ContentStore::new();
store.push(Package::open("Spore_Game",      "SPORE/Data/Spore_Game.package")?);
store.push(Package::open("Spore_Content",  "SPORE/Data/Spore_Content.package")?);
// Patches go in FRONT: they override.
store.push_front(Package::open("PatchData", "SPORE/Data/PatchData.package")?);
store.push_front(Package::open("EP1_Data",  "SPORE/DataEP1/Spore_EP1_Data.package")?);
```

`spore-assets` does not encode this policy because it is a *product* decision,
not a format fact -- but `spore-cellcontent`'s real-corpus test applies it
explicitly rather than by accident, and its
`the_globals_record_has_two_revisions_in_one_install` test proves the
three-word splice reproduces the 264 exactly.

An `ExtentMismatch` on a fixed-extent record is therefore worth reading as
"wrong package priority" before it is read as "corrupt record".

## 5. The Bevy feature set, and why each feature is there

From the root `Cargo.toml`:

```toml
bevy = { version = "0.19", default-features = false, features = [
    "bevy_asset",
    "bevy_core_pipeline",
    "bevy_log",
    "bevy_pbr",
    "bevy_render",
    "bevy_winit",
    "png",
    "tonemapping_luts",
    "wayland",
    "x11",
    "zstd_rust",
] }
```

`default-features = false` is the load-bearing part. The set is the smallest one
that gives a native window, a wgpu renderer, PBR materials and PNG image loading.

| feature | why it is there |
|---|---|
| `bevy_winit` | the window and input backend. Without it there is no `WindowPlugin` and no surface. |
| `bevy_render` | the wgpu renderer: device, surface, render graph. |
| `bevy_pbr` | `StandardMaterial` and the PBR pipeline. A gmdl's material section is a list of opaque ids; `spore-material` resolves them to textures, and something has to shade them. |
| `bevy_core_pipeline` | tonemapping, MSAA, the main render pass. |
| `bevy_asset` | `Assets<Mesh>` / `Assets<StandardMaterial>` — the loader's whole output type. |
| `bevy_log` | tracing. The engine's `info!` diagnostics. |
| `png` | PNG *image* decoding for the asset pipeline. (Distinct from decoding a Spore `png` **record**, which nothing does — see the gap list in `docs/MIGRATION.md`.) |
| `tonemapping_luts` | the LUT textures `bevy_core_pipeline`'s tonemapper needs. Omitting it is a compile error, not a size saving. |
| `wayland`, `x11` | the two display backends winit can use. Both, because the machine this was developed on is KDE Wayland/XWayland and the C++ tree already documents a `default → wayland → x11` fallback. |
| `zstd_rust` | see §7. |

### What is deliberately absent, and why

`bevy_audio`, `bevy_gilrs`, `bevy_ui` and `bevy_gizmos` are **not** in the
feature list. Two reasons, both stated in the `Cargo.toml` comment and both
checkable:

1. **No ALSA or udev dependency.** `bevy_audio` pulls `alsa`; `bevy_gilrs` pulls
   `udev`/`libudev`. On a machine without those dev packages the build breaks for
   reasons that have nothing to do with this project. Verified by inspecting the
   compiled artifacts in `target/debug/deps`: `libbevy_audio`, `libbevy_gilrs`,
   `libbevy_ui`, `libbevy_gizmos`, `libalsa` and `libudev` are all **absent**;
   the Bevy crates that *are* compiled are `bevy_asset`, `bevy_core_pipeline`,
   `bevy_input` (core input types, not gilrs), `bevy_log`, `bevy_pbr`,
   `bevy_render`, `bevy_window`, `bevy_winit`, `bevy_mesh`, `bevy_image`,
   `bevy_material`, `bevy_camera`, `bevy_light`, `bevy_transform`.
2. **An auditable graph.** `default-features = false` plus an explicit list means
   "what does OpenSpore depend on?" is answerable by reading eleven lines, not by
   auditing a transitive closure. `Cargo.lock` pins 439 packages.

The same reason is why the engine has no audio, no gamepad, no simulation, no
creature model and no UI. Each is a real subsystem that has not been written yet,
and stubbing them would make the vertical slice look further along than it is.

---

## 6. The honest limits

### GMDL `UBYTE4` normals are INFERRED, so the engine derives them

GMDL vertex buffers in the records decoded so far carry normals as `UBYTE4`. The
container stores raw bytes; the C++ reference divided by 255 and handed back
`[0,1]` triples. **That encoding is not understood.**
`docs/ASSET-PATH.md` grades it INFERRED and asks the open question outright:
*signed? scaled?* A raw unsigned byte cannot be a signed normal, so feeding
`/255` straight into a PBR shader produces a surface lit as though every normal
pointed into the positive octant.

Three honest options, and this build takes the second by default:

| option | fidelity | result |
|---|---|---|
| feed `/255` through | faithful to the reference | shading is wrong |
| **recompute from the triangles** (`NormalMode::Computed`) | a *stated derivation* | shading is right |
| decode as biased signed (`2x-1`) | an **invention** | plausible, unproven |

So `NormalMode::Computed` is the default, **and the raw bytes are still carried**
on `MeshBuffers::raw_normal_bytes`. Nothing is discarded and nothing is claimed
that was not measured. `NormalMode::RawUnorm` remains available so the two can be
compared side by side; `NormalMode::None` carries no normal attribute and lets the
material shade unlit.

That is what the `normals=derived` field on the `OPENSPORE-STAGED v1` line is
reporting. The claim is graded at `spore_gmdl::claims::VERTEX_NORMAL_ENCODING`
and is readable at runtime with
`spore_gmdl::claims::vertex_normal_encoding_level()`.

### wgpu has no triangle-fan topology

GMDL `primType` 6 is a triangle fan. `bevy::mesh::PrimitiveTopology` offers
exactly `PointList`, `LineList`, `LineStrip`, `TriangleList` and `TriangleStrip` —
**there is no fan**. So `spore_engine::convert` maps `Topology::TriangleFan` to
`PrimitiveTopology::TriangleList` *and rewrites the index buffer* through
`convert::expand_fan`, which fixes the first vertex and emits one triangle per
consecutive pair:

```
expand_fan(&[0, 1, 2, 3])  == vec![0, 1, 2, 0, 2, 3]
```

Returning `TriangleList` without doing so would silently draw garbage. The
expansion is exact and standard, not a reinterpretation, and
`Topology::needs_index_expansion()` is `true` for exactly one topology so the
rule stays checkable.

### Other things the engine does not claim

* **Material names.** What a gmdl material id names is not known.
  `MaterialModel::name` is `None` for every id and `SamplerRole` has one variant,
  `Unresolved`. The three texture references on the documented asset resolve to
  two images and **one typed refusal** — which is why the batch API returns
  `Vec<Result<..>>` instead of aborting.
* **Instance names.** DBPF has no string table. Nothing invents one.
* **gmdl versions other than 8.** Refused by name.
* **RW4 models.** An `rw4` model loads and reports **zero** meshes rather than
  pretending otherwise.

---

## 7. The two gotchas that cost real time

### 7.1 `zstd` needs an explicit backend in Bevy 0.19

Bevy 0.19 will not pick a zstd implementation for you. The asset pipeline
references `Compressed` assets and the feature simply is not satisfied until one
backend is named:

* `zstd_rust` — pure Rust. **This is what the workspace uses.**
* `zstd_c` — binds `libzstd` through a C toolchain and `zstd-sys`, i.e. it
  requires `cc` and a system zstd at build time.

The consequence of choosing `zstd_rust` is that the build needs **no C toolchain
and no system `libzstd`**, which is a large part of why the whole workspace builds
with one `cargo build`. Verified: `target/debug/deps` contains `libruzstd` (the
backend `zstd_rust` uses) and contains **no** `zstd-sys`, no `zstd_c` and no
`libzstd`.

If you see a Bevy 0.19 build complaining about a missing zstd backend, this is
why. The fix is a feature, not a system package.

### 7.2 `DefaultPlugins` builds a winit `EventLoop`, which **panics off the main thread**

`cargo test` runs test functions on spawned threads. `DefaultPlugins` constructs
a winit `EventLoop`, and winit 0.30 panics when that happens off the main thread.

So **"the app assembles" is not a headless-testable claim.** The test that
looked obvious — build the app, `finish()`, `cleanup()` — panics. This was found
by writing it the obvious way and watching it fail.

The honest headless assertion is over the plugin subset that has **no** event
loop:

```rust
#[test]
fn a_headless_app_assembles_but_the_real_one_needs_the_main_thread() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.add_systems(Update, exit_on_escape);
    app.finish();
    app.cleanup();
}
```

That is precisely the part worth testing, because it is the part a Bevy API
change can silently break.

**The real guarantee is that decoding is Bevy-free.** That is what the engine's
own test suite asserts:

```rust
#[test]
fn the_whole_asset_path_runs_with_no_window_and_no_game_install() { /* … */ }
```

If that ever needs a GPU, the vertical slice stops being verifiable in CI. This
is also why `--info` never constructs a window at all, which is what makes it
usable as a CI gate, and why `spore_engine::prepare()` is a free function that
returns a `PrepareOutcome` **before** any `App` exists.

---

## 8. Where to look next

| question | file |
|---|---|
| what the runtime does and in what order | `crates/spore-engine/src/lib.rs` (crate docs) |
| the CLI grammar and exit codes | `crates/spore-engine/src/cli.rs` |
| the `OPENSPORE-STAGED v1` line | `crates/spore-engine/src/loader.rs` (`StagedContent::report_line`) |
| the normal policy and fan expansion | `crates/spore-engine/src/convert.rs` |
| every graded claim about gmdl | `crates/spore-gmdl/src/claims.rs` |
| every graded claim about textures | `crates/spore-texture/src/claims.rs` |
| what is *not* here, stated plainly | `crates/spore-assets/src/lib.rs`, crate docs |
| the byte-level DBPF/QFS layout | `crates/spore-dbpf/src/lib.rs`, `.../src/qfs.rs` |
| the RW4 magic warning | `crates/spore-rw4/src/lib.rs` |
| the honest limits | `docs/MIGRATION.md` §"What the Rust workspace deliberately does NOT do yet" |
| the clean-room rules | `AGENTS.md` §"Hard rules (legal / ethics)" |