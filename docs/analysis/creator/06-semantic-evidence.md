# 06 — What the original binary says about creators

Agent: **EVIDENCE** (read-only research; no code edits, no commits).
Binary: `SporeApp.exe` **3.1.0.22**, `sha256 25d42a7a5c4d438f…`, image base `0x00400000`.
Sources: `spore-semantic` passport (58 757 records, read-only), Ghidra MCP (headless
project, live DB), `.spore-analysis/ghidra-exports/`.

Companion document: `docs/analysis/creator/02-parts-and-catalogs.md` established the
**asset-side** facts (one rigblock record type shared by every family; the
`<sporemodel>` XML being a flat `<blocks>` array with `<childid>` = parent ordinal and
`<symmetric>` = mirror ordinal). This document establishes the **code-side** facts, and
in §4 it names the compiled binary record that corresponds to those XML fields.

> **No runtime evidence was used.** Nothing below is justified by `spore-semantic runtime`
> or `spore-recomp`. Every claim is static (bytes, disassembly, decompilation, or an SDK
> data type imported into Ghidra). Where something would need runtime evidence to settle,
> it is listed in §5 as UNKNOWN rather than inferred.

---

## 0. Executive summary — the shared-base question

**Split verdict. The corpus justifies a shared creator core for the DATA layer and
does NOT justify one for the BEHAVIOUR layer.**

| Question | Answer | Grade |
|---|---|---|
| Is there **one generic part/limb record** used by every creator? | **Yes.** `Editors::cEditorResourceBlock`, **472 bytes**, named generically (not `cCreature*`). Fields: `parentIndex`, `symmetricIndex`, `position`, `orientation` (Matrix3), `userOrientation` (Matrix3), `snapped`, `isAsymmetric`, `limbType`, `muscleScale`, `baseMuscleScale`, 8 morph handles (weight + channel), 8 paint slots (region + id + 2 colours). | **CONFIRMED** |
| Is there **one generic created-entity container**? | **Yes.** `Editors::cEditorResource`, **172 bytes**: `ResourceObject` vftable, refcount, `ResourceKey mNameKey`, `cEditorResourceProperties mProperties` (128 B, starts with `mModelType`), `vector<Editors::cEditorResourceBlock> mBlocks`. | **CONFIRMED** |
| Is the **creature** format an extension of that generic one? | **Yes, structurally.** `Editors::cCreatureDataResource` (296 B) has a byte-identical layout prefix `0x00..0xAB` — vftable, refcount, `mNameKey`, properties, the block vector — and then *appends* creature-only vectors (`mCapabilityIDs`, `mCapabilityLevels`, `mAnimationIDs/Values/Weights`, `mEffects`). | **CONFIRMED** (prefix identity); **INFERRED** (that it is C++ inheritance rather than a duplicated struct) |
| Is there **one shared editor/part class across creature, vehicle and building**? | **Not demonstrable.** Only the creature editor has SDK-named classes. There is **no** `cVehicleEditor`, `cBuildingEditor`, `cUfoEditor`, no vehicle/building part class, and no vehicle counterpart to `EditorRigblock`. The only vehicle types in the whole SDK surface are `Simulator::cVehicle` and `Simulator::cVehicleData`, which are *gameplay* classes. | **CONFIRMED negative** for the named surface; **UNKNOWN** for the unnamed surface |
| Is there *anything* shared at the mode/enum level? | **Yes, thin.** One enum `kEditorType = {Cell, Creature, Building, Vehicle, Accessories}` with per-type arrays (`mActionStatusByEditor[5]`, `mNumEditorEntriesByEditor`) on a single class, and one cheat command (`editor validate`) whose help text says "buildings/vehicles/creatures/etc.". | **SUPPORTED** (strings + registration code), see §2.3 |

**Therefore:** a *Creator Core* covering **part records, the container, the property
block, paint slots, capabilities and manipulator handles** is justified by the corpus —
that is where the original factored generically, and the naming (`cEditorResource…`,
not `cCreature…`) is the original's own signal. A Creator Core covering **editor
behaviour** (the tool loop, the mode, the part palette) is **not** justified by the
corpus: the vehicle and building editors exist in the binary but carry no symbols, so
whether they reuse `cEditor`/`EditorRigblock` or reimplement them is **UNKNOWN** and
must not be assumed either way. See §2 and §5.

---

## 1. Creature / editor / creator classes and functions

### 1.1 Method-grade inventory (name + VA + one-line behaviour + grade)

The `VA` column is the **canonical** function entry from the frozen universe
(`.spore-analysis/ghidra-exports/functions.tsv`), lower-case `0x%08x`. Where the SDK
declared a *different* address that resolves into this entry, both are given, because
the delta is itself evidence (§6.1).

**Editor object (`/Spore/Editors/cEditor`, 1536 B, `IGameMode`-derived)**

| VA | SDK name | One-line behaviour | Grade |
|---|---|---|---|
| `0x00584300` | `Editors::cEditor::Initialize` | brings up the editor mode: viewer, model worlds, palettes, UI, anim world | CONFIRMED (name from SDK at an exact entry) |
| `0x00587270` | `Editors::cEditor::SetActiveMode` | switches `Editors::Mode` (EditMode ↔ PlayMode) | CONFIRMED (exact entry) |
| `0x00586b00` | `Editors::cEditor::SetEditorModel` | binds an `EditorModel` (224 B) as the editing target | CONFIRMED (exact entry) |
| `0x00586410` | `Editors::cEditor::CommitEditHistory` | **snapshot-based undo commit** — see §1.3 | CONFIRMED (exact entry + decompiled body) |
| `0x0058a5a0` | `Editors::cEditor::Undo` | step the history index back | CONFIRMED (exact entry) |
| `0x0058a950` | `Editors::cEditor::Redo` | step the history index forward | CONFIRMED (exact entry) |
| `0x005737d0` / `0x00585890` / `0x0058ac10` / `0x00588570` / `0x0058b650` / `0x0058be50` | `cEditor::OnMouseMove` / `OnKeyUp` / `OnKeyDown` / `OnMouseDown` / `OnMouseUp` / `Update` | the input loop | CONFIRMED (exact entries) |
| `0x0058e6d0` / `0x0058a5a0`… | `cEditor::OnEnter` / `OnExit` | mode entry/exit; `OnExit` tears down the anim world | CONFIRMED |
| `0x00591fa0` | `cEditor::HandleMessage` | message dispatch | CONFIRMED |
| `0x00582fe0` | `cEditor::AddCreature` | add a creature actor to the editor scene | CONFIRMED |
| `0x00573970` | `cEditor::SetCreatureToNeutralPose` | reset the preview pose | CONFIRMED |
| `0x00574110` | `cEditor::PostEventToActors` | fan an event out to preview actors | CONFIRMED |
| `0x00576c50` | `cEditor::Dispose` | teardown | CONFIRMED |
| `0x0057f6c0` | — (unnamed) | **the editor Save/Export transaction** — bakes via `IBakeManager`, writes `"%ls/%ls.xsf"` and `"%ls/%ls.zpr"`, emits `L"Model saved to %ls"`, and registers the profile name **`cAppModeEditorBase::Save`**. Grade: **CONFIRMED** for the behaviour (quoted strings + call graph), **UNKNOWN** for the class name (a bare string literal, see §2.3). |

**Model / part objects**

| VA | Name | Behaviour | Grade |
|---|---|---|---|
| `0x004b9c70` | — (unnamed) | **`EditorModel` factory** — `FUN_00f473a0(0xac, "Editor")` then this; the same 0x2c-byte-profile object is pushed onto `cEditor::mEditHistory` by `CommitEditHistory`. Grade **INFERRED** (constructor identified from two independent call sites + the 224 B `EditorModel` layout it matches). |
| `0x004ae3b0` | SDK-attributed `Editors::EditorModel::GetCreationName` (**name is a COLLISION — see §6.1**) | **the rigblock-assembly loader** — §3.1, quoted. Grade **CONFIRMED** for behaviour. |
| `0x004346b0` | — (unnamed) | **`EditorRigblock` constructor** — 3535 B; initialises 25+ fields at exactly the offsets the SDK `EditorRigblock` layout predicts, including all nine `fixed_vector<ResourceKey,8>` snap lists. Grade **CONFIRMED** (see §6.2 for the match list). |
| `0x0043ac40` | `Editors::EditorRigblock::SetShadedDisplay` (SDK `0x0043acf0` is +176 inside) | per-part display shading | SUPPORTED (name→entry binding is an interior hit, see §6.1) |
| `0x004ba150` | — (unnamed; references the string `Editor/cEditorResource`) | `cEditorResource`-family function in the `0x004b9c70..0x004b23a0` cluster | INFERRED |

**Skin / paint / mesh**

| VA | Name | Behaviour | Grade |
|---|---|---|---|
| `0x004c5200` | `Editors::cEditorSkin::PaintSkin` (SDK `0x004c5270` = +112) | paint the skin mesh | SUPPORTED (interior binding) |
| `0x004c4a50` | `Editors::cEditorSkin::GetMesh` | current skin mesh | SUPPORTED |
| `0x004c5920` | `Editors::cEditorSkin::IsPainting` | paint-mode predicate | SUPPORTED |
| `0x004c73f0` | `Editors::cEditorSkinMeshBase::FromRigblocks` (SDK `0x004c7460` = +112) | **12342 B**; builds the render/skin mesh from the rigblock tree — the part→mesh bridge | SUPPORTED |
| `0x005cb240` | SDK names `Palettes::PaletteUI::GetSelectedRigblockPaint` **and** `Palettes::PaletteUI::SetActiveCategory` on this **one** entry | part/paint palette selection | SUPPORTED (name collision, §6.1) |
| — | `/Spore/Skinner/cSkinPainterPipeline`, `cSkinPainter`, `cSkinnerTexturePainter`, `cSkinPainterJobPaintParts`, `…CopyRigblocksTintMaskAlpha`, `cPaintSystem` (data types only, no SDK function addresses) | the paint job pipeline | CONFIRMED as *type names*; UNKNOWN as behaviour |

