# 04 — Materials, surfaces and paint

Agent **MATERIALS**. Research only. One file created, nothing else modified, no
commits, no code edits.

Every number below came from a command run against the read-only install at
`./SPORE` or from a Ghidra read of `SporeApp.exe` 3.1.0.22; the command is quoted
next to the number so any figure can be re-derived. No asset byte is reproduced
here — hex ids, offsets, sizes, counts and structure only.

Evidence grades use `spore_core::EvidenceLevel`
(`crates/spore-core/src/evidence.rs:35`):
`UNKNOWN < APPROXIMATION < INFERRED < SUPPORTED < OBSERVED < CONFIRMED < VERIFIED`.
`PROVEN` in older documents ≈ `CONFIRMED`/`VERIFIED`.

---

## 0. Executive summary

1. **The per-record/per-instance blocker is real and it is one line wide.**
   `ModelDefinition.materials` is `Option<ModelMaterials>`
   (`crates/spore-engine/src/domain/model.rs:96`) and
   `ModelPart.material_id: u32` (`…/model.rs:65`) is filled from
   `gmdl.material_ids[mesh_index]` (`…/model.rs:121`) — the **record's** array.
   `ModelInstance` (`…/model.rs:238`) carries only `{key, label, placement}`. Two
   instances of one record therefore *cannot* differ in material, because there
   is no per-instance field to differ in (VERIFIED by reading the types).
2. **The corpus does not merely fail to support per-part materials — it forbids
   them.** Over all 4209 `gmdl` rows of `Spore_Content.package`,
   **6063 mesh slots carry exactly 3 distinct material ids**
   (`0x09cff27c` 3810, `0xd7be35f9` 1511, `0x407dfddb` 742; `0x00000000` never
   occurs), and **4208 of 4208 records with ≥1 mesh carry exactly ONE distinct id
   across all their meshes — 0 records are mixed** (VERIFIED).
3. **`material_id` is a texture-set-layout tag, not a part identity.** It
   predicts the texture-set shape exactly: `0x09cff27c` ⇒ shapes `(3,)`/`(3,3)`/
   `(3,3,3)`; `0x407dfddb` ⇒ `(3,)`/`(3,3)`; `0xd7be35f9` ⇒ `(0,)`/`(0,0)`/
   `(0,0,0)` — i.e. **1393 records carry a material id and no texture set at
   all** (VERIFIED). It cannot be a texture binding.
4. **What does vary per part is a 20-byte shader block, and it does not vary
   either.** Every one of the 6063 blocks is a `0x210` entry. There are **65
   distinct 20-byte payloads**, word 0 is always `0x26445c01|04|05|07`, and
   **4208 of 4208 records carry exactly one distinct payload across all their
   meshes** (VERIFIED). Per-record constancy is the finding.
5. **Raster records are shared assets, so paint cannot live in one.** Of 2343
   distinct `(group,instance)` texture references, **2209 are named by more than
   one `gmdl` record**; 489 are named by 8 records, 1434 by 4, and **one is named
   by 132 records**. Any per-creature paint is necessarily a layer applied at
   render time (VERIFIED).
6. **There IS a real paint subsystem in the binary, and it is not a material.**
   `Skinner::cSkinPainterPipeline` + 8 `cSkinPainterJob*` classes +
   `cSkinnerTexturePainter` + `Palettes::cSwatchManager`. `cSkinPainterJobPaintParts::Execute`
   (`0x519640`) walks **rigblocks** (stride `0x8C`) and reads **three prop keys**
   (`0x2424655/0x2424656/0x2424657`) from a per-rigblock `PropertyList`, each
   yielding a **render-target id plus an atlas rectangle** — that is a per-part
   painted surface, and it lives in a **property list, not in the gmdl record**
   (OBSERVED, decompiled).
7. **Colour in Spore is a runtime shader uniform, and the registry proves the
   repo's refusals are faithful.** `Graphics::ActiveState::SetShaderData`
   (`0x777B50`) writes `sShaderData[index]`; `RegisterShaderData` (`0x11F4270`)
   populates the id→size tables from **72 call sites in 3 functions**.
   **`0x218` — the id `spore-gmdl` refuses — is registered nowhere.** The game has
   no size for it either (VERIFIED over all 72 call sites).
8. **The `0x210` block is colour-*shaped* and nothing more.** Words 1–4 read as
   `f32`; word 3 is **exactly `1.0` in 6063/6063 blocks**, word 2 takes 12 values
   on a 0.19 grid, word 4 takes 11 on a 0.07 grid, word 1 takes 12 "round" values
   (4…60). That is *consistent with* an RGB triple whose green is pinned full and
   a separate scalar. **No shader has been read, so this is a shape, not a
   colour** — recorded `INFERRED` for shape, `UNKNOWN` for meaning.
9. **Palette records are absent from the install.** `plt` (`0x011989B7`) is in the
   canonical type table and has **0 records in all 7 installed packages**. The
   `0x6b` "palette" *category* byte exists (67 `gmdl` in `0x406b6200`), but a
   category byte is a naming convention, not a palette (VERIFIED negative).
10. **Recommendation in one line:** author paint as a **named parameter entry per
    part-instance with its own evidence grade**, referencing **resolved surfaces
    by `ResourceKey` + role name**, and never re-resolving or storing pixels —
    so the document stays serialisable with no GPU and no texture decode, and a
    future finding fills a `Fact` rather than breaking a format.

---

## 1. The per-record vs per-instance problem, stated precisely

### 1.1 The claim, and the exact code that makes it true

| fact | anchor | grade |
|---|---|---|
| `ModelDefinition` is keyed by `ResourceKey` and holds `materials: Option<ModelMaterials>` | `crates/spore-engine/src/domain/model.rs:82`, `:96` | VERIFIED |
| `Option::None` means textures were **not requested** — explicitly not "no textures" | `…/model.rs:91-96` | VERIFIED |
| `ModelPart { mesh_index, mesh, material_id }`; `material_id` is a bare `u32` | `…/model.rs:58`, `:65` | VERIFIED |
| `material_id` is copied from the **record's** `material_ids[mesh_index]` | `…/model.rs:121` | VERIFIED |
| `ModelInstance { key, label, placement }` — no material, no surface, no paint | `…/model.rs:238-249` | VERIFIED |
| `SceneObject::Model(ModelInstance)`; `instances_mut()` returns `&mut [ModelInstance]` with no material setter | `crates/spore-engine/src/domain/scene.rs:35`, `:240` | VERIFIED |
| `MeshMaterial { mesh_index, material_id, name, slots, claims, scope }` — one per mesh of a `GmdlModel`, never per instance | `crates/spore-material/src/material.rs:209-231`, `:428-468` | VERIFIED |
| `material_name(_id) -> Option<&'static str>` returns `None` for **every** id, unconditionally | `crates/spore-material/src/material.rs:257-263` | VERIFIED |
| `MeshMaterialPlan { mesh_index, material_id, material_name, scope, normal_map, masks, unidentified, ordinal_conflicts }` — `material_name` is always `None` | `crates/spore-engine/src/domain/materials.rs:643-665`, `:418-419` | VERIFIED |
| `SurfaceSet { normal_map: Option<SurfaceImage>, masks: Vec<SurfaceImage> }`; `SurfaceImage` holds decoded `pixels: Vec<u8>` | `crates/spore-engine/src/texture.rs:149-163`, `:203-206` | VERIFIED |
| A test asserts the description of a plan never names an unestablished purpose | `crates/spore-engine/src/domain/materials.rs:1006-1030` | VERIFIED |

