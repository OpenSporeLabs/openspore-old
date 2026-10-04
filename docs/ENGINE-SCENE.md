# ENGINE-SCENE — the runtime as an engine, not a preset demo

**Agent:** ENGINEFIX. **Date:** 2026-10-05.
**Crate:** `crates/spore-engine` (the only crate that depends on `bevy`).
**Reads:** `AGENTS.md`, `docs/RASTER-FRAMING.md`, `docs/MATERIAL-ROLES.md` §10,
`docs/RUST-ENGINE.md`.

`docs/RUST-ENGINE.md` is the developer-facing guide to the *workspace*. This
document is about what changed in `spore-engine`, why the material binding reads
what `spore-material` publishes, and what the four-layer split buys. Where the
other documents disagree with this one, **this one is stale** — §9 lists the
known disagreements, because two of them are now refuted outright.

> **Read §1 before implementing anything here.** Two corpus measurements moved
> the ground under this engine, and the version of the engine that read them the
> old way is documented below and no longer true.

---

## 1. The two measurements that refuted this engine's premises

### 1.1 There are no layers, and `DXT5` is BC3

**PROVEN over 4 386 / 4 386 raster records** (`Spore_Content` 2 954,
`Spore_EP1_Content_01` 1 432):

```text
header (32-byte envelope)
  + level 0 unprefixed
  + level 1 unprefixed (block formats only)
  + u32 byte length before every level >= 2
```

One mip chain. **No layers. No 16-byte layer headers.** `DXT5` is **BC3** (16
bytes per block); `DXT1` is BC1 (8 bytes per block). Every raster in
`Spore_Content` now decodes: **2 954 of 2 954, zero refusals.**

This engine used to believe otherwise, and the belief was **two errors that
cancelled exactly**:

1. the decoder charged **8** bytes per 4×4 block, and
2. the decoder divided the record by **`16 + chain`**, i.e. it charged a 16-byte
   *layer header* that does not exist.

They cancel whenever the geometry lines up, which is why 1 374 records decoded
and 373 did not, and why the failures looked like a framing curiosity rather
than an arithmetic error. Everything downstream inherited it: `layers.len() == 2`,
`layer_index` on every surface, "four mask planes per three-entry model".

**What the engine does now:** one chain, `layer_count == 1`, and no per-layer
loop anywhere. `RasterImage::layers` is length 1 for every record;
`ResolvedTexture::channels()` therefore returns **4** entries, not 8.

### 1.2 "A raster record carries no colour" was a tautology of that decoder

The previous revision of this document carried, as a *measurement*, that a
`DXT5` record's green span is 0, its blue span is 0, and that 477 771 sampled
palette entries collapse to two RGB triples. Read as BC3, the same records carry
**32 distinct greens and 32 distinct blues**. Measured on the creature record
below, over its 262 144 base-mip texels:

| lane | distinct values, file order | min | max |
|---|---:|---:|---:|
| R | **233** | 0 | 255 |
| G | **254** | 0 | 255 |
| B | **230** | 0 | 255 |
| A | 256 | 0 | 255 |

The old decoder read the colour field as two 6-bit values out of a 16-bit word,
so green and blue read as zero *by construction, for any input whatsoever*. "The
decoder cannot produce a colour" was never "the record has none".

**This does not make a raster a colour map.** Nothing measured says a `DXT5`
record *is* one, so the engine still binds no base-colour texture, and a model's
colour still comes from its `sporemodel` record (§3.6, and
`docs/COLOUR-IN-ENGINE.md`). What changed is that the engine no longer reasons
from a role *name*: it binds what `spore-material` publishes per channel (§3.3).

### 1.3 The defect that is still real, and was always real

A `0x15` record stores its tangent-space normal **Z first**, and Bevy's
`apply_normal_mapping` samples `R = X`. Feeding the bytes through unchanged
produces lighting that looks almost right and is not. §3.4 is unchanged.

---

## 2. The four layers, and what each may know about

| layer | modules | may import | window? |
|---|---|---|---|
| decode | `spore-core` … `spore-material` | nothing from this crate | no |
| **domain** | `src/domain/**` | `spore-*`, **never `bevy`** | no |
| convert | `src/convert.rs` | `bevy_mesh`-level types, no `App` | no |
| runtime | `src/loader.rs`, `src/texture.rs`, `src/scene.rs`, `src/cli.rs`, `src/lib.rs` | everything | yes |

