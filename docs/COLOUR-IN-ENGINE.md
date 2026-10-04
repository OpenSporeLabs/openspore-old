# COLOUR-IN-ENGINE — a Spore creature's colour, end to end

**Agent:** COLOUR · **date:** 2026-10-04 · **build:** 3.1.0.22 (GOG, `SPORE/`)
**code:** `crates/spore-engine/src/domain/paint.rs` (+ `texture.rs`, `loader.rs`, `cli.rs`)
**input measurement:** `docs/COLOUR-SOURCES.md`

---

## 0. What this session changed, in one picture

| | before | after |
|---|---|---|
| base colour of a Spore model | `texture::UNTEXURED_TINT`, a neutral grey | the model's `sporemodel` record, `skincolor1` / a paint's `color1` |
| where the value came from | nowhere — a stated refusal | `0x2b978c46:0x40626200:0x05e3df13`, `<skincolor1>0.498039,0.803922,0.474510` |
| the colour's grade | — | **OBSERVED** (read verbatim out of record bytes) |
| the link to *this* model | — | **INFERRED** (no byte in the corpus establishes it — §2) |
| when no record is found | the same grey | a *different* thing: `state=unavailable reason=...`, graded `UNKNOWN` |
| `paintregion` | — | **structurally absent** for a creature: 0 of 47 `crt` and 0 of 13 `cll` records hold a `<paint>` |
| raster records in the colour path | — | **none.** `base_color_texture` is still `None`, always |

```bash
# grey: the colour records were never opened
cargo run -q -p spore-engine -- --preset creature \
    --package SPORE/Data/Spore_Content.package --texture

# green: the same record, coloured from its `sporemodel`
cargo run -q -p spore-engine -- --preset creature \
    --package SPORE/Data/Spore_Content.package \
    --sporemodel-package SPORE/Data/Spore_Game.package --texture

# any other record's palette, chosen by hand
cargo run -q -p spore-engine -- --preset creature \
    --package SPORE/Data/Spore_Content.package \
    --sporemodel-package SPORE/Data/Spore_Game.package \
    --sporemodel 0x2b978c46:0x40626200:0x135ff09f --texture   # #cdc026, a yellow
```

`--info` on any of these prints one `OPENSPORE-PAINT v1` line per record.

---

## 1. The pipeline, in the order it runs

```text
  Spore_Game.package            (a DIFFERENT package from the geometry)
    0x2b978c46:0x40626200:0x05e3df13   `crt` sporemodel, UTF-8 XML
      |
      | spore_dbpf::extract_record          (no Bevy)
      v
  spore_assets::color::extract             (no Bevy — another agent's crate)
    ModelColors { skin_colors: [Option<UnitRgb>; 3], paint_lists: Vec<PaintList>, .. }
      |
      | domain::paint::resolve              (no Bevy, THIS crate)
      v
  PaintBinding::Bound(ModelPaint)  |  PaintBinding::Unavailable { reason, fact }
      |
      | ModelDefinition.paint  ->  ModelDefinition::mesh_paint(mesh_index)
      v
  MeshPaint { base_color: Option<[f32;3]>, source, association, regions }
      |
      | loader::spawn_scene -> texture::standard_material(surfaces, normal, base)
      v
  StandardMaterial { base_color: Color::linear_rgb(..), base_color_texture: None }
```

Everything above the last arrow is Bevy-free and assertable in
`cargo test -p spore-engine` with no window, no GPU and no game install. That is
the honest headless test — `DefaultPlugins` builds a winit `EventLoop` and
panics off the main thread, so "the app assembles" is not a claim this repo
makes.

**The colours are never normalised, clamped or rounded to bytes on the way to
the renderer.** `UnitRgb` carries `f32`, `base_color` is `Color::linear_rgb` —
the shader's `editorColors.mColor1` is a `float4` constant and is therefore
already linear — and the `#rrggbb` on the report line is a display convenience
produced from a *copy*.

---

## 2. How a model is linked to its `sporemodel`, and the grade of that link

### 2.1 What is PROVEN (measured over the 3.1.0.22 install)

* **P1 — the group-family relation.** All **97** `gmdl` instance ids in
  `CreatureModelsHQ` (`0x40627100`) are also `CreatureModels` (`0x40626200`)
  instance ids — a 97-for-97 set equality, and the two groups hold the same
  number of records. The committed name table
  (`spore_core::record::GROUP_NAMES`, transcribed from
  `tools/spore/types/groupnames.json`) spells the relation: `0x40627100` is
  `CreatureModelsHQ` and `0x40626200` is `CreatureModels`.
  `paint::asset_family` turns it into a comparison two group ids can be tested
  on, by stripping the one documented variant suffix (`HQ`) and refusing to
  strip a suffix whose stem is not itself a canonical group name.