**Therefore (VERIFIED by construction):** materials hang off the **record**.
`ModelDefinition` is the unit of material state, `ModelInstance` is the unit of
placement. Two `ModelInstance`s with the same `key` resolve to the *same*
`LoadedModel` and therefore the *same* `ModelPart.material_id`, the *same*
`ResourceKey`s in `MeshMaterial.slots`, and the *same* `SurfaceImage.pixels`.
There is **no field anywhere in the instance path that two instances could
differ in**, so this is not a missing feature — it is a missing *type*.

### 1.2 What the corpus says about per-part material identity

```
python3 /tmp/opencode/creator-materials/measure5.py
```

| question | answer | grade |
|---|---|---|
| distinct material ids over all 6063 mesh slots | **3** | VERIFIED |
| `0x09cff27c` / `0xd7be35f9` / `0x407dfddb` slot counts | 3810 / 1511 / 742 | VERIFIED |
| records whose meshes carry more than one id | **0 of 4208** | VERIFIED |
| records with exactly one id across all meshes | **4208 of 4208** | VERIFIED |
| `material_id == 0x00000000` | **never** (0 slots) | VERIFIED |
| `mesh_count == material-info block count` | **4209 of 4209** | VERIFIED |
| every block of one record has the *same* dataIndex recipe | **10 distinct record shapes, each internally uniform** | VERIFIED |

Per-block `dataIndex` recipes (6063 blocks; `TEX(n)` = a `0x20D` texture set with
`n` entries):

| recipe | blocks |
|---|---:|
| `[TEX(3), 0x210]` | 4552 |
| `[0x210, TEX(2), 0x218]` | 719 |
| `[0x210, TEX(2)]` | 642 |
| `[0x210, TEX(3), 0x238]` | 150 |

Per-record recipes (10 shapes):

| record shape | records |
|---|---:|
| `([TEX(3),0x210], [TEX(3),0x210])` | 1601 |
| `([TEX(3),0x210],)` | 1146 |
| `([0x210,TEX(2)],)` | 642 |
| `([0x210,TEX(2),0x218],)` | 519 |
| `([0x210,TEX(3),0x238],)` | 116 |
| `([0x210,TEX(2),0x218], [0x210,TEX(2),0x218])` | 100 |
| `([TEX(3),0x210], [TEX(3),0x210], [TEX(3),0x210])` | 68 |
| `([0x210,TEX(3),0x238], [0x210,TEX(3),0x238])` | 14 |
| `([0x210,TEX(3),0x238], [0x210,TEX(3),0x238], [0x210,TEX(3),0x238])` | 2 |
| `()` — the one `meshCount == 0` record (`0x40637e03`, 80 B) | 1 |

**Consequence for a creator (VERIFIED):** Spore's model records express **zero**
per-mesh material variation. A creator cannot *reproduce* per-part materials from
a `gmdl` record because there are none to reproduce. Any per-part look in a
creator is therefore **new authoring data**, not a decode of existing data, and
must be labelled as such rather than presented as fidelity.

### 1.3 The minimal layering that fixes it, without inventing a registry

The blocker is *where the state sits*, not the absence of a registry. Three
requirements, in order:

**R1 — an override must reference resolved surfaces, never re-resolve them.**
Re-resolving inside an override would create a second resolution path, which is
the drift `AGENTS.md` warns about and which `spore-engine`'s
`tests::the_roles_table_reproduces_the_reference_cut` exists to prevent
(`crates/spore-engine/src/domain/materials.rs:392-405`).

**R2 — an override must be able to say "nothing", and that must differ from
"not asked".** The `Option<ModelMaterials>` discipline
(`…/model.rs:91-96`) is the model: `None` = not requested,
`Some(empty)` = requested and found nothing. An override needs the same three-way
distinction or it will collapse "the author removed the paint" into "the author
never set one".

**R3 — an override must not need a GPU or a texture decode to serialise.**

The minimal shape, as three new types and no change to any existing one:

```rust
// crates/spore-material (renderer-agnostic, Bevy-free, GPU-free)

/// A role a creator authored for one part-instance, named by a string and
/// graded. Never an enum: the corpus names no purpose (claims::MASK_LAYER_PURPOSE
/// is UNKNOWN) and an enum variant would be a claim nobody can support.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthoredRole {
    pub name: String,                 // stable slug, e.g. "base", "accent-1"
    pub claim: spore_core::evidence::Fact<AuthoredRoleValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthoredRoleValue {
    /// Points at a surface the resolver already produced.
    Surface {
        key: spore_core::ResourceKey,
        /// "the whole decoded record" or "layer N of it" — the only distinction
        /// ResolvedTexture::layer(index) already makes (resolve.rs:193).
        selector: SurfaceSelector,
    },
    /// A uniform the shader is expected to read. Carries NO claim about which
    /// shader stage; see claims::SAMPLER_ROLE.
    Uniform { bytes: Vec<u8> },
}

/// Per part-instance, NOT per record. Lives beside ModelInstance, never inside
/// ModelDefinition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartSurfaceOverride {
    /// Index into ModelDefinition.parts — the mesh this applies to.
    pub part_index: u32,
    /// `None` = "this instance adds nothing"; `Some(vec![])` = "the author
    /// explicitly cleared every role". Never collapse the two.
    pub roles: Option<Vec<AuthoredRole>>,
    /// What the record's own material would have contributed, kept for undo and
    /// for the honest report line. Never used as a fallback silently.
    pub inherits_record_material: bool,
}
```

and on the instance side, one field:

```rust
// crates/spore-engine/src/domain/model.rs — additive, default = inherit
pub struct ModelInstance {
    pub key: ResourceKey,
    pub label: Option<String>,
    pub placement: Placement,
    /// `None` means "use the record's material", which is the current
    /// behaviour for every existing caller — so this is not a breaking change.
    pub surface_overrides: Option<Vec<PartSurfaceOverride>>,
}
```

**Why this is minimal and why it needs no registry:**

* it adds **no** name for a material id, so `claims::MATERIAL_NAME` and
  `claims::MATERIAL_ID_MEANING` stay `NonFinding(no_material_registry)` — the
  test at `crates/spore-material/src/claims.rs:737-750` keeps passing;
* it adds **no** `TextureRole` variant, so `docs/MATERIAL-ROLES.md` §10.5's "do
  not add a `Diffuse` variant" is untouched;
