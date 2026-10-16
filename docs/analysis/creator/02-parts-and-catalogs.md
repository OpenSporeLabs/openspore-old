# 02 — Parts and catalogs in Spore's data

Agent **PARTS**. Research only. Every number below came from a command run on the
read-only install at `./SPORE`; the command is quoted next to the number so any
figure can be re-derived. No asset byte is reproduced here — hex ids, sizes,
counts and structure only.

Evidence grades use `spore_core::EvidenceLevel`:
`UNKNOWN < APPROXIMATION < INFERRED < SUPPORTED < OBSERVED < CONFIRMED < VERIFIED`.
`PROVEN` in older documents ≈ `CONFIRMED`/`VERIFIED`.

## 0. Executive summary

1. Spore has **two disjoint record layers per content family**, not one. A
   *rigblock* layer (type `0x5caa5e28`, group `<family>0x6000`, in `Spore_Game`)
   and a *model-slot* layer (group `<family>0x6200`, in `Spore_Content`). Their
   **instance-id sets intersect in exactly 0 records for every family**
   (CONFIRMED). They are linked only by the sporemol XML.
2. **A "part" is a rigblock record.** `Editors::EditorRigblock` (3592 B) carries
   `mInstanceID`, `mGroupID`, a `ResourceKey mModelRigBlockType`, `mpParent`,
   `mChildren[8]`, `mpSymmetricRigblock`, `mFootType/mMouthType/mWeaponType` and
   nine `fixed_vector<ResourceKey,8>` snap/align/interaction lists
   (SUPPORTED, SDK struct + code).
3. **1549 rigblock records exist in `Spore_Game`**: creature 482, vehicle 540,
   building 224, flora 107, `0x40686000` 138, cell 58 (OBSERVED).
4. **4209 `gmdl` records in 30 groups / 8 families.** Creature models are
   median 511 KB — whole assembled creatures, not parts (OBSERVED).
5. **68 sporemol XML assembly records** (47 `crt`, 13 `cll`, 4 `vcl`, 2 `ufo`,
   1 `bld`, 1 `flr`) carry **680 `<blockref>` entries, of which 680 resolve**
   (0 unresolved) to rigblock records — the reference graph is closed
   (CONFIRMED).
6. **Attachment/hierarchy is real and fourfold**: gmdl skinning
   (`BLENDINDICES`/`BLENDWEIGHT` + a bone-range array), sporemol
   `<childlist><childid>` (parent index) + `<symmetric>` (mirror index) +
   `<handles><weight channel=0x…>` (muscle channel), and a code-level skeleton
   with named joints (CONFIRMED for the code, OBSERVED for the bytes).
7. **There is no catalog or index over parts.** A scan of every 4-byte word of
   all 2673 `group 0x00000000` records in `Spore_Game` against all 1549
   rigblock ids and 1660 model-slot ids returned **6 hits and 37 hits** and
   nothing else (CONFIRMED negative).
8. **The real catalog chain is a cell-world spine**, not a parts catalog:
   `world` (13) → `populate` (22) → `cell` (223) / `cell_structure` (169), with
   `lootTable` (53) → `cell` and 3 nested `lootTable` records (CONFIRMED).
9. **Package priority is load-bearing.** 17 415 of 81 199 distinct
   `ResourceKey`s (21.5 %) live in more than one package; all 1162 keys shared
   between `PatchData` and `Spore_Content` **differ in bytes** (OBSERVED).
10. **A shared "part" concept is justified**: creature, vehicle, building, UFO,
    flora and cell all use the same record type, the same XML grammar and the
    same parent/handle/symmetric mechanism. Only the *group family* differs —
    and UFO generation **reuses the vehicle rigblock family** (OBSERVED).

---

## 1. Corpus surveyed

```
target/release/osptool manifest SPORE/Data/Spore_Content.package   --out content.jsonl --stats
target/release/osptool manifest SPORE/Data/Spore_Game.package      --out Spore_Game.jsonl
target/release/osptool manifest SPORE/Data/PatchData.package       --out PatchData.jsonl
target/release/osptool manifest SPORE/Data/Spore_Pack_03.package   --out Spore_Pack_03.jsonl
target/release/osptool manifest SPORE/bp1content/BoosterPack_01.package --out BoosterPack_01.jsonl
target/release/osptool manifest SPORE/DataEP1/Spore_EP1_Content_01.package --out Spore_EP1_Content_01.jsonl
target/release/osptool manifest SPORE/DataEP1/Spore_EP1_Content_02.package --out Spore_EP1_Content_02.jsonl
target/release/osptool manifest SPORE/DataEP1/Spore_EP1_Data.package      --out Spore_EP1_Data.jsonl
```

| package | index rows |
|---|---:|
| `Spore_Content.package` | 17 119 |
| `Spore_Game.package` | 11 529 |
| `PatchData.package` | 25 535 |
| `Spore_Pack_03.package` | 266 |
| `BoosterPack_01.package` | 6 002 |
| `Spore_EP1_Content_01.package` | 5 807 |
| `Spore_EP1_Content_02.package` | 10 801 |
| `Spore_EP1_Data.package` | 22 204 |
| **total rows** | **99 263** |
| **distinct `(type, group, instance)`** | **81 199** |
| **keys present in > 1 package** | **17 415** |

`Spore_Content` per-type histogram, from the same command:

```
   4209  0x00e6bce5  gmdl          2954  0x2f4e681c  raster
   1709  0x1a99b06b  bem           1642  0x2f7d0004  png
   1641  0x030bdee3  pollen_metadata  1631  0x02d5c9af  summary
   1162  0x00b1b104  prop          1131  0x2f4e681b  rw4
   1022  0x0f43029a  world_object    18  0x02d5c9b0  summary_pill
```

---

## 2. The DBPF group scheme, measured

### 2.1 Family = upper 16 bits (CONFIRMED, byte-level)

`gmdl` records in `Spore_Content` fall in exactly 8 families:

```
0x4061  132      0x4062  388      0x4063 1312      0x4064 1140
0x4065  364      0x4066  612      0x406b   67      0x4162  194
```

Within a family the **low 16 bits** select the record's role. Two role bands
matter:

| low 16 | meaning | evidence |
|---|---|---|
| `0x6000` | **rigblock** record, type `0x5caa5e28`, lives in `Spore_Game` | OBSERVED |
| `0x6200` / `0x6201..03` / `0x7100` / `0x7e00` | **model slot**: metadata + LOD ladder, in `Spore_Content` | OBSERVED |
| `0x2900..0x2902` / `0x2800..01` / `0x2c00..01` | texture sub-blocks (`raster`) | OBSERVED |

Canonical group names come from the prop directory and are reproduced by
`target/release/osptool types --group`: `CellModels 0x40616200`,
`CellImages 0x40616201`, `CreatureModels 0x40626200`,
`CreatureModelsHQ 0x40627100`, `BuildingModels 0x40636200`,
`VehicleModels 0x40646200`, `UfoModels 0x40656200`, `FloraModels 0x40666200`,
`PaletteModels 0x406b6200`, `EditorRigblocks 0x40606000`.

### 2.2 Rigblock layer — type `0x5caa5e28` (CONFIRMED)

```
python3 -c "... group/type histogram over Spore_Game.jsonl ..."
```

