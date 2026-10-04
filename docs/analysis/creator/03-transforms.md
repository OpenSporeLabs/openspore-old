# 03 — TRANSFORMS: coordinate systems, pivots, and what a part transform must be able to say

**Agent:** TRANSFORMS · **Date:** 2026-10-04 · **Status:** research, design-grade.
**Scope:** everything a placed part needs to say about *where it is, how it is turned, and how big it is* — and nothing else. Document model, UI, authoring flow and serialization framing belong to other agents.

**Everything numeric in this file came from a command that is reproduced in §8.** Nothing is quoted from memory. Where a claim could not be measured it is marked `UNKNOWN` and left there.

Evidence grades are `spore_core::EvidenceLevel`'s seven rungs
(`crates/spore-core/src/evidence.rs:56-61`): `OBSERVED`, `VERIFIED`, `CONFIRMED`, `SUPPORTED`, `INFERRED`, `APPROXIMATION`, `UNKNOWN`.

> **The load-bearing invariant of this repository applies to this document too.** The measurements below were taken with scratch binaries built **outside** the checkout, against `CARGO_TARGET_DIR=/tmp/opencode/creator-transforms`. No file in this repository was modified. The only file this agent creates is this one.

---

## 1. Method, and the two things that had to be measured before anything could be designed

Two facts about the corpus determine almost every design choice downstream, and neither is visible in the code:

1. **A gmdl record's `bounds_min` / `bounds_max` / `radius` are bytes in the record.**
   `crates/spore-gmdl/src/parse.rs:554-600` reads them (little-endian, after `meshCount`,
   in `Walk::read_mesh_count_and_bounds`). The C++ reference agrees on the shape:
   `src/assets/Gmdl.hpp:61-63`.

2. **`spore_gmdl::Mesh`'s `bounds_min` / `bounds_max` / `radius` are *not* those bytes.**
   They are computed from the vertex positions in
   `crates/spore-gmdl/src/mesh.rs:395-436` (`compute_mesh_bounds`), and the C++
   reference computes them identically in `src/assets/Mesh.cpp:181-215`.

So the workspace contains **two different bounding quantities with two different
anchor conventions**, and a reader who does not know that will use them
interchangeably and be wrong. §3 settles the difference by measurement.

Everything in §2 and §3 is a measurement over **all 4 209 gmdl records** of
`SPORE/Data/Spore_Content.package` (995 268 083 bytes, GOG digital 3.1.0.22),
read through the shipped Rust decoders.

---

## 2. Coordinate systems and conventions, established from the corpus

### 2.1 The three coordinate systems that actually exist

| system | where it lives | measured convention | grade |
|---|---|---|---|
| **record-local** | gmdl vertex positions; one origin per *record*, shared by all its meshes | right-handed, **+Y up**, `1 unit ≈ 1 metre` (INFERRED, §2.3) | `VERIFIED` (see §2.2, §2.3) |
| **part-local** | the game's authored part data (`RigblockData`, `cEditorResourceBlock`, `EditorRigblock`) | `Vector3` position + `Matrix3` orientation + **scalar** scale; X = left/right, Y = up | `CONFIRMED` (field offsets+types from two independent provenance sources) |
| **engine/view** | this workspace's renderer | right-handed, **+Y up**, CCW front faces, Vulkan NDC depth `[0,1]` | `VERIFIED` (C++ reference) / `CONFIRMED` (Bevy 0.19) |

There is **no separate mesh-local frame** in this corpus. See §3.

### 2.2 Y is up. Three independent measurements.

**(a) The engine's own camera math.** The C++ reference builds every view matrix
with an explicit up vector of `+Y`:

```text
src/apps/cell_stage.cpp:501   const float up[3] = {0.0F, 1.0F, 0.0F};
src/apps/cell_stage.cpp:764   const float up[3] = {0.0F, 1.0F, 0.0F};
src/apps/cell_stage.cpp:1138  const float up[3] = {0.0F, 1.0F, 0.0F};
```

and its `lookAt` (`src/apps/cell_stage.cpp:209-241`) is the textbook
right-handed construction: `r = f × up`, `u = r × f`, `view = [r; u; −f]` with
`m[3]=m[7]=m[11]=0`, `m[15]=1` (column-major `mat4`, declared at
`src/apps/cell_stage.cpp:185`). The Rust engine agrees: the framing direction is
`Vec3::new(0.0, 0.42, 0.91)` (`crates/spore-engine/src/loader.rs:813`) — a
positive Y component — and it calls `looking_at(center, Vec3::Y)`
(`loader.rs:815`). **Grade: `VERIFIED`.**

**(b) The C++ reference's own visualisation treats Y as height.**

```text
src/assets/Mesh.cpp:222   const float spanY = mesh.bboxMax[1] - mesh.bboxMin[1];
src/assets/Mesh.cpp:235   float t = spanY > 0.0F ? (mesh.positions[i][1] - mesh.bboxMin[1]) / spanY : 0.0F;
```

`bboxMin[1]` is the *low* end of a vertical gradient. Index `1` is Y in every
one of these structures. **Grade: `VERIFIED` (code) / `INFERRED` (that it mirrors
the game's intent rather than OpenSpore's own choice).**

**(c) Measured over the corpus: Y is the tallest axis of a record's own authored
bounding box, per model family.** Counting a record only when *every* one of its
meshes extracted cleanly (`crates/spore-gmdl` mesh extraction):

| gmdl group (canonical name) | records | Y is the **tallest** axis | Y is the **shortest** axis |
|---|---:|---:|---:|
| `0x40616201` CellImages | 44 | **38** | 0 |
| `0x40616202` | 44 | **38** | 0 |
| `0x40617100` | 44 | **38** | 0 |
| `0x40626202` CreatureModels | 97 | 26 | 36 |
| `0x40627100` CreatureModelsHQ | 97 | 24 | 38 |
| `0x40636200` BuildingModels | 164 | 13 | 65 |
| `0x40637e03` (group unnamed) | 164 | 13 | 34 |
| `0x40646201` VehicleModels | 285 | **181** | 16 |
| `0x40656201` UfoModels | 91 | **55** | 4 |
| `0x40666201` FloraModels | 153 | 47 | 37 |

Read honestly, this splits into two families and the split is the interesting part.
The number to read is not "tallest" but **"extreme"** — Y being either the
tallest or the shortest axis means Y is the axis the model's proportions are
organised *around*, which is what an up/down axis looks like:

| family | Y is the **extreme** axis (tallest or shortest) | of records |
|---|---:|---:|
| `0x40616201` CellImages | 38 + 0 | **44** |
| `0x40626202` CreatureModels | 26 + 36 | **97** |
| `0x40627100` CreatureModelsHQ | 24 + 38 | **97** |
| `0x40646201` VehicleModels | 181 + 16 | **285** |
| `0x40656201` UfoModels | 55 + 4 | **91** |
| `0x40636200` BuildingModels | 13 + 65 | **164** |
| `0x40666201` FloraModels | 47 + 37 | **153** |