**Animation preview / play mode**

| VA | Name | Behaviour | Grade |
|---|---|---|---|
| `0x0059ca70` / `0x0059cac0` / `0x0059c6e0` / `0x0059cea0` / `0x0059cf00` | `cEditorAnimWorld::GetAnimatedCreature` / `GetCreatureController` / `DestroyCreature` / `SetTargetAngle` / `SetTargetPosition` | the **preview** world: build, address, animate, destroy | CONFIRMED (exact entries) |
| `0x0059b0f0` / `0x0059b4b0` | `EditorCreatureController::SetTargetPosition` / `Update` (2911 B) | preview locomotion controller | CONFIRMED |
| `0x0059d840` / `0x0059d8b0` | `cEditorAnimEvent::MessagePost` / `MessageSend` | preview event injection | CONFIRMED |
| `0x00628550`…`0x0062c940` (11 methods) | `Editors::EditorPlayMode::*` | play mode: `Load`, `Update`, `Dispose`, `HandleUIButton`, `OnMouse*`/`OnKey*`, `GetNextEnvironmentReactionAnimID` | CONFIRMED |
| `0x0062ce30`…`0x0062f210` | `PlayModeActor::AddWalkAction`, `PlayModeAnimations::{GenerateUI,ReadAnimButton,ReadPanel,ReadAnimations}` | walk cycles + animation UI | CONFIRMED |
| `0x0062f370`…`0x0062fbf0` (9 methods) | `PlayModeBackgrounds::*` | environment backdrops | CONFIRMED |
| `0x00634db0` / `0x00635750` / `0x006377b0` | `PlayModeUI::FindWindowByID` / `SetWindowVisible` / `HandleUIMessage` | play-mode UI | CONFIRMED |

**Camera / UI**

| VA | Name | Behaviour | Grade |
|---|---|---|---|
| `0x005a2030`…`0x005a3e00` (15 methods) | `Editors::EditorCamera::*` (`Cast`, `Initialize`, `Update`, `OnMouse*`, `OnKey*`, `OnEnter`, `OnExit`, `AddRef`@`0x00671f20`, `Release`@`0x00ae0620`) | editor camera | CONFIRMED |
| `0x005ddda0` / `0x005deeb0` / `0x005e0040` | `Editors::EditorUI::Load` / `HandleMessage` / `HandleUIMessage` | editor chrome | CONFIRMED |
| `0x005bfad0`…`0x005c0470` (5 methods) | `UI::EditorNamePanel::*` | the name/description panel | CONFIRMED |
| `0x005a92c0` | `Editors::EditorRequest::Submit` | editor→simulator request | CONFIRMED |
| `0x00b1de20` | `Simulator::cGameModeManager::SubmitEditorRequest` | the receiving end (SDK name at `0x00b1dc60`+1936 → **collides** with `GetActiveDifficultyTuning`, §6.1) | SUPPORTED |

### 1.2 Data-type inventory (structure grades)

These come from the ModAPI SDK import (1 895 structures) and are the **strongest**
part of the corpus for this question, because a struct is a *layout* claim, not a
name-on-an-address claim.

| Type | Size | Role |
|---|---:|---|
| `/Spore/Editors/cEditorResource` | 172 | **generic** created-entity container (§4) |
| `/Spore/Editors/cEditorResourceBlock` | **472** | **generic** part/limb record (§4) |
| `/Spore/Editors/cEditorResourceProperties` | 128 | `mModelType`, `mZCorpScore`, 3× skin effect id, 3× skin effect seed, 3× `ColorRGB`, 60 B padding |
| `/Spore/Editors/cCreatureDataResource` | 296 | generic prefix + `mCapabilityIDs`, `mCapabilityLevels`, `mAnimationIDs/Values/Weights`, `mEffects` |
| `/Spore/Editors/cCreatureDataResource::RigblockData` | 140 | **CONFLICT** — see §6.2 |
| `/Spore/Editors/cCreatureDataResource::RigblockDataType` | 1 (enum) | `NullBlock=0, Unk1, Standard=2, Unk3, PlantRoot=4, Vertebra=5` |
| `/Spore/Editors/cCreatureDataResource::CapabilityTag` | 4 | `char[4]` — a four-character capability code |
| `/Spore/Editors/EditorModel` | 224 | in-memory editing model: `mKey`, `vector<intrusive_ptr<EditorRigblock>> mRigblocks`, `mModelType`, name/description/accepted-name, 3× skin effect, 3× `ColorRGB`, `mZCorpScore` |
| `/Spore/Editors/EditorRigblock` | 3592 | the editing-side part: `mpParent`, `mChildren[8]`, `mpSymmetricRigblock`, `mpAsymmetricRigblock`, 6 × `Matrix3`, `mPosition`, snap/align/interact key lists, 20 capabilities, 8 paints, morph handles (§3) |
| `/Spore/Editors/EditorRigblockBoolAttributes` | 4 (enum, 48 values) | the **64-bit rule bitset** on `EditorRigblock.mBooleanAttributes` — every snap / symmetry / connector rule (§3.2) |
| `/Spore/Editors/EditorBaseHandle` | 80 | the manipulator/handle abstraction: 19-slot vftable, `mpRigblock`, `mpModel`, `mDefaultScale`, fade/animate timings, `mDefaultModel` key |
| `/Spore/Editors/EditorStateEditHistory` | 44 | undo state record: `mBudget`, `Mode mMode`, 5 ints, 2 bools |
| `/Spore/Editors/cEditor` | 1536 | the editor mode object (§1.4) |
| `/Spore/Editors/IEditorLimits` / `StdEditorLimits` | 8 / 48 | the "editor limits" interface (6 slots) + implementation, with `EditorLimitType`, `AddValue`, `EnsureLimit`, `GetValue`, `SetLimit`, `HasEnough` |
| `/Spore/Editors/cEditorSkin`, `cEditorSkinMesh`, `cEditorSkinMeshBase` | 136 / 304 / 188 | skin + derived mesh |
| `/Spore/Editors/cEditorAnimWorld`, `EditorCreatureController`, `cEditorAnimEvent` | 72 / 136 / 48 | preview world |
| `/Spore/Editors/EditorPlayMode` | **22416** | play mode — note the size: the play mode is *huge*, i.e. not a thin subclass |
| `/Spore/Editors/IPlayModeModule` + `PlayModeAnimations` / `PlayModeBackgrounds` / `PlayModeUI` | 4 / 140 / 72 / 112 | play mode is **modular**: a 6-slot interface with three named modules |
| `/Spore/Editors/IBakeManager` | 4 (20-slot vftable) | the bake step |
| `/Spore/Editors/EditorRequest`, `EditorUI`, `EditorCamera`, `VerbIconData` | 156 / 300 / 236 / 196 | request, chrome, camera, palette icon |
| `/Spore/Anim/CreatureBlock` | 1128 | the **runtime** creature block (see §3.3) |
| `/Spore/Anim/CreatureBlock::Capabilities` | 168 | `capFlags`, `capLvlFlags`, `int* capabilities[32]`, `int8_t capLevels[32]` |
| `/Spore/Anim/CreatureBlock::Deforms` | 164 | `Deform[8]`, each `{id, field_4, f32, field_C, f32}` |
| `/Spore/Anim/IAnimWorld` + `AnimatedCreature` | — / 412 | the preview/render-side animal interface (`LoadCreature`, `LoadCreatureByName`, `GetCreatures`, `UpdateCreatures`, `PostUpdateCreatures`, `DestroyCreature`, `PlayAnimation`, `SetAnimationID`, `SetLoopTimes`, …) |

Total `/Spore/Editors/*` data types in the SDK surface: **469**. All of them are
creature/play-mode/UI. **Zero** mention vehicle, building, UFO or terrain.

### 1.3 Undo / redo — snapshot-based, two parallel stacks (CONFIRMED)

`Editors::cEditor::CommitEditHistory` @`0x00586410`, quoted (trimmed to the load-bearing lines):

