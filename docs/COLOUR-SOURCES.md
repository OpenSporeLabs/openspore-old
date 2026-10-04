# COLOUR-SOURCES — where a Spore creature's colour actually is

**Agent:** PALETTE · **date:** 2026-10-04 · **build:** 3.1.0.22 (GOG, `SPORE/`)
**decoder:** `crates/spore-assets/src/color.rs`

---

## 0. The answer

A Spore model's colour is **two unit-range float RGB triples per paint**, written
as decimal text in the model's `sporemodel` XML record, plus **three
model-level `skincolor` slots**:

```xml
<properties>
  <skincolor1>0.341176,0.360784,0.576470</skincolor1>
  <skincolor2>1,1,1</skincolor2>
  <skincolor3>1,1,1</skincolor3>
</properties>
...
<blockref>
  <blockid>0x40626000, 0xd05c53a3</blockid>
  <paintlist count="3">
    <paint>
      <paintregion>0x00000005</paintregion>
      <paintid>0xf5967f30</paintid>
      <color1>0.450979,0.450979,0.450979</color1>
      <color2>0.749020,0.749020,0.749020</color2>
    </paint>
  </paintlist>
</blockref>
```

Those values are **not** in the textures. The creature skin shader ships as
readable source in the `smt` records (`0x0469A3F7`), and it takes the colours as
a **constant** shader struct while using the greyscale DXT5 raster only as a
blend *mask*:

```hlsl
struct { float4 mColor1; float4 mColor2; float4 mParams; } editorColors;

mixedColor = lerp(diffuseTexture.rgb, editorColors.mColor1.rgb, editorColors.mColor1.a);
mixedColor = lerp(mixedColor,         editorColors.mColor2.rgb, editorColors.mColor2.a * diffuseTexture.a);
Current.color.rgb = mixedColor * In.color.rgb;
Current.color.a   = In.color.a;
```

So the on-disk raster answers *where* each colour goes and the record answers
*what* the colours are. **A Spore creature renders uniform grey when only its
textures are read, and that is not a bug in the texture path — it is the texture
path doing exactly what it was given.** This is the whole finding.

---

## 1. Hypotheses, in the order they were tested

| # | hypothesis | verdict | how it was killed or confirmed |
|---|---|---|---|
| H1 | A `prop` record in each model group carries the colour | **refuted** | §2.1 |
| H2 | The `Palettes` group `0x406B6A00` ("palette" in the group table) holds colour tables | **refuted** | §2.2 |
| H3 | Type `0xEE17C6AD` (2 091 records, labelled "animation") is mislabelled and hides colour | **refuted** | §2.3 |
| H4 | gmdl's undocumented shader-data `0x218` (the `0x218` stop) is the colour block | **refuted** | §2.4 |
| H5 | RW4 section payloads hold it | **not refuted, and not needed** — §2.5 records what is and is not decoded | §2.5 |
| H6 | The `0x15` / DXT5 / DXT1 rasters carry albedo | **refuted** — and the premise is *scoped*, see §3 | §3 |
| H7 | A `sporemodel` XML record carries it | **CONFIRMED** | §4 |

---

## 2. What was ruled out, and how

Every number below is a byte count or a census over the real install, produced
with `tools/spore/dbpf/dbpf.py` (the committed Python oracle) through a
throwaway probe. No Spore byte is committed.

### 2.1 `prop` (`0x00B1B104`) — refuted

* 1 162 records in `Spore_Content`, 11 460 in `PatchData`, 7 030 in `Spore_Game`.
* 305 of them sit in a model group: 164 in `0x40636200` (BuildingModels), 97 in
  `0x40626200` (CreatureModels), 44 in `0x40616200` (CellModels). Sizes cluster
  at 266 / 268 / 338 / 194 bytes.
* A full ASCII/UTF-16 string scan over **every record of all five data packages**
  for `color`, `colour`, `palette`, `tint`, `hue`, `saturation` returned **no
  `prop` hit at all**. 6 065 string hits were found in total; every one of them
  is in a shader/effect record or an XML record (§4, §5).
* The 266-byte CreatureModels `prop` decodes as: a 4-byte header, 8-byte
  reference keys, hashed property keys (`f9 ef b9` … `f9 ef be`, incrementing)
  with `0x000d`/`0x0020` value-type words and `f32` payloads that are transforms
  (values like `-0.0703`, `0.1105`, `0.1466`). No unit-range RGB triple.

**Conclusion:** the `prop` family carries transforms and hashed scalar
properties. `INFERRED`, and negative: nothing in it looks like a colour.

### 2.2 The `Palettes` group `0x406B6A00` — refuted

The brief pointed at this group because `tools/spore/types/groupnames.json`
names it `Palettes` (`1080781312` = `0x406B6A00`).

* 44 records, **all** of type `0x00B1B104 prop`, in `Spore_Game.package` and
  again in `PatchData.package`. Empty in `Spore_Content`.