| group | records | meaning | size min/med/max |
|---|---:|---|---|
| `0x40616000` | 58 | cell rigblocks | 36 / 66 / 228 |
| `0x40626000` | 482 | **creature** rigblocks | 36 / 48 / 804 |
| `0x40636000` | 224 | **building** rigblocks | 36 / 84 / 3 108 |
| `0x40646000` | 540 | **vehicle** rigblocks | 36 / 228 / 49 188 |
| `0x40666000` | 107 | **flora** rigblocks | 36 / 84 / 804 |
| `0x40686000` | 138 | **UNKNOWN family** | 48 / 84 / 84 |
| **total** | **1 549** | | |

`Spore_EP1_Data.package` adds 43 more `0x5caa5e28` records (43 at `0x40686000`,
plus its own share of the others). `PatchData.package` carries a byte-identical
copy of all 1 549 (see §7).

Type id `0x5caa5e28` is **not** in `tools/spore/types/typenames.json` and not in
the in-game type dictionary (§6.1). Its identity as the rigblock record is
SUPPORTED: it is the only type in the `0x?6?6000` bands, it is what every
`<blockid>` in the sporemol XML resolves to (680/680), and it sits at the
`(group, instance)` the XML names.

### 2.3 Model-slot layer — group `<family>0x6200` (CONFIRMED)

Every group in this layer has `nrec == ninst`: one record per instance, i.e. a
**slot** is an instance id, and the types hanging off it are the slot's facets.

| slot group | name | slots | gmdl LOD groups | raster | other facets |
|---|---|---:|---|---|---|
| `0x40616200` | CellModels | 44 | `0x40616201`, `0x40616202`, `0x40617100` (44 each) | `0x40612900/01` | prop, bem, png, pollen, summary, world_object |
| `0x40626200` | CreatureModels | **759** | `0x40626202/03`, `0x40627100/01`, `0x41626202/03` (**97 each**) | `0x40622900/01` (97) | prop (97), bem (759), png (759), pollen (759), summary (758), world_object (758) |
| `0x40636200` | BuildingModels | 310 | `0x40636200/01/02/03`, `0x40637e00/01/02/03` (164 each) | `0x40632900/01/02` (164) | prop, rw4, bem, png, pollen, summary |
| `0x40646200` | VehicleModels | 285 | `0x40646200/01/02/03` (285 each) | `0x40642900/01/02` (285) | prop, rw4, bem, png, pollen, summary |
| `0x40656200` | UfoModels | 91 | `0x40656200/01/02/03` (91 each) | `0x40652900/01/02` (91) | prop, rw4, bem, png, pollen, summary |
| `0x40666200` | FloraModels | 153 | `0x40666201/02/03`, `0x40667100` (153 each) | `0x40662800/01`, `0x40662900/01`, `0x40662c00/01` | prop, bem, png, pollen, summary, world_object |
| `0x406b6200` | PaletteModels | 67 | `0x406b6200` (67) | `0x406b2900/01` | prop, bem, world_object |

Each populated family has a **4-step LOD ladder** under one instance id, sizes
falling monotonically:

```
target/release/osptool manifest … ; python3 size stats per group
creature 0x40627100 n= 97 min  138528 med  511224 max 1351408
creature 0x40626202 n= 97 min   18616 med  105120 max  371776
creature 0x41626202 n= 97 min   46768 med  210420 max  741348
creature 0x40626203 n= 97 min    5188 med   26064 max   73932
vehicle 0x40646200 n=285 min  172300 med  506272 max 1104768
vehicle 0x40646203 n=285 min    8124 med   31980 max   87012
building 0x40636200 n=164 min  181664 med  716776 max 1490780
```

### 2.4 The two layers do not share instance ids (CONFIRMED — the key negative)

```
python3 -c "set(0x5caa5e28 @0x40626000 in Spore_Game) & set(model-slot ids @0x40626200 in Spore_Content)"
```

| family | rigblock ids | model-slot ids | `rig ∩ slot` | `rig ∩ gmdl` |
|---|---:|---:|---:|---:|
| creature `0x4062` | 482 | 759 | **0** | **0** |
| vehicle `0x4064` | 540 | 285 | **0** | **0** |
| building `0x4063` | 224 | 310 | **0** | **0** |
| flora `0x4066` | 107 | 153 | **0** | **0** |
| cell `0x4061` | 58 | 44 | **0** | **0** |

Consequence: **no field of either record carries the other's key**, and the only
observed bridge is the sporemol XML `<blockid>`. Any creator document that
treats one layer as a sub-range of the other is wrong.

---

## 3. Model (gmdl) records as part candidates

### 3.1 Counts and clustering

```
target/release/osptool types --package SPORE/Data/Spore_Content.package
   4209  0x00e6bce5  gmdl
python3 group/type histogram over content.jsonl  → 30 distinct gmdl groups
```

| group | records | median bytes | decoded | bone ranges > 0 | has `BLENDINDICES` |
|---|---:|---:|---:|---:|---:|
| `0x40616201` CellImages | 44 | 37 247 | 44 | 44 | 44 |
| `0x40616202` | 44 | — | 44 | 44 | 44 |
| `0x40617100` | 44 | 203 915 | 44 | 44 | 44 |
| `0x40626202` | 97 | 105 120 | **0** | 0 | 0 |
| `0x40626203` | 97 | 26 064 | **0** | 0 | 0 |
| `0x40627100` CreatureModelsHQ | 97 | 511 224 | **0** | 0 | 0 |
| `0x40627101` | 97 | — | **0** | 0 | 0 |
| `0x41626202` | 97 | 210 420 | **0** | 0 | 0 |
| `0x41626203` | 97 | — | **0** | 0 | 0 |
| `0x40636200..03` | 164 each | 716 776 (L0) | 164 each | 0 | 66 / 66 / 0 / 0 |
| `0x40637e00..03` | 164 each | — | 164 each | 0 | 26 / 25 / 0 / 0 |
| `0x40646200..03` | 285 each | 506 272 (L0) | 285 / 284 / 285 / 285 | 0 | 268 / 268 / 268 / 0 |
| `0x40656200..03` | 91 each | 536 412 (L0) | 91 each | 0 | 65 / 65 / 0 / 58 |
| `0x40666201..03` FloraModels | 153 each | 56 650 | 153 each | 0 | 0 |
| `0x40667100` FloraModels | 153 | 115 536 | 153 | **153** | 153 |
| `0x406b6200` PaletteModels | 67 | 34 540 | 30 | 30 | 30 |
| **total** | **4 209** | | **3 590** | **315** | **1 490** |

Command for the decode/failure split:

```
target/release/osptool verify SPORE/Data/Spore_Content.package --type 0x00e6bce5
  walked 4209   decoded 3472   failed 737   errors 103 distinct
      619  gmdl: undocumented shader-data id 0x218 in material info
```

The 619 shader refusals are exactly **582 + 37**: all 582 records of the six
creature gmdl groups (97 × 6) and 37 of the 67 `0x406b6200` records. Per the
repo rule "do not relax a decoder to make a test pass", that refusal is the
specification, and it has the consequence that **no attachment measurement is
possible on any creature mesh with the current build** (grade: a limit, not a
finding about the data).