* it adds **no** `SamplerRole` variant, so the one-variant invariant at
  `crates/spore-material/src/material.rs:494-504` is untouched;
* `SurfaceSelector` names a record and a layer index, both of which
  `ResolvedTexture` already publishes (`resolve.rs:193`) — **the
  reference is to something the resolver produced, satisfying R1**;
* `surface_overrides: None` reproduces today's behaviour exactly, so the
  vertical slice and every existing test are unaffected.

**What it deliberately does NOT do:** it does not synthesise a second
`BindingScope`. `MaterialTextureOwnership` is already a `NonFinding`
(`crates/spore-material/src/claims.rs:154`) and `BindingScope::is_inferred()` is
`true` for both variants (`crates/spore-material/src/material.rs:202-204`); an
override that re-derived a cut would manufacture a *proven* binding out of an
inference.

---

## 2. What actually varies per part in a real creature

### 2.1 Material ids: three, and constant per record

```
$ ./target/release/osptool list SPORE/Data/Spore_Content.package --type 0x00e6bce5 --limit 0 | wc -l
```

`measure5.py` output (quoted verbatim in §1.2): 3 distinct ids over 6063 slots,
0 mixed records.

Cross-tabulated against the model group (`material id × model group`, all 30
groups):

| material id | model groups | records |
|---|---|---:|
| `0x09cff27c` | `0x40636200-03`, `0x40646200-03`, `0x40656200-03` (building, vehicle, ufo) | 2160 |
| `0x407dfddb` | `0x40637e00-03` (building, second family) | 659 |
| `0xd7be35f9` | `0x40616201/02`, `0x40617100`, `0x40626202/03`, `0x40627100/01`, `0x40666201-03`, `0x40667100`, `0x406b6200`, `0x41626202/03` (cell, creature, flora, palette) | 1393 |

The id is **constant across a whole model family** — 4 or 12 groups share one
value. It is a family-level tag, which is the strongest available statement that
it is not a per-part identity.

### 2.2 Texture references: massively shared between records

```
$ python3 /tmp/opencode/creator-materials/measure5.py      # section F
```

| statistic | value |
|---|---:|
| texture-set entries, all 4209 records | 16 828 |
| distinct `(group, instance)` references | 2343 |
| refs named by exactly 1 record | 134 |
| by 3 | 88 |
| by 4 | 1434 |
| by 6 | 194 |
| by 7 | 3 |
| by 8 | 489 |
| **by 132** | **1** |
| refs shared by >1 record | **2209 of 2343** |

A single `(group, instance)` pair is named by **132** different `gmdl` records.
Those records are the 132 models in group `0x40616201` whose texture-set entry
names **group id `0`** (150 entries, VERIFIED) — and those same 132 records are
exactly the 132 that carry a `0x238` entry (**132/132 overlap**, VERIFIED).

**Consequence (VERIFIED):** a raster record instance is a *shared, deduplicated
asset*. A creator that wants two instances of one part to look different cannot
do it by editing the raster — the raster belongs to up to 132 other models. The
override in §1.3 (a layer at draw time) is the only layering consistent with
this.

### 2.3 The `0x210` shader block — the only per-mesh numeric payload

```
$ python3 /tmp/opencode/creator-materials/measure6.py      # 6063 blocks
$ python3 /tmp/opencode/creator-materials/measure7.py      # quantisation
```

| property | value | grade |
|---|---|---|
| blocks carrying `0x210` | 6063 in 4208 records | VERIFIED |
| payload length | 20 bytes, every block | VERIFIED |
| distinct payloads | **65** | VERIFIED |
| word 0 (u32) — 4 distinct values | `0x26445c01` (4552), `0x26445c07` (720), `0x26445c05` (641), `0x26445c04` (150) | VERIFIED |
| word 1 (`f32`) — 12 distinct | `4, 5, 8, 10, 12.749999, 15, 20, 30, 40, 50.000004, 51, 60` | VERIFIED |
| word 2 (`f32`) — 12 distinct | `0.19 … 1.14`, all on a **0.19** grid (plus `1.0`) | VERIFIED |
| word 3 (`f32`) — **1** distinct | **`1.0` in 6063/6063** | VERIFIED |
| word 4 (`f32`) — 11 distinct | `0 … 0.63`, all on a **0.07** grid | VERIFIED |
| distinct `(w1,w2,w3,w4)` tuples | 44 | VERIFIED |
| **records whose meshes disagree on `0x210`** | **0 of 4208** | VERIFIED |
| records whose `word1` correlates with material id | `0x09cff27c` ⇒ 3810/3810 at `60.0`; `0x407dfddb` ⇒ 742/742 at `60.0`; `0xd7be35f9` ⇒ spread over all 12 | VERIFIED |

**Reading, graded honestly:**

* word 3 pinned to exactly `1.0` across every block is an arithmetic fact about
  the bytes and is **VERIFIED**;
* words 2/3/4 on two independent quantisation grids with one channel pinned is
  **consistent with** an RGB triple whose green is always full — graded
  `INFERRED` as a *shape*;
* **the meaning of any of the four words is `UNKNOWN`.** No shader has been
  read; `claims::SAMPLER_ROLE` is a `NonFinding(no_role_field_in_texture_entry)`
  and its basis states that "which shader stage the game binds a texture to is
  stated somewhere this workspace has never read"
  (`crates/spore-material/src/claims.rs:401-410`). Naming words 2–4 "red,
  green, blue" would be exactly the fabrication `MATERIALS-DESIGN.md`'s
  successors were built to prevent, and `crates/spore-engine/src/texture.rs:117-146`
  documents the same rule for the renderer.
* word 0's `0x26445c` family also appears in the `gmdl` **animation** tail
  (`animDataCount` read as `642014215` = `0x26445C17` on row 0), so it is a
  RenderWare extension id, **not** a material name. Graded `OBSERVED` that the
  bytes are `0x26445cNN`; `UNKNOWN` what the extension means.

### 2.4 The `0x238` entry — an 8-byte-stride index, meaning unknown

```
$ python3 /tmp/opencode/creator-materials/measure7.py      # 0x238 section
```

150 blocks, 1 byte each, **15 distinct values**:

```
0x09 0x11 0x19 0x21 0x29 0x39 0x41 0x49 0x51 0x59 0x61 0x69 0x79 0x91 0xb1
```

| property | value | grade |
|---|---|---|
| payload length | 1 byte, all 150 | VERIFIED |
| all values satisfy `(v − 1) mod 8 == 0` | **15 of 15**, i.e. `v = 8k + 1`, `k ∈ {1,2,3,4,5,7,8,9,10,11,12,13,15,18,22}` | VERIFIED |
| records holding a `0x238` block | 132 | VERIFIED |
| overlap with the 132 records naming texture group `0` | **132 of 132** | VERIFIED |
| what `k` indexes | — | **UNKNOWN** |

The arithmetic regularity is real and worth recording; the meaning is not. It is
**not** evidence of a palette: 15 values over 150 blocks, none of them a colour
triple, and the population is exactly the population of models whose texture
reference is *missing*.