The load-bearing rule is unchanged and is still `domain` being Bevy-free. That is
what lets `tests/material_wiring.rs` assert the *whole* material path — role
cut, swizzle bytes, upload format, tangent frame, multi-object scene — with no
window, no GPU and no game install.

### `src/domain/`

| module | what it owns |
|---|---|
| `world.rs` | `ResourceWorld`: the packages, and the `ContentStore` over them |
| `model.rs` | `ModelDefinition` (one record decoded), `ModelPart`, `ModelInstance`, `Placement`, `DecodeStatus` |
| `creature.rs` | `Creature`: one or more model instances in a common frame |
| `scene.rs` | `Scene`, `SceneObject`, `ObjectExtent`, `Layout`, the layout algorithm |
| `materials.rs` | `ModelMaterials`, `MeshMaterialPlan`, the flat-index cut, the normal swizzle |

These are ordinary Rust structs. The domain is **not** ECS, on purpose: a scene is
a value that can be built, compared and asserted on without a renderer. Only
`src/loader.rs` turns one into entities, and only `src/texture.rs` turns its
surfaces into GPU images.

---

## 3. Material binding, step by step

`docs/MATERIAL-ROLES.md` §10.3 is the specification; this is what the engine does
with it, and where it departs.

### 3.1 The index math, the THREE scopes, and why the engine does not zip

```text
flat = mesh_index * refs_per_mesh + within      // BindingScope::PerMeshBlock { .. }
flat = within                                    // BindingScope::WholeModel
                                                 // BindingScope::Undetermined { .. }
```

**`BindingScope` has three arms and every `match` on it here lists all three.**
The third, `Undetermined { collected, contradicted }`, is the arm that exists
because a **short** list divides just as neatly as a complete one: the gmdl
material-info walk stops on the undocumented shader-data id `0x218` in 619 of
4 209 gmdl records, and because what is missing is *whole blocks*, the truncated
list still divided evenly — so the division used to succeed on incomplete input
and hand mesh 1 a texture out of mesh 0's block. `spore-material` refuses that
cut; the engine follows it with `applies_whole_set()` arithmetic and
`stated_ordinal() == None`, so a refused cut cannot manufacture an ordinal
conflict either.

`ModelTextures::roles()` computes the mapping inside `spore-material`.
`ModelMaterials::roles()` computes it again from the same inputs, because the
engine also has to handle the route below. The two are asserted to agree, field
for field, in
`tests/real_corpus.rs::the_two_resolution_routes_agree_field_for_field_on_a_record_the_strict_one_accepts`.
`textures` and `meshes` are **never** zipped: under `PerMeshBlock` with three
references per mesh, `textures[i]` is mesh `i / 3`'s `(i % 3)`-th reference.

#### The engine's own guard: a slot binds only its own reference

`spore-material` refusing the cut is upstream of the renderer, but the engine
does not *rely* on it. `ModelMaterials::bind_slot` compares the entry at the
cut index with the reference the slot names, and on any disagreement it binds
**nothing** and records `unbound=entry-reference-mismatch`. So:

> a mesh is never bound to a texture from another mesh's block — not because the
> arithmetic is trusted, but because the binding refuses a slot whose flat entry
> names somebody else's reference.

`domain::materials::tests::a_mesh_is_never_bound_to_a_texture_from_another_meshes_block`
is that invariant, and it is the shape the documented creature used to be in.

### 3.2 The two routes, and why there are two

`spore_material::resolve_model_record` re-reads the record with the **strict**
`spore_gmdl::parse`, which refuses every record whose material-info tail names
shader-data id `0x218`. Measured on this corpus:

```text
$ osptool describe SPORE/Data/Spore_Content.package 0x00e6bce5:0x40627100:0x067c79d2
osptool: record `...`: gmdl: gmdl: undocumented shader-data id 0x218 in material info
```