* 102–142 bytes each; **28 distinct payloads** across the 44 records.
* Every payload decodes as a 12-byte header, then a **UTF-16LE name**:

  ```text
  0000  00 00 00 03  00 b2 cc ca  00 13 00 0d  00 00 00 13
  0010  43 00 65 00 6c 00 6c 00  ...  "Cell Editor Palette" (UTF-16LE)
  ```

  The name-length word at offset `0x0C` is **big-endian** (`00 00 00 13` = 19
  characters) — the same class of trap as gmdl's `refCount`, and a second
  instance of it in this repo.
* The 28 distinct names are **editor UI registries**, not colours:

  | name | ×  |
  |---|---|
  | Vehicle Editor Parts Palette | 14 |
  | Cell Editor Palette | 3 |
  | Tribe Planner Palette | 2 |
  | Building Editor Palette, City Hall Building Editor Palette, Civ Accessories Editor Palette, Colony Planner Palette, Cultural City Planner Palette, Economic City Planner Palette, Entertainment Building Editor Palette, Factory Building Editor Palette, Flora Editor Palette, House Building Editor Palette, Military City Planner Palette, Planner Thumbnails Palette, Space Accessories Editor Palette, Tribe Accessories Editor Palette, UFO Editor Paint Palette, UFO Editor Parts Palette, Vehicle Editor Paint Palette, Cell Editor Texture Palette, Cell To Creature Editor Texture Palette, Cell to Creature Editor Palette, Creature Editor Palette, Creature Editor Texture Palette, Building Editor Texture Palette, Cake Editor Palette | 1 each |

* Byte counts: 3 records are byte-identical (`sha256` prefix `990db5e56acc`)
  despite different instance ids.

**Conclusion:** this group names editor palettes and points elsewhere. It holds
no RGB value. **REFUTED.**

> Side note: the sibling group `0x406B6200` (`PaletteModels`) holds 268 records
> = 67 instances × {`prop`, `gmdl`, `world_object`, `bem`}. That *is* the paint
> model family — see §3.3.

### 2.3 Type `0xEE17C6AD`, 2 091 records — refuted, and the label is right

* 2 091 in `Spore_Game.package`, 2 223 in `PatchData`. Sizes 3 888 – 68 832.
* Every record opens with the four ASCII bytes **`ANIM`**, then a `u32`, then
  `15 00 00 00` (a `u16` length) and an **ASCII authoring path**:

  ```text
  0000  41 4e 49 4d 5c 7a 00 00 15 00 00 00 43 3a 5c 53  |ANIM\z......C:\S|
  0010  70 6f 72 65 5f 50 34 5f 43 72 6f 6e 75 73 5c 44  |pore_P4_Cronus\D|
  0020  61 74 61 5c 43 72 65 61 74 75 72 65 73 5c 41 6e  |ata\Creatures\An|
  0030  69 6d 61 74 69 6f 6e 73 5c 5f 66 69 6e 61 6c 5c  |imations\_final\|
  0040  63 72 65 61 74 75 72 65 5c 63 6f 6d 5f 70 75 6e  |creature\com_pun|
  0050  63 68 2e 61 6e 69 6d 61 74 69 6f 6e 00 00 00 00  |ch.animation....|
  ```

  Other records name `space\sp_scanning.animation`,
  `creature\super_inestaposse_spawn.animation`.

**Conclusion:** the canonical label `animation` is **correct**; the record even
carries the path of the source `.animation` file it was baked from. These are
skeleton/clip data. **REFUTED as a colour source.** *(The brief's guess that the
label "may be wrong" is itself refuted.)*

Adjacent unknowns checked at the same time and also refuted:

| type | n | what the bytes are |
|---|---|---|
| `0x5CAA5E28` | 1 549 | float keyframe/curve arrays in groups `0x4061/62/63/46/66/86 6000`; sizes 36 / 48 / 84 / 228 / 804 / 3 108. Values like `-0.0703, -0.1317, 0.1105` — animation, not colour. |
| `0x030BDEE3` `pollen_metadata` | 1 641 | a UTF-8 tag string: `#tag:spore.com,2006:AssembledContent`, plus `0xFFFFFFFF` padding. |
| `0x02D5C9AF` `summary` | 1 631 | 12-byte hashed triples (`02 e1 a7 5d` repeats 5× per record) and `u32` counters. |
| `0x0F43029A` `world_object`, `0x1A99B06B` `bem` | 1 022 / 1 709 | float grids (`34 12 00 00` strides) — placement data. |
| `0x2F7D0004` `png` | 1 642 | raw PNG, `89 50 4E 47`; editor thumbnails, `IHDR` 128×128 RGBA8. Not referenced by any creature gmdl. |

### 2.4 gmdl's undocumented shader-data `0x218` — refuted, and it is *not* where the colour hides

This is the id `spore-gmdl` stops on. Measured over all 4 209 `gmdl` records of
`Spore_Content`:

| outcome | records |
|---|---|
| full material-info table decoded | **3 590** |
| stopped at `0x218` | **619** |

The 619 are exactly these groups:

| group | name | records |
|---|---|---|
| `0x40627100` | CreatureModelsHQ | 97 |
| `0x40627101` | CreatureModelsHQ variant 01 | 97 |
| `0x40626202` | CreatureModels variant 02 | 97 |
| `0x40626203` | CreatureModels variant 03 | 97 |
| `0x41626202` | (byte-1 `0x16` variant) | 97 |
| `0x41626203` | (byte-1 `0x16` variant) | 97 |
| `0x406B6200` | PaletteModels | 37 |

So **every creature-model record that would let us read its texture set is
blocked by `0x218`.** That makes `0x218` the most interesting remaining lead,
so it was measured rather than waved away.

* The id is reached through the committed `SHADER_DATA_SIZES` gap. A Python
  mirror of `crates/spore-gmdl/src/parse.rs`'s walk (throwaway, not committed)
  confirms `0x218` is the *last* entry of the *first* material's entry list in
  every one of the 619 records; the entry list before it is
  `0x210` (20 bytes) then `0x20D` (a 2-entry texture set).
* Its payload opens:

  ```text
  0x29f58  01 00 00 00  00 00 00 00  22 00 00 00  06 00 00 00
  0x29f68  00 00 00 00  00 00 00 00  00 00 00 00  00 00 00 00
  0x29f78  00 00 80 3f  00 00 80 3f  00 00 00 00  00 00 00 00
  0x29f88  00 00 00 00  00 00 80 3f  ...
  ```

  `06 00 00 00` followed by six 16-byte groups of `0.0`/`1.0` floats, then
  `12 00 00 00` and three `0xFFFFFFFF`, then `06 00 01 00` and unit-length
  float triples (`0.2607, -0.4977, 1.4518`, `0.99765, 0.99853, 0.80268`).
  Those are bind-pose axes and baked-deform samples.
* The payload is **byte-identical** between the HQ and non-HQ copies of the same
  instance (`0x40627100:0x067b0fbc` and `0x40627101:0x067b0fbc` agree for at
  least the first 0x100 bytes), which is why it is geometry, not a per-creature
  choice.
* Its **size is not determined here.** Forcing every candidate size 4…4 196 and
  requiring the remaining entries to parse yields 850 candidates for
  `0x40627100:0x067b0fbc` — because `0x218` is last, so no later id constrains
  it. Requiring the gmdl trailer to close exactly on the record end eliminates
  **all** of them: this family's baked-deform trailer overruns the committed
  framing (`spore-gmdl`'s own `TRAILER_FRAMING` claim says so). So the size
  stays **UNKNOWN** and is left to `spore-gmdl`'s owner.

**Conclusion:** `0x218` contains bone matrices and baked deform samples. It is
**not** a colour block. **REFUTED.** Its size remains open, and that is a
`spore-gmdl` question, not a colour one.

### 2.5 RW4 section payloads — not decoded, and not needed

* 1 131 `rw4` records in `Spore_Content`, 9 368 in `Spore_Graphics`, 200 in
  `PatchData`, 98 in `Spore_Pack_03`; all carry the 28-byte `RW4w32` magic.
* `spore-rw4` decodes the **section directory only** and says so in its own
  module docs. Section payloads (raster, mesh, skeleton, animation) are out of
  scope there by design.
* The type-code tables disagree between the C++ and Python references (22 vs 29
  ids, 9 in common, two ids with different names), and Spore emits `0x7000C` /
  `0x7000F`, which are in neither. **No opinion is offered here.**
* This is recorded as a *bounded* gap, not as a refutation: nothing measured
  here says a payload cannot hold colour. It says the workspace has not read one,
  and that the answer in §0 does not require it.

---

## 3. The textures — where the premise needs **scoping**, not refuting

The session premise — "DXT5 rasters carry no colour; over 1 374 records /
720 371 712 texels green span 0, blue span 0, red at most 2 values, 477 771
palette entries collapse to 2 RGB triples" — is **not refuted**. It is
**confirmed and bounded**, with one scoping correction that matters for anyone
who generalises it.

### 3.1 The full raster envelope census (`Spore_Content`, 2 954 `raster` records)

Every raster's envelope (`width@0x04`, `height@0x08`, `mip@0x0C`,
`fourcc@0x14`) takes one of **six** values, and the fourcc field is only ever
`DXT5`, `DXT1` or the family id `0x15`:

| group | records | w × h | mips | fourcc | memory bytes |
|---|---|---|---|---|---|
| `0x40612900` CellModels | 44 | 512×512 | 10 | DXT5 | 15 383 104 |
| `0x40612901` | 44 | 256×256 | 9 | `0x15` | 15 381 696 |
| `0x40622900` CreatureModels | 97 | 512×512 | 10 | DXT5 | 33 912 752 |
| `0x40622901` | 97 | 256×256 | 9 | `0x15` | 33 909 648 |
| `0x40632900` BuildingModels | 164 | 512×512 | 10 | DXT5 | 57 337 024 |
| `0x40632901` | 164 | 512×512 | 10 | DXT5 | 57 337 024 |
| `0x40632902` | 164 | 512×512 | 10 | `0x15` | 229 298 896 |
| `0x40642900` VehicleModels | 285 | 512×512 | 10 | DXT5 | 99 640 560 |
| `0x40642901` | 285 | 512×512 | 10 | DXT5 | 99 640 560 |
| `0x40642902` | 285 | 512×512 | 10 | `0x15` | 398 476 740 |
| `0x40652900` UfoModels | 91 | 512×512 | 10 | DXT5 | 31 815 056 |
| `0x40652901` | 91 | 512×512 | 10 | DXT5 | 31 815 056 |
| `0x40652902` | 91 | 512×512 | 10 | `0x15` | 127 232 924 |
| `0x40662800` | 153 | 256×256 | 9 | DXT5 | 13 382 604 |
| `0x40662801` | 153 | 256×256 | 9 | DXT5 | 13 382 604 |
| `0x40662900` FloraModels | 153 | 512×512 | 10 | DXT5 | 53 491 248 |
| `0x40662901` | 153 | 256×256 | 9 | `0x15` | 53 486 352 |
| `0x40662c00` | 153 | 256×256 | 9 | **DXT1** | 6 695 892 |
| `0x40662c01` | 153 | 256×256 | 9 | **DXT1** | 6 695 892 |
| **`0x406B2900`** | **67** | **64×64** | **7** | **DXT5** | **371 180** |
| **`0x406B2901`** | **67** | **32×32** | **6** | **`0x15`** | **369 036** |

Two rows are not like the others and one family is missing entirely from any
list a reader would build by pattern-matching on "512² DXT5":

* **`0x406B2900` / `0x406B2901` are 64×64 and 32×32.** They are the *only*
  DXT5/`0x15` pair in the corpus that is not 256² or 512², and the **only** DXT5
  records the committed raster decoder refuses. Verbatim:

  ```text
  $ osptool describe SPORE/Data/Spore_Content.package 0x2f4e681c:0x406b2900:0x0660eab3
  osptool: record `0x2f4e681c:0x406b2900:0x0660eab3`: texture: texture:
  5508 payload bytes is not a multiple of the 2760-byte layer stride
  ```

  **Flagged for `spore-texture`'s owner, not edited here.** The arithmetic:
  `dxt5_chain_size(64,64,7)` with `BLOCK_BYTES = 8` is 2 744, so the layer
  stride is 2 760 and `5 508 / 2 760` is not an integer. The same 8-byte block
  size is what makes the 512² records derive as **2 layers** and land exactly:
  `(349 616 − 32) / (16 + 349 552/2) = 2`. So the framing is self-consistent for
  19 of the 21 groups and off by 20 bytes for this one. Whether
  `0x406B2900` is one 16-bytes-per-block DXT5 chain, two 8-byte layers, or
  something else is **UNKNOWN** and belongs to `spore-texture`. What *is*
  established here is only the byte count and the refusal.
* **`0x40662C00` / `0x40662C01` are DXT1** and therefore a genuine RGB format.
  They belong to the `0x40666200` FloraModels family and are **not referenced by
  any gmdl texture set** (the reference census in §3.3 lists every group that
  is).

### 3.2 Independent corroboration that the DXT5 families are masks

Sampled with the committed decoder's own framing (8 bytes per block, colour word
read as R5G5B5, matching `spore-texture`'s documented departure from BC3), over
the first 40 records of each group:

| group | base-level endpoint words read | distinct endpoint words | dominant words |
|---|---|---|---|
| `0x40622900` CreatureModels | 1 310 720 | 46 721 | `0x0000` × 752 312, then `0x5555`, `0x2492`, `0x4924` |
| `0x40612900` CellModels | 1 310 720 | 43 357 | `0x0000` × 386 051, `0x5555` × 241 536, `0xAAAA`, `0x2492` |
| `0x40662900` FloraModels | 1 310 720 | 27 647 | `0x0000` × 620 579, `0x5555` × 293 124 |
| `0x406B2900` PaletteModels | 20 480 | 4 698 | `0x0000` × 9 890, `0x5555` × 995 |

The dominant non-black words — `0x5555`, `0xAAAA`, `0x2492`, `0x4924`, `0x00AA`,
`0xAA00`, `0x2482`, `0x0002`, `0x2482` — all have **repeating bit fields**
(`0x5555` is `0101` repeated, `0xAAAA` is `1010` repeated). That is not the
distribution a sampled albedo produces.

The same payload read at **16 bytes per block** (real BC3) tells the same story
from the other side. For `0x406B2900:0x0660EAB3` the first blocks are

```text
block  a0 a1   6-byte alpha index   colour word 0  colour word 1  4-byte colour index
  0    00 00   00 00 be f7 7d ef   be f7          de f7          ff ff ff 00
  1    00 00   00 00 df ff 9e f7   9e f7          df ff          ff ff ff 00
  2    00 00   00 00 9e f7 7d ef   7d ef          ff 3f          ff ff ff 00
  3    00 00   00 00 9e f7 7d ef   7d ef          b7 88          ff 00 48 82
```