### 2.5 The texture-set entry header, re-measured over all 4209 records

`docs/MATERIAL-ROLES.md` §6 measured 6684 references from the Python oracle's
1819 records. The full population gives **16 828** entries:

| property | value | grade |
|---|---|---|
| first header word values | `{0: 6063, 1: 6063, 2: 4702}` — never `3` | VERIFIED |
| equals the entry's position in its set | **16 828 / 16 828** | VERIFIED |
| distinct 12-byte runs | **6** | VERIFIED |
| dominant run | `00000000 07010202 00521400` × 13 647 | VERIFIED |
| second | `00000000 0f030202 01521400` × 1810 | VERIFIED |
| third | `00000000 0f010202 01521400` × 1360 | VERIFIED |
| three rare runs | ×9, ×1, ×1 (`…6ddc08`, `…865409` ×2) | VERIFIED |

The header still carries no role field, now over 2.5× the reference population.
`claims::SAMPLER_ROLE`'s non-finding is unchanged and better supported.

---

## 3. Paint / colour: what the evidence actually is

### 3.1 There is a real paint subsystem, and it is `Skinner`

Found by grepping every `sdk_name` in the semantic passport
(`knowledge/semantic/function-passport-v1.jsonl`, 58 757 records) for
`palette|paint|tint|colou|team` — 60 distinct names, all in `Skinner::`,
`Palettes::`, `UI::` and `UTFWin::`:

```
Skinner::cPaintSystem::Get                              0x00401080
Skinner::cSkinPainter::CreateTextures                   0x00506590
Skinner::cSkinPainterJob::Initialize                    0x00517310
Skinner::cSkinPainterJob::DrawLayer                     0x00517160
Skinner::cSkinPainterJobPaintParts::Execute             0x00519640   (3331 B)
Skinner::cSkinPainterJobApplyBrushes::Execute           0x005182F0
Skinner::cSkinPainterJobColorDilateRepeat::Execute      0x005183C0
Skinner::cSkinPainterJobBumpToNormal::Execute           0x00518CF0
Skinner::cSkinPainterJobCopyTex1AlphaToTex0::Execute    0x00518BF0
Skinner::cSkinPainterJobCopyRigblocksTintMaskAlpha::…   0x0051A350
Skinner::cSkinPainterJobAmbientOcclusion::Execute       0x00518F10
Skinner::cSkinPainterPipeline::Initialize                0x0051A9A0
Skinner::cSkinPainterPipeline::PreloadTextures           0x0051AB50
Skinner::cSkinPainterPipeline::ExecutePipeline           0x0051AFD0   (5210 B)
Skinner::cSkinnerTexturePainter::StartRender            0x00528E90
Skinner::cSkinnerTexturePainter::SetColorWriteEnable     0x00529280
Skinner::cSkinnerTexturePainter::Clear                  0x005292D0
Skinner::cSkinnerTexturePainter::LoadMaterial           0x005293A0
Skinner::cSkinnerTexturePainter::PaintRegion            0x005296B0
Skinner::cSkinnerTexturePainter::EndRender              0x00529BF0
Palettes::cSwatchManager::Get                           0x00401020
Palettes::cSwatchManager::AddViewer                     0x005F0CA0
Palettes::PaletteItem::Load                             0x005C6810
Palettes::PaletteUI::GetSelectedRigblockPaint           0x005CB240
Palettes::PaletteCategoryUI::GetSelectedRigblockPaint   0x005C2E00
Graphics::MaterialShader::GetMaterialShader             0x011FA030
Graphics::ActiveState::SetShaderData                    0x00777B50
```

### 3.2 `cSkinPainterJobPaintParts::Execute` — where painted parts live

Decompiled at `0x519640` (Ghidra, `SporeApp.exe` 3.1.0.22). Grade **OBSERVED**
for the structure; the SDK type names Ghidra recovered
(`cEditorSkinMesh`, `RigblockData`, `cSkinPainter`, `ColorRGBA`,
`PropertyList`, `RenderTargetID`, `Rectangle`) are from the community SDK
import, so a name is `CONFIRMED`, a layout is `OBSERVED`.

| observation | detail | grade |
|---|---|---|
| the unit of painting is a **rigblock**, not a mesh | `RigblockData` stride **`0x8C`** (140 B), read as `*(param_1 + i*0x8c)`; count `= (end − begin) / 0x8c` | OBSERVED |
| a rigblock is skipped when `mFlags & 1` | `((RigblockData*)…)->mFlags & 1U) == 0` is the *continue* guard | OBSERVED |
| **three** prop keys, one per paint layer | `HasProperty(0x2424655)` (layer 0), `0x2424657` (layer 1), `0x2424656` (layer 2) | OBSERVED |
| the property yields a **resource id**, resolved to a **render target** | `GetPropertyObject` → `IRenderTargetManager::GetRegionViewport2` → `GetTexture` → `Texture::mpRaster` | OBSERVED |
| the painted area is an **atlas rectangle** | `local_70 = {Rectangle.left, .top, .right, .bottom}` → `PaintRegion(&painter, &rect, …)` | OBSERVED |
| the property list is **per rigblock**, held by the editor skin mesh | `((vector<intrusive_ptr<PropertyList>>*)(skinMesh + 0xc))->mpBegin[rigblockIndex]` | OBSERVED |
| **two colours are read from globals** per pass | `FUN_004098a0(&Skinner::sCurrentColors)` and `FUN_004098a0(&DAT_015df0fc)` → `mCustomParams[1].r/g/b`, `mCustomParams[2].r/g/b` | OBSERVED |
| a colour is a **4-float RGBA** | `ColorRGBA {r,g,b,a}` at `+0xc`, stride **`0x10`**, count at `+0x44` | OBSERVED |
| rasters are bound per pass | `Raster*` array at `+0x3c`, stride 4, count at `+0x48` | OBSERVED |
| four passes, ≤10 rigblocks each | `param_1+0x10` ∈ 0..3; `param_1+0x14` is the rigblock cursor | OBSERVED |
| per-pass colour-write masks | `(1,1,0)`, `(0,0,1)`, `(0,0,0)`, `(0,1,0)` | OBSERVED |
| per-pass shader ids | `0xd9ec39bc`, `0xa91b6551`, `0x03815422`, `0x968e3fff` | OBSERVED |
| pass 1 writes pure white | `r=g=b=a=1.0` | OBSERVED |
| pass 2 writes a grey read from the painter at `+0x30` | `fVar8 = *(float*)(painter + 0x30)` into `r=g=b`, `a=1.0` | OBSERVED |
| pass 3 writes a grey read from `+0x2c` scaled by a param | `*(float*)(painter + 0x2c) * mCustomParams[0].b` | OBSERVED |

**This is the answer to "where does paint state live":** in a **property list per
rigblock**, naming a **render target** and an **atlas rectangle**. Not in the
`gmdl` record, not in the raster, and not in a vertex stream.