That is the **creature preset**. So `ModelMaterials::resolve` tries the strict
route first and, when it refuses, composes the identical three steps from the same
public functions (`resolve_model_textures` + `model_materials`) applied to the
model `spore-assets` already decoded with the recovering parse. It is not a second
algorithm: it is the same one, minus a re-read that cannot succeed.

The refusal is reported, not hidden:

```text
OPENSPORE-MATERIAL v1 record=0/1 key=0x00e6bce5:0x40627100:0x067c79d2 ... route=recovering \
  shape=unavailable ordinals=0/0 ...
```

`shape=unavailable` and `ordinals=0/0` are **absences of an observation** on the
recovering route, and they are printed as absences. `observe_shape` and
`observe_ordinal_agreement` are private to `spore-material`, so the engine cannot
reproduce them, and inventing a second shape check is exactly the drift this
workspace keeps paying for. The documented asset, which the strict route accepts,
gets both: `route=strict shape=(mask, mask, normal) ordinals=3/3`.

### 3.3 What binds, read from the channel bindings — not from a role name

The rule, and it is the whole of the rework:

> **A surface exists because a channel of the record carries a channel role the
> renderer can act on.** Not because the record's `fourcc` is `DXT5`, not
> because it was the first texture in a list, and not because a role *variant*
> has a name this engine expects.

`spore-material` publishes `ChannelRole` per `(layer, lane)`. The engine matches
on those:

| published channel | Bevy slot | count |
|---|---|---|
| `NormalZ` + `NormalX` + `NormalY`, lanes `[1,2,0]` | `StandardMaterial::normal_map_texture` | 0 or 1 per mesh |
| `MaskChannel` | **none** — uploaded and carried on the entity | one per published channel |
| `ColourPlane`, `UnidentifiedLane`, anything else | **none**, and reported with a reason | — |

`NormalMapPlan::from_channels` returns three things and keeps them apart:
`Ok(Some(plan))` (binds), `Ok(None)` (nothing here is a normal — **not** a
finding; most records are not), and `Err(reason)` (the record publishes normal
components this build cannot place, so nothing binds and it is reported). It
never reads `TextureRole`. Consequences:

* a role variant this build has never seen binds correctly **with no line
  changed here**, if it publishes a normal triple in the right lanes;
* a role *called* normal that publishes something else does not bind;
* `Err(UnestablishedNormalLayout)` is reachable from a test through
  `from_bindings`, which takes the bindings as an argument — through
  `channels()` alone both `Err` arms would be unreachable defensive code.

`MaskChannelPlan` carries the **published lane**, and `mask_channel_surface`
reads that lane out of the texels. Hardcoding "alpha" would have been the same
assumption one layer up. `mask_channel_surface` also re-checks that the record it
found still publishes that channel, so a stale plan produces no surface rather
than a guess.

#### What the corpus publishes today

Measured over **all 2 954 raster records** of `Spore_Content.package`
(`tests/real_corpus.rs::every_raster_record_decodes_and_only_the_luminance_family_publishes_a_normal`):

| published role | records | binds |
|---|---:|---|
| `tangent-space-normal` | 901 | the normal map |
| `colour-and-mask` | 1 747 | one mask channel each, to no slot |
| `unidentified` | 306 (`DXT1`) | nothing |

`layers.len() == 1` for **2 954 of 2 954** records, and zero refusals.

The mask channels go nowhere on purpose. A `DXT5` record's fourth plane is a
full-range 8-bit channel and its purpose is `UNKNOWN`
(`spore_material::claims::MASK_LAYER_PURPOSE`). Binding it to
`metallic_roughness_texture` would name it "specular"/"gloss"/"occlusion", which
`MATERIAL-ROLES.md` §10.5 forbids. So each channel is uploaded as `R8Unorm` —
one channel, one value — and carried on the mesh entity as
`MaskChannels { planes }` for a future consumer that has evidence.

**And its colour planes bind nothing either**, which is *not* the same claim as
§1.2's refuted one. Those planes now carry real values (233/254/230 distinct on
the creature record); what is missing is a measurement that a `DXT5` record
*is a colour map*. Refuting "it has no colour" does not supply that, and
supplying it by inference is the mistake this engine was rewritten to avoid.