* **P2 — the model-type hash.** **All 46** `crt` records that write
  `<modeltype>` write `0x9ea3031a` (and the 47th, `formatversion 11`, writes the
  same value in decimal: `2661483290`). That hash appears verbatim as a
  little-endian `u32` word in **exactly one** `prop` record of group
  `0x40626000` — instance `0xd05c53a3` — which is precisely the block every one
  of those records names in `<blockid>`. Two independent routes
  (`<modeltype>` and `<blockid>`) reach the same record.
* **P3 — the blocks resolve.** All 565 `<blockid>` references across the 134
  `sporemodel` records resolve to a record that exists
  (`docs/COLOUR-SOURCES.md` §P2b). Zero dangling.

### 2.2 What is REFUTED — and this is the load-bearing negative

**There is no byte-level `model → sporemodel` identity in this corpus.**
Measured:

* **0 of 97** creature `gmdl` records in `0x40627100` contain the four bytes of
  any block instance their family's `sporemodel` names (`0xd05c53a3`).
* **0 of 4 209** `gmdl` records in `Spore_Content.package` contain the
  `modeltype` hash `0x9ea3031a`.
* Scanning all 1 465 `prop` records of the block base groups
  (`0x4062/61/63/64/65/66 6000`) in `Spore_Graphics.package` for any word equal
  to a `Spore_Content` instance id returns references into `0x41606100`,
  `0x406b6200`, `0x406b2900` and `0x406b2901` — and **nothing at all** into
  `0x40627100` or `0x40626200`. A block never names the geometry that renders it.

So the association the renderer makes is an **argument, not an observation**, and
`PaintBinding` says so on every path:

* the **colours** are `EvidenceLevel::Observed`
  (`paint::contents_provenance()`), read verbatim out of record bytes;
* the **association** is `EvidenceLevel::Inferred`
  (`paint::association_grade()`), on both `PaintOrigin::Explicit` and
  `PaintOrigin::FamilyDefault`.

They are never merged. A test asserts the two grades are different.

### 2.3 What the resolver actually does

```text
explicit?  --sporemodel T:G:I --> read that record whatever its group.  origin=explicit
otherwise:
  family = asset_family(model.group_id)          None -> unavailable(model_group_unnamed)
  candidates = every `crt`/`cll`/`vcl`/`ufo`/`bld` identity whose group has that family,
               sorted ascending by (type, group, instance)
  none?      -> unavailable(no_family_match | no_sporemodel_records)
  first that decodes AND carries a colour -> PaintBinding::Bound
  all refused -> unavailable(decode_refused)
```

Two decisions worth naming:

* **The choice among a family is deterministic from the data, not from file
  layout.** 47 creature presets share one family and they disagree, so
  `ContentStore::identities_of_type`'s package-then-index order is re-sorted by
  identity before the first is taken. An arbitrary choice has to at least be
  reproducible, and the winner is `0x05e3df13` — the **lowest** `crt` identity in
  the family. `candidates=47` is on the report line so "one of 47" is visible.
* **`asset_family` strips exactly one suffix and only when the stem is a real
  group name.** `CreatureModelsHQ → CreatureModels` merges. `CellModels` /
  `CellImages` and `Palettes` / `PaletteModels` do **not**: they share a first
  word or a stem, and merging either would put a cell's colour on a texture
  record. Three unit tests pin both directions.

---

## 3. The shader formula: verified, approximated, refused

The game's own source, `smt` `0x0469A3F7`, function `setupEditorPaint`:

```hlsl
mixedColor = lerp(diffuseTexture.rgb, editorColors.mColor1.rgb, editorColors.mColor1.a);
mixedColor = lerp(mixedColor,         editorColors.mColor2.rgb, editorColors.mColor2.a * diffuseTexture.a);
Current.color.rgb = mixedColor * In.color.rgb;
Current.color.a   = In.color.a;
```

What this build does with each term:

| term | status | why |
|---|---|---|
| `editorColors.mColor1.rgb` / `mColor2.rgb` | **INFERRED** — and applied | `spore_assets::color::claims::SKIN_COLOR_TO_SHADER`. The shader names, the effect cheatsheet's `SPSkinPaintClear -diffuse color1` (`0x497767B9`) and the XML's `<color1>` are the same words; no record states the assignment. |
| the numeric values | **VERIFIED by bytes** | `color::extract` reads them; `tests/real_corpus.rs` asserts `skincolor1` of `0x05e3df13` is exactly `0.498039,0.803922,0.474510`. |
| `editorColors.mColor1.a` / `mColor2.a` | **UNKNOWN — refused** | `claims::COLOR_ALPHA_SOURCE`: the record stores **three** components and nothing in the corpus says where the fourth comes from. Inventing a weight (0.5, "fully opaque", …) would be inventing a field. |
| `diffuseTexture` as the `lerp` weight | **UNKNOWN — refused** | `spore_material::claims::MASK_LAYER_PURPOSE`: the `DXT5` families' four alpha planes have no established per-plane purpose, so no plane is treated as a mask weight. The planes stay uploaded and unaddressed-by-guess (`MaskChannels`), exactly as before. |
| `In.color.rgb` (vertex colour) | **not available** | `spore_gmdl` yields positions, normals and uvs; no vertex-colour channel is decoded. |
| `Current.color.a` | **not reproduced** | alpha blending would be a claim about mask planes that is not established; the material stays `AlphaMode::Opaque`. |