### 3.3 The material registry exists and is runtime-populated

```
Graphics::MaterialShader::GetMaterialShader @ 0x011FA030  (19 B, matches the passport's size)
```

```c
MaterialShader *GetMaterialShader(uint32_t shaderID) {
  do {
    if (in_EAX->mID == in_ECX) { DAT_017186d4 = in_EAX; return in_EAX; }
    in_EAX = (MaterialShader *)in_EAX->field_40;
  } while (in_EAX != 0);
  return 0;
}
```

| fact | detail | grade |
|---|---|---|
| the registry is a singly-linked list, node stride **`0x40`** | `field_40` is `next` | VERIFIED |
| the key is a **32-bit `mID` at offset 0** | `in_EAX->mID == shaderID` | VERIFIED |
| a hit is cached in a global | `DAT_017186d4` | VERIFIED |
| the list is built at runtime | only 3 xrefs to `MaterialShader::Initialize` (`0x11F4440`), all in `.text`/init | VERIFIED |
| the population sites | `FUN_00713F20`, `FUN_00777840`, `FUN_011F45D0` | VERIFIED |

**This is the load-bearing confirmation of `claims::NO_MATERIAL_REGISTRY`:** the
real engine's material table is populated by three runtime calls with
*programmer-chosen* 32-bit keys, not read from the assets. The gmdl `material_id`
must therefore be a key into a table the game builds at startup, and **nothing in
the packages can name it**. `crates/spore-material`'s refusal is not a gap in this
crate — it is a property of the design. Grade **VERIFIED** for the mechanism,
**UNKNOWN** for any specific id's meaning.

### 3.4 The shader-data registry — and why `0x218`'s refusal is faithful

```
Graphics::RenderUtils::RegisterShaderData @ 0x011F4270
```

```c
void RegisterShaderData(ushort id, u32 a, u32 b, u32 c, u32 d) {
  (&DAT_016fa5f8)[id * 8] = a;   (&DAT_016fa5fc)[id * 8] = b;
  *(ushort *)(&DAT_016fa5e8 + id * 8) = id;   *(u32 *)(&DAT_016fa5ec + id*8) = 0;
  (&DAT_016fa5f0)[id * 0x20] = 0; (&DAT_016fa5f1)[id * 0x20] = 0;
  (&DAT_016fa600)[id * 8] = c;   (&DAT_016fa604)[id * 8] = d;
}
```

**72 call sites** in three functions (`FUN_0077E6D0` 60, `FUN_0067D0C0` 9,
`FUN_0082E470` 1, plus one from `FUN_0082E470`'s single site and the remainder in
`FUN_0077E6D0`). Extracted ids (Ghidra script, pushed as the last argument before
each call):

```
0x003 0x004 0x200
0x201 0x202 0x203 0x204 0x205 0x206 0x207 0x208 0x209 0x20a 0x20b 0x20c
0x20d 0x20e 0x20f 0x210 0x211 0x212 0x213 0x214 0x215 0x216 0x217
0x219 0x21a 0x21b 0x21c 0x21e 0x21f
0x220 0x221 0x222 0x223 0x224 0x225 0x226 0x227 0x228 0x229
0x22d 0x22e(×2) 0x22f 0x230 0x231 0x232 0x233 0x234 0x235 0x236 0x237 0x238
0x23b 0x23c 0x23e 0x241 0x242 0x243 0x244 0x245 0x246 0x247 0x248
0x24a 0x24b 0x252 0x254 0x255 0x256
```

| question | answer | grade |
|---|---|---|
| is `0x20D` (texture set) registered? | **yes** | VERIFIED |
| is `0x210` registered? | **yes** | VERIFIED |
| is `0x238` registered? | **yes** (`FUN_0067D0C0`) | VERIFIED |
| **is `0x218` registered anywhere?** | **NO — 0 of 72 call sites** | VERIFIED |
| other gmdl-visible gaps | `0x21D`, `0x22A`, `0x22B`, `0x22C`, `0x239`, `0x23A`, `0x23D`, `0x23F`, `0x240`, `0x249` also unregistered | VERIFIED |

**This settles an existing open question in the repo's favour.**
`crates/spore-gmdl/src/tables.rs:340` says "the gaps are real: ids such as
`0x207`, `0x214`, `0x215` and `0x218` are …" and
`crates/spore-gmdl/src/parse.rs:348` refuses `0x218` on 619 records.
**The game itself has no size entry for `0x218`.** The Rust refusal is therefore
faithful to the engine, not a gap in the port — and `MaterialRoute::Recovering`
(`crates/spore-engine/src/domain/materials.rs:176-190`) is the right answer for
those 619 records, not a workaround to be removed. Grade **VERIFIED**.

### 3.5 The runtime colour channel

```
Graphics::ActiveState::SetShaderData @ 0x00777B50  (160 B, matches the passport)
```

```c
void SetShaderData(short index, void *value, bool overrideIfEqual) {
  ... *(undefined4 *)(&DAT_016312f4 + DAT_01632bf0 * 8) =
        (&Graphics::ActiveState::sShaderData)[*puVar3];
      DAT_01632bf0 += 1; ...
}
```

and `cSkinnerTexturePainter::LoadMaterial` (`0x5293A0`) uploads colours then
finishes with `FUN_00777AE0(0x201, &DAT_015DEA6C, 0)`:

| fact | detail | grade |
|---|---|---|
| shader-data values live in a **global per-slot table** `sShaderData[id]` | read as `(&sShaderData)[*puVar3]` | VERIFIED |
| only *dirty* slots are appended to a draw list | the loop walks `puVar3[0..]` and appends `(id, value)` pairs | VERIFIED |
| the painter binds up to N `Raster*` and up to M 16-byte `ColorRGBA` values | `+0x3c + i*4` and `+0xc + i*0x10` | VERIFIED |
| the painter's colour uniform is **shader-data `0x201`** | `FUN_00777AE0(0x201, …)` | OBSERVED |
| `0x201` **is** registered | §3.4 | VERIFIED |

**Two different things must not be merged.** The gmdl `0x210` block (§2.3) is
*stored per record*; shader-data `0x201` is a *runtime uniform the painter sets*.
A creator that wants a team colour needs the runtime path, because §2.2 proved
the rasters are shared by up to 132 records. A creator that wants fidelity to
`Spore_Content` needs the `0x210` block. Both are needed and they are not the
same slot.

### 3.6 What is UNKNOWN — stated plainly, no paint model invented