Blocks 0–3 have `a0 = a1 = 0x00` and a zero 3-bit alpha index in every texel
slot — **fully transparent**, alpha 0. Their colour words are `0xF7BE`, `0xF79E`,
`0xEF7D`, `0x3FFF` and their colour indices are `0x00`, so every texel resolves
to endpoint 0: near-white (`0xF7BE` and `0xF79E` have all-ones top five bits).
Blocks 4–5 are the exceptions: `a0 = 0x24`, `a1 = 0x48`.

Read the other way — at the committed decoder's 8-bytes-per-block stride, where
bytes 8–11 of each group above become the two colour words — the words are
`0xF7DE`, `0xEF3F`, `0xF7BE`, `0x9EF7`, `0xB788`, … : again near-white or
transparent black, never a sampled skin tone.

**Under either stride the payload is a weight/mask, not an albedo.**

Stated precisely: the *decode* is `spore-texture`'s, including its documented
departure from BC3 (8 bytes per block, colour word read R5G5B5). The *texel*
claim — that these raster families carry no chroma — is that crate's
`SHADER_DATA_PAYLOADS` territory and is **not restated here as a new finding**.
What is new is the envelope census (§3.1), the reference graph (§3.3) and the
observation that both strides agree the payload is a mask.

`0x15` (901 records) is already established elsewhere as a tangent-space normal
map, Z first. That is consistent: a normal map and a mask are exactly the two
things a two-tone paint shader needs besides the colours.

### 3.3 The reference graph: which textures a model actually binds

Every `0x20D` texture set was read out of all 3 590 fully-decoding gmdl records
and the referenced groups histogrammed:

| referenced group | refs | referenced by |
|---|---|---|
| `0x40612900` / `0x40612901` | 44 / 44 | CellModels |
| `0x40632900` / `…01` / `…02` | 164 each | BuildingModels |
| `0x40642900` / `…01` / `…02` | 285 each | VehicleModels |
| `0x40652900` / `…01` / `…02` | 91 each | UfoModels |
| `0x40662900` / `…01` | 153 each | FloraModels |
| **`0x406B2900` / `0x406B2901`** | **30 / 30** | **PaletteModels only** |
| `0x00000000` | 1 | one record |

Two things follow.

1. **`0x40622900` / `0x40622901` — the 97 CreatureModels texture pairs — are
   referenced by nothing.** They are invisible to this census only because all
   97 creature gmdl that would reference them stop at `0x218` (§2.4).
2. **The 67 paint-pattern textures in `0x406B2900` are referenced by exactly 30
   gmdl records, every one of them in group `0x406B6200` (`PaletteModels`).**
   The 30 instances match 30 of the 67 texture instances one-for-one. So
   `0x406B2900` is the **editor paint pattern** — the *mask* the shader blends
   through — and it is not a creature's albedo. **PROVEN by reference, not by
   name.**

The six `paintid` values observed in the XML (`0xF5967F30` ×350, `0x7DA7E22C`
×14, `0x7DA7E228` ×10, `0x77EB6B21` ×8, `0x17B6A469` ×6, `0xB228FE0E` ×2)
match **no record instance id in any installed package**, so `paintid` does not
resolve by DBPF identity. See §6.

---

## 4. The confirmed source: `sporemodel` XML

### 4.1 How it was found

A single ASCII scan of every record of all five data packages, for the byte
patterns `color`, `colour`, `palette`, `tint`, `hue`, `saturation`, `skin`,
`paint`, `creature`, yielded **6 065 distinct (type id, string) pairs**. Sorting
them by type id put three families at the top that no committed decoder touches:

* **`0x0469A3F7` (`smt`)** — 10 908 hits for `color`, 1 384 for `colorIn`, 120
  for `DecalColorTint`. These are **shader sources in plain text**.
* **`0x24682294` (`vcl`)**, **`0x476A98C7` (`ufo`)**, **`0x2399BE55` (`bld`)**,
  **`0x2B978C46` (`crt`)**, **`0x3D97A8E4` (`cll`)** — `<skincolor1>`,
  `<color1>`, `<paintid>`, `<paintregion>`, `<paintlist count="N">`.
* **`0x497767B9`** — effect *cheatsheets*, also plain text (§5).

### 4.2 Which records, and how many

All 134 `sporemodel` records in `Spore_Game.package` (identical 134 in
`PatchData.package`):

| type id | name | records | group | `<name>` | colour fields | distinct triples |
|---|---|---|---|---|---|---|
| `0x2B978C46` | `crt` | 47 | `0x40626200` CreatureModels | `PaintBuddy` | `skincolor1..3` (141 elements) | 25 |
| `0x3D97A8E4` | `cll` | 13 | `0x40616200` CellModels | `PaintSwatch` | `skincolor1..3` (39 elements) | 14 |
| `0x24682294` | `vcl` | 4 | `0x40646200` VehicleModels | `VehiclePaintSwatch`, `genair`, `gencar`, `gensea` | `color1` ×140, `color2` ×140 | 7 |
| `0x476A98C7` | `ufo` | 2 | `0x40656200` UfoModels | `UFOPaintSwatch`, `genufo` | `color1` ×35, `color2` ×35 | 8 |
| `0x2399BE55` | `bld` | 1 | `0x40636200` BuildingModels | — | `color1` ×20, `color2` ×20 (40 elements) | 13 |