* **Creature-family records (cells, creatures, vehicles, UFOs): Y is the extreme
  axis on 63–86 % of records**, and for CellImages it is *never* the shortest and
  always the tallest (38 of 44). The 35–39 % remainder is not disagreement — it
  is records where Y is the *middle* axis, which a near-isotropic blob would also
  produce. Combined with (a) and (b) this is **+Y up**, with front/back along Z
  and left/right along X (§2.4). **Grade: `VERIFIED`.**
* **Flora records are much weaker evidence**, at 55 % extreme, and should not be
  used to support the convention.
* **Building and structure records are weak evidence either way** — `0x40636200`
  BuildingModels reports Y shortest on 65 of 164 records and tallest on 13, for
  78 extreme of 164. Their mean spans are `(18.90, 18.44, 29.11)`: a near-square
  footprint, Z longest. A building model that is 19 units wide and 18 tall is a
  *scenery prop*, not a standing creature. This does **not** undermine +Y up; it
  means **"tall along Y" is a property of creature-shaped records, not a
  validation rule a creator may apply to any record.**
  **Grade: `VERIFIED` (the measurement) / `INFERRED` (the reading).**

### 2.3 Unit scale: 1 unit is metre-like, and the numbers that say so

There is no unit field anywhere in a gmdl record, so "1 unit = 1 m" is an
inference from magnitudes. What *is* measured is the distribution of record
extents per family (mean of `max(bboxMax − bboxMin) / 2` over records whose every
mesh extracted):