### 3.2 Which gmdl are plausibly parts?

**None of the 4 209 `gmdl` are parts.** Evidence:

* The six creature gmdl groups hold **the same 97 instance ids** — one LOD
  ladder per model, not 6 × 97 models.
* Creature model meshes are median 511 KB (max 1.35 MB) with 4 LODs; a limb or
  an eye would not need a 4-step LOD ladder at that size. `0x40626203` (LOD 3)
  still has a median of 26 KB.
* `gmdl` never references another `gmdl`. The `referenced files` key table
  resolves to `raster`/`rw4` texture groups; RESOURCE-GRAPH.md §9 already
  records this and `osptool describe` reproduces it, e.g.
  `0x00e6bce5:0x40646200:0x3bf4bc9a` → `texture refs 3` all in
  `0x40642900/01/02`. A part-referencing-parts mesh would show a second mesh
  group here; **0 of 4 209 show one** (SUPPORTED).
* The part geometry is not in a `gmdl` at all: the sporemol `<blockid>` points
  at `0x5caa5e28` records, and 1 549 of those exist against 4 209 `gmdl`.

So the honest statement is: **`gmdl` is the whole-model layer, and a "part" is a
rigblock, not a mesh.** Grade: SUPPORTED (four independent measurements agree).

### 3.3 The 759-vs-97 anomaly in the creature family (OBSERVED, unexplained)

| family | model slots | slots with a `gmdl` | slots with **no** `gmdl` |
|---|---:|---:|---:|
| creature | 759 | 97 | **662** |
| building | 310 | 164 | **146** |
| vehicle | 285 | 285 | 0 |
| flora | 153 | 153 | 0 |
| cell | 44 | 44 | 0 |
| ufo | 91 | 91 | 0 |
| palette | 67 | 67 | 0 |

The 662 creature slots still carry `bem` (1 195–11 922 B, median 4 292), a
**128×128 RGBA `png` thumbnail** (verified: every `png` at `0x40626200` starts
`89 50 4e 47 … 00 00 00 80 00 00 00 80 08 06`), `pollen_metadata`,
`summary` and `world_object`. They have **no** `gmdl`, **no** `raster`, **no**
`prop`.

Whether those 662 are the part library, editor-only model templates, or
unbaked abstract models is **UNKNOWN**. Facts that constrain any later answer:

* 662 ≠ 482 (the creature rigblock count), so a 1:1 "one rigblock per slot"
  reading is excluded by arithmetic.
* `Spore_EP1_Content_01.package` has **179** creature slots and **all 179** have
  a `gmdl` in every one of the six gmdl groups, plus 2 `raster`. The
  mesh-less-slot phenomenon is a **base-game-only** property.
* `Spore_EP1_Content_02.package` has **515** building slots and **all 515** have
  a `gmdl` in `0x40636200` and `0x40637100` and `0x40637e00..03`.

Grades: the counts are OBSERVED; the meaning is UNKNOWN. Do not promote it.

### 3.4 `bem` and `world_object` are near-relatives, not the same format (OBSERVED)