Structural facts, all measured over 134/134 records:

* 134/134 are **valid UTF-8**.
* 134/134 open with the byte-identical 39-byte
  `<?xml version="1.0" encoding="UTF-8"?><` and close with `</sporemodel>`.
* `<formatversion>` ∈ {**11**, **15**, **16**} (2 / 14 / 118 records).
* `<paintlist count="N">`: in **160/160** occurrences the attribute equals the
  number of `<paint>` children. Sizes 1 (×2), 2 (×104), 3 (×36), 4 (×18).
* `paintregion` and `paintid` are always `0x` + exactly 8 **lowercase** hex
  digits — 23 distinct region masks, 6 distinct paint ids.
* `<skincolor1..3>` appear **only** inside `<properties>` (120 records).
* `<paintlist>` appears **only** inside `<blockref>` (12 records).
* Colour component grammar, over all **1 710 colour components in one package**
  (570 colour elements × 3): **1 453 with six fractional digits, 251 bare
  integers (`0` or `1`), 6 with seven**. Every
  triple is exactly three components and every component is in `[0, 1]`.
  **No** component anywhere uses `e`/`E`, a sign, `inf`, `NaN`, leading or
  trailing space, or a bare `.`.

### 4.3 The colours themselves

Per element, over both packages:

Per package: **570 colour elements / 1 710 colour components.** `PatchData`
carries the same 134 records, so every count below doubles when both packages
are read.

| element | occurrences (both packages) | distinct values |
|---|---|---|
| `skincolor1` | 120 | **30** |
| `skincolor2` | 120 | **6** |
| `skincolor3` | 120 | **6** |
| `color1` (per paint) | 390 | **10** |
| `color2` (per paint) | 390 | **13** |
| **union** | **1 140 elements / 3 420 components** | **59** |

Channel spans over the 59 distinct triples: **R 0.000000 … 1.000000 (span
1.000000, 48 distinct values), G 0.000000 … 1.000000 (span 1.000000, 52), B
0.000000 … 1.000000 (span 1.000000, 49).**

**All three channels span the entire unit range.** This is the direct,
quantitative refutation of "the data has no colour in it".

The complete 59-triple table (this is the shipped palette):

```text
0,0,0                                0.384314,0.384314,0.384314
0.141176,0.576470,0.105882           0.419607,0.576470,0.576470
0.157000,0.168999,0.0859999          0.423529,0.462745,0.941176
0.243137,0.282353,0.254902           0.443136,0.223528,0.141176
0.282353,0.117647,0.0509799          0.443136,0.384314,0.262744
0.282353,0.278431,0.243137           0.446999,0.0939999,0.118000
0.286274,0.576470,0.262745           0.450979,0.450979,0.450979
0.317647,0.509804,0.694118           0.490000,0.280000,0.219999
0.329412,0.219608,0.156863           0.498039,0.803922,0.474510
0.341176,0.360784,0.576470           0.521568,0.419607,0.576470
0.368627,0.588234,0.803922           0.572548,0.572548,0.572548
0.368627,0.588235,0.803921           0.576470,0.294117,0.184313
0.384000,0.810999,0.850000           0.576470,0.694117,0.411764
0.384313,0.576470,0.105882           0.592156,0.800000,1
                                    0.650980,0.650980,0.650980
0.666666,0.666666,0.666666           0.666666,0.803921,0.474509
0.674510,0.235293,0.192157           0.682352,0.694118,0.600000
0.694117,0.352941,0.223529           0.694118,0.513726,0.129412
0.694118,0.513726,0.129412           0.701960,0.905882,0.411764
0.733332,0.411764,0.905882           0.749020,0.749020,0.749020
0.760783,0.760783,0.760783           0.769999,0.680000,0.579999
0.772548,0.772548,0.772548           0.796078,0.258823,0.356862
0.803921,0.733333,0.584313           0.803921,0.752941,0.149019
0.803922,0.329412,0.149020           0.803922,0.486275,0.368627
0.819607,0.658824,0.905882           0.850979,0.580392,0.239216
0.905882,0.639216,0.537254           0.905882,0.749020,0.411764
0.925490,0.925490,0.925490           0.937255,0.964706,0.925490
0.949019,0.905882,0.427451           0.949020,0.541176,0.301961
0.949020,0.905882,0.427451           0.980392,0.878431,0.717646
1,0.600000,0                         1,0.937255,0.690196
1,1,1
```

Note the greys that are *deliberate* (`0.450979,0.450979,0.450979` is the
dominant `color1`, `1,1,1` the dominant `skincolor2`/`skincolor3`) sitting
alongside fully saturated entries (`0.141176,0.576470,0.105882` is a green,
`0.733332,0.411764,0.905882` a violet). A dataset with no colour would not have
both.

Alongside the colours, each `crt`/`cll` record also carries
`<skineffect1..3>` (20 distinct ids, `OBSERVED`; `0x00000000` appears 24 times
per slot and is a **recorded zero**, not an absence) and `<skineffectseed1..3>`
(32 distinct integers).

---