```c
void Editors__cEditor__CommitEditHistory(cEditor *this, bool arg1, EditorStateEditHistory *pStateHistory)
{
  if (*(int *)(in_ECX + 0x188) < *(int *)(in_ECX + 0x178) - *(int *)(in_ECX + 0x174) >> 2) {   // index < count
    FUN_005e77e0(*(int *)(in_ECX + 0x188));
    FUN_00584160(*(undefined4 *)(in_ECX + 0x188));
  }
  piVar3 = _arg1;
  if (0 < *(int *)(in_ECX + 0x188)) { *(undefined1 *)(in_ECX + 0x4b3) = 1; }          // dirty flag
  local_14 = (int *)0x0;
  if (_arg1 == (int *)0x0) {
    piVar3 = (int *)FUN_00f473a0(0x2c,"Editor",0,0,0,0);                            // EditorStateEditHistory
    if (piVar3 != (int *)0x0) {
      piVar3[1] = 0;  *piVar3 = (int)&PTR_VTDISC_005D6980_014774b0;  piVar3[2] = 0;
      piVar3[3] = 0;  piVar3[4] = 0;  *(undefined1 *)(piVar3 + 5) = 0;
      *(undefined1 *)((int)piVar3 + 0x15) = 0;  piVar3[6..10] = 0;
      (**(code **)(*piVar3 + 4))();
      local_14 = piVar3;
    }
    Editor_FillEditorStateEditHistory(local_14);
  } else { (**(code **)(*_arg1 + 4))(); local_14 = piVar3; }

  puVar5 = *(undefined4 **)(in_ECX + 0x164);        // cEditor.mStateEditHistory.begin  (+356)
  if (puVar5 < *(undefined4 **)(in_ECX + 0x168)) *(undefined4 **)(in_ECX + 0x164) = puVar5 + 1;
  else FUN_005c8480(puVar5,&local_14);

  iVar2 = FUN_00f473a0(0xac,"Editor",0,0,0,0);      // <-- same profile as the EditorModel factory
  if (iVar2 == 0) piVar3 = 0; else piVar3 = (int)FUN_004b9c70();
  puVar5 = *(undefined4 **)(in_ECX + 0x178);        // cEditor.mEditHistory.begin        (+372)
  if (puVar5 < *(undefined4 **)(in_ECX + 0x17c)) *(undefined4 **)(in_ECX + 0x178) = puVar5 + 1;
  else FUN_006650c0(puVar5,&arg1);

  *(int *)(in_ECX + 0x188) = *(int *)(in_ECX + 0x188) + 1;   // cEditor.mEditHistoryIndex    (+392)
  if (*(int *)(in_ECX + 0x78) != 0) { FUN_005dce40(0); FUN_005dceb0(1 < *(int *)(in_ECX + 0x188)); }
  …
  if (*(int *)(in_ECX + 0x31c) == 0) { /* Editors::Mode == EditMode */
    …  Editors__cEditorAnimWorld__DestroyCreature(*(cEditorAnimWorld **)(in_ECX + 0x364), unaff_EDI);
        *(undefined4 *)(in_ECX + 0x364) = 0;
    *(undefined1 *)(in_ECX + 0x39a) = 1;
  }
```

Every offset cross-checks against the SDK `cEditor` layout: `+352/356/360`
`mStateEditHistory` (a `vector<intrusive_ptr<EditorStateEditHistory>>`, 20 B at 352),
`+372/376/380` `mEditHistory` (a `vector<intrusive_ptr<cEditorResource>>`, 20 B at 372),
`+392` `mEditHistoryIndex`, `+796` `mMode`, `+864` `mpEditorAnimWorld`, `+152`
`mpEditorModel` (dereferenced with a `vtable+4` call), `+1203` a dirty byte.

What this establishes:

* Undo/redo is **snapshot-based**, not command-based: each commit pushes a whole
  freshly-constructed `cEditorResource` (same 0x2c-profile class as the model factory)
  onto `mEditHistory`, plus a small 44-byte `EditorStateEditHistory` onto
  `mStateEditHistory`. **CONFIRMED.**
* The redo tail is **truncated on commit** (`if (index < count) { … }` before the
  push). **CONFIRMED.**
* A monotonically increasing `mEditHistoryIndex` drives both `Undo` and `Redo`.
  **CONFIRMED.**
* **Grade: CONFIRMED.** The undo model is a *single* mechanism on `cEditor`. Whether the
  vehicle/building editors reuse it is **UNKNOWN** (§2).

### 1.4 `cEditor` — one object, config-driven, not subclassed per editor type

Selected SDK-named `cEditor` fields (1536 B) that matter for a Creator Core decision:

| Offset | Field | Why it matters |
|---:|---|---|
| 0..23 | six vtable pointers: `IGameMode`, `ILayer`, `IHintConditioner`, `INameableEntity`, `IMessageListener`, `DefaultRefCounted` | `cEditor` **is** an app game mode; it is not a subclass of a creator base — the SDK layout shows no base-class subobject at all |
| 152 | `EditorModel *mpEditorModel` | the editing target |
| 204 / 208 / 212 | `intrusive_ptr<EditorRigblock> mpActivePart / mpMovingPart / mpSelectedPart` | selection is part-typed |
| 336 / 340 | `intrusive_ptr<cEditorSkin> mpEditorSkin` + a second skin pointer | skin/mesh stage |
| 352 / 372 / 392 | `mStateEditHistory` / `mEditHistory` / `mEditHistoryIndex` | undo (§1.3) |
| 432 | `map<unsigned,unsigned> mDefaultEditors` (28 B) | **per-config default editor selection** |
| 680 / 684 | `mSaveExtension`, `mSaveDirectory` | where creations are written |
| 752–762 | `mbCellPinningToRigBlocks`, `mbUseSkin`, `mbUseSpine`, `mbInitSpine`, `mbOnlyEditFromPalette`, `mbMoveModelToGround`, `mbMoveModelToCenterOfMass`, `mbUseBoundsForDelete` | behaviour switches |
| **757** | **`bool mbAllowAsymmetry`** | the editor-wide symmetry switch — see §3.2 for what the code actually does with it |
| 796 | `Mode mMode` (`UnkMode / EditMode / PlayMode`, 3 values) | **edit vs play, not creature vs vehicle** |
| 800 | `vector<unsigned> mEnabledManipulators` | which manipulator/handle types are active |
| 820 | `vector<unsigned> mModelTypes` | **which part model types this editor accepts** |
| 844 | `int mnDefaultBrainLevel` | creature-specific (sporepedia brain level) |
| 864 / 868 | `mpEditorAnimWorld`, `field_368` | preview world |
| 952–964 | `mpPartsPalette`, `mpPartsPaletteUI`, `mpPaintPalette`, `mpPaintPaletteUI` | **two** palettes: parts and paint |
| 1076 | `intrusive_ptr<StdEditorLimits> mpEditorLimits` | the limits interface |
| 1137 | `mbShowBoneLengthHandles` | skeleton UI |
| 1207–1209 | `mbModelForceSaveOver`, `mbModelCopyConsequence`, `mbModelSaveLastChild` | save semantics |
| 1436 / 1440 | `mDaisType`, `mfDaisRadius` | the display pedestal |

**Reading:** `cEditor` is *configured* per editor type (`mDefaultEditors`, `mModelTypes`,
`mEnabledManipulators`, `mbAllowAsymmetry`, `mbUseSkin`) rather than *subclassed* — at
least for the modes the SDK names. This is a **shared-implementation-by-configuration**
shape, which is the corpus's strongest indirect hint towards a shared creator core.
Grade: **SUPPORTED** (config fields are SDK-named; that they are *per editor type* is
INFERRED, because the type discriminator is `cEditorResourceProperties::mModelType` /
`EditorModel::mModelType` and its value→meaning mapping is **UNKNOWN**, §5).

---

## 2. Vehicle, building, UFO, terrain — the measured negative

### 2.1 What the SDK surface contains

A full-text search of the 1 666-row SDK function table
(`.spore-analysis/ghidra-exports/sdk_functions.tsv`) and of all 1 895 imported
structures:

| Query | Hits |
|---|---|
| `creature` (functions) | **41** — all `Editors::*`, `Simulator::cCreature*`, `Simulator::Cell::*`, `App::cCreatureModeStrategy` |
| `vehicle` (functions) | **2** — `Simulator::cCity::SpawnVehicle` @`0x00bddef0`, `Simulator::cVehicle::Load` @`0x00ca67c0` |
| `building` (functions) | **3** — all `Simulator::cCity::{AddBuilding,ProcessBuildingUpdate,RemoveBuilding}` |
| `Vehicle` (structures) | **13** — `Simulator::cVehicle` (3480 B), `cVehicleData` (20 B), `VehiclePurpose` (4), `VehicleLocomotion` (4), plus eastl wrappers. **No editor, no part, no block.** |
| `Building` (structures) | **0** |
| `UFO` (structures) | **2** — `Simulator::cGameDataUFO`, `cGetOutOfUFOToolStrategy`. Both gameplay. |
| `/Spore/Editors/*` (all 469 types) | **0** mention vehicle / building / UFO / terrain |
| `Symmetr*`, `Mirror*`, `Socket*` (structures) | **0**, **0**, **0** — symmetry and sockets exist only as *field names* (§3) |

`Simulator::cVehicleData` is 20 bytes and is **not** a creation record:
`{ Vector3 mPosition; int mLocomotion; int mPurpose; }`, with
`VehiclePurpose = {kVehicleMilitary, kVehicleCultural, kVehicleEconomic, kVehicleColony}`
and a `VehicleLocomotion` enum. `VehiclePurpose`/`VehicleLocomotion` are **top-level
`/Spore/*` enums**, not members of any `Editors` namespace.

**Grade: CONFIRMED negative** for the named surface. This is the load-bearing
observation for the whole document: *the corpus has exactly one named creator
implementation.*

### 2.2 The vehicle/building editors are nevertheless present

Established positively, by strings and by code:

* `FUN_00f6a930` @`0x00f6a930` (40 B) — creates the terrain editor by profile path:

  ```c
  undefined4 FUN_00f6a930(void)
  {
    int iVar1; undefined4 uVar2;
    iVar1 = FUN_00f473a0(0x20c,"Terrain/TerrainEditor/cTerrainEditor",0,0,0,0);
    if (iVar1 != 0) { uVar2 = FUN_00f6a4b0(); return uVar2; }
    return 0;
  }
  ```

  The profile **path literal** `"Terrain/TerrainEditor/cTerrainEditor"` and the sibling
  `"Terrain/TerrainEditor/TempExtractBuffer0/1"` (`0x0140fa4f`, `0x0140fa7c`) name a
  **terrain editor** as a distinct subsystem with its own profile id `0x20c`. Grade:
  **CONFIRMED** (string literal + single caller `0x00fc1c70`, no decompiler
  interpretation needed). Its class is **unnamed**.