| # | question | status |
|---|---|---|
| 1 | what words 1–4 of the `0x210` block mean | **UNKNOWN** — no shader read |
| 2 | whether words 2–4 are an RGB triple | **UNKNOWN**; shape is `INFERRED`, meaning is not |
| 3 | what `0x26445cNN` (word 0) selects | **UNKNOWN** |
| 4 | what `0x218` is (719 blocks, 619 records) | **UNKNOWN**; size unregistered by the game too |
| 5 | what `0x238`'s single byte indexes | **UNKNOWN**; `(v−1) % 8 == 0` in 15/15 |
| 6 | what the three prop keys `0x2424655/56/57` are *named* | **UNKNOWN**; three layers, one per key, is `OBSERVED` |
| 7 | which record type holds the per-rigblock `PropertyList` | **UNKNOWN** — not searched to ground this session (see §7) |
| 8 | how `Skinner::sCurrentColors` / `DAT_015df0fc` are set | **UNKNOWN** |
| 9 | whether any *team* colour concept exists | **UNKNOWN** — searched, nothing found (§7) |
| 10 | what `Palettes::*` reads as its data source | **UNKNOWN** — `plt` records are absent (§7) |
| 11 | whether a material id's value is a `MaterialShader::mID` | **INFERRED** — the key widths match (u32) but no call site pairs them |

---

## 4. Where paint state belongs

### 4.1 The split, and the invariant