`MaskChannels` is one component holding a `Vec`, and that is a constraint rather
than a style choice: `Commands::insert` **replaces** a component of the same type,
so a per-channel component would have silently left exactly one channel on a
three-entry record. The first version of this code had that bug and the
integration test caught it.

### 3.4 The swizzle — the likeliest mistake in the whole task

```text
on disk        (Z, X, Y, ?)          file byte 0 is Z, 901/901 records
after swizzle  (X, Y, Z, 255)        what Bevy's apply_normal_mapping samples
```

Three separate things have to be right, and each is tested on texel values:

0. **Which layout, decided from the record.** `NormalLayout` stays a closed set
   of **one** variant, `ZFirst`, and the record's published lanes are checked
   against it (`NormalMapPlan::from_channels`). A record that publishes a normal
   in some other order is **refused and reported**, not swizzled with a guess:
   adding the layout to the enum would claim such a record exists, and guessing
   the permutation produces a wrong normal map that looks almost right. Both
   directions of the permutation are stated — `lane_components` (which component
   each file byte holds) and `NormalLayout::lanes()` (which byte each component
   is read from) — and a test asserts they are inverses.
1. **The permutation.** `swizzle_z_first_to_xyz` is
   `(z, x, y) → (x, y, z)`. Tested on the flat normal `(255,128,128)` → blue
   becomes 255; on the measured creature record's first texel
   `[255,128,126,181]` → `[128,126,255,255]`; and on a lane-mean measurement
   that the **blue** lane carries file byte 0's mean.
2. **The upload format.** `Rgba8Unorm`, **never** `Rgba8UnormSrgb`. An sRGB
   decode applies a transfer curve to a vector that is already linear; nothing
   about that looks broken on screen.
3. **The tangent frame.** Bevy 0.19 wraps `apply_normal_mapping` in
   `#ifdef VERTEX_TANGENTS`, and that define is only pushed when the mesh carries
   `Mesh::ATTRIBUTE_TANGENT`. A mesh with a normal map bound and no tangents binds
   the texture, samples nothing, and shades as though the map were absent —
   invisible in a screenshot. `bevy_mesh`'s mikktspace generator is behind the
   `bevy_mikktspace` feature, which this workspace's fixed dependency list does not
   enable, so `convert::compute_tangents_for_mesh` accumulates the frame itself
   (per-triangle tangent/bitangent, Gram-Schmidt against the vertex normal,
   handedness sign) and `NoTangents` names the four ways it can be undefined.

The swizzle is *measured*, not asserted from the code path: the test compares the
output bytes of a known input.

### 3.5 The green-channel convention, which is UNKNOWN

`--flip-normal-y` exists and is **off by default**. Whether Spore's `0x15` normal
maps are DirectX-style (Y down) or OpenGL-style cannot be settled from the
records — both encode the same `(x, y, z)` and differ only in the sign convention
of the author, and no reference frame of the original game has been captured. The
corpus establishes the **byte order**, which the engine honours unconditionally.

The flip is applied to the **uploaded bytes** (`g → 255 - g`), and
`StandardMaterial::flip_normal_map_y` is left `false` for every orientation, so
the two cannot compound and negate twice back to the stored value.

### 3.6 What is never bound

| slot | why |
|---|---|
| `base_color_texture` | **no record in this corpus is published as a colour map.** A `DXT5` record carries colour now (§1.2) and still feeds nothing here: "it has colour" and "it *is* a colour map" are different claims, and only the first is measured. |
| `metallic_roughness_texture` | the only single-channel records available are mask channels, and their purpose is UNKNOWN |
| alpha blending | the mask channels are not established as coverage; the material stays `AlphaMode::Opaque` |
| the mask channels themselves | uploaded, named by lane, carried on the entity, and bound to **no** slot |

`StandardMaterial::base_color` is the model's own `sporemodel` colour when one
resolves (`#7fcd79` for the documented creature, from
`0x2b978c46:0x40626200:0x05e3df13` `<skincolor1>`), and `UNTEXURED_TINT` — a
neutral grey, `srgb(0.62, 0.62, 0.64)` — only when no record was found. The
"not found" case is an **absence** (`state=unavailable reason=...` on the report
line), never a colour, and never `(0,0,0)`: black is a colour a record can
carry. See `docs/COLOUR-IN-ENGINE.md`.