| family | mean max half-extent (units) |
|---|---:|
| `0x40616201` / `0x40616202` CellImages | 0.618 / 0.592 |
| `0x40617100` | 0.626 |
| `0x40626202` / `0x40626203` CreatureModels | 0.883 / 0.858 |
| `0x40627100` / `0x40627101` CreatureModelsHQ | 0.906 / 0.905 |
| `0x406b6200` PaletteModels | 0.949 |
| `0x40646201` VehicleModels | 2.627 |
| `0x40656201` UfoModels | 2.851 |
| `0x40666201` / `0x40667100` FloraModels | 4.621 / 4.628 |
| `0x40637e0x` (group unnamed in this build's group table) | 9.970 (`…01`) / 9.688 (`…03`) |
| `0x40636200` / `0x40636201` BuildingModels | 14.977 / 14.592 |

A cell organ at ~0.6, a creature part at ~0.9, a vehicle at ~2.6, a UFO at ~2.9,
a plant at ~4.6, a structure at ~10 and a building prop at ~15. That ladder is
**consistent with metres** and inconsistent with centimetres (a 0.6 cm cell) or
with decimetres (a 0.6 dm cell would be 6 cm). Corroboration, weak but real: the
C++ world-object decoder's fitted plausibility window for the placement vectors
inside `0x0F43029A` records is `|component| < 16` with at least one component
`≥ 0.001` (`src/assets/WorldObject.cpp:93`), and its size model was fitted at
`R² = 0.9997` over 1 365 records (`src/assets/WorldObject.hpp:16-17`). A window
of ±16 units on a part placement is a metre-scale window.

**Grade: `INFERRED`.** There is no unit registry, no authored scale constant and
no calibration table anywhere in this repository. A creator document must
**declare its own unit as "gmdl record units, treated as metres"** and must not
claim it is a metre.

The largest absolute coordinate component measured in any record's authored bbox
is **57.390457**. That is a hard upper bound a loader may use to reject nonsense
placements, and it is also why `crates/spore-gmdl/src/mesh.rs:588-611` pins the
`f32` radius overflow instead of widening to `f64`: the corpus does contain
records where the `f32` square overflows to infinity, and that behaviour is
reproduced rather than corrected.

### 2.4 Handedness is CCW-outward, measured without using the stored normals

The stored `NORMAL` attribute cannot be used to answer this question: it is
`UBYTE4` divided by 255, and `crates/spore-engine/src/convert.rs:13-34` and
`crates/spore-gmdl/src/mesh.rs:27-41` both document that the encoding is **not
understood** (`INFERRED`, "signed? scaled? unknown"). A winding measurement
against those bytes is meaningless — I ran it and it produced 1 319 355 triangles
"agreeing" and 438 416 "opposing", which is noise, not a finding, and is
deliberately **not** used as evidence anywhere below.

The valid measurement uses **geometry only**. For a closed, consistently wound
mesh the divergence-theorem volume `V = Σ dot(p0, cross(p1, p2)) / 6` is
non-zero with a consistent sign, independently of any normal attribute. Over the
first 400 gmdl records by index order:

```text
signed volume > 0: 543    signed volume < 0: 3    ~0 (open / planar): 4
```

543 of 546 meshes wind **positive**, and the three negative ones sit in
`0x40646203` / `0x40666203` where 4 further meshes in the same two groups came
out at ~0 — i.e. the exceptions are open shells or flipped islands, not a
different convention. Under the standard right-handed cross-product rule a
positive signed volume means the front faces point **outward** and the vertex
order is **counter-clockwise seen from outside**.

That is exactly the convention the C++ renderer declares:

```text
src/renderer/VulkanRenderer.cpp:722   raster.frontFace = VK_FRONT_FACE_COUNTER_CLOCKWISE;
src/renderer/VulkanRenderer.cpp:721   raster.cullMode  = VK_CULL_MODE_NONE;
```

**Grade: `VERIFIED`.** **Consequence for mirroring:** a negative determinant on a
part transform inverts this, so a mirrored part's winding disagrees with the
rasteriser's front-face setting. Any mirror implementation must either reverse
the index order or accept back-face shading. `cullMode = NONE` means the current
reference is insensitive to it; Bevy's default `Mesh` pipeline is **not**, and a
creator document must say which it assumes.

### 2.5 What the record's authored bbox is *not*

The record-level `bounds_min`/`bounds_max`/`radius` are **authored values, not a
tight bound on the geometry the record contains.** Measured over all 4 209
records with all meshes extracted:

* over the 4 090 records where **every** mesh extracted cleanly, the authored
  bbox **contains** the computed union of its meshes' bounding boxes on 2 869
  and **does not** on 1 221 — and the direction is always
  *outward*: the authored box is larger. Examples (`authored min` vs
  `computed min`, printed from the census):
  * `0x00e6bce5:0x40616201:0xa1c46df2` — authored `-0.643747`, computed `-0.625657` (Y)
  * `0x00e6bce5:0x40616202:0x6ed040f4` — authored `-0.5286`, computed `-0.3534` (Y)
  * `0x00e6bce5:0x40656201:0xbef6797b` — authored `-3.7708`, computed `-3.2789` (Y)
* the record's authored `radius` is **larger** than the geometry's own maximum
  vertex distance on some families by a wide margin: `0x40637e03` mean authored
  radius **17.389** against mean computed **11.833** (a factor of 1.47).

Concrete worst illustration, the record this repository uses as its
"documented asset":

```text
0x00e6bce5:0x40637e03:0x067a07f0   (1156 bytes, walks to the very end)
  record  bounds_min = [-6.509403, -3.387016,  0.081329]
  record  bounds_max = [ 3.245249,  6.451122, 11.816221]
  record  radius     = 13.773765
  mesh 0  bounds_min = [-2.070057,  0.139144,  0.081329]
  mesh 0  bounds_max = [-0.371524,  2.164293,  5.326164]
  mesh 0  radius (computed) = 2.936603
```

The record claims a box roughly 3× the size of the 32 vertices it contains.
Whatever the authored bbox is for — animation extents, a model-group envelope, a
conservative cull volume — it is **not** "the geometry's bounding box".

**Grade: `VERIFIED`. Consequence:** a creator document must **never** derive a
part's placement from the record's authored bbox or radius. If a document wants
a reference extent it must use `spore_gmdl::Mesh`'s *computed*
`bounds_min`/`bounds_max`, which is the quantity the engine already computes at
`crates/spore-engine/src/domain/model.rs:150-152`.

---

## 3. Pivot / origin semantics — measured, and it is not any of the four obvious answers

### 3.1 The four candidate answers, all measured over all 5 939 extracted meshes

The question "where is a part's origin?" has four plausible answers. Measured:

| candidate pivot | meshes where it holds |
|---|---:|
| mesh bounding-box **centre** at the origin | **0 / 5 939** |
| mesh bounding-box **min** at the origin | **0 / 5 939** |
| mesh **vertex centroid** at the origin | not counted exactly; mean &#124;centroid&#124; = **3.544543** and mean mesh box-centre distance = **3.845576**, so no mesh is anywhere near its own origin |
| record bounding-box centre at the origin | **0 / 4 209 records** |

So: **no.** The origin is none of the geometric centres. And it is not at the
origin-relative extremes either — the mean mesh box-centre distance from the
record origin is **3.845576** units and the mean vertex-centroid distance is
**3.544543** units.

The negative answers are as informative as a positive one. The vertex centroid
is **not** the box centre: 5 777 of 5 939 meshes have them more than 1 % of the
box diagonal apart. Examples:

```text
0x00e6bce5:0x40627101:0x067b0e97 mesh0: centroid=( 0.0001,-0.0430, 0.7243)
                                          boxcentre=(0.0000, 0.2678, 0.6692)   off/diag=0.1464
0x00e6bce5:0x40666203:0x8c9d0727 mesh0: centroid=(-0.0249, 0.0310, 2.6808)
                                          boxcentre=(0.0278, 0.0632, 2.0602)   off/diag=0.0919
0x00e6bce5:0x40666202:0xd78ebf78 mesh0: centroid=( 0.2178, 0.2105, 9.2942)
                                          boxcentre=(0.2028, 0.8909, 7.1340)   off/diag=0.0669
```

**So the answer is "it varies, and it is not derivable from geometry."**
**Grade: `VERIFIED`.**

### 3.2 …but it is not arbitrary: the record's own `radius` is measured from it

This is the decisive measurement. The record's authored `radius` is a single
float per record (`crates/spore-gmdl/src/parse.rs:591-598`). If it were computed
from the bounding-box centre it would match `max |v − boxCentre|`. If it were
computed from the record's local origin it would match `max |v|`. Measured over
876 records whose every mesh extracted (the first 900 gmdl records by index
order, minus those where at least one mesh failed to extract):

```text
r == max|v − boxCentre| :  3 records out of 876   (0.34 %)
r == max|v|  (origin)   : 672 records out of 876   (76.7 %)
```

Per family (first 900 records):

```text
group        rec   r==max|v|   r==max|v-centre|   mean r   mean max|v|  mean max|v-c|
0x40616201     6           1                 0   1.0073       0.7712       0.6868
0x40626202    18          16                 0   1.7625       1.7529       1.1261
0x40627100    14          13                 0   1.8359       1.8348       1.1499
0x40636200    32          32                 0  27.9702      27.9702      17.5584
0x40637e00    36          36                 0  17.6872      17.6872      13.1059
0x40637e03    40           3                 0  17.3891      11.8325      12.4484
0x40646201    70          67                 1   3.5460       3.5388       3.0122
0x40656201    18          17                 0   3.8424       3.8151       3.4339
0x40666201    34          33                 0   8.7227       8.7212       5.7036
0x40667100    24          24                 0   6.2413       6.2413       4.1264
0x406b6200     3           3                 0   3.0375       3.0375       1.9206
```

**Conclusion, and it is the one that decides the pivot question: the gmdl
record's local origin `(0,0,0)` is a real, game-meaningful point — it is the
point the record's own bounding radius is measured from.** It is a *socket-like*
attachment point baked into the exported vertex coordinates, and the geometry
hangs off it wherever the exporter put it. The 76.7 % figure rather than 100 % is
explained by §2.5: on the remaining records the authored radius is a conservative
envelope larger than the static geometry's extent about that same origin, so it
still satisfies `radius ≥ max|v|` but not `radius == max|v|`. Verified: on
`0x40637e03` the authored radius 17.389 exceeds `max|v|` 11.833 — the anchor is
still the origin, the radius is just loose.

**Grade: `VERIFIED`** (the anchoring) / **`INFERRED`** (that it is an attachment
socket, and the *reason* the radius is loose).

Worked example, the creature preset this engine renders by default:

```text
0x00e6bce5:0x40627100:0x067c79d2   (1 351 408 bytes, 2 meshes)
  record bounds_min = [-0.776000, -0.772762, -0.010258]
  record bounds_max = [ 0.776000,  0.441339,  2.098687]
  record box centre = [ 0.000000, -0.165712,  1.044214]
  record radius     = 2.149050
```

The origin sits at the extreme front (z ≈ 0 while the body runs to z = 2.1), at
**exactly** mid-width (x = ±0.776, symmetric to 6 decimal places), and 0.773 units
above the lowest point. That is the shape of a **mouth/base socket at the front
centreline**, not a centroid and not a bounds centre.

### 3.3 Left/right symmetry of the origin, measured per family

The `|mid_x| / half_width` of the record's authored bbox says where the origin
sits left/right. Over all 4 209 records:

```text
group        rec   |mid| < 1% of half-width   mean |mid|/half
0x40626202 CreatureModels    97            94                    0.000386
0x40626203                   97            92                    0.000294
0x40627100 CreatureModelsHQ  97            93                    0.000260
0x40627101                   97            93                    0.000265
0x40646201 VehicleModels    285           236                    0.001020
0x40656201 UfoModels         91            84                    0.000512
0x41626203                   97            91                    0.000365
0x40616201 CellImages        44            23                    0.004520
0x40636200 BuildingModels   164            14                    0.061666
0x40637e03 (unnamed)       164            12                    0.242143
0x40666201 FloraModels      153             2                    0.076233
0x40667100                  153             0                    0.076013
0x406b6200 PaletteModels     67             0                    0.947358
```

**Creature, vehicle and UFO records place their origin on the model's own
left/right mirror plane to within 0.1 % of the model's half-width** — a
statement about *authored data*, measured, that is exactly as strong as the
corpus allows. Building, unnamed-group, flora and palette models are not symmetric
and their origins sit well off centre.

**Grade: `VERIFIED`** (the measurement) / **`INFERRED`** (that this is a
deliberate left/right convention rather than an artefact of the exporter).

**This is the answer to "does a creator need an explicit pivot field?" — no, not
a *derived* one.** The pivot is already the record's local origin; it is not
recoverable from geometry, so it must be **carried, never recomputed**. What a
creator document *may* need is an authored **offset** applied *in the record's
own frame*, which is a different thing (§4.4).

---

## 4. Transform composition — what the corpus forces, and what is speculation

### 4.1 What the game's own authored part data looks like

Four structures from two independent provenance sources are listed. Fields and
offsets are `CONFIRMED` in
`knowledgegraph/research/data-model/04-creature-data.json`
(`SCHEMA-CREATURE-BLOCK`, `SCHEMA-CREATURE-RIGBLOCK`) and
`knowledgegraph/research/data-model/07-editor-data.json` (`EditorRigblock`,
`cEditorResourceBlock`), both citing `E-GHIDRA` **and** `E-SDK`:

| structure | position | orientation | scale |
|---|---|---|---|
| `Editors::cCreatureDataResource::RigblockData` (140 B) | `+0x54 Vector3` (12 B) | `+0x30 Matrix3` (**36 B** = 0x30→0x54) | `+0x2C float`, `+0x6C mScaleRelative float` |
| `Editors::cEditorResourceBlock` (472 B) | `+0x14 Vector3` (12 B) | `+0x38 Matrix3` (**36 B** = 0x38→0x5C) + `+0x5C userOrientation Matrix3` (**36 B**) | `+0x10 float` |
| `Editors::EditorRigblock` (3592 B, runtime editor object) | `+0x48 mPosition Vector3` | `+0x60 mTotalOrientation`, `+0xA8 mOrientation`, `+0xF0 mUserOrientation` — all `Matrix3` | `+0x454 mModelScale float`, `+0x1E0 mModelMinScale`, `+0x1E4 mModelMaxScale` |
| `Anim::CreatureBlock` (1128 B, runtime assembly) | `+0x144 position Vector3` | `+0x118 orientation` — **width not determined** (next listed field is `+0x144`, so the span is 44 B) | `+0x378 scale` (8 B) |
| `Anim::AnimatedCreature` | `+0x04 mPosition Vector3` | `+0x10 mOrientation` (type given as `Quaternion`) | — |
| cell-stage player entity | `+0x08/0c/0x10 position` | `+0x14/0x18/0x1c/0x20` = `orient_qx..qw` (**quaternion**, 16 B) | — |

The orientation field spans are **byte-exact 36-byte gaps** in two structures
where the offsets abut: `RigblockData +0x30 → +0x54` and `cEditorResourceBlock
+0x38 → +0x5C` (and `+0x5C → +0x80` for `userOrientation`, giving a *second*
independent 36-byte gap in the same struct). That is arithmetic, not inference:
`Matrix3` is 9 floats and there is no translation inside it, because the position
field sits separately as 12 bytes.

`EditorRigblock` is listed for the third vote but **not** as the source of the
36-byte figure: its field list is explicitly partial (*"relevant fields only"*,
`knowledgegraph/research/data-model/07-editor-data.json`), so `+0x60 → +0xA8` is
a 72-byte span that accommodates two 36-byte matrices and bounds nothing.
`Anim::CreatureBlock`'s orientation width is likewise undetermined — `+0x118` to
`+0x144` is 44 bytes, which does not fit 9 floats cleanly and is why this table
does **not** claim `CreatureBlock` stores a 3×3.

**Conclusion, and it is the single most important design fact in this document:**

> **The corpus's authored part transform is `(Vector3, Matrix3, float)`.**
> Uniform scalar scale. Full 3×3 orientation. Position in the parent's frame.
> **Grade: `CONFIRMED`.**

And a second, separate conclusion:

> **The corpus's *runtime* orientation is a quaternion** (`AnimatedCreature`,
> the cell-stage player entity, and the 64-comparison table in
> `docs/REPLACEMENT-DIFF.md` whose `orient_qx/qy/qz/qw` columns are all
> populated). **Grade: `CONFIRMED` for the field, `INFERRED` for the meaning**
> (`docs/REPLACEMENT-ABI.md` §3 marks the player's field offsets VERIFIED and the
> quaternion *layout* INFERRED).