**Invariant: a creator document must serialise with no GPU, no window, no
texture decode, and no `ContentStore`.** That is the same invariant the Rust
workspace already keeps for the asset layer (`AGENTS.md`: "everything above
`loader` is renderer-agnostic and Bevy-free"), and it is what makes a document
diffable, testable and hermetic.

| lives in the **creator document** (authoring data, pure data, `spore-material` or below) | lives in the **resolved runtime resource** (`spore-engine` / `spore-texture`) |
|---|---|
| which `ResourceKey` a part-instance's role points at | the decoded `SurfaceImage.pixels` |
| which layer of that record (`SurfaceSelector`) | the GPU handle from `texture::upload` (`texture.rs:316`) |
| authored scalars, as `f32`/`u32` **plus** a grade | the `StandardMaterial` built by `texture::standard_material` (`texture.rs:353`) |
| the `EvidenceLevel` + reason code for every parameter | the swizzle actually applied (`NormalMapPlan::swizzle`) |
| the prop-name slugs (`"0x2424655"`), never a decoded name | the resolved role census (`ModelMaterials::role_census`) |
| **nothing** decoded, **nothing** uploaded | |

### 4.2 Named types

Authoring side (new; renderer-agnostic, in or beside `spore-material`):

```rust
/// One authored parameter. `Fact` is `spore_core::evidence::Fact` — the same
/// envelope the format crates use, so `unavailable` is representable and a
/// default is not.
pub struct AuthoredParam {
    /// Stable slug. Never a decoded game name: `MATERIAL_NAME` is a non-finding.
    pub name: String,
    pub value: ParamValue,
    pub claim: Fact<ParamValue>,
}

pub enum ParamValue {
    F32(f32),
    U32(u32),
    /// 0xRRGGBB / 0xRRGGBBAA. A creator-side convention; NOT a claim that the
    /// game stores a packed colour, and NOT the `0x210` block.
    Rgba8(u32),
    /// Reference into the model's own resolved surfaces. Never pixels.
    Surface { key: ResourceKey, selector: SurfaceSelector },
}

/// `None` = textures were never requested. `Some` = asked, and this is what
/// came back. The three-way distinction `model.rs:91-96` insists on.
pub struct MaterialAuthoring {
    pub roles: Option<Vec<AuthoredRole>>,
    pub params: Vec<AuthoredParam>,
}
```

Runtime side (already exists, plus one additive field):

* `SurfaceImage` (`texture.rs:149`) — decoded, uploaded, disposable.
* `SurfaceSet` (`texture.rs:203`) — one mesh's surfaces.
* `BoundMaterial` (`texture.rs:388`) — what `bind_material` produces.
* **new:** `surface_overrides: Option<Vec<PartSurfaceOverride>>` on
  `ModelInstance` (`model.rs:238`).

The separation is one-directional and total: **document → runtime** only. Nothing
in the resolved path may write back into the document, exactly as
`spore-semantic`'s runtime overlay never writes a runtime fact back into a static
artifact.

---

## 5. Determinism: what must be stable so reload reconstructs the identical image

Ordered by how easily each is got wrong.

1. **Raster ids, never decoded pixels.** The document stores `ResourceKey` +
   `SurfaceSelector`. Reload re-runs `spore-material`'s one resolution path
   (`resolve.rs:511` `resolve_model_record`, or `:493` on the recovering route).
   Storing pixels would make the document 512×512×4 × surfaces per part and would
   make it depend on `spore-texture`'s codec, whose four deviations are
   byte-pinned and deliberately not "corrected"
   (`crates/spore-material/src/claims.rs:350-355`).
2. **The `assumed_type` word must be stored, because it is not in the record.**
   `TextureSlot::key(assumed_type)` takes it as a parameter
   (`material.rs:127-133`) and `claims::ASSUMED_RECORD_TYPE` grades it `Inferred`
   precisely because the reference carries no type word. A document that omits it
   re-guesses on reload. Store the number and the grade, not the conclusion.
3. **Role *names*, not role enums, in the document.** `TextureRole` is a closed
   enum derived from content statistics (`role.rs:470` `classify`). If the
   document stores `TextureRole::TangentSpaceNormal`, a future corpus that
   reveals a third role either breaks the format or silently re-labels. Store the
   role **name string** plus the grade; resolve it at load. This is the same
   reason `SamplerRole` has exactly one variant
   (`material.rs:493-504`) — an enum variant is a claim.
4. **`scope` must be stored, not recomputed.** `BindingScope` decides
   `flat = mesh_index * refs_per_mesh + within` (`materials.rs:372-379`). If the
   document stores per-mesh `SurfaceKey`s the cut is already applied and
   `scope` is irrelevant; if it stores flat indices, `scope` **must** be stored,
   because a record that divided evenly last week may not divide evenly after a
   corpus change and `BindingScope::is_inferred()` is `true` for both variants.
   Prefer storing the resolved surface key per role: it removes the dependency.
5. **Ordering: slot order, then layer order, then declaration order.**
   `MeshMaterial.slots` is in record order (`material.rs:447-457`);
   `SurfaceSet.masks` is "in slot then layer order"
   (`texture.rs:206-207`). Both are stable, and a document must preserve them
   rather than sorting by anything else — a hash map's iteration order is not a
   format.
6. **The normal-map swizzle must be a recorded value, not an implicit step.**
   `swizzle_z_first_to_xyz` (`materials.rs:112-123`) is a *permutation decided by
   `NormalLayout`*, and `NormalLayout` has exactly one variant today
   (`materials.rs:70-87`). Store `layout: "z-first"`; if a second layout ever
   appears, a document that stored the bytes-free layout name reloads correctly
   and one that stored nothing does not.
7. **Mip policy must be explicit.** `SurfaceImage::image()` sets
   `ImageSampler::linear()` with a comment that mip selection is off by
   construction (`texture.rs:181-186`). A document must record "base mip only",
   because uploading mips would change the image and nothing in the document
   would say so.
8. **Grade every parameter, and let the grade be part of the comparison.** Two
   documents that produce the same pixels with different grades are *not* the
   same document; a diff must show the grade change. This is the same discipline
   as `spore-semantic`'s `content_sha256` covering record lines only.
9. **No float in a hash.** `f32` round-trips exactly through JSON only if the
   writer is careful; the `0x210` corpus already shows this trap with
   `50.000004` and `12.749999` (§2.3). Store floats as their bit pattern
   (`u32::to_bits`) or as fixed-point, and canonicalise on write.
10. **`Option::None` round-trips as absent, `Some(vec![])` as present-and-empty.**
    Both appear in §1.3; collapsing them is the single most likely
    determinism bug in this design.

---

## 6. Evidence-preserving openness

### 6.1 The shape

A material/paint concept whose game semantics are `UNKNOWN` must be
**representable without asserting the semantics**. Concretely: an *open* list of
named parameter entries, each carrying its own grade, where **adding a meaning
later fills a `Fact` and does not change a byte of the format**.

```rust
/// One claim about a parameter, using spore-core's envelope unchanged:
///   state: available | unavailable
///   evidence_level: EvidenceLevel
///   evidence_state: Observed | Derived | Persisted | Missing
///   provenance: Vec<Provenance>
///   value: Omitted when unavailable
///   reason: closed-vocabulary code
///
/// Closed reason vocabulary for this domain (adding a code is a minor version;
/// the SET is closed, so a reader can exhaustively match):
pub mod paint_reason {
    pub const NO_PAINT_RECORD_IN_ANY_PACKAGE: &str = "no_paint_record_in_any_package";
    pub const GAME_HAS_NO_SIZE_FOR_SHADER_DATA_ID: &str = "game_has_no_size_for_shader_data_id";
    pub const NO_MATERIAL_REGISTRY: &str = "no_material_registry";           // reuse
    pub const NO_ROLE_FIELD_IN_TEXTURE_ENTRY: &str = "no_role_field_in_texture_entry"; // reuse
    pub const SHADER_DATA_WORD_MEANING: &str = "shader_data_word_meaning";
    pub const PROP_NAME_UNRESOLVED: &str = "prop_name_unresolved";
    pub const MESH_HAS_NO_PAINT_SLOT: &str = "mesh_has_no_paint_slot";
}

/// The document's material block. Note what is NOT here: no `MaterialName`,
/// no `Diffuse`, no `Specular`, no `TeamColor`. Each of those would be a claim.
pub struct MaterialBlock {
    /// `None` = textures were never requested (NOT "no textures").
    pub resolved: Option<ResolvedSummary>,
    /// Authored roles, keyed by a stable slug. `None` = inherit the record.
    pub roles: Option<Vec<AuthoredRole>>,
    /// Open parameter list. Order is stable and significant.
    pub params: Vec<AuthoredParam>,
    /// Every non-finding this document carries, so a reader can enumerate what
    /// is missing without scanning for absence.
    pub unknowns: Vec<UnknownClaim>,
}

pub struct UnknownClaim { pub subject: String, pub reason: &'static str }
```

### 6.2 Why a future finding costs zero format bytes

| a future discovery | what changes | what does **not** |
|---|---|---|
| `0x218`'s real size | `UnknownClaim { subject: "shader_data_0x218", reason: … }` becomes a graded value in `params` | the field list, the version, the ids of anything else |
| a `MaterialShader::mID` ↔ `gmdl.material_id` pairing | `params` gains `"material_shader_key"` at `Confirmed` | `roles`, `SurfaceSelector`, the ordering |
| words 2–4 of `0x210` proven to be RGB | `params["shader_0x210_word2"].claim.level` rises `Unknown → Confirmed` | the stored `f32` — it was stored as a number, never as a colour |
| a mask plane's purpose established | a *new* `AuthoredRole` slug appears | `SamplerRole` stays one-variant until a shader is read; `TextureRole` gains a variant only when a record earns it |
| a `plt` palette record turning up in a DLC | a new record type enters the resolver | the document format |

The rule that makes this work: **a parameter is stored as a value plus a grade,
never as a value plus a meaning.** If the meaning were in the type, the format
would break when the meaning changed.

### 6.3 The three rules that keep it honest

1. **A non-finding is never a default.** `claims.rs:646-688` asserts every
   non-finding is `Unknown` + `Missing` + **no provenance** + `value == None`.
   The same test must run over `MaterialBlock.unknowns`.
2. **A name is a name, not a meaning.** `AuthoredRole.name` is author-chosen.
   A *decoded* name requires a persisted table plus a grade — the pattern
   `graded_persisted` implements (`claims.rs:607-623`) and the reason
   `spore-core` refuses to add `0x40632900/01/02` to `GROUP_NAMES`
   (`crates/spore-material/src/lib.rs:119-132`).
3. **A grade change is a document change.** A hash covering the document must
   cover the `params` grade fields, so "we learned something" shows up as a diff
   rather than passing silently.

---

## 7. Negative results — what was searched

Every one of these is a deliverable. Reproduce with the commands shown.

### 7.1 In the packages (`./SPORE`, read-only)

| searched | command | result |
|---|---|---|
| palette records | `osptool list <pkg> --type 0x011989b7 --limit 0` for all 7 packages | **0 records everywhere.** `plt` is in the canonical type table (`osptool types`) and appears in no package |
| every record type, all 7 packages | `osptool types --package <pkg>` | `Spore_Content` holds **10 types**; no colour, palette, tint or paint type in any package |
| the `0x6b` "palette" category | `osptool list Spore_Content.package --group 0x406b6200 --limit 0` | 268 records (`prop`, `gmdl`, `world_object`, `bem`) — a **group**, not a palette record type. `docs/RESOURCE-GRAPH.md:174` calls `0x6b` a palette *stage byte*; that is a naming convention |
| models whose texture ref is missing | `measure5.py` §F | 150 entries naming **group id `0`** in 132 records, 0 of which resolve — the same 132 that carry `0x238` (132/132) |
| vertex-colour channels | `measure2.py` vertex-element histogram | **0 of 4209 records declare a `COLOR` usage (10).** Only 6 (usage, declType) pairs exist: `POSITION/FLOAT3`, `NORMAL/UBYTE4`, `TEXCOORD/FLOAT2`, `BLENDINDICES/UBYTE4`, `TANGENT/UBYTE4`, `BLENDWEIGHT/UBYTE4N` |
| `gmdl`→`reference` type words | `measure5.py` §H | **exactly 1** type word `0x1B684E2F` over 7989 references, 4 distinct groups — so no palette/type variety hides in the reference list |
| the `0x210` word as a colour | `measure6.py`, `measure7.py` | word 3 ≡ `1.0` in 6063/6063; words 2/4 on 0.19 and 0.07 grids. **Shape only**; meaning `UNKNOWN` |
| a per-mesh colour anywhere in a record | `measure5.py` §B, `measure6.py` | **0 of 4208** records have differing material ids or differing `0x210` payloads across their meshes |

### 7.2 In the binary (`SporeApp.exe` 3.1.0.22)

| searched | how | result |
|---|---|---|
| every symbol named palette/paint/tint/colour/team | `grep -o -E '"sdk_name":"[^"]*(palette\|paint\|tint\|colou\|team)[^"]*"' knowledge/semantic/function-passport-v1.jsonl` | 60 distinct names, all in `Skinner::` (19), `Palettes::` (30), `UI::` (1), `UTFWin::` (6). **No `TeamColor`, no `Tint`, no `Color` type** |
| `spore-semantic lookup-symbol` | `lookup-symbol Palette / Paint / TeamColor / Tint / Color / Colour` | all six return `unknown_symbol` (exit 3) — the CLI is exact-match, so this is a weak negative and the grep above is the real one |
| the material registry | decompile `Graphics::MaterialShader::GetMaterialShader` @ `0x11FA030` | a runtime-linked list, stride `0x40`, keyed on a u32 `mID`; **3** population sites |
| the shader-data registry | decompile `0x11F4270`; enumerate all 72 call sites with a Ghidra script | 71 distinct ids; **`0x218` absent** |
| the runtime colour channel | decompile `0x777B50`, `0x5293A0`, `0x519640` | `sShaderData[]` table; painter writes shader-data **`0x201`**; per-rigblock property lists name render targets + atlas rects |
| the knowledge graph | `openspore_kg_query name_substring=paint / palette / Palette` | **empty** — `knowledgegraph/spore.db` is 109 MB but the MCP sidecar returned nothing for these names |
| Ghidra symbol search | `ghidra_search_functions name_pattern=ShaderData` | 1 hit, `Graphics::ActiveState::SetShaderData` |

### 7.3 Not searched (stated so it is not mistaken for a negative)

* **The SDK headers.** `AGENTS.md` places them at `~/apps/Spore-ModAPI`, but
  that directory **does not exist on this machine** (`ls ~/apps/` →
  `ghidra-mcp` only). Every SDK-derived name in §3.1 therefore comes from the
  committed `sdk_functions.tsv` via the passport, not from re-reading headers.
* **`Palettes::PalettePage::ReadItemsModule` / `PaletteItem::Load`.** Located at
  `0x5C7FF0` / `0x5C6810` but not decompiled — these are the most likely place a
  palette *record* would be named, and `plt` has no records, so the negative in
  §7.1 stands regardless.
* **Which record type holds the per-rigblock `PropertyList`.** `0x519640` reads
  it through `cEditorSkinMesh::mpCreatureData->mRigblocks` at `skinMesh + 0xc`;
  identifying the on-disk record behind that is a separate task.
* **`RigblockData`'s `0x8C` layout.** Only the stride and `mFlags & 1` were read.

---

## 8. Corrections to documents this work touches

Recorded rather than edited — none of these files is in this agent's ownership.

| # | source | said | the measurement says |
|---|---|---|---|
| 1 | `docs/MATERIAL-ROLES.md` §2.3 | the Python oracle "raises on 2390; its `f32` unpacks run off the end of 2390 records" | the raise is **after** the material-info section. An instrumented walk with the oracle's own offsets completes the material-info section on **4209 of 4209** rows and is **byte-identical to the stock parser on 1819 of 1819** rows where the stock completes (`measure4.py`). The 1819 figure is a limit of the oracle's *animation* tail, not of the material section — so every material census in that document can be widened from 1819 to 4209 |
| 2 | `docs/MATERIAL-ROLES.md` §6 | the header census is over "6684 references" | the full population is **16 828 entries**, and the first header word still equals the entry position in **16 828/16 828**. The conclusion is unchanged and better supported |
| 3 | `docs/MATERIAL-ROLES.md` §7.5 | "132 models … 150 references naming group id 0" is presented as an unexplained finding | those **132 records are exactly the 132 that carry a `0x238` entry** (132/132), and every `0x238` payload byte satisfies `(v−1) mod 8 == 0` (15/15 distinct). The correlation is exact; the meaning is `UNKNOWN` |
| 4 | `docs/MATERIAL-ROLES.md` §9.9 | "A material id's meaning — `UNKNOWN`, `no_material_registry`, unchanged" | confirmed **and mechanised**: `MaterialShader::GetMaterialShader` is a runtime-populated list keyed on a u32, with 3 population sites. The registry is not in the data by construction |
| 5 | `docs/ASSET-PATH.md:23` | `0x210` is listed with no note beyond "shader ids `0x20D` + `0x210`" | the `0x210` payload is 20 bytes with **65 distinct values**, word 0 in `{0x26445c01,04,05,07}`, and word 3 **≡ 1.0** — worth stating so a future reader does not assume it is a scalar |
| 6 | `docs/CELLSTAGE-RECON.md:109` | "`shader 0x210 (20B ShaderData, fixed-function) — INFERRED params`" | the shape claim is right and the grade is right; the block's **per-record constancy (4208/4208)** is a stronger, measurable statement than "INFERRED params" and should replace it |

---

## 9. Reproduction

```bash
# corpus + type tables
target/release/osptool --help
target/release/osptool types --package SPORE/Data/Spore_Content.package
for p in SPORE/Data/*.package; do
  target/release/osptool list "$p" --type 0x011989b7 --limit 0   # plt: 0 records, all 7
done
target/release/osptool list SPORE/Data/Spore_Content.package --group 0x406b6200 --limit 0

# the material census (read-only; imports tools/spore/{dbpf,gmdl} only)
python3 /tmp/opencode/creator-materials/measure4.py   # validates the walk vs the stock parser
python3 /tmp/opencode/creator-materials/measure5.py   # ids, uniformity, sharing, recipes
python3 /tmp/opencode/creator-materials/measure6.py   # the 0x210 block, in full
python3 /tmp/opencode/creator-materials/measure7.py   # recipes, 0x238 bytes, quantisation

# the binary
spore-semantic stats
grep -o -E '"sdk_name":"(Skinner|Palettes)::[^"]*"' \
  knowledge/semantic/function-passport-v1.jsonl | sort -u
spore-semantic lookup-symbol Palette        # -> unknown_symbol
# Ghidra: decompile 0x519640 0x11FA030 0x11F4270 0x777B50 0x5293A0
#         xrefs to 0x11F4270 (72 sites), enumerate the pushed id per call
```

The measurement scripts live in `/tmp/opencode/creator-materials/` and import
**only** `tools/spore/dbpf` and `tools/spore/gmdl` from this repository. They
read `SPORE/` and write nothing into the checkout. `measure4.py`'s
`walk_materialinfo()` is the oracle's own offset arithmetic, lifted verbatim and
cross-checked against the stock `Gmdl.parse` — that check is what licenses
extending the census from 1819 to 4209 records, and it is asserted, not assumed.

---

## 10. Clean-room

Nothing here is derived from EA or Maxis code, data or assets. Layouts were read
out of record bytes via this repository's own Python oracles; names come from the
community SDK as transcribed in `knowledge/semantic/function-passport-v1.jsonl`;
structure comes from reading the shipped binary's own instructions. No Spore
asset byte is committed or reproduced here.