**The reduced form actually applied is one constant linear colour per model.**
That is the whole of `mColor1.rgb` with no invented weight, and it is a stated
approximation of the original graph — not a reproduction of it.

Note also that the *raster path is untouched*: `base_color_texture` stays `None`
even when `base_color` carries a real colour. The value came out of an XML
record, not a texel, and `texture::tests::no_surface_ever_goes_into_a_base_colour_slot`
still asserts it.

---

## 4. `paintregion`: absent for a creature, unmapped elsewhere

The brief's hard question — what does a `paintregion` bit select? — turned out
to have a better answer than an unknown, and it was measured:

| type | records | `<paint>` elements | `<paintregion>` masks |
|---|---|---|---|
| `0x2b978c46` `crt` (creature) | 47 | **0** | — |
| `0x3d97a8e4` `cll` (cell) | 13 | **0** | — |
| `0x24682294` `vcl` (vehicle) | 4 | 140 | 21 distinct |
| `0x476a98c7` `ufo` | 2 | 35 | 8 distinct |
| `0x2399be55` `bld` (building) | 1 | 20 | 7 distinct |

(`PatchData.package` carries the same 134 records, so every number doubles when
both packages are read.)

So for a creature the region mask **does not exist**. There is no bit to decode
and inventing one would be inventing a field the record does not have.
`RegionMapping` is typed so the two situations cannot be confused:

* **`AbsentFromRecord { elements }`** — measured `0` for every creature and cell
  record. The field is absent, not unread, and the engine says so on the report
  line as `regions=absent paint_elements=0`. This is an **observation**, not a
  non-finding.
* **`Unmapped { elements, masks }`** — the vehicle/ufo/building shape. Every
  mask is carried verbatim in document order and **no bit is decoded**
  (`RegionMapping::is_decoded()` is `const`-`false` and asserted as such). The
  first paint's `color1` is applied to the whole model — the brief's option (c),
  and it yields a coloured model with an explicit `UNKNOWN` instead of a fake
  bit decoder. The report line says `regions=unmapped … bits=UNKNOWN`.

The whole-model application is graded **APPROXIMATION** in substance and
`EvidenceLevel::Inferred` in the machine-readable line: for a creature, if
`skincolor2`/`skincolor3` are regions, then `skincolor1` alone on the whole body
is a simplification. What *is* known is that those two slots are read and
reported — `skin_slots=3 unapplied_colours=1` on the line — and that the weight
which would select them is UNKNOWN, so this build declines to invent one.

---

## 5. Unavailable is never grey, and never black

```text
OPENSPORE-PAINT v1 record=0/1 key=0x00e6bce5:0x40627100:0x067c79d2 state=unavailable \
  reason=no_sporemodel_records link=UNKNOWN base=absent
```

`PaintBinding::Unavailable` carries a **closed** four-variant
`PaintUnavailableReason` and a `Fact::unavailable` (`UNKNOWN` / `MISSING`, no
provenance — a non-finding has no source to repeat). The renderer substitutes
`texture::UNTEXURED_TINT` so the normal map stays readable, and the log says
`base_color=absent from=absent link=UNKNOWN`.

`None` and `Some([0,0,0])` are different facts and the type keeps them apart:

* `texture::tests::an_absent_record_colour_falls_back_to_the_neutral_tint_and_not_to_black`
* `texture::tests::a_zero_record_colour_is_still_black_because_black_is_a_colour_a_record_can_carry`
  — `0,0,0` appears verbatim in the corpus's paint lists, so black is a colour a
  record *can* carry and must not be mistaken for "nothing found".

---

## 6. Where the choice is visible at runtime

* **`OPENSPORE-PAINT v1`**, one per record, next to the existing `STAGED v1` and
  `MATERIAL v1` lines. Both variants are in
  `paint::tests::the_bound_report_line_carries_every_colour_it_chose_between`
  and `…::an_unavailable_binding_grades_unknown_and_names_its_reason`, so a
  change to either fails a test rather than quietly reformatting a line.
* **`OPENSPORE-SCENE v1`** gains one additive field, `models_painted=`. The
  `STAGED v1` field set is unchanged and pinned by
  `loader::tests::the_staged_line_keeps_the_v1_field_set`.