So the game itself uses **two different rotation representations on the two sides
of the save/load boundary**, and a creator document must pick one and say which.

### 4.2 Option-by-option, graded against the corpus

| representation | corpus support | grade | verdict |
|---|---|---|---|
| **uniform scalar scale** | `+0x2C float`, `+0x10 float`, `+0x454 float` — three structures, all one float | `CONFIRMED` | **required**; `Placement::scale` already matches |
| **non-uniform scale** | no per-axis scale field anywhere in the three authored structures | `INFERRED` (absent) | **speculative.** Nothing in the corpus forces it. Optional; must be flagged `INFERRED` if added |
| **3×3 rotation matrix** | the authored representation, byte-exact 36 B in the two structures whose offsets abut (§4.1) | `CONFIRMED` | the corpus-faithful choice for a *document* |
| **quaternion** | the runtime representation in two structures | `CONFIRMED` | faithful for *runtime state*; lossy as a document (§6.2) |
| **axis-angle** | nothing in the corpus | — | **speculative**; expresses fewer rotations than a matrix at equal size |
| **Euler XYZ** | nothing in the corpus; `EditorCreatureController +0x44 mTargetAngle float` + `+0x48 mCurrentAngle float` is a **single** angle, i.e. a 1-DOF yaw controller (`knowledgegraph/research/data-model/07-editor-data.json`) | `INFERRED` | **speculative and worse**: gimbals, and no corpus counterpart |
| **parent/child chain** | `RigblockData +0x04 mParentIndex`, `+0x06 mSymmetricIndex`; `cEditorResourceBlock +0x08 parentIndex`, `+0x0C symmetricIndex`; `EditorRigblock +0x33C mpParent`, `+0x340 mChildren` (fixed_vector, capacity **8**) | `CONFIRMED` | **required**; the chain exists in the corpus and is index-based, not pointer-based |
| **negative / mirroring scale** | see §5 — `UNKNOWN` as a transform | `UNKNOWN` | **speculative as a transform.** Authored symmetry is an index, not a scale sign |
| **3-axis free rotation** | the matrix orientation permits it; nothing in the corpus *requires* more than a yaw controller | `INFERRED` | permitted; needed for limb attachment |

### 4.3 Composition order — measured where possible, stated where not

Bevy's `Transform` composes as `parent_matrix * child_matrix` and this engine
relies on that already: `crates/spore-engine/src/loader.rs:592-609` spawns one
parent per object carrying the placement, and `loader.rs:673-696` parents every
mesh leaf with `Transform::default()` so a creature's parts share one frame
(`docs/ENGINE-SCENE.md`, "Ownership"). The leaf being identity is what makes the
parent's frame the object's frame.

The corpus's own composition is **parent-relative**, and there is direct evidence
for *accumulated* orientation being a distinct stored field rather than a
recomputation: `RigblockData +0x30 mTotalOrientation` ("accumulated orientation")
coexists with `cEditorResourceBlock +0x38 orientation` and `+0x5C userOrientation`
("authored orientation"), and `EditorRigblock` carries all three
(`mTotalOrientation`, `mOrientation`, `mUserOrientation`). So the game stores
**authored local orientation** *and* **accumulated world orientation** side by
side.

**Which multiply order produces `mTotalOrientation` from `orientation` and
`parentIndex` is `UNKNOWN`.** `knowledgegraph/research/data-model/07-editor-data.json`
gap **G-04** states it outright: *"Exact conversion among EditorRigblock,
cEditorResourceBlock, and cCreatureDataResource::RigblockData is unproved."*
A creator document must therefore **store local transforms and compose at use
time in a declared order**, and must not try to reproduce `mTotalOrientation`.