---

## 4. Many assets in one scene

`--record` and `--preset` are **repeatable**; `--creature` starts an object that
`--part` can add to. That is the difference between a preset demo and an engine.

```bash
# two records side by side, laid out by their own measured widths
openspore --preset creature --preset documented-asset \
          --package SPORE/Data/Spore_Content.package --texture

# one creature built from two records, on a grid
openspore --creature 0x00e6bce5:0x40637e03:0x067a07f0 \
          --part 0x00e6bce5:0x40627100:0x067c79d2 \
          --layout grid --package ... --texture
```

Measured on the first form (creature + documented asset, both textured):

```text
OPENSPORE-SCENE v1 objects=2 models=2 parts=3 triangles=47036 \
  normal_maps_bound=3 mask_channels_bound=4 models_painted=0 \
  packages=Spore_Content mapped_bytes=995268083
```

Three parts from **two** records. The same record placed twice is one decode, one
`ModelDefinition` behind an `Arc`, and one dropped copy. (`mask_channels_bound=4`
is two channels for the documented asset's single mesh plus the creature's two
meshes, each holding the whole collected list — see §3.1.)

Layout is computed from each object's **own measured bounds**, not from a grid of
a chosen cell size: a fixed cell needs a size, and the only honest size is the
largest object, which makes one object's position depend on which *other* assets
were loaded. A missing extent makes the whole layout unavailable rather than
inventing a gap.

### Ownership, unchanged and deliberate

* `ScenePlugin` owns the **world**: camera, two lights, ground. It never spawns
  Spore content.
* `SporeAssetPlugin` owns the **content**. It never spawns a camera.

`scene.rs`'s tests assert exactly one camera and zero `OnStage` entities after
`ScenePlugin` alone, because the historical failure here was a duplicate camera
that presented as a framing bug.

Entity shape:

```text
SceneObjectMarker { index, kind }      parent, carries the object's placement
  └── ModelPartMarker { key, mesh_index, material_id }   leaf, identity transform
        Mesh3d, MeshMaterial3d, MaskChannels?
```

The placement lives on the parent so a creature's parts share one frame. The
hierarchy is maintained with `add_child`, not with `ChildOf` on the child alone:
Bevy's invariant is that the parent carries `Children` too, and its B0004 warning
says a one-sided relationship "will cause inconsistent behaviors".

### Resource lifetime

`prepare` holds the `ResourceWorld` in a local, decodes everything, and lets the
mapping go. The renderer needs vertex buffers and texels, both **copied** out of
the mapping by the decoders, so nothing downstream of the definitions reads the
store. `WorldResources` is still inserted as a resource — the ownership is
explicit, and a future streaming subsystem can use it — but no system reads it,
so no borrow can keep 995 MB alive by accident. `OPENSPORE-SCENE v1` prints
`mapped_bytes` so the cost is visible rather than assumed.

### Deferred commands

Every system that spawns has **no** query, and camera framing runs in
`PostStartup` against a `CameraFraming` resource, because `Commands::spawn` is
deferred and a `Query` in the same system sees nothing. `CameraFraming` is a
resource rather than a per-app closure so the framing step is a named system a
test can drive.

---

## 5. Report lines

`OPENSPORE-STAGED v1` is **byte-compatible** with the line this engine has always
printed: same fields, same order. `README.md` records that line and the current
build reproduces it exactly; that is why the v1 field set was frozen.

`OPENSPORE-MATERIAL v1` **changed**, and each change is additive except the
`plans=[…]` field, whose `unidentified=N` was replaced by `unbound=N
unbound_reasons=…`.

Real output, `--preset creature --texture --info`, 2026-10-05:

```text
OPENSPORE-MATERIAL v1 record=0/1 key=0x00e6bce5:0x40627100:0x067c79d2 meshes=2 \
  scope=undetermined refs_per_mesh=unavailable inferred=true route=recovering \
  shape=unavailable ordinals=0/0 roles=colour-and-mask:1,tangent-space-normal:1 \
  normal_maps_bound=2 mask_channels_bound=2 \
  plans=[mesh 0 material=0xd7be35f9 scope=undetermined normal_maps=1 mask_channels=1 \
          unbound=0 unbound_reasons=none ordinal_conflicts=0 inferred=true | mesh 1 … ]
```