## 5. The consumer: the game's own shader and effect source

This is what makes §0 a finding rather than a coincidence.

* `0x0469A3F7` (`smt`) records are **plain-text shader source**. The creature
  skin shader is named `setupEditorPaint`, and its body is:

  ```hlsl
  float specE        = editorColors.mParams.x;
  float specStrength = editorColors.mParams.y;
  float invBumpiness = editorColors.mParams.z;
  float gloss        = editorColors.mParams.w;
  float3 mixedColor;
  mixedColor = lerp(diffuseTexture.rgb, editorColors.mColor1.rgb, editorColors.mColor1.a);
  mixedColor = lerp(mixedColor,         editorColors.mColor2.rgb, editorColors.mColor2.a * diffuseTexture.a);
  Current.color.rgb = mixedColor * In.color.rgb;
  Current.color.a   = In.color.a;
  float4 specNMap    = tex2D(nmapSpecSampler, In.texcoord0);
  float3 nmapNormal  = specNMap.rgb * 2 - 0.9985;
  ...
  struct { float4 mColor1; float4 mColor2; float4 mParams; } editorColors;
  ```

  Read literally: the albedo is `lerp(lerp(diffuse, colour1, a1), colour2,
  a2·diffuseAlpha) × vertexColour`. The texture contributes a **weight**
  (`diffuseTexture.a`) and a multiply; the two colours are **constants**.
* A sibling shader `setupEditorPaintNMapSpec` uses
  `editorColors[1].rgb * In.color.rgb`, and `shShowDiffuse` is the same
  `lerp`/`lerp` pair — three independent shaders agreeing.
* `0x497767B9` records are the effect cheatsheets, and they **name the same two
  values in prose**:

  ```text
  SPSkinPaintClear -diffuse color1 -spec 0 -bump 0.5
  SPSkinPaintSettings -exponent 4 -phong 1 -gloss 0.5
  ```

  along with the paint-pattern families the editor offers —
  `skinpaint_base_amigurumi`, `skinpaint_base_melt`, `skinpaint_coat_crocodile`,
  `skinpaint_coat_platesSmall`, `skinpaint_coat_polkatdot_spots`,
  `skinpaint_coat_rubberband`, `skinpaint_coat_toupee`, `skinpaint_coat_washShiny`,
  `skinpaint_coat_lumpy`, `skinpaint_detail_goo`, … — each with a
  `…_brush`, a `…_lightener` and a `…_dist` (distribution) entry. **That is the
  paint catalogue, and it is shipped as text.**
* Creature-specific shading in the same blob: `shLighting_BakedCreaturesNormalMap`,
  `shLightingFloraLow`, `ApplyCel`, `kDefaultGloss = 0.35`, `kCelStrength = 0.3`,
  `kSH_A1 = 2/3`, `kSH_A2 = 1/4`, `kSH_A3 = 0`, `kSH_A4 = -1/24`.

**The colour names in the shader, the colour names in the effect cheatsheet and
the colour names in the XML are the same words.** That is the cross-check that
turns "there is a `<color1>` tag somewhere" into "this is the creature's colour".

---

## 6. Newly PROVEN / INFERRED / still UNKNOWN

### PROVEN (`OBSERVED`, from bytes)

* P1. `sporemodel` records exist in 5 DBPF types (`crt`, `cll`, `vcl`, `ufo`,
  `bld`), 134 per pair of packages, all valid UTF-8 with an identical
  declaration and a `</sporemodel>` terminator.
* P2. They carry 1 140 colour elements / 3 420 colour components in
  `skincolor1..3` / `color1` / `color2`, resolving to **59 distinct triples
  whose R, G and B each span the full `[0, 1]`**. Creature colour is in this
  data.
* P2b. **Every `<blockid>` reference resolves.** The 134 records name 565
  `<blockid>` elements over 8 distinct identities; 486 name group
  `0x40626000` and 79 name `0x40616000`, and **all 565 resolve to a record
  instance that exists** — zero dangling. Those base groups live in
  `Spore_Game.package` (`0x5CAA5E28`, 482 + 58 records) and
  `Spore_Graphics.package` (`0x00B1B104`, 503 + 58 records), **not** in
  `Spore_Content`'s `0x40 6X 62 00` asset groups. A `crt` record's part list
  therefore points one byte down the group id from the asset group to the base
  group, and across two packages.
* P3. `<paintlist count="N">`'s attribute equals its child count in 160/160
  occurrences; `paintregion`/`paintid` are always `0x` + 8 lowercase hex.
* P4. The creature skin shader `setupEditorPaint` is shipped as readable source
  and takes its two colours from a **constant** struct `editorColors`, using the
  sampled texture only as a `lerp` weight and multiply.
* P5. The effect cheatsheets name the two values `color1` and `color2` in prose
  (`SPSkinPaintClear -diffuse color1`).
* P6. Type `0xEE17C6AD` is an `ANIM` container carrying its source
  `.animation` path. The canonical label `animation` is correct.
* P7. The `0x406B6A00` `Palettes` group holds 44 `prop` records with 28 distinct
  **names**, all editor UI registries. No RGB value.