### 4.4 The minimal honest part transform

Graded summary of what a part under a parent needs, and what is speculative:

| capability | forced by corpus? | grade |
|---|---|---|
| translation relative to the parent, 3 components | yes (`Vector3` position ×3) | `CONFIRMED` |
| uniform scalar scale | yes (`float` scale ×3) | `CONFIRMED` |
| full 3-DOF rotation | permitted; a matrix permits it, a yaw-only angle does not | `CONFIRMED` (representable) / `INFERRED` (required) |
| parent index into a fixed-capacity child list | yes | `CONFIRMED` |
| symmetry-partner index | yes | `CONFIRMED` |
| authored pivot offset in the part's own frame | **no** — the pivot is the record origin (§3) | `INFERRED`; must be `Option`, never a fabricated `0.0` |
| non-uniform scale | no | `INFERRED` |
| negative scale / mirror transform | no | `UNKNOWN` |
| submesh selection | `Anim::CreatureBlock +0x33C submeshIndex` exists (`CONFIRMED`) | belongs to the part model, **not** to the transform |

The pivot row deserves the emphasis: **the record's local origin is the pivot and
it is already in the data.** A document therefore needs no *pivot* field. It may
need an *offset* field for the case where a creator wants to attach a part at a
different point than the exporter chose, and that field must be `None`-able,
because a zero written into an offset field is indistinguishable from "the
creator measured zero" — exactly the collapse `AGENTS.md` forbids.

---

## 5. Symmetry and mirroring

### 5.1 What I searched, exactly

* `rg -i "mirror|symmetr|negative scale|winding|handed"` across `docs/`, `crates/`, `reconstruction/`, `knowledgegraph/`, `tools/`, excluding `knowledge/semantic/`.
* `knowledgegraph/research/data-model/04-creature-data.json` — every schema, every field.
* `knowledgegraph/research/data-model/07-editor-data.json` — every schema, every field, plus `gaps` and `contradictions`.
* `knowledgegraph/research/types/06-editor-ui.md`.
* `reconstruction/staging/wave13-w1-dispatch-b05/editor_rigblock_00451e50.{hpp,cpp}` and `wave13-w1-dispatch-b01/{00573d70_set_primary_part,00438700_cascade,opaque_types}.hpp`.
* The gmdl format itself: `crates/spore-gmdl/src/{parse,mesh,claims}.rs`, `src/assets/Gmdl.hpp`, `src/assets/Mesh.cpp`, `src/assets/WorldObject.{hpp,cpp}`.
* Corpus measurements: the X mirror-symmetry census (§3.3), the winding census (§2.4), the record-bbox/radius census (§2.5, §3.2).
* The gmdl **trailer**'s two "baked transforms, 64 bytes each" (`crates/spore-gmdl/src/parse.rs:82-83`, `941-964`) — brute-force searched for rotation matrices (§5.4).

### 5.2 What exists: symmetry is an *index*, not a mirror flag

Symmetry is real, pervasive and consistently modelled as a **partner
reference**, never as a sign flip or a negative scale:

| structure | field | grade |
|---|---|---|
| `Editors::cCreatureDataResource::RigblockData` | `+0x06 mSymmetricIndex : int16` — "symmetry partner" | `CONFIRMED` |
| `Anim::CreatureBlock` | `+0x200 symmetricIndex : int` — "symmetry partner" | `CONFIRMED` |
| `Editors::cEditorResourceBlock` | `+0x0C symmetricIndex : int` **and** `+0x81 isAsymmetric : bool` | `CONFIRMED` |
| `Editors::EditorRigblock` | `+0x3E0 mpSymmetricRigblock`, `+0x3E4 mpAsymmetricRigblock` — **two** partner pointers | `CONFIRMED` |
| `Editors::cEditor` | `+0x2F5 mbAllowAsymmetry : bool` — "editor policy" | `CONFIRMED` |
| `Editors::EditorRigblock` | `+0x220 mModelSymmetrySnapDelta : float`, `+0x22C mModelSymmetryRotationSnapAngle : float` — authored snap policy | `CONFIRMED` |

Independently corroborated by a reconstructed package whose header cites the SDK
field names directly:

```text
reconstruction/staging/wave13-w1-dispatch-b05/editor_rigblock_00451e50.hpp:22
    /* 33Ch */ EditorRigblockPtr mpParent;
    /* 3E0h */ EditorRigblockPtr mpSymmetricRigblock;
```

Note that `EditorRigblock` has **two** partner pointers, `mpSymmetricRigblock`
*and* `mpAsymmetricRigblock`. That is the shape of a left/right **and** a
front/back partner, i.e. the corpus's own model of "which mirror plane" is
**two distinct references**, not one boolean. Combined with `mModelSymmetrySnapDelta`
(a `float`) and `mModelSymmetryRotationSnapAngle` (a `float`), the picture is:
*authored data records which partners exist and a snap tolerance for applying
them.*

**Grade: `CONFIRMED`.**

### 5.3 What does not exist: any evidence for mirroring as a transform

Searched and **not found**, in any structure, field or record type in this
repository:

* a negative scale, a flip flag, a `mirror` matrix, or a negated determinant on
  any authored part transform;
* a record type or field encoding "reflect this part about plane P";
* any per-axis scale (which a mirror could have been expressed as, and was not).

**Grade: `UNKNOWN`** — and per `AGENTS.md`, `UNKNOWN` must be preserved, not
filled in.

### 5.4 The gmdl trailer does not settle it either — a measured negative

The gmdl trailer documents "two baked transforms (64 bytes each)"
(`crates/spore-gmdl/src/parse.rs:82-83`), which would be a natural place to find
scale. I brute-force searched **every byte offset** of 60 records whose trailer
walked completely, testing four candidate readings of a 16-float window as a 3×3
matrix (rows-unit + row-dot-zero within 1e-3):

```text
records with a COMPLETE trailer:                60
records containing ANY orthonormal 3x3 window:   4
total (offset, reading) orthonormal hits:       72
determinant > 0: 144    determinant < 0: 0
top stride gaps between consecutive hits: [(4,36), (156,18), (452,6), (52,2), ...]
```

Only 4 of 60 records contain anything orthonormal anywhere, the stride gaps are
irregular (4, 156, 452, 52, 100, 556, 1764), and **zero** of the 588 blocks I
could locate by the documented framing are orthonormal (`0 / 588`; the framing
yields row lengths like `(0.00000, 1.41421, 1.00000)` and determinant `0.000000`,
which is not a rotation matrix). This is exactly what
`crates/spore-gmdl/src/parse.rs:37-51` predicts: the real cell-stage records
carry a larger opaque baked-deform trailer with a different framing that overruns
the known walk.

**Grade: `UNKNOWN` / non-finding, and it is recorded as such. No transform
evidence is drawn from the gmdl trailer.**

### 5.5 What a creator document must therefore store, to stay honest

The corpus gives symmetry as a *relationship between two parts*, never as a
transform. So a creator document should mirror that shape and must be explicit
that it is authoring, not recovery:

1. **`symmetric_part: Option<PartId>`** — a partner *reference*, mirroring
   `mSymmetricIndex`. `CONFIRMED` that this is the corpus's shape.
2. **`is_asymmetric: bool`** or the absence of (1) — mirroring
   `cEditorResourceBlock +0x81 isAsymmetric`. `CONFIRMED` as a corpus field.
3. **A mirror *operation*, stored as an explicit authored step, never as a
   negative scale smuggled into the scale field.** If a creator mirrors a part,
   the document records `mirror: Option<MirrorStep { plane: Axis }>` with a doc
   comment stating: *this is authored OpenSpore data; it is not a claim about how
   the game performs mirroring, which is `UNKNOWN` (§5.3).*
4. **`None`, never a fabricated value**, for every field a creator has not set.
   A `mirror: None` means "not authored"; it must never be read as `false`
   meaning "the game does not mirror".

The negative determinant consequence must be documented where the mirror is
applied: §2.4 measured CCW-outward winding corpus-wide, so a mirror inverts it,
and Bevy's default mesh pipeline back-face-culls. `src/renderer/VulkanRenderer.cpp:721`
sets `cullMode = NONE` and is therefore insensitive; a Bevy default is not.

---

## 6. Rotation units, precision, and determinism

### 6.1 Degrees vs radians

The corpus is silent on the question — no rotation unit is recorded in any
gmdl byte and no part-transform field carries a documented unit. What is
established:

* `Placement` today stores **degrees**: `crates/spore-engine/src/domain/model.rs:198-199`
  (`rotation_y_degrees: f32`), and the single conversion site is
  `crates/spore-engine/src/loader.rs:600`
  (`.rotation_y_degrees.to_radians()`).
* The C++ reference hard-codes degrees→radians as a named constant:
  `src/apps/cell_stage.cpp:763` `const float kDeg = 3.14159265358979F / 180.0F;`
  and the camera FOV is written in degrees throughout
  (`crates/spore-engine/src/loader.rs:57` `DEFAULT_VERTICAL_FOV_DEGREES: f32 = 60.0`).

**Recommendation, `INFERRED` but low-risk and consistent with the codebase: keep
degrees as the authored unit.** Do not switch to radians. The only cost is the
conversion at one call site, and the benefit is a document whose numbers are
readable and directly comparable to the C++ reference's.

**Grade: `INFERRED`** (no corpus evidence either way; consistency argument only).

### 6.2 Measured precision, and the trap it creates

Measured over a 0.01° grid spanning `0.00 .. 360.00` in `f32`:

```text
worst |deg − deg.to_radians().to_degrees()| = 1.5259e-05 deg   (at 210.3000030517578 deg)
```

Individual values, so the error is not uniformly distributed:

```text
   0.100000 deg -> 0.0017453292 rad -> 0.1000000015 deg    delta 0
  15.000000 deg -> 0.2617993951 rad -> 15.0000009537 deg    delta 9.537e-07
  45.000000 deg -> 0.7853981853 rad -> 45.0000000000 deg    delta 0
  90.000000 deg -> 1.5707963705 rad -> 90.0000000000 deg    delta 0
 180.000000 deg -> 3.1415927410 rad -> 180.0000000000 deg    delta 0
 359.900000 deg -> 6.2814397812 rad -> 359.8999938965 deg    delta 0
```

Quaternion → 3×3, 200 000 deterministic samples (LCG-seeded, so the numbers are
reproducible), from an `f32` quaternion normalised in `f32`:

```text
worst |row length − 1| = 5.643e-07
worst |det − 1|        = 1.131e-06
```

**The trap this creates, and it is exactly the repo's existing 1e-6 tolerance.**
`docs/REPLACEMENT-DIFF.md:29` states the differential harness compares floats
"with tolerance `1e-6` (float32)". A quaternion↔matrix round trip lands at
`5.6e-7 .. 1.1e-6` — i.e. **at or above the tolerance the repository already
uses for "these two things agree"**. A creator document that stores a quaternion
and is compared through a matrix will produce spurious mismatches, and one that
stores a matrix and is compared through a quaternion likewise.

I also measured a related asymmetry, because it is a compiler behaviour and not
obvious. On `rustc 1.98.1` / LLVM, in **both** `dev` and `release`:

```text
-72.0f32 .to_degrees()            = -4125.296386719
-72.0f32 .to_degrees().to_radians() =  -72.000007629   (NOT the identity; err 7.6e-06 deg)
-72.0f32 .to_radians().to_degrees().to_radians() = -1.256637096  (bit-exact)
```

So a "normalise the degrees" step that round-trips in one direction silently
perturbs the value while the other direction is free. **This is a property of
this toolchain and must be re-measured, not assumed, on any other** — a different
LLVM may fold either direction. **Grade: `VERIFIED` on this toolchain, and
`UNKNOWN` in general.**

### 6.3 What a creator document must guarantee for deterministic reconstruction

The repository's determinism requirement is byte-level, and it is already stated
twice: `crates/spore-tools/src/commands/manifest.rs:10` — *"a double run is
byte-identical"* — and `src/sim/Sim.hpp:249` — *"Deterministic text dump of the
full state (tests compare two runs)"*. A creator document is a serialised
artifact and is held to the same standard. Concretely, it must guarantee:

1. **Exactly one rotation representation is populated at a time.** The
   recommended `Rotation` (§7.2) carries `degrees: [f32; 3]` and
   `matrix: Option<[[f32; 3]; 3]>` because `Option` is what makes the invariant
   checkable: `matrix: None` means the degrees are authoritative, and
   `matrix: Some(..)` means the **degrees are a placeholder and must not be
   read**. Measured, a matrix-only rotation carrying placeholder
   `degrees = [0,0,0]` *silently* disagrees with itself — a consumer reading
   `degrees` gets the identity and produces a wrong transform with no error.
   Populating both fields with two different answers is the defect the `Option`
   exists to make impossible. If a derived form is carried for interchange it
   must be labelled derived and must never be read as authoritative.
2. **Degrees are the authored value; radians are computed at the point of use**
   and are never written back. The 1.5e-5 deg round-trip error means a document
   whose numbers came from a rendered matrix is not the document that was
   authored.
3. **Round trips must be bit-exact or declared lossy.** The three candidate
   round trips and their measured status: `f32` degrees → radians → degrees,
   **lossy at ≤ 1.5259e-05 deg**; quaternion → matrix, **lossy at ≤ 1.131e-06**;
   `f32` matrix → quaternion → matrix, not measured here and therefore not
   claimed.
4. **Field order in the serialised form is fixed and explicit.** A matrix has
   nine floats and two plausible layouts (row-major vs column-major); the two
   differ by a transpose and produce a *plausible wrong* rotation. This
   repository already has the matching trap in a different form —
   `AGENTS.md` on `refCount` endianness, and
   `crates/spore-engine/src/convert.rs:146-163` on index buffers. State the
   layout in the type's own documentation, in the same sentence as the type.
5. **No derived value is ever stored as if it were authored.** Anything computed
   — a composed world matrix, a total orientation, a normalised scale — is
   recomputed on load. `RigblockData` stores `mTotalOrientation` *alongside* its
   local `orientation` (`CONFIRMED`), which is evidence that the game makes the
   same distinction and evidence that both must be written by whoever owns them.