`bem` opens `d3 7e 65 48`, `world_object` opens `b7 55 b4 ab`. For all **1 022**
instance ids that carry both (`0x40616200` 44, `0x40626200` 758, `0x40666200` 153,
`0x406b6200` 67) the longest common run is **60–81 bytes starting at offset 13
(bem) / 17 (world_object)** — a shared sub-header (model bounds, seeds, counts)
after which the payloads diverge. Same family of container, different records.
Grade OBSERVED; the shared header's field meanings are UNKNOWN. This upgrades
the standing note in `docs/RESOURCE-GRAPH.md` §10 ("`bem` — nothing
established") to "related container, header shared, payload not decoded".

---

## 4. The catalog / index question

### 4.1 What exists (the cell-world spine) — CONFIRMED

All in `Spore_Game.package`, all at `group 0x00000000`:

| type id | name | count | what it enumerates |
|---|---|---:|---|
| `0x9b8e862f` | `world` | 13 | 289 u32 word slots; **45 name a `populate` instance, 22 distinct = all 22 `populate` records** |
| `0xda141c1b` | `populate` | 22 | 7 441 u32 word slots; **12/12** of the ids I sampled resolve to `cell` (0xdfad9f51) or `cell_structure` (0x4b9ef6dc) |
| `0xd92af091` | `lootTable` | 53 | 1 401 u32 word slots; **15/15** sampled non-lootTable ids resolve to `cell`; **3** slots name *another* `lootTable` (`0x21b779b4`→`0xdfd2adee`, `0x7cc79d94`→`0x9e91f6dd`, `0x90e48b5a`→`0xb95325d3`) → a tree |
| `0xdfad9f51` | `cell` | 223 | leaves (796 B each, fixed size) |
| `0x4b9ef6dc` | `cell_structure` | 169 | **33 of 169** name a `CellModels` slot id at word[11] |
| `0x8c042499` | `look_table` | 10 | small word tables, 16–88 B; no id resolves |
| `0xf9c3d770` | `randomCreature` | 10 | 36–120 B; the 120 B one (`0x9a391385`) enumerates ids that resolve to **`cell`** (`0xe219e5d0`, `0xe219e5d3`) |
| `0xdba35ae2` | `look_algorithm` | 1 | 428 B, pairs of (u32, ResourceKey-ish) |
| `0x055ada24` | `cnv` | 56 | groups `0x02ae1a8e` (3) and `0x055ada23` (53) |

Commands: `python3` word dumps over the decompressed records (RefPack via
`tools/spore/dbpf/dbpf.py:qfs_decompress`), then id resolution against the eight
manifests.

`cll` (0x3d97a8e4) at group `0x40616200` — 13 records — is **not** a catalog; it
is sporemol XML (§5).

### 4.2 What does not exist (the negative that matters) — CONFIRMED

**No record in any of the eight packages enumerates the rigblock families or the
model-slot families.**

Test: for every one of the **2 673** records at `group 0x00000000` in
`Spore_Game.package` (0 unreadable), scan every little-endian u32 word and test
membership in the 1 549 rigblock instance ids and the 1 660 model-slot instance
ids.

```
word slots naming a rigblock instance id : 6   (all in type 0xee17c6ad `animation`)
word slots naming a model-slot instance id: 37  (33 in 0x4b9ef6dc cell_structure
                                                word[11]; 4 in 0xf9c3d770)
```

The 6 `animation` hits are worth recording because they are the only
out-of-XML pointer at a part:

| animation record | word | rigblock id | group |
|---|---:|---|---|
| `0x7b68daad` | 18 547 | `0x00013530` | `0x40626000` |
| `0x938dd2d4` | 19 750 | `0x00013531` | `0x40626000` |
| `0xa69c769c` | 6 502 | `0x00013530` | `0x40626000` |
| `0xa69c769c` | 6 612 | `0x00013531` | `0x40626000` |
| `0xa69c769c` | 6 832 | `0x00013533` | `0x40626000` |
| `0xa69c769c` | 7 162 | `0x00013536` | `0x40626000` |

Grade INFERRED (four of the six are a sequential run `0x00013530..36`, which is
what a genuine reference looks like, but six hits is also within reach of
coincidence over 2 673 records; a corpus-wide word scan on `Spore_EP1_Data` and
`PatchData` was not run for this line of inquiry).

**What the docs call "the creature DB" is the `world`→`populate`→`cell` spine
above, and it indexes *cell creatures*, not parts.** Grade CONFIRMED for the
chain; the phrase "creature DB" is UNSUPPORTED as a parts catalog.

### 4.3 `summary`, `pollen_metadata`, `prop` — measured, not decoded

* `pollen_metadata` (`0x030bdee3`, 1 641 in `Spore_Content`) is 135–139 B and
  carries **its own instance id** at offset 12 (BE): `0x8933cc09` in record
  `0x030bdee3:0x40626200:0x8933cc09`. This is the self-identity already
  recorded in `docs/RESOURCE-GRAPH.md` §10 — confirmed again here.
* `summary` (`0x02d5c9af`, 1 631) is a table of (u32, u32) pairs, e.g.
  `00 00 00 04 | 00 00 00 27 | 02 dd 90 af | 02 e1 a7 5d | 2b 97 8c 46 | …`.
  The constant `0x468c972b` recurs in both `summary` and `pollen_metadata`.
  Its meaning is UNKNOWN.
* `prop` (`0x00b1b104`) uses **big-endian** 32-bit header words
  (`00 00 00 06 | 00 b2 cc ca | 00 13 00 0d | <BE char count> | UTF-16LE
  value`). BE, not LE — this is worth recording because it is the opposite of
  `pollen_metadata`'s self-id and of `gmdl`'s payload. A named `prop` record
  carries its value as a *literal property key* followed by the value, e.g.
  `0x00b1b104:0x406a0200:0xc40166d0` → key `name`… no: key `04 "Plaster 01"` at
  offset 16. The full layout stays **UNKNOWN**, consistent with the standing
  `graph::claims::PROP_LAYOUT_UNRESOLVED` claim. Nothing here was used to
  derive an identity.

  The same group also contains **44 palette definitions** at
  `0x00b1b104:0x406b6a00:*`. Their first property is a UTF-16LE title, including
  `Creature Editor Palette` (instance `0x0c`), `Creature Editor Texture Palette`
  (`0x0d`), `Flora Editor Palette` (`0x14`), `Building Editor Palette` (`0x28`),
  and multiple `Vehicle Editor Parts Palette` records (`0x32..0x3d`, with gaps).
  This establishes that palette definitions and descriptive titles exist; it does
  **not** establish which of the six creature-editor tabs selects which category,
  nor a category/page/item-to-rigblock mapping. The binary tail contains further
  resource-like values, but their record grammar and semantics remain UNKNOWN.
  The editor therefore keeps its tab identities positional until the runtime
  lookup chain (`PaletteMain` → `PaletteCategory` → pages → items) is fully
  reconstructed.

  **Runtime follow-up (Ghidra, `SporeApp.exe` 3.1.0.22, image base
  `0x00400000`).** The UI side of that chain is now better bounded:

  * `FUN_005cb5a0` constructs one page/category view for every entry in a
    runtime vector and passes each entry to `FUN_005c53c0`.
  * `FUN_005cb240` validates a selected category index against that vector,
    stores the active entry, and calls `FUN_005cadb0(index / 9)`.
  * `FUN_005cadb0` toggles previous/next-page controls based on a page size of
    nine. `FUN_005cae60` lays out the items in a 3×3 page.
  * `FUN_005c9230` constructs a page from the category's runtime item array and
    creates item UI objects from those records.

  This confirms that category selection and nine-item pages are real runtime
  behavior. It does **not** recover the six visible creature-editor tab slots'
  category ids, nor the `prop` grammar that links category items to rigblock
  identities. OpenSpore therefore still does not filter its rigblock catalog by
  those tabs. The runtime validates a category index against a live category
  vector; treating that position as a category identity would still be a guess.

  **Attachment follow-up.** The recovered authoring document supports parent
  indices and arbitrary local transforms, and the runtime `EditorRigblock`
  exposes parent/child links plus snap-compatibility lists. The contents of those
  lists and the socket transforms remain unknown. OpenSpore's new manual drop
  behavior uses its existing bounds picker: dropping a catalog part on a picked
  block adds a child whose local position is the hit point transformed into the
  parent's local space. This is an OpenSpore authoring interaction, not a claim
  to reproduce Spore's automatic snap or attachment rules.

### 4.4 Where names *do* live (measured, all UTF-16LE strings inside `prop`)

| package | group | named `prop` records | sample strings |
|---|---|---:|---|
| `Spore_Game` | `0x406a0200` | 787 of 916 | `Plaster 01`, `stone 01`, `stone 02`, `skinpaint_detail_giraffe`, `skinpaint_cell_blankMatte` |
| `Spore_Game` | `0x40606600` | 163 of 163 | `Muscle_mkbicepUberLimb`, `Group_ThighMax_0`, `Group_FE_muscle_branch_03a_1` |
| `Spore_Game` | `0x30608f0b` | 351 of 363 | `Create Cute Plateau`, `Messianic Rising`, `color water green` |
| `Spore_Game` | `0xe38475d9` | 162 of 162 | `Mesas_pickle`, `Mountains_coral`, `Crystal`, `Cube_ashtray`, `Bumpy_tentacle` |
| `Spore_Content` | `0x40646200` VehicleModels | 285 of 285 | the literal key `name` only |
| `Spore_Content` | `0x40636200` BuildingModels | 164 of 164 | the literal key `name` only |
| `Spore_Content` | `0x40637e00` | 164 of 164 | the literal key `name` only |
| `Spore_Content` | `0x40656200` UfoModels | 91 of 91 | the literal key `name` only |
| `Spore_Content` | `0x40626200/0x40666200/0x40616200/0x406b6200` | **0** | none — **no creature, flora, cell or palette model has any name string** |

Grade OBSERVED for the presence/absence; UNKNOWN for the value encoding (the
value that follows the literal key `name` in the model props did not decode).

The `0x40606600` set is the closest thing to a **muscle-channel dictionary** and
it is valuable: 163 named animation/muscle channel groups exist as `prop`
records, which is the vocabulary the `<weight channel="0x…">` ids draw on
(§5.3). Mapping channel id → name is UNKNOWN.

---

## 5. The assembly records: `<sporemodel>` XML (CONFIRMED)

### 5.1 Census

```
python3 -c "type histogram of 0x2b978c46/0x2399be55/0x24682294/0x3d97a8e4/0x476a98c7/0x438f6347 over Spore_Game.jsonl"
```

| type | name | group | count | `formatversion` | `<name>` |
|---|---|---|---:|---|---|
| `0x2b978c46` | `crt` | `0x40626200` CreatureModels | 47 | 16 ×46, 11 ×1 | `PaintBuddy` ×1 |
| `0x3d97a8e4` | `cll` | `0x40616200` CellModels | 13 | 16 ×12, 15 ×1 | `PaintSwatch` ×1 |
| `0x24682294` | `vcl` | `0x40646200` VehicleModels | 4 | 15 | `gencar`, `genair`, `gensea`, `VehiclePaintSwatch` |
| `0x476a98c7` | `ufo` | `0x40656200` UfoModels | 2 | 15 | `genufo`, `UFOPaintSwatch` |
| `0x2399be55` | `bld` | `0x40636200` BuildingModels | 1 | 16 | — |
| `0x438f6347` | `flr` | `0x40666200` FloraModels | 1 | 15 | `paintBuddy` |

`PatchData.package` holds byte-identical copies of all 68 (§7).
`Spore_EP1_Data.package` holds none. **There are no other assembly records in any
package** — this is the complete set.

`modeltype` per family: `crt` → `0x9ea3031a` ×46 (and decimal `2661483290` =
the same value, ×1); `cll` → `0xdfad9f51` (= the `cell` type id); `vcl` →
`0xc0b74287`/`0x441cd3e6`/`0x8f963dcb`/`0x7d433fad`; `ufo` → `0x98e03c0d`;
`bld` → `0x47c10953`; `flr` → `0xbcd73e89`.

### 5.2 Family → rigblock family mapping (CONFIRMED by exhaustive resolution)

```
python3 -c "parse all 68 records; for each <blockid> group,instance test membership
            in {0x5caa5e28: (group, instance)} from Spore_Game"
  assembly records 68   blockrefs 680   unresolved 0   distinct rigblock ids 60
```

| assembly family | blockrefs | referenced rigblock group | distinct ids |
|---|---:|---|---:|
| creature (`crt`) | 486 | `0x40626000` (482 available) | 6 |
| cell (`cll`) | 79 | `0x40616000` (58) | 3 |
| vehicle (`vcl`) | 57 | `0x40646000` (540) | 43 |
| **ufo** (`ufo`) | 47 | **`0x40646000`** — the *vehicle* family | (in the 43) |
| building (`bld`) | 8 | `0x40636000` (224) | 6 |
| flora (`flr`) | 2 + 1 | `0x40666000` (107) **+ `0x40626000`** (creature) | 2 + 1 |
| **total** | **680** | | **60** |

Two findings that matter for a creator document:

* **UFO generation reuses the vehicle rigblock family** (`0x40646000`). The
  UFO editor has no rigblock family of its own. Grade CONFIRMED.
* **Flora generation borrows one creature rigblock** (`flr 0x40666200:0xde727fa9`
  references `0x40666000:0xf80fc958`, `0x40666000:0xe6b1310a` **and**
  `0x40626000:0xa077208c`). Grade CONFIRMED; independently matches
  `docs/CELLSTAGE-RECON.md` §4.

The 47 cell-creature `crt` records draw on only **6 distinct rigblocks**, of
which `0x40626000:0xd05c53a3` accounts for 480 of 486 blockrefs — i.e. the cell
creatures in the base game are built from a very small pool. Grade CONFIRMED
(measured, not sampled).

### 5.3 Attachment points inside the XML (OBSERVED — byte-level, exact counts)

Over the 68 records / 680 `<blockref>` entries:

| element | occurrences | reading |
|---|---:|---|
| `<blockid>group, instance</blockid>` | 680 | rigblock identity; **680/680 resolve** |
| `<transform>` / `<scale>` / `<position>` / `<orientation>` | 680 | placement |
| `<snapped>` | 680 | boolean |
| `<childlist>` / `<childid>` | 437 / **439** | **`childid` is the PARENT block's ordinal** — a real hierarchy |
| `<symmetric>N</symmetric>` | **84** | **N is the ordinal of the mirror-symmetric block** |
| `<handles>` / `<weight channel="0x…">` | **119** / **128** | animation **muscle channel id** + influence weight |
| `<limbtype>` | 5 | limb role |
| `<musclescale>` / `<basemusclescale>` | 5 / 8 | muscle deformation scale |
| `<blockbbox>` / `blockbbox_min` / `blockbbox_max` | 10 | per-block bounds |
| `triangledirection` / `trianglepickorigin` | 680 / 476 | pick frame |

Distinct muscle channel ids across all 68 records: **14**, most frequent
`0xd02751de` ×85, `0x2e6521f4` ×56, `0x7cc96c02` ×46, `0x9bfe8bf8` ×18.

Worked example, `crt 0x40626200:0xde727fa9` (`PaintBuddy`, `formatversion` 11,
10 blocks) — the only record carrying `limbtype`, `symmetric` and `handles`:

```
<blockref><blockid>0x40626000, 0xd05c53a3</blockid>
  <transform><scale>0.758334</scale><position>0,-0.054405,0.500937</position>
    <blockbbox><blockbbox_min>-0.206856,-0.053101,-0.335873</blockbbox_min>
               <blockbbox_max> 0.206856, 0.053101, 0.022412</blockbbox_max></blockbbox>
  </transform><snapped>false</snapped>
  <childlist count="1"><childid>1</childid></childlist></blockref>

<blockref><blockid>0x40626000, 0xdec3c22a</blockid>
  <transform><musclescale>0.3</musclescale><limbtype>0</limbtype>
    <basemusclescale>0.6</basemusclescale>…</transform>
  <childlist count="1"><childid>5</childid></childlist>
  <symmetric>6</symmetric>
  <handles count="1"><weight channel="0xf320ba37">0.054213</weight></handles></blockref>

<blockref><blockid>0x40626000, 0x7c830510</blockid>…
  <symmetric>9</symmetric>
  <handles count="2"><weight channel="0x2e6521f4">0.337822</weight>
                     <weight channel="0x7cc96c02">0.203777</weight></handles></blockref>
```

Note the ordinals: blocks 4↔6, 5↔8, 7↔4 (via `childid`) and 8↔5, 9↔9 —
`<symmetric>` is a back-reference into the same `<blocks>` array, and
`<childid>` names the parent. So a `<sporemodel>` is a **flat array of limb
instances plus two index arrays**, exactly the shape of a scene graph.

### 5.4 The rigblock record's own layout (UNKNOWN — stated as a negative)

Type `0x5caa5e28`, 1 549 records, 36 B – 49 188 B. What is measurable:

* The **first 24 bytes are six little-endian floats** that read as a plausible
  `bbox_min`/`bbox_max` pair, e.g. `0x5caa5e28:0x40626000:0x00013530`
  (36 B) = `-0.088639, -0.128505, -0.110354, 0.088327, 0.038573, 0.046983`
  followed by 12 zero bytes.
* An **optional packed key/value tail** follows, introduced by a run of `04`
  bytes (`04 04 04 00 00 00 00 00` in a 228 B record, `04 04 04 04 00 00 00 00`
  in an 804 B one) followed by a byte stream that repeats `11`, `51 00`,
  `a1 11 11`, `f0 37` — a sparse/varint encoding I did **not** crack.
* The smallest creature rigblocks (36 B) carry **no tail at all**.

Searched and did not find: any 4-byte little-endian word in a rigblock record
that equals a model-slot instance id, a `raster` instance id, or any
`0x406x?200`-family id; any ASCII or UTF-16 string in any of the 1 549
records. Grade **UNKNOWN** — an explicit "we did not decode it", which is a
different statement from "there is nothing in there".

---

## 6. Attachments and joints: what the code says

This is the part where the data alone is thin and the binary is decisive. All
figures are from the Ghidra MCP session against `SporeApp.exe` 3.1.0.22
(image base `0x00400000`).

### 6.1 Joints exist and are named (CONFIRMED at code level)

`FUN_005002d0` writes a `<SKELETON>` file (`"%ls.xsf"`,
`<SKELETON VERSION="1000" NUMBONES="%d">`). Per node it writes
`<BONE ID=… NAME=… NUMCHILDREN=…>`, `<TRANSLATION>` (from node `+0x78/+0x7c/+0x80`),
`<LOCALTRANSLATION>`, `<LOCALROTATION>`, `<PARENTID>` and one `<CHILDID>` per
child. Node stride **0xd8 (216 B)**; the node vector is `[+0x54, +0x58)`.

`NAME` is selected by `*(int *)(node + 0x0c)`:

| value | emitted name | note |
|---:|---|---|
| 0 | `joint_rootnode_%d` | |
| 1 | `joint_bone_%d` | or `joint_connbone_%d` when `*(param_1+0x24)+0x44+idx*0x54 != 0` |
| 2 | `joint_fakenode_%d` | |
| 3 | `joint_eenode_%d` | |
| 4 | `joint_rigblock_%d` | **a rigblock is a skeleton node** |

`ghidra_search_strings "(?i)rigblock"` on the binary returns 5 hits total:
`0x013eb688` "dump text descriptions for all rigblocks to
basePath or UserData/Debug/BlockDescriptions.", `0x013f158c`
`joint_rigblock_%d`, `0x014097d0` `RigblockInfo`, `0x01411d24`
`rigblockeffect`, `0x01411d34` `rigblock`.

Grade: CONFIRMED that joints exist, are typed, carry parent/child ids and that
a rigblock is one of the node kinds. Note the "text descriptions for all
rigblocks" string — a debug dump of per-rigblock *text*, which is a lead, not a
result.

### 6.2 `Editors::EditorRigblock` — 3 592 bytes (SDK struct, SUPPORTED)

`ghidra_get_struct_layout EditorRigblock`. The fields that bear on "what is a
part":

| offset | field | why it matters |
|---:|---|---|
| `+0x1c` | `mInstanceID` | the part's own instance id |
| `+0x20` | `mGroupID` | the part's own group id |
| `+0x608` | `mModelRigBlockType : ResourceKey` | **the part's full 12-byte identity** |
| `+0x10` / `+0x18` | `mpModel`, `field_14 : Graphics::Model*` | resolved mesh handles |
| `+0x33c` | `mpParent : EditorRigblock*` | **parent link** |
| `+0x340` | `mChildren : fixed_vector<EditorRigblock*, 8>` | **up to 8 children** |
| `+0x3e0` / `+0x3e4` | `mpSymmetricRigblock`, `mpAsymmetricRigblock` | **mirror pair** |
| `+0x1c8` | `mLimbType` | limb role (`<limbtype>`) |
| `+0x1cc` / `+0x1d0` | `mMuscleScale`, `mBaseMuscleScale` | (`<musclescale>`) |
| `+0x1b4/1b8/1bc` | `mDeformBoneEndJointIndex`, `…BaseJointIndex`, `…MiddleIndex` | **bone attachment** |
| `+0xdb4/0xdb8/0xdbc` | `mFootType`, `mMouthType`, `mWeaponType` | **part roles: foot / mouth / weapon** |
| `+0x3f4` | `mpSocketConnectorModel : Graphics::Model*` | **socket** |
| `+0x400/+0x40c` | `mModelMin/MaxSocketConnectorOffset`, `mSocketConnectorOffset` | **socket offsets** |
| `+0x584` | `mModelNumberOfSnapAxes` | snapping |
| `+0x79c…0xa78` | `mModelSnapDownTo`, `mModelSnapToParentTypes`, `mModelStayAbove`, `mModelAlignLateralWith`, `mModelAlignHeightWith`, `mModelAlignXYWith`, `mModelTypesToInteractWith`, `mModelTypesToSnapReplace`, `mModelTypesNotToInteractWith` — nine `fixed_vector<ResourceKey, 8>` | **the compatibility / attachment-compatibility rules, expressed as ResourceKeys** |
| `+0xc18` | `mCapabilities : fixed_vector<EditorRigblockCapability, 20>` | 20 capability slots |
| `+0x6f4/0x70c/0x734` | `mMorphHandles`, `mMorphHandleWeights`, `mMorphHandleChannels` (`fixed_vector<…, 8>`) | **(`<handles><weight channel=…>`)** |
| `+0x4f4` | `mPaints : fixed_vector<pair<uint,EditorRigblockPaint>, 8>` | `skinpaint_*` |
| `+0x5f4/0x5f8/0x5fc` | `mModelPrice`, `mModelComplexityScore`, `mModelRunTimeBoneCount` | economy + bake metadata |
| `+0x460` | `mModelMinMuscleFile`, `mModelMaxMuscleFile : ResourceKey*` | muscle curve files |

`Editors::EditorRigblockCapability` (20 B): `mPropertyID` (+0), `mIsVariableLevel`
(+4), `mCapabilityLevel` (+8), `mCapabilityLevel2` (+12).

### 6.3 `Editors::EditorModel` — 224 bytes = the XML, field for field

`mKey : ResourceKey` `+0x0c`; `mRigblocks : vector<EditorRigblock*>` `+0x18`;
`mModelType` `+0x58`; `mName` `+0x5c`; `mDescription` `+0x6c`;
`mAcceptedName` `+0x7c`; `mSkinEffectIDs[3]` `+0x8c`; `mSkinEffectSeeds[3]`
`+0x98`; `mColors[3]` `+0xa4`; `field_C8 : vector<BoundingBox>` `+0xc8`;
`mZCorpScore` `+0xdc`.

Compare with the XML's `<properties>`: `modeltype`, `skineffect1..3`,
`skineffectseed1..3`, `skincolor1..3`, `requiredpack`, `zcorpscore`, plus
`name`/`description` in `formatversion` 11 — and `<blocks>` ↔ `mRigblocks`.
The XML **is** a serialised `EditorModel` holding serialised
`EditorRigblock`s. Grade: SUPPORTED (two independent sources, one structural
and one byte-level, agree field-for-field).

### 6.4 `summary` of the attachment evidence

| mechanism | where | grade |
|---|---|---|
| parent/child hierarchy | XML `<childlist><childid>` (439 refs) **and** `EditorRigblock.mpParent/mChildren` | CONFIRMED / SUPPORTED |
| mirror symmetry | XML `<symmetric>` (84 refs) **and** `mpSymmetricRigblock` | CONFIRMED / SUPPORTED |
| muscle-channel binding | XML `<handles><weight channel>` (119 refs, 14 channels) **and** `mMorphHandleChannels`; channel-name vocabulary in 163 named `prop` at `0x40606600` | OBSERVED / SUPPORTED |
| skeletal skinning | gmdl vertex usage `BLENDINDICES`/`BLENDWEIGHT` in 1 490 of 4 209 records | OBSERVED |
| bone ranges | gmdl trailer `(start,count)` array: 315 records, **8 005 entries** total; CellImages 44/44 (702), `0x40617100` 44/44 (2 660), FloraModels `0x40667100` 153/153 (3 668), PaletteModels 30/30 decodable (273) | OBSERVED |
| socket | `mpSocketConnectorModel` + 3 socket offsets — **code only, no byte found in any record** | SUPPORTED (code) / UNKNOWN (data) |
| compatibility / snap rules | nine `fixed_vector<ResourceKey,8>` — **code only** | SUPPORTED (code) / UNKNOWN (data) |
| part roles | `mFootType`/`mMouthType`/`mWeaponType` — code only; the XML analogue `<limbtype>` carries `0` in all 5 occurrences | SUPPORTED (code) / OBSERVED (`limbtype`) |
| named joints | `joint_rigblock_%d` and 4 sibling names in `FUN_005002d0` | CONFIRMED (code) |

**Explicitly not established:** any *string* name for a rigblock or a socket, any
mapping from a `<weight channel="0x…">` id to a human-readable name, and the
contents of the nine snap/interaction ResourceKey lists. Searched: all 1 549
`0x5caa5e28` records (full bytes, ASCII and UTF-16); all 68 assembly records
(full XML); all 2 673 group-0 `Spore_Game` records (every u32 word against every
rigblock and model-slot id); every UTF-16 string in every `prop` record of
`Spore_Game` and `Spore_Content` (3 177 named records). Grade UNKNOWN.

---

## 7. Catalog identity and package priority

### 7.1 `ResourceKey` is the only identity (CONFIRMED)

`spore_core::ResourceKey` is `(type_id, group_id, instance_id)`. DBPF has **no
name table** (`docs/RENDERWARE-RESEARCH.md` §13: "no name table; 32-bit binary
IDs (FNV-1a hashes of names)"). Consequences measured here:

* `EditorRigblock` stores `mInstanceID`/`mGroupID` *and* a full
  `mModelRigBlockType : ResourceKey` — the type is recoverable at runtime but
  the XML's `<blockid>` carries **only group and instance**. The type is
  recoverable only because `0x5caa5e28` is the sole occupant of each `0x?6?6000`
  group (CONFIRMED for all 6 families).
* Instance ids are opaque hashes. Nothing in any record maps an id back to a
  string. The only human-readable anchors found are the 3 177 named `prop`
  records (§4.4) and the 5 named assembly templates (§5.1) — and none of them
  is keyed by instance id.

### 7.2 Collisions are common and mostly real overrides (OBSERVED)

17 415 of 81 199 keys (21.5 %) are in more than one package:

| combination | keys |
|---|---:|
| `PatchData` + `Spore_Game` | 10 869 |
| `BoosterPack_01` + `Spore_EP1_Data` | 4 475 |
| `PatchData` + `Spore_Content` | 1 162 |
| `PatchData` + `Spore_EP1_Data` + `Spore_Game` | 647 |
| `PatchData` + `Spore_EP1_Data` | 162 |
| `PatchData` + `Spore_Pack_03` | 56 |
| `BoosterPack_01` + `PatchData` | 25 |
| `Spore_EP1_Content_01` + `Spore_Content` | 18 |
| 4-package | 1 |

Byte comparison (RefPack-decompressed, exhaustive):

```
PatchData vs Spore_Content : common 1162  identical 0     differing 1162
PatchData vs Spore_Game    : common 11517 identical 3299  differing 8218
```

* **`PatchData` overrides all 1 162 `Spore_Content` `prop` records**, every one
  byte-different at the same memory size. Priority is load-bearing.
* **`PatchData` leaves the binary model layer alone**: all **1 549**
  `0x5caa5e28` rigblocks, all 47 `crt`, 222 `cell`, 169 `cell_structure`,
  56 `cnv`, 53 `lootTable`, 46 `txt` are byte-identical.
* **`PatchData` rewrites the text/config layer**: 6 059 `prop` and **all 2 091
  `animation` (0xee17c6ad)** records differ, plus 48 × `0x0472329b`,
  15 × `0x25df0112`, 3 × `0x24a0e52` (`txt`), 1 × `globals`, 1 × `cell`.

Grade CONFIRMED as a byte fact; the reading "patch overlay over text, base over
binary" is INFERRED.

### 7.3 What `spore-assets` does with it (code, SUPPORTED)

`crates/spore-assets/src/store.rs`: `ContentStore` is an ordered `Vec<Package>`.
`push` appends at the **lowest** priority; `push_front` inserts at the
**highest**. `find` returns the **first** package holding the key —
first-match-wins, no implicit sort, no "largest package wins", no deduplication.
`spore_core::WILDCARD` components make a partial key legal, and wildcards
resolve to the **lowest package index**, then the lowest matching row.

So, given §7.2:

* A `ResourceKey` is stable **as an identity** but not as **bytes**. The same
  key can name different content in `PatchData` and `Spore_Content`.
* Anything that reads a `prop` or an `animation` must state its package order
  or it is not reproducible.
* Anything that reads a rigblock, an assembly record, or a `gmdl` is
  order-independent among the packages surveyed (the byte-identical duplicates),
  but that is an *observation about these 8 packages*, not a guarantee.
* A creator document should treat `(type, group, instance)` as the primary key
  and record the answering package alongside it. This is a statement about what
  the data supports, not a data-model design.

---

## 8. Cross-domain check: creature / vehicle / building / UFO / flora / cell

Grade CONFIRMED for the table; the last column is the answer to "is a shared
'part' concept justified".

| domain | assembly type | assembly group | model-slot group | slots | rigblock group | rigblocks | gmdl LOD groups | part mechanism identical? |
|---|---|---|---|---:|---|---:|---|---|
| **creature** | `crt` 0x2b978c46 | `0x40626200` | `0x40626200` | 759 | `0x40626000` | **482** | `0x40626202/03`, `0x40627100/01`, `0x41626202/03` | — |
| **vehicle** | `vcl` 0x24682294 | `0x40646200` | `0x40646200` | 285 | `0x40646000` | **540** | `0x40646200/01/02/03` | — |
| **building** | `bld` 0x2399be55 | `0x40636200` | `0x40636200` | 310 | `0x40636000` | **224** | `0x40636200/01/02/03`, `0x40637e00/01/02/03` | — |
| **UFO** | `ufo` 0x476a98c7 | `0x40656200` | `0x40656200` | 91 | **`0x40646000` (vehicle)** | 540 | `0x40656200/01/02/03` | — |
| **flora** | `flr` 0x438f6347 | `0x40666200` | `0x40666200` | 153 | `0x40666000` (+ `0x40626000`) | 107 | `0x40666201/02/03`, `0x40667100` | — |
| **cell** | `cll` 0x3d97a8e4 | `0x40616200` | `0x40616200` | 44 | `0x40616000` | 58 | `0x40616201/02`, `0x40617100` | — |
| **palette** | — | — | `0x406b6200` | 67 | — | — | `0x406b6200` | — |
| **UNKNOWN** | — | — | — | — | `0x40686000` | **138** | — | — |

**Yes, a shared concept is justified, with one measured caveat.**

Shared, byte-identical, across all six domains:

* the same record type `0x5caa5e28` for the part;
* the same `<sporemodel>` XML grammar (`formatversion` 11/15/16), the same
  `<blockid>`, `<childlist><childid>`, `<symmetric>`, `<handles><weight channel>`,
  `<limbtype>`, `<musclescale>`, `<basemusclescale>`, `<blockbbox>` vocabulary;
* the same group layout (rigblocks at `<family>0x6000`, models at
  `<family>0x6200/+0x7100`, textures at `<family>0x29xx`);
* the same code-level types (`EditorRigblock`, `EditorModel`,
  `cEditorResourceBlock`).

Different, and must not be collapsed:

* **family** (`0x4061`…`0x4068`, `0x4162`) is the only discriminator. It is a
  *partition*, not a type.
* **UFO reuses the vehicle rigblock family.** A UFO part is a vehicle part.
  Any model that assumes "one rigblock family per domain" is wrong here
  (CONFIRMED, 47/47 `ufo` blockrefs → `0x40646000`).
* **Flora borrows a creature rigblock** (1 of 3 `flr` blockrefs).
* **The assembly type id is per-domain** (`crt`/`vcl`/`bld`/`ufo`/`flr`/`cll`)
  even where the payload grammar is identical. All six are listed in the in-game
  type dictionary (§9).
* **Skinning reach differs sharply**: bone ranges exist only in
  cell (44/44 + 44/44), flora `0x40667100` (153/153) and palette (30/67
  decodable). Vehicles have `BLENDINDICES` in 268/285 but **0 bone ranges**;
  buildings 66/164 and **0**; creature meshes are unmeasurable today (0/582
  decode). A part abstraction must not assume every domain is skinned the same
  way.
* `0x40686000` (138 rigblocks, 48–84 B, no model-slot counterpart in any of the
  8 packages, no assembly record referencing it) is **UNKNOWN**. It has the
  shape of an editor part library with no shipped models.

---

## 9. The in-game type dictionary (`0x01C7AC81`) — partially decoded

Two records, both `type 0x00b1b104`, both at `group 0x01c7ac81`, in
`Spore_Game.package` (and byte-identical in `PatchData.package`):

```
target/release/osptool extract SPORE/Data/Spore_Game.package \
  0x00b1b104:0x01c7ac81:0x01c7ac81 --out …      # 1044 bytes
target/release/osptool extract SPORE/Data/Spore_Game.package \
  0x00b1b104:0x01c7ac81:0x011ac199 --out …      #  228 bytes
```

Layout established (big-endian header words, UTF-16LE strings):

```
u32 version (2 | 3)
u32 propertyKey (0x00b2ccca)
u32 descriptor (00 13 00 0d)
u32 charCount + UTF-16LE   description string
u32 propertyKey (0x04334f33)
u32 descriptor (00 13 00 9c)
u32 entryCount  (42 | 2)
u32 arrayType   (0x10 = string array)
entryCount × { u32 keyCharCount, UTF-16LE keyHex, u32 valCharCount, UTF-16LE value }
```

* `0x01c7ac81:0x01c7ac81` (1 044 B), version 2, description
  **"File extension <-> resource type mapping applied to all directory save
  areas"**, 42 declared entries. My walker decoded the first **21** and consumed
  exactly 1044/1044 bytes, so the remaining 21 use a stride my parse does not
  model. Grade: the **21 mappings below are CONFIRMED**; the tail is UNKNOWN.

  `0x2f7d0002`→`jpeg`, `0x2f7d0004`→`png`, `0x1a99b06b`→`bem`,
  `0x2b978c46`→`crt`, `0x2399be55`→`bld`, `0x24682294`→`vcl`,
  `0x476a98c7`→`ufo`, `0x438f6347`→`flr`, `0x00b1b104`→`prop`,
  `0x00e6bce5`→`gmdl`, `0x2f4e681c`→`raster`, `0x011989b7`→`plt`,
  `0x0376c3da`→`hm`, `0x02d5c9af`→`summary`, `0x02d5c9b0`→`summary_pill`,
  `0x01ad2416`→`creature_traits`, `0x01ad2417`→`building_traits`,
  `0x01ad2418`→`vehicle_traits`, `0x01c3c4b3`→`trait_pill`,
  `0x030bdee3`→`pollen_metadata`, `0x01c135da`→`gmsh`.

  This is the same list, in the same order, as `target/release/osptool types`
  rows 1–21 — an independent confirmation of both.

* `0x01c7ac81:0x011ac199` (228 B), version 3, description
  **"default group and extra resource type mappings for creature save area"**,
  2 declared entries; the first decodes to `0x2b978c46`→`crt`.
  **The creature save area uses `crt` (`0x2b978c46`) as its model record type.**
  Grade CONFIRMED (the description string and the mapping are both bytes).

* `0x5caa5e28` is **absent** from the 21 decoded mappings. Grade UNKNOWN whether
  the remaining 21 entries name it. This is the single cheapest open question
  left in this document: finishing the entry stride would settle the rigblock
  type's canonical name.

---

## 10. Findings a creator document must not assume

1. **A part is not a mesh.** No `gmdl` is a part. `gmdl` is the whole-model
   layer; parts are `0x5caa5e28` records whose geometry is not stored in any
   format this workspace decodes.
2. **A part's mesh is not present in the data.** 1 549 rigblock records, and the
   only geometry-adjacent content in them is 6 floats of bounds plus an
   undecoded packed tail. Any statement about a part's shape is currently
   UNKNOWN.
3. **There is no name.** No record maps a rigblock or model-slot instance id to a
   string. Names that do exist (`gencar`, `genufo`, `PaintBuddy`, `Plaster 01`,
   `Muscle_mkbicepUberLimb`) are attached to a *different* record and are not
   joinable by id.
4. **`<blockid>` is (group, instance), not a `ResourceKey`.** The type is implied
   by the group only because `0x5caa5e28` is the sole occupant — true for all 6
   families today, and a property of the data, not of the format.
5. **Package order changes bytes for 21.5 % of keys.** Read order is part of the
   answer.
6. **The 662 mesh-less creature slots are unexplained.** Do not label them
   "parts" on the strength of a count.
7. **`0x40686000` is unclassified.**
8. **Skinning is not uniform across domains** and is unmeasurable for creatures
   until the material-info shader id `0x218` is understood (619 records).

---

## Appendix — commands used

```bash
target/release/osptool --help
target/release/osptool types --group
target/release/osptool types --package SPORE/Data/Spore_Content.package
target/release/osptool manifest <8 packages> --out <jsonl> --stats
target/release/osptool list SPORE/Data/Spore_Game.package --group 0x40626000 --limit 0 --json
target/release/osptool describe <pkg> <T:G:I>            # 4 209 gmdl, full sweep
target/release/osptool verify SPORE/Data/Spore_Content.package --type 0x00e6bce5
target/release/osptool extract <pkg> <T:G:I> --out /tmp  # 47 crt, 21 asm, 2 prop-dir, rigblock samples
python3 - <<'EOF'  # /tmp/opencode/creator-parts/*.py: DBPF index walk + RefPack (dbpf.py qfs_decompress)
EOF
# Ghidra MCP, program SporeApp.exe
ghidra_get_struct_layout EditorRigblock | EditorModel | EditorRigblockCapability
ghidra_decompile_function 0x005002d0
ghidra_search_strings "(?i)rigblock"
ghidra_get_xrefs_to 0x013f158c
ghidra_list_open_programs
```

Nothing under `SPORE/` was written, copied, or committed. All extracted records
were written to `/tmp/opencode/creator-parts/` and are hex-inspected only; no
asset byte appears in this document.