* Five editor *display names* exist as a group — `CellEditor`, `CreatureEditor`,
  `HutEditor`, `BuildingEditor`, `PlanetEditor` — inside a name-string lookup table at
  `FUN_00b1e0c0` @`0x00b1e0c0` (701 B), together with the `…GameFromLevelEditor` /
  `…From…Game` pairs. That function is a **shader/thread-name LUT** (it ends in
  `ITextureManager::AddTextureLoadDependency`), so these are *labels*, not classes.
  Grade: **CONFIRMED** that the string set exists; **NOT** evidence of a class hierarchy.

* `BuildingEditor` @`0x0145d9e8` and `PlanetEditor` @`0x0145d9d8` are the only occurrences
  of those tokens in the binary. There is **no** `cVehicleEditor` / `cBuildingEditor` /
  `cUfoEditor` string anywhere. Grade: **CONFIRMED negative** for class-name strings.

* Per `docs/analysis/creator/02-parts-and-catalogs.md` §0.3/§0.10 (asset side,
  OBSERVED): 1 549 rigblock records exist in `Spore_Game` — **creature 482, vehicle 540,
  building 224, flora 107, `0x40686000` 138, cell 58** — all of the **same record type**
  `0x5caa5e28`, and **UFO generation reuses the vehicle rigblock family**. So on the
  *asset* side there is unambiguously one part format across families.

**Synthesis.** The asset layer is shared (CONFIRMED, from 02). The *binary part record*
is shared (CONFIRMED, §4). The *editor behaviour layer* is shared **only if** the
vehicle and building editors instantiate `cEditor` + `EditorRigblock`, and **nothing in
the corpus shows that**. Two readings remain open:

* **(a) one `cEditor`, configured per family** — supported by `mDefaultEditors`,
  `mModelTypes`, `mEnabledManipulators`, `mbAllowAsymmetry`, `mbUseSkin`, and by the
  generic `cEditorResource`/`cEditorResourceBlock` naming;
* **(b) separate editor objects per family, sharing only the record format and the
  `kEditorType` enum** — equally consistent with everything observed, and the vehicle and
  building editors being *entirely unnamed* is weak evidence for it.

**The corpus does not decide between (a) and (b).** Do not assume either.

### 2.3 The one genuinely shared cross-family artefact: `kEditorType`

`FUN_01277f40` @`0x01277f40` (784 B) is a property-registration routine. It registers,
on **one class**, with an identical descriptor template (`PTRREF_00692CA0`,
`PTRREF_00584230`, `FUN_005767f0`, `FUN_00c2e4e0`, `PTRREF_00692FB0`, `FUN_00b1fbf0`,
`FUN_0057efa0`), a set of fields whose *own names* enumerate the editor families:

| Property name string | Field offset | Stride |
|---|---:|---:|
| `mActionStatus` | `0x4d8` | — |
| `mNumEditorEntries` | `0x4ec` | — |
| `mNumEditorEntriesByEditor` | `0x4f0` | — |
| `mActionStatusByEditor [kEditorTypeCell]` | `0x508` | 20 B |
| `mActionStatusByEditor [kEditorTypeCreature]` | `0x51c` | 20 B |
| `mActionStatusByEditor [kEditorTypeBuilding]` | `0x530` | 20 B |
| `mActionStatusByEditor [kEditorTypeVehicle]` | `0x544` | 20 B |
| `mActionStatusByEditor [kEditorTypeAccessories]` | `0x558` | 20 B |

So the shipped binary itself contains
`kEditorType = { Cell, Creature, Building, Vehicle, Accessories }`
(plus `UnkMode`-style unnamed members) as a **single flat array of five per-editor-type
slots on one class**. Strings live at `0x013f60af`–`0x013f6164` and are referenced only
by this function (`0x01277f40`, itself referenced from `0x013d3b10`).

Grade: **SUPPORTED** — five editor families, one enum, one owner class, established from
string literals + the registration arithmetic. This is *evidence of a shared
enumeration*, **not** evidence of a shared base class. The owner class is **UNKNOWN**
(`0x01277f40`'s `PTRREF_00584230` resolves into a code region, not a vtable; the passport
records `classification: UNKNOWN / subsystem: Unknown` for it).

One more cross-family artefact, weaker but worth recording: the cheat help string at
`0x013f51c8` reads

```
editor validate [<keyFilter>] -- print the validation information for the given buildings/vehicles/creatures/etc.
```

One validation command serving buildings, vehicles and creatures. Grade: **SUPPORTED**
(string literal).

And one name that hints at a base class without any structure behind it:
`"cAppModeEditorBase::Save"` @`0x013f5a94`, referenced **once**, from `FUN_0057f6c0` —
a profile/telemetry label in the save path (§1.1). There is **no** `cAppModeEditorBase`
type in the 1 895 imported structures, and the only other `cAppMode*` token is
`"cAppModeSpace"` @`0x01494004` (zero references in the data-ref snapshot).
**Grade: UNKNOWN.** One string literal is not a class. It is a *hypothesis generator*,
nothing more — and it must not be cited as evidence of a shared base.

---

## 3. Assembly semantics

### 3.1 The tree-building function — `0x004ae3b0` (quoted)

`FUN_004ae3b0(EditorModel *this, …)`, 3676 bytes, called from the editor save path
(`FUN_0057f6c0`). It is the code that turns a flat record array into a part hierarchy.
Load-bearing excerpt (Ghidra pseudocode, `int *` arithmetic verbatim):

```c
  local_18 = FUN_004b0010(param_2);                    // -> cEditorResource *
  if (local_18 == 0) { local_2ad = 0; }
  else {
    local_10        = (int *)(local_18 + 0x18);        // +24  = mProperties
    local_2b4       = (int *)(local_18 + 0x98);        // +152 = mBlocks.begin
    local_8         = (*(int *)(local_18 + 0x9c) - *local_2b4) / 0x1d8;   // COUNT, stride 472
    …
    local_28[3] = *(int *)(local_18 + 8);               // +8  = mNameKey.type
    local_28[4] = *(int *)(local_18 + 0xc);             // +12 = mNameKey.group
    local_28[5] = *(int *)(local_18 + 0x10);            // +16= mNameKey.instance
    …
    /* pass 1: child-count histogram, i.e. a root/topology prepass */
    for (local_25c = 0; local_25c < (int)local_8; local_25c = local_25c + 1) {
      if (*(uint *)(*(int *)(local_18 + 0x98) + 8 + local_25c * 0x1d8) < local_8) {   // rec.parentIndex
        piVar4 = (int *)(local_258 + *(int *)(local_25c * 0x1d8 + *(int *)(local_18 + 0x98) + 8) * 4);
        *piVar4 = *piVar4 + 1;
      }
    }
    /* pass 2: one EditorRigblock per record */
    for (local_260 = 0; local_260 < (int)local_8; local_260 = local_260 + 1) {
      local_284 = FUN_00f473a0(0xe08,"Editor",0,0,0,0);          // profile 0xe08
      if (local_284 == 0) local_3e4 = 0; else local_3e4 = (int *)FUN_004346b0();   // EditorRigblock ctor
      …
    }
    /* pass 3: wire the graph */
    for (local_268 = 0; local_268 < (int)local_8; local_268 = local_268 + 1) {
      local_274    = *(int **)(local_28[6] + local_268 * 4);          // rigblock[i]
      local_26c    = (int *)(local_268 * 0x1d8 + *(int *)(local_18 + 0x98));  // &record[i]
      local_274[10] = (int)local_28;                                   // owner model
      local_26d = *(char *)((int)local_26c + 0x81) != '\0';            // rec.isAsymmetric (byte 129)

      if ((bool)local_26d) {                                           // ASYMMETRIC:
        local_274[0x373] = local_274[0x373] | 0x2000000;               //   set bool-attr bit 57
        piVar4 = local_274 + 0xf8;                                     //   mpSymmetricRigblock
        if (*piVar4 != 0) { piVar8 = (int *)*piVar4; *piVar4 = 0; … }  //   and CLEAR it
      } else {
        local_274[0x373] = local_274[0x373] & 0xfdffffff;              //   clear bit 57
        if (local_26c[3] != -1) {                                      // SYMMETRIC:
          piVar4 = *(int **)(local_28[6] + local_26c[3] * 4);         //   rigblocks[rec.symmetricIndex]
          piVar8 = local_274 + 0xf8;  *piVar8 = piVar4;                //   mpSymmetricRigblock =
          piVar8 = local_274 + 0xf9;  *piVar8 = piVar4;                //   mpAsymmetricRigblock =
        }
      }
      if (local_26c[2] != -1) {                                        // PARENT:
        piVar8 = local_274 + 0xcf;                                     //   +0xcf*4 = 828 = mpParent
        piVar4 = *(int **)(local_28[6] + local_26c[2] * 4);           //   rigblocks[rec.parentIndex]
        *piVar8 = piVar4; …
      }
      local_274[8]     = *local_26c;          // rec.groupID     -> rigblock +32  = mGroupID
      local_274[7]     = local_26c[1];       // rec.instanceID  -> rigblock +28  = mInstanceID
      local_274[0x76]  = local_26c[4];       // rec.scale       -> rigblock +472 = size
      local_274[0x12]  = local_26c[5];       // rec.position.x  -> rigblock +72  = mPosition
      local_274[0x13]  = local_26c[6];       // rec.position.y
      local_274[0x14]  = local_26c[7];       // rec.position.z
      /* 9 ints: rec+0x0e (byte 56)  -> rigblock +168 = mOrientation     (Matrix3)  */
      /* 9 ints: rec+0x17 (byte 92)  -> rigblock +240 = mUserOrientation (Matrix3)  */
      /* DERIVED: total = userOrientation * orientation                    (Matrix3)  */
      piVar4 = (int *)_Unchecked_idl0__(local_2ac, local_274 + 0x3c /*240*/, local_274 + 0x2a /*168*/);
      piVar8 = local_274 + 0x18;                                          /* +96 = mTotalOrientation */
      for (iVar7 = 9; iVar7 != 0; iVar7--) { *piVar8 = *piVar4; piVar4++; piVar8++; }
      local_274[0xe8] = local_26c[8];   // rec.triangleDirection.x -> +928
      … 0xe9..0xed
      local_274[0x73] = local_26c[0x21];  // rec.limbType        -> +460 = mLimbType
      local_274[0x74] = local_26c[0x22];  // rec.muscleScale     -> +464
      local_274[0x75] = local_3f0;       // rec.baseMuscleScale -> +468 (0.0 -> 1.0)
      puVar6 = (uint *)(local_274 + 0x372);                              // +3528 = mBooleanAttributes
      if ((char)local_26c[0x20] == '\0') *puVar6 &= 0xffffefff;           // rec.snapped -> bit 12
      else                              *puVar6 |= 0x1000;
      FUN_004d04f0(local_26c + 0x25,  local_26c + local_26c[0x24] + 0x25,  local_354);  // handleWeights  (byte 148, N=byte 144)
      FUN_004d04f0(local_26c + 0x2d,  local_26c + local_26c[0x24] + 0x2d,  local_35b);  // handleChannels (byte 180, N)
      local_27c = 0; local_278 = local_26c[0x35];                                      // paintListCount (byte 212)
      for (; local_27c < local_278; local_27c++) {
        local_280 = (int *)FUN_00454420(local_26c + local_27c + 0x36);                 // paintRegions (byte 216)
        *local_280 = local_26c[local_27c + 0x3e];                                     // paintIDs     (byte 248)
        piVar4 = local_26c + local_27c * 3 + 0x46;  …3 ints…                          // paintColors1 (byte 280)
        piVar4 = local_26c + local_27c * 3 + 0x5e;  …3 ints…                          // paintColors2 (byte 376)
      }
    }
```