6. **A near-zero is not a zero, and the writer must say which it emits.**
   Measured: the Y-rotation matrix this workspace builds for exactly 90° is
   `[[-4.371139e-8, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, -4.371139e-8]]`, i.e.
   **`f32` `cos(90°)` is `-4.37e-8`, not zero**. Any normalisation ("snap
   near-zero to zero") therefore *changes bytes* and must be an explicit,
   documented step in the writer rather than something a formatter does on the
   way out. **Grade: `VERIFIED` for the measurement; `INFERRED` for the policy.**
   The related `-0.0` / `0.0` question — whether a serialiser must fold them —
   was **not** measured here and is therefore **not** claimed. It is a one-line
   decision the document owner should make explicitly rather than inherit from a
   formatter's defaults.

**On quantisation:** the corpus does **not** require it. The C++ reference
compares floats at `1e-6` and does not quantise
(`docs/REPLACEMENT-DIFF.md:29`); the game's own authored fields are plain `f32`
and plain `float`. Quantising to a fixed grid would be an invention with no
evidence behind it. **Grade: `INFERRED` that quantisation is not required; the
determinism guarantee should come from storing authored `f32` verbatim and
recomputing derived values, not from rounding.**

---

## 7. The compatibility seam

### 7.1 The constraint, measured from the call sites

Every construction and every read of `Placement` in `spore-engine` today:

| site | shape | constraint |
|---|---|---|
| `crates/spore-engine/src/domain/model.rs:194-202` | `#[derive(Debug, Clone, Copy, PartialEq)] pub struct Placement` | must stay `Copy` — `crates/spore-engine/src/domain/scene.rs:223` copies `instance.placement` out of a shared reference |
| `domain/model.rs:204-212` | hand-written `impl Default` | new fields need `Default`, and `Default` must remain **identity** |
| `domain/model.rs:216-221` | `Placement::at` uses `..Self::default()` | struct-update syntax must keep working |
| `crates/spore-engine/src/cli.rs:183,192,200` | `Placement { scale: s, ..Placement::default() }` | struct-update syntax |
| `crates/spore-engine/tests/material_wiring.rs:427-430` | `Placement { translation: …, ..Placement::default() }` | struct-update syntax |
| `domain/scene.rs:224-228` | writes `placement.translation` after copying | `translation` must keep its name and `[f32;3]` type |
| `crates/spore-engine/src/loader.rs:99-100` | reads `instance.placement.translation`, `.scale` | both names/types fixed |
| `crates/spore-engine/src/loader.rs:598-602` | reads `.translation`, `.rotation_y_degrees.to_radians()`, `.scale` | all three fixed |

`Copy` rules out any `Vec`/`String` in the new fields, so the full rotation must
be fixed-size: `[f32; 3]` (Euler) or `[[f32; 3]; 3]` (matrix) or `[f32; 4]`
(quaternion). All three are `Copy`.

### 7.2 The recommendation — additive fields, nothing removed or renamed

I compiled and ran this exact shape outside the repository against **every**
construction and read pattern listed in §7.1. Verbatim construction sites from
`cli.rs` and `material_wiring.rs`, the copy-and-add in `scene.rs:223-229`, and
the three reads in `loader.rs` all compile and behave identically, with
`Placement::default()` still exactly identity.

```rust
// crates/spore-engine/src/domain/model.rs  -- ADDITIVE ONLY. No existing field
// is renamed, retyped or removed; every existing construction site keeps
// compiling because each one either uses `Placement::default()` or
// `..Placement::default()`.

/// A rotation, in ONE authoritative representation.
///
/// `degrees` is the authored value (matching `rotation_y_degrees`, which stays
/// for compatibility). `matrix` is `None` unless a caller supplied a 3x3
/// DIRECTLY; when it is `Some` the `degrees` field is a placeholder and MUST
/// NOT be read -- measured, a matrix-only rotation carrying degrees [0,0,0]
/// agrees with nobody and produces a silently wrong transform. See
/// docs/analysis/creator/03-transforms.md §6.2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation {
    /// Euler angles in DEGREES, applied about X, then Y, then Z (extrinsic XYZ).
    pub degrees: [f32; 3],
    /// Row-major 3x3, column vectors. `None` = "degrees are authoritative";
    /// `Some` = "the matrix is authoritative and `degrees` is a PLACEHOLDER
    /// that must not be read" -- exactly one of the two is ever populated.
    pub matrix: Option<[[f32; 3]; 3]>,
}

impl Default for Rotation {
    fn default() -> Self { Self { degrees: [0.0; 3], matrix: None } }
}

impl Rotation {
    pub const fn about_y(degrees: f32) -> Self {
        Self { degrees: [0.0, degrees, 0.0], matrix: None }
    }
    pub fn from_matrix(m: [[f32; 3]; 3]) -> Self {
        Self { degrees: [0.0; 3], matrix: Some(m) }
    }
}

/// A pivot adjustment, in the record's own local frame.
///
/// The record's local origin IS the pivot -- VERIFIED over 5 939 meshes: 0 are
/// centred on it, 0 have their bounds min on it, 0 have their vertex centroid
/// on it, and the record's own authored `radius` is measured from it (672 of
/// 876 records match `max|v|` exactly; only 3 match the box centre instead).
/// See docs/analysis/creator/03-transforms.md §3.2. So `None` means "the record's
/// own origin", which is the honest default; a stored `Some([0.0;3])` means the
/// creator measured a zero offset, which is a DIFFERENT claim.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pivot { pub offset: Option<[f32; 3]> }

impl Default for Pivot { fn default() -> Self { Self { offset: None } } }

/// Which axis a mirror step reflects about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis { X, Y, Z }

/// An AUTHORED mirror operation.
///
/// This is not a claim about how the game mirrors anything: the corpus records
/// symmetry as a PARTNER INDEX (`mSymmetricIndex`, `mpSymmetricRigblock`), never
/// as a transform, and no negative scale, flip flag or negated determinant was
/// found anywhere. See docs/analysis/creator/03-transforms.md §5.3.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Mirror {
    pub plane: Option<Axis>,
    pub offset: Option<[f32; 3]>,
}
```

and the additive change to `Placement` itself:

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    // ---- unchanged: every existing call site reads exactly these ----
    pub translation: [f32; 3],
    pub rotation_y_degrees: f32,
    pub scale: f32,
    // ---- added: all defaulted, all Copy, all ignored unless set ----
    pub rotation: Rotation,
    pub pivot: Pivot,
    pub mirror: Mirror,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation_y_degrees: 0.0,
            scale: 1.0,
            rotation: Rotation::default(),
            pivot: Pivot::default(),
            mirror: Mirror::default(),
        }
    }
}
```

`Default` stays identity **in the full form too** — I asserted
`default().rotation_matrix() == [[1,0,0],[0,1,0],[0,0,1]]` — which is the
property that keeps every existing expected value in every existing test true.

### 7.3 The projections

```rust
impl Placement {
    /// Whether this placement is expressible in the three legacy fields alone.
    /// `false` is a reportable fact, not an error.
    pub fn is_legacy_expressible(&self) -> bool {
        self.rotation.matrix.is_none()
            && self.rotation.degrees[0] == 0.0
            && self.rotation.degrees[2] == 0.0
            && self.pivot.offset.unwrap_or([0.0; 3]) == [0.0; 3]
            && self.mirror.plane.is_none()
    }