* **`MeshPaintMarker`** on every leaf entity, beside `MaskChannels`, holding the
  `MeshPaint` verbatim — so "why is this mesh this colour" is answerable from
  the entity, with its grade and its `paintregion` disposition.
* **`--sporemodel-package`**, repeatable, appended **after** `--package` at the
  lowest resolution priority: a package named only for colour must never shadow
  a record the caller asked for. `Spore_Content.package` holds **no**
  `sporemodel` record; `Spore_Game.package` and `PatchData.package` hold all of
  them.
* **`--sporemodel T:G:I`**, once per scene (a second value is refused rather
  than won by flag order).

---

## 7. Verification actually run (2026-10-04)

```text
cargo fmt --all --check                                   clean
cargo test -p spore-engine                               142 + 4 + 5 passed, 0 failed
cargo test -p spore-engine --release -- --ignored          6 passed (real corpus)
cargo clippy -p spore-engine --all-targets -- -D warnings  clean
```

Three of the six real-corpus tests are the colour path and they assert, against
the install:

* the creature's family default resolves to
  `0x2b978c46:0x40626200:0x05e3df13`, `origin=family-default`,
  `association=INFERRED`, `candidates=47`, `base=#7fcd79`,
  `base_rgb=0.498039,0.803922,0.474510`, `regions=absent`;
* without the colour package the same model is `state=unavailable
  reason=no_sporemodel_records`;
* all **60** `crt`+`cll` records decode, carry **0** `<paint>` elements
  between them and **38** distinct colours whose per-channel ranges are red
  `0.141176..1.0`, green `0.223528..1.0`, blue `0.0..1.0`.

### The screenshots

`spectacle -b -n -o <path>` on a real X/Wayland display, same command, same
geometry, three package/flag combinations:

| run | what the frame shows | sampled pixels |
|---|---|---|
| no `--sporemodel-package` | the creature **uniform grey** | body core mean `(81,83,91)`, spike mass `(86,88,96)`; `G − R ≤ 2` everywhere |
| default family record | the creature **pale green** `#7fcd79` | body core `(177,199,173)`, bright limb `(169,190,166)`; `G − R ≈ +23…+30` |
| `--sporemodel …:0x135ff09f` | the creature **yellow** `#cdc026` | visibly and unambiguously yellow |

98 203 pixels in the creature's bounding box are green-dominant against 21 454
neutral ones, versus **0** green-dominant pixels in the grey run's box. The
screenshots are **not committed** — they are renderings of Spore geometry, and
no Spore asset byte belongs in this repository.

---

## 8. Still UNKNOWN after this session

Nothing below was upgraded for convenience; each is a typed `Fact::unavailable`
or a closed `reason`.

| # | question | where it is recorded |
|---|---|---|
| U1 | which model a given `sporemodel` record colours, at byte level | `PaintBinding` grades the link `INFERRED`; §2.2 lists the three negative measurements |
| U2 | the fourth colour component (`mColor1.a` / `mColor2.a`) | `spore_assets::color::claims::COLOR_ALPHA_SOURCE`; named on every report line as `alpha_source=UNKNOWN` |
| U3 | what one `paintregion` bit selects | `claims::PAINT_REGION_BITS`; `RegionMapping::is_decoded()` is `const`-`false` |
| U4 | how `paintid` resolves | `claims::PAINT_ID_RESOLUTION`; the id is carried and never looked up |
| U5 | the purpose of any of the four mask planes | `spore_material::claims::MASK_LAYER_PURPOSE`; `mask_purpose=UNKNOWN` on every report line |
| U6 | where a *player's own* creature colour lives | `claims::USER_CREATURE_COLOR_LOCATION`; every `sporemodel` in the install is a shipped preset |
| U7 | whether `skincolor2`/`skincolor3` are regions or pattern tints | not stated anywhere; `unapplied_colours=1` on the line is the honest accounting |
| U8 | the two creatures that share a family and disagree | `--sporemodel` exists for that, and `candidates=47` is printed |

### Also recorded, not caused by this work

`crates/spore-material` gained `BindingScope::Undetermined { collected,
contradicted }` concurrently. The creature record's material-info walk stops at
the undocumented shader-data id `0x218`, so its two collected references divide
evenly over two meshes and are therefore wrong; `spore-material` refuses the
cut. `crates/spore-engine/src/domain/materials.rs` handles it as
`applies_whole_set()` does — the same flat-index arithmetic as `WholeModel`,
with `stated_ordinal` returning `None` so the refusal cannot manufacture an
ordinal conflict — and `tests/real_corpus.rs` was rewritten to assert the new,
measured truth: `Undetermined { collected: 2, contradicted: 1 }`, both meshes
carrying both references, `ordinal_conflicts == 0` on each.