What this proves, field by field, with nothing inferred:

1. **A created creature is a flat array of fixed-stride part records.** `count = (end − begin) / 0x1d8`, `0x1d8 = 472`. There is no nesting and no per-record length prefix. **CONFIRMED.**
2. **Assembly is index-based, not pointer-based.** `mpParent = rigblocks[rec.parentIndex]`,
   `mpSymmetricRigblock = mpAsymmetricRigblock = rigblocks[rec.symmetricIndex]`, with
   `-1` meaning "none". The parent index is a **0-based ordinal into the same array**,
   proven by pass 1: it reads `parentIndex` of record *i*, bounds-checks it against
   `count`, and increments `childrenCount[parentIndex]`. **CONFIRMED.**
3. **The mirror partner is stored per part, not derived from a plane.** A record with
   `isAsymmetric == true` gets its mirror pointer explicitly **cleared**; a symmetric
   one gets it set from `symmetricIndex`. **CONFIRMED.**
4. **The total/world orientation is DERIVED, not stored.** The 472-byte record stores
   `orientation` and `userOrientation`; `mTotalOrientation` is computed on load as a
   3×3 product `userOrientation × orientation`. **CONFIRMED** for the *user × local*
   product; **UNKNOWN** whether a second, parent × child composition exists elsewhere
   (§5).
5. **Two bool attributes survive serialization** by bit position: byte 128 →
   `kEditorRigblockIsSnapped` (bit 12) and byte 129 → `kEditorRigblockModelIsAsymmetric`
   (bit 57 of the 64-bit `mBooleanAttributes`; `0x2000000` is bit 25 of the *high* dword,
   i.e. 32 + 25). **CONFIRMED** — the bit indices come from the SDK enum, the dword
   arithmetic from the code.

Note the **asymmetry is per part, and it is a first-class stored fact** — that is the
single most load-bearing observation for a Creator Core: symmetry is part of the *data*,
not a rendering-time mirror pass.

### 3.2 The attachment-point table and the snap/magnet rule set

`EditorRigblock` carries the attachment and snapping rules as **data**, keyed by
`ResourceKey`, not as code:

**Attachment / connector fields (SDK `EditorRigblock`, offsets in bytes)**

| Off | Field |
|---:|---|
| 588 / 644 | `Transform mEffectsBoneTransform1 / 2` (56 B each) + `int mEffectsBoneIndex` @700 |
| 704 / 760 | `Transform mCSnapBoneTransform1 / 2` (56 B each) + `int mCSnapBoneIndex` @816 |
| 1008 | `Graphics::Model *mpSocketConnectorModel` |
| 1012 / 1024 / 1036 | `Vector3 mModelMinSocketConnectorOffset / mModelMaxSocketConnectorOffset / mSocketConnectorOffset` |
| 1004 | `EditorBaseHandle *mpBallConnectorHandle` |
| 536 / 540 | `float mModelMinBallConnectorScale / mModelMaxBallConnectorScale` |
| 500 | `Matrix3 mModelEditorInitialRotation` |
| 1104 | `float mModelBottomEdgeDistanceAdjustment` |
| 1108 | `float mModelScale` |
| 1112 / 1116 | `ResourceKey *mModelMinMuscleFile / mModelMaxMuscleFile` |

**The attachment-point *table*** — nine `fixed_vector<ResourceKey, 8>` (120 B each) on
`EditorRigblock`:

| Off | Field | Meaning |
|---:|---|---|
| 1908 | `mModelSnapDownTo` | parent types this part snaps *down onto* |
| 2028 | `mModelSnapToParentTypes` | **parent-type compatibility** — the magnet rule |
| 2148 | `mModelStayAbove` | types this part must stay above |
| 2268 | `mModelAlignLateralWith` | lateral alignment partners |
| 2388 | `mModelAlignHeightWith` | height alignment partners |
| 2508 | `mModelAlignXYWith` | XY alignment partners |
| 2628 | `mModelTypesToInteractWith` | interaction whitelist |
| 2748 | `mModelTypesToSnapReplace` | snap-replace list |
| 2868 | `mModelTypesNotToInteractWith` (16 entries, 216 B) | interaction blacklist |

**Snap tolerances** (floats on `EditorRigblock`):

| Off | Field | ctor default |
|---:|---|---|
| 544 | `mModelSymmetrySnapDelta` | `0.0f` |
| 552 | `mModelReplaceSnapDelta` | `0.4f` (`0x3ecccccd`) |
| 556 | `mModelSymmetryRotationSnapAngle` | `0.5f` (`0x3f000000`) |
| 584 | `int mModelNumberOfSnapAxes` | `-1` |
| 480 / 484 | `mModelMinScale` / `mModelMaxScale` | `0.2f` / `3.0f` |

**The rule bitset** — `EditorRigblockBoolAttributes`, the 64-bit
`mBooleanAttributes` @3528. 48 named bits, verbatim from the SDK enum, grouped:

*Symmetry (the plane-of-symmetry axis rules)*
`0x22 PreferToBeOnPlaneOfSymmetry` · `0x28 ActLikeFinOnPlaneOfSymmetry` ·
`0x30 AllowAsymmetricalRotationOnPlaneOfSymmetry` ·
`0x34 DoNotFlipBasedOnSurfaceNormalOnPlaneOfSymmetry` ·
`0x39 IsAsymmetric` · `0x3B StartAsymmetric`

*Snapping*
`0x00 OrientToSurfaces` · `0x04 OrientWhenSnapped` · `0x0C IsSnapped` ·
`0x12 MoveBottomEdgeToSurface` · `0x13 PointForward` ·
`0x16 HasStayAbove` · `0x17 HasSnapDawnTo` *(sic — "Dawn", a typo in the original)* ·
`0x1A HasAlignLateralWith` · `0x1B HasAlignHeightWith` · `0x35 HasAlignXYWith` ·
`0x1C SnapToParentCenter` · `0x1D SnapToParentSnapVectors` ·
`0x1E SnapToCenterOfEditor` · `0x31 CircularTopAlignment` · `0x32 RemainUprightOnTop`

*Connectors / sockets*
`0x0A HasSocketAndBallConnector` *(aliased in the enum to `0x0A UseSkin` — a duplicate
value; Ghidra reports "some values do not have unique names")* ·
`0x1F HasBallConnector` · `0x20 HasSocketConnector` ·
`0x26 HasRotationBallHandle` · `0x27 HasEffectsBone`

*Hierarchy / lifecycle*
`0x07 IsVertebra` · `0x08 IsPlantRoot` · `0x14 IsNullBlock` ·
`0x23 CanBeParentless` · `0x24 BoundsCheckOnlyForDelete` ·
`0x2F HasForcePinningAgainstType` · `0x2C ActsLikeGrasper` · `0x2D ActsLikeFoot` ·
`0x21 CellAllowOnTopOfBody` · `0x05 UseDummyBlocks` · `0x09 OverrideBounds` ·
`0x15 UseHullForBBox` · `0x2E UseHullForPicking` · `0x18 HideDeformHandles` ·
`0x19 HideRotationHandles` · `0x29 WarpCursorToPinPoint` · `0x36 Has2002C980Bone` ·
`0x37 AllowTopBehaviors` · `0x38 CellApplyBehaviorToUserRotation` ·
`0x11 RemainUpright` · `0x33 IsAllowedOutOfBounds` · `0x03`(absent)