* P8. The 67 `0x406B2900` paint-pattern textures are referenced by exactly 30
  gmdl records, **all** in group `0x406B6200` (`PaletteModels`). They are the
  editor's paint pattern, not a creature's albedo.
* P9. All 619 gmdl records that stop at `0x218` are the creature-model groups
  plus 37 `PaletteModels` records; `0x218`'s payload holds `f32` bone matrices
  and baked-deform samples and is byte-identical across a model's HQ/non-HQ pair.
* P10. The raster envelope census (§3.1) — 21 group/fourcc combinations, and
  `0x406B2900`/`0x406B2901` are the only 64×64/32×32 pair and the only DXT5
  records the committed decoder refuses (exact refusal quoted).

### INFERRED

* I1. `skincolor*` / `color1` / `color2` populate `editorColors.mColor1` /
  `mColor2`. **Graded `Inferred`** in `color.rs::claims::SKIN_COLOR_TO_SHADER`:
  the shader names, the cheatsheet names and the XML names agree, but no record
  or shader in these packages states the assignment.
* I2. `paintregion` is a bitmask of body regions. 23 distinct values, all
  low-population (`0x1`…`0x32`), consistent with a mask and nothing more.
* I3. `skineffect1..3` + `skineffectseed1..3` are procedural detail effects with
  noise seeds, consistent with the shader blob's `mNoise`/fBm helpers. The
  blob does not reference the ids, so this stays an inference.

### UNKNOWN — and recorded as `Fact::unavailable`, never as a zero

* U1. **`color_alpha_source`.** The record stores **three** components; the
  shader blends with `mColor1.a` / `mColor2.a`. Nothing in the installed
  packages says where the fourth comes from. `UnitRgb` has no field to hide it.
* U2. **`paint_region_bits`.** No table in the corpus maps a bit to a region.
* U3. **`paint_id_resolution`.** Six ids; none is a record instance id in any
  package. How a `paintid` becomes a texture or effect set is not determined.
* U4. **`skin_effect_seed_meaning`.** 32 distinct integers; no consumer named.
* U5. **`user_creature_color_location`.** Every `sporemodel` record in the
  install is a shipped preset (`PaintSwatch`, `PaintBuddy`, `gencar`, `genair`,
  `gensea`, `genufo`). A player's *own* creature colour is not in these packages;
  where it is persisted is outside them.
* U6. **`0x218`'s on-disk size**, which blocks the creature gmdl texture-set
  read (§2.4). A `spore-gmdl` question.
* U7. **`0x406B2900`'s true framing** — 1×16-byte DXT5 chain, 2×8-byte layers,
  or otherwise (§3.1). A `spore-texture` question.
* U8. **RW4 section payloads.** Not decoded here; recorded as a gap, not as a
  negative (§2.5).

---

## 7. What the renderer should do with this

Not implemented here — `spore-material` and `spore-engine` are other agents'
crates — but the measurement says exactly what a correct implementation needs:

1. Load the model's `sporemodel` record (`crt` / `cll` / …) by the **same
   instance id as the geometry group**, not by a texture reference. The parts
   are named `<blockid>0x40626000, 0xD05C53A3</blockid>` — group `0x40626000`,
   instance `0xD05C53A3`, which is a real record in `Spore_Game.package`
   (`0x5CAA5E28`). Note the group byte: the asset group is `0x40626200` and the
   base group is `0x40626000` — **one byte down**, and in a **different
   package**. All 565 `<blockid>` references across the 134 records resolve
   (P2b), so this is a measured mapping, not a guess. It is worth a look by
   whoever owns `spore-assets::model`.
2. Take `skincolor1..3` and each `<paint>`'s `color1`/`color2` as `f32` triples
   in `[0, 1]` — **do not** normalise them, clamp them, or round them to bytes
   before the shader sees them.
3. Blend exactly as `setupEditorPaint` does. A renderer that multiplies the
   texture in and stops will be uniformly grey for the same reason.
4. Do **not** invent the alpha weights (U1) and do **not** resolve `paintid`
   by DBPF identity (U3).

---

## 8. Reproducing the measurements

Every number above came from throwaway probes run against `SPORE/Data/` through
`tools/spore/dbpf/dbpf.py`, the committed Python DBPF oracle. The probes were
deleted before this commit; nothing in this document requires re-running them to
be believed, and no Spore byte is committed. The durable artefact is
`crates/spore-assets/src/color.rs`, whose `#[ignore]`d corpus test
(`every_sporemodel_record_decodes_and_every_colour_is_in_range`) re-measures
P2 on any install and skips cleanly when `SPORE/` is absent:

```bash
cargo test -p spore-assets
cargo test -p spore-assets -- --ignored          # needs SPORE/
cargo run -q -p spore-tools -- describe SPORE/Data/Spore_Content.package \
    0x2b978c46:0x40626200:0x145ff205
```

`osptool describe` on a `sporemodel` record currently prints only the index
extent and says the payload is not decoded — `color::extract` is the decoder for
it, and wiring it into `spore-tools` is `spore-tools`' owner's call.