and on the documented asset, whose material-info tail parses strictly:

```text
OPENSPORE-MATERIAL v1 record=0/1 key=0x00e6bce5:0x40637e03:0x067a07f0 meshes=1 \
  scope=per-mesh-block refs_per_mesh=3 inferred=true route=strict \
  shape=(colour-and-mask, colour-and-mask, normal) ordinals=3/3 \
  roles=colour-and-mask:2,tangent-space-normal:1 normal_maps_bound=1 \
  mask_channels_bound=2 plans=[mesh 0 … mask_channels=2 unbound=0 …]
```

Four things to read there:

* `scope=undetermined` and `refs_per_mesh=unavailable` on the creature — the
  third arm, printed with its own spelling rather than as `whole-model`.
* `shape=unavailable` and `ordinals=0/0` on the recovering route are **absences
  of an observation**. `observe_shape` and `observe_ordinal_agreement` are
  private to `spore-material`, so the engine cannot reproduce them, and inventing
  a second shape check is exactly the drift this workspace keeps paying for.
* `unbound=0 unbound_reasons=none` on both records: every reference resolved and
  every reference bound something. `unbound_reasons` is the field that replaces
  the old bare `unidentified=N` count — seven distinct reasons, none of which is
  "a decoding failure" unless it says `refused`.
* `inferred=true` is printed on every material line and cannot be turned off:
  `BindingScope::is_inferred()` is `true` for **every** variant, because nothing
  establishes that material-info block *i* is mesh *i*.

---

## 6. What was measured, on this corpus

`cargo test -p spore-engine --release -- --ignored --nocapture`, over
`SPORE/Data/Spore_Content.package` (995 268 083 bytes, GOG digital 3.1.0.22):

| record | meshes | triangles | refs | published roles | route | scope |
|---|---:|---:|---:|---|---|---|
| `0x00e6bce5:0x40627100:0x067c79d2` (`creature`) | 2 | 47 016 | 2 | `colour-and-mask`, `tangent-space-normal` | recovering | `undetermined { 2, 1 }` |
| `0x00e6bce5:0x40637e03:0x067a07f0` (`documented-asset`) | 1 | 20 | 3 | `colour-and-mask` ×2, `tangent-space-normal` | strict | `per-mesh-block { 3 }` |

Both records resolve **3 / 3** textures with **zero refusals**, and both plans are
`unbound=0`: every slot bound either the normal map or one mask channel.

### The creature's normal map — unchanged, and still Z-first

`0x2f4e681c:0x40622901:0x067c79d2`, 256², 9 mips, one chain:

```text
lane means (file order)  [249.330, 128.618, 128.008]
first texel, file order  [255, 128, 126, 181]
first texel, swizzled    [128, 126, 255, 255]
```

Lane 0 leads the brighter lateral lane by **120.71**, against a required 64, and
sits at **249.33** against a floor of 224. That is the Z-first signature, measured
on this record rather than quoted, and it is what `lanes=[x:1,y:2,z:0]` in the
plan means.

### The creature's second record — the refutation, on one record

`0x2f4e681c:0x40622900:0x067c79d2`, `DXT5`, 512², 10 mips, **one** chain:

| lane | distinct | min | max | what the old engine said |
|---|---:|---:|---:|---|
| R | **233** | 0 | 255 | ≤ 2 values |
| G | **254** | 0 | 255 | 1 value |
| B | **230** | 0 | 255 | 1 value |
| A | 256 | 0 | 255 | full range — unchanged |

And the corpus census behind those four numbers:

```text
rasters=2954 normal_maps=901
roles={"colour-and-mask": 1747, "tangent-space-normal": 901, "unidentified": 306}
chains={1: 2954}
```

### What a record looks like on screen

`--preset creature --sporemodel-package SPORE/Data/Spore_Game.package --texture`,
real window, 1920×1080, screenshot measured:

```text
green-dominant pixels inside the creature's bbox   97 784
neutral (grey) pixels in the same bbox              17 265
G − R over the creature                    +9 … +30, mean +22.7
```