Every one of the nine `ResourceKey` lists has a matching "Has…"/"…To…" boolean bit.
That is a **CONFIRMED** pairing: the boolean says *whether* the list is meaningful, the
list says *with what*. A Creator Core can therefore model snapping as
`(rule-bit, [ResourceKey; ≤8])` pairs with **corpus** justification — this is the single
most reusable piece of creator semantics in the corpus.

**A measured negative about symmetry.** `cEditor::mbAllowAsymmetry` sits at byte 757
(`0x2f5`). A scan of every instruction operand in the whole program for the scalar
`0x2f5` returns **zero hits**. The byte immediately below it, `0x2f4`
(`cEditor::field_2F4`, a bool), has exactly one referencing function
(`FUN_004dd060`, 8809 B). So either the SDK's `mbAllowAsymmetry` offset is wrong, or the
flag is set only from data (a property/config write) and never branched on.
**Grade: UNKNOWN.** Do not build a symmetry switch on the strength of that field name.

### 3.3 The runtime counterpart: `Anim::CreatureBlock` (1128 B)

The **runtime** form of the same idea, and the clearest statement of the transform
chain in the corpus:

| Off | Field |
|---:|---|
| 0 | `char name[260]` |
| 260 / 264 | `uint32 instanceID`, `uint32 groupID` |
| 268 | `Vector3 bboxCenter` |
| 280 / 296 | `Quaternion orientation`, `Quaternion field_128` |
| 312 | `Vector3 bboxDirection` |
| 324 | `Vector3 position` |
| 336 | `Capabilities capabilities` (168 B: `capFlags`, `capLvlFlags`, `int *capabilities[32]`, `int8_t capLevels[32]`) |
| 504 / 508 / 512 | `int blockIndex`, `int parentIndex`, `int symmetricIndex` |
| **532** | **`CreatureBlock *pParent`** |
| 536 | `CreatureBlock *field_218` |
| 540 | `int blockID` |
| **544** | **`char cal3DBoneName[260]`** — the skeleton attachment point, by name |
| 804 / 816 | `Vector3 cal3DOrigin`, `Vector3 effectsOrigin` |
| 828 | `int submeshIndex` |
| 832 | `bool isLeftHanded` |
| 888 | `float scale` |
| 896 | `Deforms deforms` (164 B = `Deform[8]`, each `{uint32 id, int, float, int, float}`) |
| 1060 / 1064 | `uint32 soundIDFoot`, `uint32 soundIDMouth` |

This is the **skeleton-attachment mechanism made explicit**: `blockIndex`,
`parentIndex`, `symmetricIndex`, a resolved `pParent` pointer, **and**
`cal3DBoneName[260]` — the part declares *which Cal3D bone* it is welded to, plus
`cal3DOrigin`. So an attachment point is addressed **by bone name at runtime** and **by
`ResourceKey` list + snap flags at edit time**. Grade: **CONFIRMED** (SDK struct; no
behaviour claimed).

### 3.4 What is NOT here

Measured absences, stated as negatives rather than as "there's nothing":

* No `Socket`, `Mirror`, `Symmetr*`, `Part`, `Piece` **type** exists in the 1 895
  imported structures (search returned 0 for each). Sockets and symmetry exist only as
  field names and enum bits.
* No data type carries a table of *named* attachment points (e.g. an
  `AttachmentPoint[]`). The closest is `fixed_vector<ResourceKey,8>` ×9 plus
  `mCSnapBoneTransform1/2` and `mEffectsBoneTransform1/2` — two named transform slots,
  not a general table.
* No function named for symmetry, mirroring, snapping or undo/redo of *parts* exists.
  `Undo`/`Redo`/`CommitEditHistory` are the only undo-named symbols and they are
  `cEditor` methods.

---

## 4. Serialization of a created creature — the record layout (CONFIRMED)

**Yes: a created creature is stored as a list of (part-id, transform) records — flat,
fixed-stride, with index-based parent and mirror linkage.**

### 4.1 The container

`Editors::cEditorResource`, **172 bytes** (`/Spore/Editors/cEditorResource`):

| Off | Size | Field |
|---:|---:|---|
| 0 | 4 | `ResourceObject__vftable *` |
| 4 | 4 | `int mnRefCount` |
| 8 | 12 | `ResourceKey mNameKey` |
| 20 | 4 | `cIReleaseCallback *mpFinalReleaseCallback` |
| 24 | 128 | `cEditorResourceProperties mProperties` |
| **152** | **20** | **`vector<Editors::cEditorResourceBlock> mBlocks`** |

`cEditorResourceProperties` (128 B): `uint32 mModelType` @0, `int mZCorpScore` @4,
`mSkinEffectID1..3` @8/12/16, `mSkinEffectSeed1..3` @20/24/28,
`ColorRGB mSkinColor1..3` @32/44/56, `char padding_48[60]` @68.

### 4.2 The part record

`Editors::cEditorResourceBlock`, **472 bytes** (`/Spore/Editors/cEditorResourceBlock`):

| Off | Size | Type | Field | Maps to |
|---:|---:|---|---|---|
| 0 | 4 | `uint32` | `groupID` | `EditorRigblock.mGroupID` @28 |
| 4 | 4 | `uint32` | `instanceID` | `EditorRigblock.mInstanceID` @32 |
| **8** | 4 | `int` | **`parentIndex`** | `EditorRigblock.mpParent` @828 (index into this array; `-1` = root) |
| **12** | 4 | `int` | **`symmetricIndex`** | `mpSymmetricRigblock` @992 / `mpAsymmetricRigblock` @996 |
| 16 | 4 | `float` | `scale` | `EditorRigblock.size` @472 |
| **20** | 12 | `Vector3` | **`position`** | `mPosition` @72 |
| 32 | 12 | `Vector3` | `triangleDirection` | `mTriangleDirection` @928 |
| 44 | 12 | `Vector3` | `trianglePickOrigin` | `mTrianglePickOrigin` @944 |
| **56** | 36 | `Matrix3` | **`orientation`** | `mOrientation` @168 |
| **92** | 36 | `Matrix3` | **`userOrientation`** | `mUserOrientation` @240 |
| 128 | 1 | `bool` | `snapped` | bool-attr bit 12 `kEditorRigblockIsSnapped` |
| 129 | 1 | `bool` | `isAsymmetric` | bool-attr bit 57 `kEditorRigblockModelIsAsymmetric` |
| 132 | 4 | `int` | `limbType` | `mLimbType` @460 |
| 136 | 4 | `float` | `muscleScale` | `mMuscleScale` @464 |
| 140 | 4 | `float` | `baseMuscleScale` | `mBaseMuscleScale` @468 |
| 144 | 4 | `int` | `handlesCount` (N, ≤8) | — |
| 148 | 32 | `float[8]` | `handleWeights` | morph handle weights |
| 180 | 32 | `uint32[8]` | `handleChannels` | muscle channel ids |
| 212 | 4 | `int` | `paintListCount` (M, ≤8) | — |
| 216 | 32 | `int[8]` | `paintRegions` | paint region ordinals |
| 248 | 32 | `uint32[8]` | `paintIDs` | paint ids |
| 280 | 96 | `ColorRGB[8]` | `paintColors1` | first colour slot |
| 376 | 96 | `ColorRGB[8]` | `paintColors2` | second colour slot |

Every one of the 24 rows is confirmed **twice**: once by the SDK struct layout, and once
by an independent field-offset match in the decompiled loader `0x004ae3b0` (§3.1) —
including the derived `mTotalOrientation`, the two attribute bits, and all four paint
arrays with both index bases (216 and 248) and both colour bases (280 and 376).
**Grade: CONFIRMED.**

### 4.3 The reader that proves it

`FUN_004bf9f0` @`0x004bf9f0` (the function immediately after the SDK-named
`cCreatureDataResource::Read` entry `0x004bf770`+629), quoted in full because it is
short and completely decisive:

```c
undefined4 FUN_004bf9f0(int *param_1, undefined4 param_2)
{
  …
  local_10 = FUN_004b0010(param_2);                          /* -> cEditorResource *   */
  if (local_10 == 0) { uVar2 = 0; }
  else {
    iVar3 = (**(code **)(*param_1 + 0x30))(&local_1c, 8);
    if ((((iVar3 == 8) && (local_1c == 0x48657ed3)) &&        /* magic u32            */
         (2 < local_18) && (local_18 < 6)) {                   /* version in {3,4,5}   */
      iVar3 = (**(code **)(*param_1 + 0x30))(&local_c, 8);
      if (((iVar3 == 8) && (local_c < 0x401)) && (local_8 < 0x81)) {   /* count<1024, ?<128 */
        FUN_004afd00(local_c);
        memset((void *)(local_10 + 0x18), 0, 0x80);            /* +24, 128 B properties*/
        local_14 = 0x80;
        if (local_18 < 5) { local_14 = 0x44; }                 /* v3/v4: 68 B          */
        cVar1 = FUN_004bf2a0(param_1, local_10 + 0x18, local_14);
        if (cVar1 == '\0') { uVar2 = 0; }
        else {
          cVar1 = FUN_004bf2a0(param_1, *(undefined4 *)(local_10 + 0x98),
                               local_c * 0x1d8);                /* +152, count * 472    */
          if (cVar1 == '\0') { uVar2 = 0; } else { uVar2 = 1; }
        }
      } else { uVar2 = 0; }
    } else { uVar2 = 0; }
  }
  return uVar2;
}
```

Reading, with no interpretation added:

* Header = two 8-byte reads. `local_1c` is a magic **`0x48657ed3`**; `local_18` is a
  version constrained to **3, 4 or 5**.
* Second word pair: `local_c < 0x401` ⇒ **at most 1024 parts**; `local_8 < 0x81` ⇒ at
  most 128 of the second quantity.
* The properties block is **128 bytes** for version ≥ 5 and **68 bytes** for 3/4 —
  `cEditorResourceProperties` is declared 128 bytes, so **v5 is the current width** and
  68 is a legacy width. **CONFIRMED.**
* The block array is read as **one contiguous `count × 472` byte run** from `+152` — a
  bulk read, no per-element framing. **CONFIRMED.**

The magic `0x48657ed3` is quoted as a raw little-endian `uint32`, not decoded to ASCII:
its four bytes are `d3 7e 65 48`, and I did not establish a byte order or a tag
convention that makes it meaningful. **Grade: OBSERVED.**

### 4.4 Relation to the `<sporemodel>` XML (02 §5)

`docs/analysis/creator/02-parts-and-catalogs.md` §5.3 measured the asset-side XML:
`<blockid>group, instance</blockid>`, `<transform>` with `<scale>`/`<position>`/
`<orientation>`, `<snapped>`, `<childlist><childid>` (parent ordinal),
`<symmetric>N</symmetric>` (mirror ordinal), `<handles count><weight channel="0x…">`,
`<limbtype>`, `<musclescale>`, `<basemusclescale>`, `triangledirection`,
`trianglepickorigin`.

That is a **field-for-field match** with `cEditorResourceBlock`:

| XML element | binary field |
|---|---|
| `<blockid>` group / instance | `groupID` @0 / `instanceID` @4 |
| `<childlist><childid>` | `parentIndex` @8 |
| `<symmetric>` | `symmetricIndex` @12 |
| `<scale>` | `scale` @16 |
| `<position>` | `position` @20 |
| `<triangledirection>` / `<trianglepickorigin>` | @32 / @44 |
| `<orientation>` | `orientation` @56 |
| (no XML element) | `userOrientation` @92 |
| `<snapped>` | `snapped` @128 |
| `<limbtype>` | `limbType` @132 |
| `<musclescale>` / `<basemusclescale>` | @136 / @140 |
| `<handles count><weight channel=…>` | `handlesCount` @144, `handleChannels` @180, `handleWeights` @148 |

Grade: **CONFIRMED** for the correspondence (two independent sources, 12 matched
fields). Two loose ends, both **UNKNOWN**:

* The XML carries `<blockbbox>` (min/max) which `cEditorResourceBlock` has **no room
  for** (376 + 96 = 472 exactly). The bounds are presumably recovered from the model, or
  the XML is a lossier/lossless variant — **not established**.
* The XML reports `formatversion 11`; the binary header accepts versions **3–5**. These
  are probably different numbering spaces. The link between the two version numbers is
  **UNKNOWN**.

### 4.5 In-memory twin of the serialized form

`Editors::EditorModel` (224 B) carries the same content live:
`mKey` @12 (ResourceKey), `vector<intrusive_ptr<EditorRigblock>> mRigblocks` @24,
`mModelType` @88, `mTranslationOptions` @84, `mName`/`mDescription`/`mAcceptedName`
@92/108/124 (UTF-16, 16 B each), `mSkinEffectIDs[3]` @140, `mSkinEffectSeeds[3]` @152,
`ColorRGB mColors[3]` @164, `mZCorpScore` @220 — i.e. the *properties* half of
`cEditorResource`, plus a pointer vector instead of a value array.

**Reading:** `EditorModel` (live, intrusive_ptr parts) ⇄ `cEditorResource` (saved, value
parts), with `cCreatureDataResource` as the creature-specific subclass of the saved form.
Grade: **SUPPORTED** (field correspondence is exact; the ⇄ direction is INFERRED from
`CommitEditHistory` pushing constructed models onto the undo stack).

### 4.6 The CONFLICT: `RigblockData` (140 B) is not the record

The SDK also declares `Editors::cCreatureDataResource::RigblockData` at **140 bytes**,
with `mIndex` @0, `mParentIndex` @4, `mSymmetricIndex` @6, `mType` @11,
`mBoundingBox` @20, `mScale` @44, `mTotalOrientation` @48, `mPosition` @84,
`mSocketConnectorDistance` @112, `mGroupID` @132, `mInstanceID` @136 — and
`cCreatureDataResource::mRigblocks` is declared as `vector<RigblockData>`.

**This does not match the binary.** Measured:

* the loader computes `count = (end − begin) / 0x1d8` = **472**, not 140;
* the reader computes `count * 0x1d8` = **472**, not 140;
* the loader reads a **32-bit** `parentIndex` at byte 8 and a **32-bit**
  `symmetricIndex` at byte 12, whereas `RigblockData` declares 16-bit fields at bytes 4
  and 6;
* the loader reads `position` at byte **20**, whereas `RigblockData` declares
  `mPosition` at byte **84**.

The genuine 472-byte type, `Editors::cEditorResourceBlock`, exists in the same SDK
surface and matches on all counts. So `RigblockData` is a **stale or partial SDK
declaration**, not the record the engine reads.

**Grade: CONFLICT, resolved in favour of `cEditorResourceBlock` by a 4-way independent
offset match plus two stride computations.** Any consumer that sizes a part record from
`RigblockData` will be wrong by 332 bytes. Flagged for the contradiction ledger.

---

## 5. What stays UNKNOWN, and what would settle it

Stated as non-findings, per the repo's evidence discipline: *we did not look / we looked
and did not establish*, never "there is nothing".

