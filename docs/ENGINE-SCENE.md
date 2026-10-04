# ENGINE-SCENE — the runtime as an engine, not a preset demo

**Agent:** ENGINE. **Date:** 2026-10-04.
**Crate:** `crates/spore-engine` (the only crate that depends on `bevy`).
**Reads:** `AGENTS.md`, `docs/MATERIAL-ROLES.md` §10, `docs/RUST-ENGINE.md`.

`docs/RUST-ENGINE.md` is the developer-facing guide to the *workspace*. This
document is about what changed in `spore-engine`, why the material binding is
role-driven, and what the four-layer split buys. Where the two disagree,
`RUST-ENGINE.md` §3a still describes the **previous** behaviour ("applies the
first that decodes as the diffuse colour") and is now wrong; that file is not in
this agent's ownership, so the disagreement is recorded here rather than edited.

---

## 1. What was wrong, in one sentence

The engine bound **one** texture to a whole model and used it as diffuse colour.
`docs/MATERIAL-ROLES.md` §4 measures why that was wrong: over 1 374 decodable
DXT5 records the green span is **0**, the blue span is **0**, red takes at most
**two** values, and 477 771 sampled palette entries collapse to exactly **two**
RGB triples, `(0,0,0)` and `(8,0,0)`. There is no base-colour map in this corpus,
so the diffuse channel was being fed a mask and the render came out flat and dark.

The second defect was quieter: a `0x15` record stores its tangent-space normal
**Z first**, and Bevy's `apply_normal_mapping` samples `R = X`. Feeding the bytes
through unchanged produces lighting that looks almost right and is not.

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

### 3.1 The index math, and why the engine does not zip

```text
flat = mesh_index * refs_per_mesh + within      // BindingScope::PerMeshBlock
flat = within                                    // BindingScope::WholeModel
```

`ModelTextures::roles()` computes this inside `spore-material`.
`ModelMaterials::roles()` computes it again from the same inputs, because the
engine also has to handle the route below. The two are asserted to agree, field
for field, in
`tests/real_corpus.rs::the_two_resolution_routes_agree_field_for_field_on_a_record_the_strict_one_accepts`.
`textures` and `meshes` are **never** zipped: under `PerMeshBlock` with three
references per mesh, `textures[i]` is mesh `i / 3`'s `(i % 3)`-th reference.

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

### 3.3 The three bindings

| record role | Bevy slot | how many |
|---|---|---|
| `TangentSpaceNormal` | `StandardMaterial::normal_map_texture` | 0 or 1 per mesh |
| `LayeredAlphaMask` | **none** — uploaded and carried on the entity | one per layer, so **4** for the corpus's three-entry set |
| `Unidentified` | **none**, and reported | — |

The masks go nowhere on purpose. A `DXT5` record's alpha plane is a full-range
8-bit channel and its purpose is `UNKNOWN`
(`spore_material::claims::MASK_LAYER_PURPOSE`). Binding one to
`metallic_roughness_texture` would name it "specular"/"gloss"/"occlusion", which
`MATERIAL-ROLES.md` §10.5 forbids. So each plane is uploaded as `R8Unorm` — one
channel, one value, because the record's colour planes are `DegenerateColour`
under the byte-pinned codec — and carried on the mesh entity as
`MaskChannels { planes }` for a future consumer that has evidence.

`MaskChannels` is one component holding a `Vec`, and that is a constraint rather
than a style choice: `Commands::insert` **replaces** a component of the same type,
so a per-plane component would have silently left exactly one of the four planes
on a three-entry record. The first version of this code had that bug and the
integration test caught it.

### 3.4 The swizzle — the likeliest mistake in the whole task

```text
on disk        (Z, X, Y, ?)          file byte 0 is Z, 901/901 records
after swizzle  (X, Y, Z, 255)        what Bevy's apply_normal_mapping samples
```

Three separate things have to be right, and each is tested on texel values:

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
| `base_color_texture` | no raster record in this corpus decodes as base colour (§4 of `MATERIAL-ROLES.md`) |
| `metallic_roughness_texture` | the only full-range channels available are mask planes, and their purpose is UNKNOWN |
| alpha blending | the mask planes are not established as coverage; the material stays `AlphaMode::Opaque` |
| `Diffuse` role | `TextureRole` has no such variant and this crate does not add one |

`StandardMaterial::base_color` is therefore `UNTEXURED_TINT`, a neutral grey
(`srgb(0.62, 0.62, 0.62)`). A hue there would be an invention; grey is the only
value that asserts nothing and it lets the normal map's shading be read.

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

Measured on the second form:

```text
OPENSPORE-SCENE v1 objects=3 models=2 parts=3 triangles=47036 \
  normal_maps_bound=2 mask_channels_bound=6 packages=Spore_Content mapped_bytes=995268083
```

Three objects, **two** records decoded. The same record placed twice is one
decode, one `ModelDefinition` behind an `Arc`, and one dropped copy.

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
printed: same fields, same order. `--info` on the documented asset produces
exactly what `README.md` records.

```text
OPENSPORE-MATERIAL v1 record=0/1 key=... meshes=2 scope=per-mesh-block refs_per_mesh=1 \
  inferred=true route=recovering shape=unavailable ordinals=0/0 \
  roles=layered-alpha-mask:1,tangent-space-normal:1 normal_maps_bound=1 \
  mask_channels_bound=2 plans=[mesh 0 ... | mesh 1 ...]

OPENSPORE-SCENE v1 objects=1 models=1 parts=2 triangles=47016 normal_maps_bound=1 \
  mask_channels_bound=2 packages=Spore_Content mapped_bytes=995268083
```

`inferred=true` is printed on every material line and cannot be turned off:
`BindingScope::is_inferred()` is `true` for both variants, because nothing
establishes that material-info block *i* is mesh *i*.

---

## 6. What was measured, on this corpus

`cargo test -p spore-engine -- --ignored --nocapture`, over
`SPORE/Data/Spore_Content.package` (995 268 083 bytes, GOG digital 3.1.0.22):

| record | meshes | triangles | refs | roles | route |
|---|---:|---:|---:|---|---|
| `0x00e6bce5:0x40627100:0x067c79d2` (`creature`) | 2 | 47 016 | 2 | `layered-alpha-mask`, `tangent-space-normal` | recovering |
| `0x00e6bce5:0x40637e03:0x067a07f0` (`documented-asset`) | 1 | 20 | 3 | `layered-alpha-mask` ×2, `tangent-space-normal` | strict |

The creature's normal map, `0x2f4e681c:0x40622901:0x067c79d2`, 256², 9 mips, one
layer:

```text
lane means (file order)  [249.330, 128.618, 128.008]
first texel, file order  [255, 128, 126, 181]
first texel, swizzled    [128, 126, 255, 255]
```

Lane 0 leads the brighter lateral lane by **120.71**, against a required 64, and
sits at **249.33** against a floor of 224. That is the Z-first signature, measured
on this record rather than quoted.

The creature's mask, `0x2f4e681c:0x40622900:0x067c79d2`, is DXT5, 512², 10 mips,
**two** layers, alpha means 70.45 and 78.01, green and blue spans both 0.

### The disagreement this record carries

`refs_per_mesh` is 1, so mesh 1's slot is *derived* ordinal 0 — while the
referenced record's group id low byte says **1**. Two independent statements of
one fact, and they do not agree on this record. The engine reports
`ordinal_conflicts=1` and does not prefer either. Whether mesh 1 is really the
mesh the normal map belongs to is therefore **UNKNOWN**, and that is the honest
state of the binding.

---

## 7. Test inventory

| file | what it protects | count |
|---|---|---:|
| `src/cli.rs` | repeatable objects, creatures, layout, exit behaviour | 19 |
| `src/convert.rs` | buffers, normals, fan expansion, tangent frame and its four absences | 17 |
| `src/scene.rs` | one camera, two lights, a ground, **no** content | 4 |
| `src/texture.rs` | role → slot binding, swizzle bytes, upload formats, no diffuse | 14 |
| `src/domain/materials.rs` | the flat cut, the swizzle, `Unidentified`, inferred scope | 18 |
| `src/domain/model.rs` | decode status, bounds, placements | 9 |
| `src/domain/scene.rs` | layout, multi-object, missing extents | 9 |
| `src/domain/creature.rs` | instances vs distinct records | 4 |
| `src/domain/world.rs` | package priority, mapped bytes | 4 |
| `src/loader.rs` | bounds, report lines, camera framing | 10 |
| `src/lib.rs` | headless assembly, failure reporting | 7 |
| `tests/binary_names.rs` | the shipped binary is `openspore` | 4 |
| `tests/material_wiring.rs` | **the wiring, headless** | 5 |
| `tests/real_corpus.rs` | **the two documented assets over real bytes** | 3 |

`cargo test -p spore-engine` is **122 hermetic tests + 3 `#[ignore]`d corpus
tests**, needs no game, no GPU and no window.

### The headless smoke test

`tests/material_wiring.rs` is the deliverable for "the wiring is right without a
window". It builds a synthetic model carrying the corpus's
`(mask, mask, normal)` set, runs both plugins under `MinimalPlugins` +
`AssetPlugin`, and asserts on the **uploaded asset bytes**:

1. the normal map is in `normal_map_texture` and nothing is in `base_color_texture`;
2. the image format is `Rgba8Unorm`;
3. blue carries file byte 0, red carries file byte 1, green carries file byte 2 —
   compared against the numbers, not against the function that produced them;
4. four `R8Unorm` mask planes exist, two per record, carrying the alpha lane of
   each layer, and are attached to the mesh entity that owns them;
5. the Bevy mesh carries a tangent frame;
6. two assets in one scene produce two object entities and two distinct
   materials, each leaf parented to its object.

`DefaultPlugins` is never used there: it builds a winit `EventLoop`, which panics
off the main thread, so "the app assembles" is not a headless-testable claim.
**The wiring is**, and that is what the file tests.

---

## 8. Still UNKNOWN, stated as a list

1. **The purpose of each mask plane.** Four per three-entry model, none named.
2. **Which shader stage each texture feeds.** No shader has been read;
   `SamplerRole` still has one variant.
3. **Whether material-info block *i* is mesh *i*.** The whole per-mesh cut rests
   on it. `inferred=true` on every line, and the creature's `ordinal_conflicts=1`
   is a live counter-example to the naive cut.
4. **Whether Spore's normal maps are DirectX- or OpenGL-style.** `--flip-normal-y`
   exists, is off, and is documented as a guess.
5. **The `0x15` family's fourth lane.** `UnidentifiedLane`; correlation 0.032 with
   the normal. It is *not* uploaded into the normal map's alpha — the swizzle
   writes 255 — because nothing reads a normal map's alpha and carrying an
   unread unknown byte would be noise dressed as data.
6. **`0x218`'s size**, hence the `shape=` and `ordinals=` fields on every
   recovering-route record.
7. **DXT1** (`0x31545844`, 306 records): still refused by name.
8. **A material id's meaning.** `material_name` is `None` for every id, and the
   engine does not substitute a colour.

---

## 9. Known deviations and stale text

| # | where | says | now |
|---|---|---|---|
| 1 | `docs/RUST-ENGINE.md` §3a | "`--texture` … applies the first that decodes as the diffuse colour" | superseded by §3 here. The diffuse claim is **refuted** by `MATERIAL-ROLES.md` §4. |
| 2 | `docs/RUST-ENGINE.md` §6 "Other things the engine does not claim" | "the batch API returns `Vec<Result<..>>`" and `SamplerRole` framing | still true, but the engine now also has a *role* table and a per-mesh cut. |
| 3 | `crates/spore-engine/src/loader.rs` (prior text) | one `StagedContent`, one model, "Extra meshes are spread along X" | replaced by `LoadedScene` + `domain::scene::Layout`. The 6.0-unit hard-coded spread was a placeholder that made a two-mesh record lie about its own extent. |
| 4 | `docs/RUST-ENGINE.md` §2 test counts | `spore-engine` 40 | 124 hermetic + 3 corpus. |

`README.md`'s `OPENSPORE-STAGED v1` line for the creature is reproduced exactly by
the current build, which was the reason the v1 field set was frozen.

---

## 10. Clean-room

Nothing here is derived from EA or Maxis code, data or assets. The roles come from
`spore-material`'s measurements over records this repository reads out of a local
GOG install at run time; the layout, the tangent frame, the domain types and the
report lines are OpenSpore's own design. No Spore asset byte is committed: every
fixture in `tests/` is synthesized in the test, and the corpus tests read the
git-ignored `SPORE/` tree and write nothing.