which is the prior session's measurement reproduced (`98 203` green-dominant,
`21 454` neutral, `G−R ≈ +23…+30`). **Colour survived the role rework**, and it
is still not coming from a raster: the `OPENSPORE-PAINT v1` line says
`base=#7fcd79 … link=INFERRED`.

The normal map's contribution was measured the same way, by rendering the same
frame **without** `--texture` and comparing local luminance statistics over the
creature at identical scale:

| | smooth-skin band (local std) | smooth-skin band (\|ΔLuma\| per px) | spike band (local std) |
|---|---:|---:|---:|
| with `--texture` | **14.61** | **7.25** | 61.86 |
| without | 13.60 | 6.14 | 61.89 |

~18 % more surface variation on the smooth skin, and **no** change on the spikes
— which is the shape of the truth: the spikes are geometry and shade either way,
while the skin striations are the normal map. The spikes are visibly faceted and
the limbs visibly striated in the textured frame, and both are flat in the
untextured one.

### The disagreement this record used to carry, and no longer can

Before the third `BindingScope` arm, `refs_per_mesh` was 1, so mesh 1's slot was
*derived* ordinal 0 while the referenced record's group id low byte said **1** —
and the engine bound mesh 1 to a texture from mesh 0's block anyway. Now the
record reports `Undetermined { collected: 2, contradicted: 1 }`,
`ordinal_conflicts=0` on both meshes, and both meshes hold the whole collected
list. Whether the *per-mesh* ownership of either record is right remains
**UNKNOWN**; what is no longer unknown is that the engine would not have silently
invented it.

---

## 7. Test inventory

| file | what it protects | count |
|---|---|---:|
| `src/cli.rs` | repeatable objects, creatures, layout, exit behaviour | 19 |
| `src/convert.rs` | buffers, normals, fan expansion, tangent frame and its four absences | 17 |
| `src/scene.rs` | one camera, two lights, a ground, **no** content | 4 |
| `src/texture.rs` | channel → slot binding, swizzle bytes, upload formats, no diffuse | 14 |
| `src/domain/materials.rs` | the flat cut, the swizzle, unbound reasons, the mis-bind guard | 22 |
| `src/domain/model.rs` | decode status, bounds, placements | 9 |
| `src/domain/scene.rs` | layout, multi-object, missing extents | 9 |
| `src/domain/creature.rs` | instances vs distinct records | 4 |
| `src/domain/world.rs` | package priority, mapped bytes | 4 |
| `src/loader.rs` | bounds, report lines, camera framing | 10 |
| `src/lib.rs` | headless assembly, failure reporting | 7 |
| `tests/binary_names.rs` | the shipped binary is `openspore` | 4 |
| `tests/material_wiring.rs` | **the wiring, headless** | 5 |
| `tests/real_corpus.rs` | **the documented assets over real bytes, plus the raster census** | 7 |

`cargo test -p spore-engine` is **148 hermetic tests** and **7 `#[ignore]`d
corpus tests**, needs no game, no GPU and no window.

### The headless smoke test

`tests/material_wiring.rs` is the deliverable for "the wiring is right without a
window". It builds a synthetic model carrying the corpus's
`(colour-and-mask, colour-and-mask, normal)` set, runs both plugins under
`MinimalPlugins` + `AssetPlugin`, and asserts on the **uploaded asset bytes**:

1. the normal map is in `normal_map_texture` and nothing is in `base_color_texture`;
2. the image format is `Rgba8Unorm`;
3. blue carries file byte 0, red carries file byte 1, green carries file byte 2 —
   compared against the numbers, not against the function that produced them;
4. **two** `R8Unorm` channels exist, one per mask record, each carrying the lane
   the record published — and the fixture's four lanes hold four *different*
   values, so the assertion can tell which lane was uploaded;
5. the Bevy mesh carries a tangent frame;
6. two assets in one scene produce two object entities and two distinct
   materials, each leaf parented to its object.

`DefaultPlugins` is never used there: it builds a winit `EventLoop`, which panics
off the main thread, so "the app assembles" is not a headless-testable claim.
**The wiring is**, and that is what the file tests.

---