| # | UNKNOWN | Why it is open | What would settle it |
|---:|---|---|---|
| U1 | **Whether the vehicle / building / UFO / terrain editors reuse `cEditor` + `EditorRigblock`, or reimplement them.** | Only the creature editor has symbols. Both readings are consistent with every observation (§2.2). This is the load-bearing unknown for the Creator Core decision. | Two class-hierarchy facts: (a) the vtable pointer(s) actually installed in a vehicle-editor instance, compared against `cEditor`'s six interface slots (`IGameMode`, `ILayer`, `IHintConditioner`, `INameableEntity`, `IMessageListener`, `DefaultRefCounted`); (b) an xref from a vehicle-editor code region into `EditorRigblock::`-cluster functions (`0x004346b0`, `0x00441440`, `0x004ae3b0`). Concretely: find the creator of the object at profile id `0x20c` (`"Terrain/TerrainEditor/cTerrainEditor"`, `FUN_00f6a4b0` called from `FUN_00f6a930`) and check its size and vtable against `cEditor`'s 1536 B. |
| U2 | **Whether a parent × child transform composition exists, and where.** | The only composition proven is `userOrientation × orientation` inside `0x004ae3b0` (§3.1 #4). That is *user × local*, not *parent × world*. Whether `mTotalOrientation` is later re-based on `mpParent->mTotalOrientation` (a second pass) was not established: no function in the program reads both `0x33c` (`mpParent`) and `0x60` (`mTotalOrientation`) as data operands except the constructor and the loader, both of which only *initialise*. | Decompile the per-frame rigblock update. `cEditorSkinMeshBase::FromRigblocks` (`0x004c73f0`, 12342 B) is the strongest candidate: it must walk the tree, and it is the only named function whose job is to consume the rigblock array. Trace whether it recurses on `mpParent`/`mChildren` and what it writes into `mTotalOrientation`. |
| U3 | **The value → meaning map of `cEditorResourceProperties::mModelType` (`@0`) / `EditorModel::mModelType` (`@88`).** | This is almost certainly the creature/vehicle/building discriminator, and it is the cheapest possible route to U1. The field exists; its enumeration does not. | The in-game type dictionary at `0x01C7AC81` (partially decoded in 02 §9) cross-referenced with every site that reads offset `@0` of the 128-byte property block; or the palette/config assets that populate `mDefaultEditors`/`mModelTypes`. |
| U4 | **The owner class of `kEditorType` / `mActionStatusByEditor[5]`.** | `FUN_01277f40` registers the fields but its receiver is not resolvable — `PTRREF_00584230` points into a code region, not a vtable; the passport records `UNKNOWN / Unknown` for it. | Find the registration root: the sole reference is `0x013d3b10`. Type the receiver of the descriptor template and read its vtable; compare against `cGameModeManager`'s. |
| U5 | **Whether `cAppModeEditorBase` is a real class.** | One string literal, one reference, zero structures. | Any data type named `cAppModeEditorBase`, or a vtable whose owning function also passes `"cAppModeEditorBase::"` to `FUN_00f473a0`. As it stands this is a hypothesis generator only (§2.3). |
| U6 | **`cEditor::mbAllowAsymmetry` (byte 757).** | Zero instruction operands equal `0x2f5` in the entire program (§3.2). Either the offset is wrong or the flag is write-only from config. | Search for the *write* by data flow: find stores to `cEditor + 0x2f5` via a config/property load, or locate the property name that feeds it. Also re-derive the offset from a second `cEditor` method whose body is unambiguous. |
| U7 | **The 60 padding bytes of `cEditorResourceProperties`, and the 68-byte legacy properties width (versions 3/4).** | Byte count measured; content not decoded. | The v3/v4 sample bytes for the same `ResourceKey` — the diff against v5 is the field list. Requires the game data, which is not committed. |
| U8 | **The two 32-byte tail arrays in the 472-byte record beyond `handlesWeights/Channels` and the four paint arrays.** | Every field of `cEditorResourceBlock` up to byte 472 is now named and mapped (§4.2), so this is *not* an unexplained gap — but the **semantics** of `paintRegions` (which mesh region a paint applies to) and of `handleChannels` (the muscle-channel id space) are not decoded. Note 02 §5.3 measured 14 distinct muscle channel ids in the XML, which constrains but does not explain the encoding. | The `0xd02751de`-style channel ids are almost certainly `Property` keys — resolve them through `App::Property::GetFloat`-style consumers (the SDK shows `App::Property::GetFloat` used with hashed keys in `FUN_0057f6c0`, e.g. `0x711306cd`). A cross-reference of those hashes against `paintRegions` would close it. |
| U9 | **The magic `0x48657ed3` and the byte at `local_8 < 0x81`.** | Read and bounds-checked; neither decoded. `local_8` is the second dword of the second 8-byte header read and is bounded by 128, so it is a count of something capped at 128 — plausibly the capability-tag count (`cCreatureDataResource::mCapabilityIDs`, which the loader reads as 4-char `CapabilityTag`s), but not established. | Dump two real `.xsf`/creature files and read bytes 0..16 directly; compare the count against `mCapabilityIDs.size()` after load. |
| U10 | **Whether the SDK function-name → VA binding is trustworthy anywhere in this document.** | Measured, and it is **not**: of 1 666 SDK addresses, **198 (11.9 %) resolve to nothing** and **148 real entries carry more than one SDK name**; 61 of those collisions involve `Editors`/`Simulator`/`App`/`Palettes`/`Anim`/`Skinner`. Concretely, `Editors::cCreatureDataResource::Write` (SDK `0x004bfcb0`) resolves *inside* `0x004bfc40`, whose disassembly is an **EASTL allocator** (block coalescing with `0x3fffffff` size fields, `RET 0x8`) — not a serializer at all. And `0x004ae3b0` carries three SDK names (`EditorModel::SetDescription`, `::GetCreationName`, `::Load`) while `0x004af260` carries `App::Property::GetUInt32` **and** `EditorModel::Save`. | Per-function: check that the SDK address is an exact entry (not an interior hit) and that no second SDK name maps to the same entry, before quoting a name. Both checks are cheap against `.spore-analysis/ghidra-exports/functions.tsv` + `sdk_functions.tsv`; I used them throughout and marked every interior binding `SUPPORTED` rather than `CONFIRMED`. |
| U11 | **The live Ghidra DB has drifted from the committed export** (83 820 live functions vs 59 245 at export time per `AGENTS.md`). Everything structural in this document (`cEditorResourceBlock` = 472 B, the `0x1d8` stride, the field offsets) is corroborated by the committed `functions.tsv` entry sizes and by the SDK structs, so it survives the drift — but individual `FUN_*` boundaries inside the editor clusters should be A/B'd against a same-DB baseline before being quoted as function identities. | See `AGENTS.md` "Reference exports". | Re-run the exporter against the current DB and diff against `knowledgegraph/triage/xrefs-2540f2ca.tsv`, as `AGENTS.md` prescribes. |
| U12 | **Runtime behaviour of any of this** — actual snap success, actual symmetry mirroring, actual save round-trip, whether the vehicle editor ever touches `EditorRigblock`. | **No runtime evidence was gathered**, deliberately. `spore-semantic runtime` / `spore-recomp` are available and are the right instrument, but a runtime observation **cannot upgrade a static verdict** in this repo's model; it can only supply facts the static record leaves unavailable. | `spore-semantic runtime import-recomp` over a capture that drives the vehicle editor, then `runtime lookup` on the editor VAs above. Report as *runtime* evidence with its own provenance — never folded into a static grade. |

---

## 6. Method, and two measured traps that affect every claim above

### 6.1 Trap: an SDK name is not a proof that a VA holds that function

Measured over the whole SDK table against the frozen function universe
(`.spore-analysis/ghidra-exports/functions.tsv`, 58 757 entries; the repo's own
`canonical_identity` rule — bisect on entry starts, then `start <= a < start + size`):

```
sdk rows                                 1666
unresolved (not entry, not in any body)   198   (11.9 %)
real entries carrying >1 SDK name         148
  …of which touching Editors/Simulator/App/Palettes/Anim/Skinner   61
```

Concrete failures, all in the creator region:

| SDK name | SDK addr | Resolves to | Reality |
|---|---|---|---|
| `Editors::cCreatureDataResource::Write` | `0x004bfcb0` | `0x004bfc40` @ +112 | an **EASTL allocator** (`0x3fffffff` block headers, coalescing, `RET 0x8`). Disassembly-verified. |
| `Editors::EditorModel::Load` | `0x004ae8d0` | `0x004ae3b0` @ +1312 | the rigblock-assembly loader (§3.1) |
| `Editors::EditorModel::GetCreationName` | `0x004ae520` | `0x004ae3b0` @ +368 | **same entry** |
| `Editors::EditorModel::SetDescription` | `0x004ae440` | `0x004ae3b0` @ +144 | **same entry — three SDK names, one function** |
| `Editors::EditorModel::Save` | `0x004af780` | `0x004af260` @ +1312 | an entry that also carries `App::Property::GetUInt32` |
| `Simulator::cGameModeManager::SubmitEditorRequest` | `0x00b1de20` | `0x00b1dc60` @ +1936 | an entry that also carries `GetActiveDifficultyTuning` |
| `App::cGameModeManager::InitializeModes` | `0x007d82f0` | **nothing** | not an entry, not inside any body |
| `Palettes::PaletteUI::GetSelectedRigblockPaint` | `0x005cb240` | `0x005cb240` | exact entry, but **also** carries `SetActiveCategory` |

Consequence for this document: I graded a name `CONFIRMED` only when the SDK address is
an **exact** entry with no competing name; otherwise `SUPPORTED`, and where the body
contradicts the name I said so. §6.1's three-way `EditorModel` collision is why
`0x004ae3b0` is described by its *behaviour*, not by `EditorModel::Load`.

The one *refuted* identity in the passport (`0x008414c0`,
`ArgScript::FormatParser::ParseFloat`) is unrelated to this region, so **this collision
class is currently unrecorded** in `reconstruction/knowledge/index.json`. That is a gap
worth filing.

### 6.2 Trap: two independent matches beat one, and struct sizes are not negotiable

`FUN_004346b0` (`EditorRigblock` constructor) is named `UNKNOWN` by every artifact. I
identified it because its 3535-byte body writes **25+ fields at exactly the offsets the
SDK `EditorRigblock` layout predicts**, including all nine `fixed_vector<ResourceKey,8>`
snap lists and every named symmetry/snap tolerance with a matching non-zero default:

```
byte  57 field_38        (bool)            byte 425 field_1A9      (bool)
byte  65 field_40        (bool)            byte 460 mLimbType = 3
byte 969 field_3C9       (bool)            byte 464 mMuscleScale = 0.5f
+0x55 340 mAxisHandles[3] cleared in a 3-iteration loop
+0xcf 828 mpParent = 0   +0xf8 992 mpSymmetricRigblock   +0xf9 996 mpAsymmetricRigblock
+0xfd 1012 mModelMinSocketConnectorOffset  +0x100 1024 mModelMaxSocketConnectorOffset
+0x103 1036 mSocketConnectorOffset         +0x106 1048 mModelRotationBallHandleOffset
+0x132 1224 mPaints   +0x185 1556 field_614   +0x1b3 1740 mMorphHandles
+0x1c1 1796 mMorphHandleWeights   +0x1cf 1852 mMorphHandleChannels
+0x1dd 1908 mModelSnapDownTo      +0x1fb 2028 mModelSnapToParentTypes
+0x219 2148 mModelStayAbove       +0x237 2268 mModelAlignLateralWith
+0x255 2388 mModelAlignHeightWith +0x273 2508 mModelAlignXYWith
+0x291 2628 mModelTypesToInteractWith  +0x2af 2748 mModelTypesToSnapReplace
+0x36d 3508 mFootType
+0xcf/0xcf+1 828/832  mpParent / mChildren, cleared with AddRef/Release discipline
```

That is why `EditorRigblock` field *meanings* in §3.2 are `CONFIRMED` even though the
function is unnamed. Two independent sources agreeing on an offset is the standard this
document tries to meet everywhere; where only one source exists (§4.6 `RigblockData`), the
claim is downgraded to `CONFLICT`.

---

## Appendix — commands used

```
# read-only passport (no writes; never `export` into a committed path)
~/.local/bin/spore-semantic stats
~/.local/bin/spore-semantic lookup <VA>          # bare digits are HEX
~/.local/bin/spore-semantic lookup <VA> --json

# universe / SDK tables (committed artifacts)
.spore-analysis/ghidra-exports/functions.tsv       # 58 757 entries: address\tname\tsize\t…
.spore-analysis/ghidra-exports/sdk_functions.tsv    # 1 666 entries: address\tname
knowledgegraph/triage/datarefs-2540f2ca.tsv         # data-reference snapshot
knowledgegraph/triage/triage-f0e310e0.triage-v6.jsonl

# Ghidra MCP, program SporeApp.exe, headless
ghidra_get_struct_layout / get_enum_values / search_data_types / search_strings
ghidra_decompile_function / disassemble_function / get_xrefs_to
ghidra_run_script_inline        # operand-scalar sweeps; Ghidra 12 API (ins.getScalar(i))
```

Bare address digits are **hex** throughout, as in `AGENTS.md`; every VA in this document
is written `0x%08x` lower-case. The address-spelling trap applies to the tool arguments
too: `lookup 004ae3b0` and `lookup 0x4ae3b0` are the same request, `lookup 925050` is
**not** `0x00925050`.