    /// The Y-only rotation this projects to. Exactly `rotation.degrees[1]` when
    /// the added fields are default or Y-only; otherwise it reports the legacy
    /// field and the caller must use the full form.
    pub fn effective_rotation_y_degrees(&self) -> f32 { /* ... */ }

    /// Row-major 3x3 for the FULL transform. Falls back to the legacy Y-only
    /// path when `degrees[0] == degrees[2] == 0`, which is bit-identical to
    /// what loader.rs computes today.
    pub fn rotation_matrix(&self) -> [[f32; 3]; 3] { /* ... */ }
}
```

Then the projection into the existing type. `loader.rs:598-602` becomes:

```rust
Transform::from_translation(Vec3::from(instance.placement.translation))
    .with_rotation(Quat::from_rotation_y(
        instance.placement.rotation_y_degrees.to_radians()))
    .with_scale(Vec3::splat(instance.placement.scale))
```

**unchanged**, and a second construction for the full form:

```rust
let m = Mat3::from_cols_array(&placement.rotation_matrix().flat_map(|r| r).collect::<Vec<_>>());
let q = Quat::from_mat3(&m);
let s = placement.pivot.offset.map_or(1.0, |o| /* pivot scale policy */);
Transform::from_translation(...).with_rotation(q).with_scale(Vec3::splat(placement.scale * s))
```

Verified equalities I checked:

* `Placement { rotation: Rotation::about_y(90.0), rotation_y_degrees: 90.0, ..default() }`
  is `is_legacy_expressible() == true` and `effective_rotation_y_degrees() == 90.0`.
* That same placement's `rotation_matrix()` is
  `[[-4.371139e-8, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, -4.371139e-8]]` — the
  exact shape `loader.rs` produces today for a Y-only rotation, including the
  `f32` `cos(90°) = -4.37e-8` that a naive identity assertion would miss.
* `Placement { rotation: Rotation { degrees: [30.0, 0.0, 0.0], matrix: None }, ..default() }`
  is `is_legacy_expressible() == false` and reports it, rather than silently
  dropping the X rotation.

### 7.4 What the seam does **not** do

* It does **not** change `domain/scene.rs`'s `Layout`, which shifts
  `placement.translation` only (`domain/scene.rs:224-228`). A layout that
  rotates or scales an object must be a separate, explicit operation — the
  current one is translation-only by design and that is correct for it.
* It does **not** change `LoadedScene::bounds` (`loader.rs:93-110`), which
  multiplies bounds by `scale` and adds `translation` with **no rotation**. That
  is already an approximation for any rotated placement and would become a
  visible one. A full-transform creator must either widen it or frame the camera
  from the transformed corner points; either way it is a separate, stated change,
  not part of this seam.
* It does **not** add non-uniform scale. Absent from the corpus (`INFERRED`); add
  it when something needs it, and grade it as a new claim when it arrives.
* It does **not** model mirroring as negative scale. See §5.5.

---

## 8. Reproducing every number in this document

All measurements are from scratch binaries built outside the checkout, reading
`SPORE/` read-only, against the workspace crates by path:

```bash
# 4 209 gmdl records in Spore_Content.package; pivot + bbox census
txprobe  SPORE/Data/Spore_Content.package census
txprobe  SPORE/Data/Spore_Content.package one 0x00e6bce5:0x40627100:0x067c79d2

# per-group record-header geometry, radius anchoring, winding
probe2   SPORE/Data/Spore_Content.package pivot
probe2   SPORE/Data/Spore_Content.package winding 400
probe3   SPORE/Data/Spore_Content.package volume 400
probe3   SPORE/Data/Spore_Content.package radius 900

# trailer framing brute-force, degrees/radians precision, X symmetry, seam compile
probe3   SPORE/Data/Spore_Content.package scan 60
probe4   SPORE/Data/Spore_Content.package
probe5                      # no data; compiles and runs the proposed Placement
```

Corroborating runs inside the checkout, both read-only:

```text
$ ./target/release/osptool types --group --package SPORE/Data/Spore_Content.package
  0x40616200  CellModels      0x40626200  CreatureModels   0x40627100  CreatureModelsHQ
  0x40636200  BuildingModels  0x40646200  VehicleModels    0x40656200  UfoModels
  0x40666200  FloraModels     0x406b6200  PaletteModels

$ ./build/src/assets/worldobj_test SPORE/Data/Spore_Content.package \
                                  SPORE/DataEP1/Spore_EP1_Content_01.package
worldobj: ALL PASS (1365 records fully decoded)
```

---

## 9. Everything this document does not know

Stated as a list, because a creator document built on this must inherit these
gaps rather than paper over them.

1. **Why a gmdl record's authored `radius` exceeds the static geometry's extent
   on 204 of the 876 measured records.** The anchor is measured (the origin); the reason for the
   slack is not. Animation extents and model-group envelopes are both consistent
   with it and neither is established.
2. **Why 1 221 of the 4 090 fully-extracted records' authored bbox does not
   equal the union of their meshes' computed bbox.** Direction is measured (always outward); cause is not.
3. **The gmdl trailer's transform blocks.** Zero of 588 locatable 64-byte blocks
   read as rotation matrices under any of four candidate framings (§5.4). No
   scale evidence exists in the gmdl format at all.
4. **Composition order.** `mTotalOrientation` from `orientation` × `parentIndex`
   is `UNKNOWN`; `07-editor-data.json` gap G-04 says so.
5. **The unit.** "Metre-like" is `INFERRED` from a magnitude ladder (§2.3). No
   registry, no authored constant, no calibration.
6. **Whether the game mirrors anything as a transform.** `UNKNOWN` (§5.3). The
   corpus models symmetry as partner indices only.
7. **Rotation units in the authored data.** `UNKNOWN`; degrees recommended on
   consistency grounds only (§6.1).
8. **Whether matrix + position + scalar scale is really the authored triple, or
   whether `Matrix3` is something other than a rotation** (e.g. a shear-carrying
   3×4 with a zero fourth column). The 36-byte span is `CONFIRMED`; the
   interpretation as a pure rotation is `INFERRED` — no runtime instance of any
   of these structures has been captured.
9. **What Spore's UV/handedness conventions imply for a mirrored part's normal
   map**, beyond the winding measurement in §2.4. Bevy's TBN handedness sign `w`
   is `+1` for this corpus's unmirraced parameterisation
   (`crates/spore-engine/src/convert.rs:753-765`) and flips on a mirrored
   parameterisation (`convert.rs:783-793`); whether the game's own normal maps
   are DirectX- or OpenGL-style is separately `UNKNOWN`
   (`docs/ENGINE-SCENE.md` §3.5).

---

## 10. Clean-room

Nothing here is derived from EA or Maxis source, data or assets. The measurements
are made by this repository's own decoders over a local GOG install read at run
time; the structural offsets are copied from this repository's own committed
knowledge-graph artifacts, which cite Ghidra and the Spore-ModAPI SDK and
nothing else; and the design conclusions are OpenSpore's own. No Spore asset byte
is committed, and this document created exactly one file.