## 8. Still UNKNOWN, stated as a list

1. **The purpose of the mask channel a raster record publishes.** One per
   record; `spore_material::claims::MASK_LAYER_PURPOSE` is an `UNKNOWN`
   non-finding and the engine names the channel by lane only.
2. **What a `DXT5`/`DXT1` record is *for*.** §1.2 refuted "it carries no
   colour"; it did **not** establish "it is a colour map". No record in the
   corpus is published as one, so `base_color_texture` stays `None` and a model's
   colour comes from its `sporemodel` record.
3. **Which shader stage each texture feeds.** No shader has been read;
   `SamplerRole` still has one variant.
4. **Whether material-info block *i* is mesh *i*.** The whole per-mesh cut rests
   on it, and `Undetermined` exists because on 100 of 4 209 records the record
   itself contradicts it. `inferred=true` on every line; the engine's
   `entry-reference-mismatch` guard means a wrong cut shows up as *nothing bound*
   rather than as a texture in the wrong place.
5. **Whether Spore's normal maps are DirectX- or OpenGL-style.** `--flip-normal-y`
   exists, is off, and is documented as a guess.
6. **The `0x15` family's fourth lane.** `UnidentifiedLane`; correlation 0.032 with
   the normal. It is *not* uploaded into the normal map's alpha — the swizzle
   writes 255 — because nothing reads a normal map's alpha and carrying an
   unread unknown byte would be noise dressed as data.
7. **`0x218`'s size**, hence the `shape=unavailable` and `ordinals=0/0` fields on
   every recovering-route record.
8. **The BC3 codec itself.** `spore_texture::claims::BC3_CODEC` is graded
   `Inferred`: the block stride is measured, but no decode has yet been compared
   against the game's own output. Every colour number in §6 inherits that grade.
9. **A material id's meaning.** `material_name` is `None` for every id, and the
   engine does not substitute a colour.

---

## 9. Known deviations and stale text

| # | where | says | now |
|---|---|---|---|
| 1 | `docs/MATERIAL-ROLES.md` §4 and `spore-material`'s own prose | a `DXT5` record's green/blue spans are 0, red takes two values, it is a "two-layer alpha mask" | **refuted.** The block stride is 16 (BC3), there is one chain, and green/blue carry 32 distinct values. The claims live in the crates that own them; this file records the consequence for the renderer. |
| 2 | this file, previous revision | "four mask planes per three-entry model", `LayeredAlphaMask`, "layer 1 starts at 200" | one channel per record. §1.1 is the refutation and §7 the fixtures that now assert the new numbers. |
| 3 | `docs/RUST-ENGINE.md` §3a | "`--texture` … applies the first that decodes as the diffuse colour" | superseded by §3 here, and now twice over: no record is published as a colour map, and the diffuse claim rested on the refuted colourlessness. |
| 4 | `docs/RUST-ENGINE.md` §6 | "the batch API returns `Vec<Result<..>>`" and `SamplerRole` framing | still true, but the engine now also has a *channel-driven* binding and a per-mesh cut with a third scope arm. |
| 5 | `crates/spore-engine/src/loader.rs` (prior text) | one `StagedContent`, one model, "Extra meshes are spread along X" | replaced by `LoadedScene` + `domain::scene::Layout`. The 6.0-unit hard-coded spread was a placeholder that made a two-mesh record lie about its own extent. |
| 6 | `docs/RUST-ENGINE.md` §2 test counts | `spore-engine` 40 | 148 hermetic + 7 corpus. |

`README.md`'s `OPENSPORE-STAGED v1` line for the creature is reproduced exactly by
the current build, which was the reason the v1 field set was frozen. The
`OPENSPORE-MATERIAL v1` line is **not** reproduced by it — see §5 for the current
field set and which field changed.

---

## 10. Clean-room

Nothing here is derived from EA or Maxis code, data or assets. The roles come from
`spore-material`'s measurements over records this repository reads out of a local
GOG install at run time; the layout, the tangent frame, the domain types and the
report lines are OpenSpore's own design. No Spore asset byte is committed: every
fixture in `tests/` is synthesized in the test, and the corpus tests read the
git-ignored `SPORE/` tree and write nothing.