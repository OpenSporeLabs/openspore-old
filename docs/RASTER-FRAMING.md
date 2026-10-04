# Raster framing — how a Spore `raster` record actually frames itself

**Status:** the framing is `VERIFIED` over **4386 of 4386** raster records measured
(2954 in `Spore_Content.package`, 1432 in `Spore_EP1_Content_01.package`).
The block codec inside those bytes is `INFERRED`. The codec the repository
previously used is `REFUTED`, with the byte count that refutes it below.

This document supersedes §1 of `docs/MATERIALS-DESIGN.md` and the layer-split
rule in `tools/spore/raster/raster.py`. It does not delete them: §1's
disagreements are listed at the end, by section, so a reader who arrives from
either can see exactly which sentences no longer hold and why.

---

## 1. The measurement that started it

373 of the 2954 raster records in `Spore_Content.package` had a perfectly
readable envelope and were refused with *"payload bytes is not a multiple of the
derived DXT5 layer stride"*:

| shape | records | payload bytes | derived stride | remainder |
|---|---|---|---|---|
| 256², 9 levels, `DXT5` | 306 | 87 436 | 43 720 | **−4** from 2 layers |
| 64², 7 levels, `DXT5` | 67 | 5 508 | 2 760 | **−12** from 2 layers |
| 512², 10 levels, `DXT5` | 1374 | 349 584 | 174 792 | exactly 0 |
| 256², 9 levels, `DXT1` | 306 | 43 732 | — | never divided at all |
| 512², 10 levels, `0x15` | 540 | 1 398 136 | — | consumed exactly |
| 256², 9 levels, `0x15` | 294 | 349 556 | — | consumed exactly |
| 32², 6 levels, `0x15` | 67 | 5 480 | — | consumed exactly |

The remainder was 0 for one shape and non-zero for two, and the "fix" was always
the same arithmetic: subtract a whole number of layer strides. That is the shape
of a rule that is wrong rather than incomplete.

The clue that broke it open was in the header the rule was ignoring. `field_18`
read `0x40000` on a 512² record and `0x10000` on a 256² record — exactly **twice**
`w * h`, and exactly **16 bytes per 4x4 block** for a base level that is
`w * h / 16` blocks. Under the 8-byte block size the crate charged, `field_18`
read as "two base levels", and the crate duly built a two-layer record.

## 2. The framing

```text
off        field
0x00       version    u32, always 1
0x04       width      u32
0x08       height     u32
0x0c       mip_count  u32
0x10       field_10   u32, always 8            (UNRESOLVED)
0x14       fourcc     u32
0x18       field_18   u32 == base level's byte count   (see §4)
header     level 0, NO length prefix
           level 1, no prefix        (block formats only)
           level m >= first_prefixed: u32 LE == level m's byte count, then the bytes
```

and the record's length is **fully determined**:

```text
record_size == header + level0_bytes
                       + SUM over m in first_prefixed..mip_count-1 of (4 + level_m_bytes)
```

with **nothing left over and nothing unconsumed**. This is an identity, not a
division: there is no remainder to round and no layer count to derive.

### Per-format table

| `fourcc` | header | bytes per 4x4 block | first prefixed level | records measured |
|---|---|---|---|---|
| `0x35545844` `DXT5` | 32 | **16** | 2 | 1 747 + 731 = 2 478 |
| `0x31545844` `DXT1` | 32 | **8** | 2 | 306 + 328 = 634 |
| `0x00000015` | **28** | 4 bytes per *texel* | **1** | 901 + 373 = 1 274 |

(`Spore_Content.package` + `Spore_EP1_Content_01.package`.)

Three of those cells are load-bearing and one is a plain fact:

* **16 bytes per block for `DXT5`.** A 256² base level occupies **65 536** bytes
  in the record, which is 4096 blocks of 16. At 8 bytes it would be 32 768. Every
  recorded level prefix is the 16-byte figure, in **2478 of 2478** records across
  the two packages. This is forced, not chosen.
* **8 bytes per block for `DXT1`.** The same argument: a 256² base level occupies
  **32 768** bytes = 4096 blocks of 8, and every prefix says so.
* **The `0x15` family's header is 28 bytes** and its prefix starts at **level 1**.
  That was already established in `docs/LUMINANCE-RASTER.md`; it is in the shared
  table now rather than in a rule of its own.
* **The block formats prefix from level 2, not level 1.** The bytes where a
  level-1 prefix would sit hold block data: on record
  `0x40662c00:0x824928c8` the `u32` at offset 32 800 is 1 939 370 904, not the
  8 192 a level-1 prefix would carry. *Why* the two families differ is
  **UNKNOWN** — `claims::FIRST_PREFIXED_LEVEL`.

### No layers

Every raster record is **one** chain. `field_18` is not a layer count: it is the
base level's own byte count, in all three formats, which is why it read as "two
base levels" only while the block size was wrong.

## 3. How the framing was found, and how sure it is

Three independent measurements, in increasing order of how much they constrain:

1. **Byte counts.** Each of the seven shapes has exactly one record length, and
   each is reproduced by the identity in §2. Verified over every record of every
   shape: 1747/1747, 306/306, 901/901 in `Spore_Content.package` and the same
   over `Spore_EP1_Content_01.package`. A wrong block size has no freedom left:
   it would have to be wrong in the same way in 1747 separate files.

2. **The prefix values themselves.** These are the strongest evidence, because
   they are the record *stating* its own level lengths at specific offsets. For
   `0x40662900` (512², 10 levels) the `u32`s at 327 712, 344 100, 348 200,
   349 228, 349 488, 349 556, 349 576 and 349 596 are 16 384, 4 096, 1 024, 256,
   64, 16, 16 and 16 — each exactly the level's size at 16 bytes per block, and
   each **constant across all 1374 records of that shape** (which is what
   distinguishes a length from block data). For `0x40662c00` (256² `DXT1`) the
   `u32`s at 40 992, 43 044, 43 560, 43 692, 43 728, 43 740 and 43 752 are
   2 048, 512, 128, 32, 8, 8 and 8, matching all 306 records.

3. **Decoded-image coherence.** Horizontal total variation of the decoded alpha,
   mean over 25 sampled records per shape:

   | shape | BC3 (16-byte block) | the old 8-byte rule |
   |---|---|---|
   | `DXT5` 512² | **1.95** | 60.49 |
   | `DXT5` 256² | **8.72** | 106.06 |
   | `DXT5` 64² | **2.20** | 50.25 |

   A factor of 20 to 50 on every shape, in the direction of "the right bytes are
   being read as the right structure". This is corroboration, not proof: a
   misaligned read of low-contrast data can score well on its own, which is why
   the framing rests on (1) and (2).

**What is deliberately *not* claimed.** The decoded mip chains do **not**
reproduce each other: the correlation between level 0 and a box-downsample of
level 0, measured over the same samples, is around 0.0–0.5 under every reading
tried, while the `0x15` family — whose framing and codec are both established —
scores 0.99. Whatever Spore put in those levels, it is not "level 0 blurred by
half". That is an open question about content, not about framing, and it is why
the codec is graded `Inferred` and not `Verified`.

## 4. `field_18` is the base level's byte count

Verified in 4386 of 4386 records, in all three formats:

| `fourcc` / shape | `field_18` | base level bytes | equals? |
|---|---|---|---|
| `DXT5` 512² | `0x40000` = 262 144 | 128·128·16 = 262 144 | yes |
| `DXT5` 256² | `0x10000` = 65 536 | 64·64·16 = 65 536 | yes |
| `DXT5` 64² | `0x1000` = 4 096 | 16·16·16 = 4 096 | yes |
| `DXT1` 256² | `0x8000` = 32 768 | 64·64·8 = 32 768 | yes |
| `0x15` 512² | `0x100000` = 1 048 576 | 512·512·4 = 1 048 576 | yes |
| `0x15` 256² | `0x40000` = 262 144 | 256·256·4 = 262 144 | yes |
| `0x15` 32² | `0x1000` = 4 096 | 32·32·4 = 4 096 | yes |

So `field_18` is **not** format-specific, and the old claim that it is was an
artefact of the wrong block size. It is redundant with the geometry, which means
it can be used as a **cross-check**: the walk still places every level from the
geometry and *refuses* a record whose `field_18` disagrees, rather than trusting
either side. That is the check the crate's old "derived, never read" rule wanted
and could not have — you cannot cross-check a word nobody understands.

## 5. The block codec

### 5.1 What changed

The crate previously decoded **8-byte "Spore blocks"**: one 2-bit texel index
driving both alpha and colour, an *inverted* palette-size test, R5G5B5 channel
reads, a 6/2 blend on the endpoint codes, and no `alpha0 == alpha1` collapse.
Four documented departures from BC3, each reproduced on purpose from
`tools/spore/dxt5/dxt5.py` and `src/assets/Dxt5.cpp`, and each pinned by
`tests/test_textures.py::TestDxt5Synthetic`.

None of it survives the byte count. The records hold **standard BC3**: an 8-byte
BC4 alpha half (`alpha0`, `alpha1`, sixteen 3-bit indices) followed by an 8-byte
BC1 colour half (two `R5G6B5` words, sixteen 2-bit indices). The structural
difference from the old rule is that **alpha and colour have separate index
streams**.

### 5.2 "These records carry no colour" was a tautology

The crate recorded — as a measured fact over 477 771 palette entries — that
green span 0 and blue span 0, so the records were "alpha/mask surfaces". Both
numbers are artefacts of R5G5B5 applied to a **6-bit** value: a 6-bit word cannot
set bits 10..19, so green and blue read as 0 *by construction*, on any input,
including random noise.

Read as BC3 with the published `R5G6B5`, the same records carry **32 distinct
greens and 32 distinct blues** — the full 6-bit green range and the full 5-bit
red/blue ranges. The claim measured the codec, not the data.

### 5.3 Grading

| subject | grade | why |
|---|---|---|
| framing (§2) | `Verified` | exact identity, 4386/4386 records |
| `field_18` (§4) | `Verified` | 4386/4386 records, all three formats |
| BC3 codec | **`Inferred`** | published reading of a measured stride; no real decode compared against the game's own output |
| BC1 codec | **`Inferred`** | same, and no measurement even isolates it |
| BC4 ramp rounding | `Unknown` | truncation vs rounding is a real 1-LSB risk and the corpus does not decide it |
| BC4 six-level indices 6, 7 | `Unknown` | the specification leaves them undefined; this decoder clamps to `alpha0` |
| BC1 punch-through entries 4, 5 | `Unknown` | **unreachable**: a BC1 colour half carries a 2-bit index |
| why level 1 is bare in the block formats | `Unknown` | measured; cause not established |

`docs/` must not read "BC3 is verified". The distinction is load-bearing: the
**block stride** is measured and the **codec inside a block** is the standard
reading that stride makes the only candidate, and the test that would settle it —
decoding a real record and comparing against what the game draws — has not been
run.

## 6. Census after this change

```text
$ osptool stats SPORE/Data/Spore_Content.package
raster fourcc
       1747  0x35545844 DXT5
        901  0x00000015 0x15 normal-map family
        306  0x31545844 DXT1
raster layer splits
       2954  derived
decodes
       2954  raster decoded
```

**2954 of 2954 raster records decode. There is no raster refusal left in this
package** — `spore_texture::TextureError` is unreachable from
`decode_raster` over the whole measured corpus.

Two labels elsewhere in the tree are now stale and were left alone on purpose,
because they are not mine to edit:

* `crates/spore-tools/src/commands/stats.rs::fourcc_label` still spells DXT1
  *"present in the corpus, refused by name"*.
* `crates/spore-differential/tests/luminance_corpus.rs::no_raster_record_decodes_under_a_format_it_does_not_declare`
  still asserts `by_fourcc[&0x3154_5844].1 == 0` and, in the DXT5 arm,
  `dxt5_ok > 0 && dxt5_ok < dxt5_total` — *"a corpus where every DXT5 record
  decodes would mean the division stopped refusing"*. That sentence is now a
  statement that the decoder must be wrong, and no fix short of reverting §2
  satisfies it.

Neither was worked around. Refusing `DXT1` to keep a stale assertion green would
be relaxing the decoder to suit a test, which is the one thing this repository's
rules forbid outright.

## 7. Disagreements with existing documents, recorded rather than deleted

* **`docs/MATERIALS-DESIGN.md` §1**, `field_18`: *"unresolved (same in every
  sample)"* — it is the base level's byte count in all three formats (§4), and it
  varies with the dimensions, so "same in every sample" was a property of the
  three 512² records that document sampled.
* **§1**, `field_1c`: *"a float 1.0 in 0x15 records"* — it is the first four bytes
  of the base level's payload; `0x008080ff` is not `1.0f` and it varies per
  record. Already recorded as
  `claims::ENVELOPE_FIELD_1C_MEANING`.
* **§1**, *"per-layer 16-byte record headers (observed `name[4]` + 3 u32, e.g.
  `"e9D1"`)"* — there are none. Those bytes are the first two BC3 blocks of the
  base level, and they vary from record to record, which is what block data does
  and is not what a header does. The `"e9D1"` spelling was read out of a
  synthetic record the crate's own tests built.
* **§1**, *"nPresentLayers is derived … must divide evenly … Verified on all
  three 512 DXT5 records: n=2"* — there is one chain and nothing divides (§2).
* **§1**, *"first block = 20 00 02 00 65 39 00 00 (valid DXT5: a0=0x20 a1=0x00,
  c0=0x02 c1=0x6539>>)"* — under BC3 those bytes are `alpha0=0x20`,
  `alpha1=0x00` and the first five of sixteen 3-bit indices; there is no 6-bit
  colour word at offsets 2..3.
* **§1**, *"(records `raster_1`/`_2` are format 0x15 …) — out of scope"* and
  **§2**'s *"0x15 = luminance records, not DXT5"* — the family is decoded, and it
  is not luminance: lanes 0–2 are a tangent-space normal map with **Z first**
  (`claims::LUMINANCE_FOURCC`).
* **`tools/spore/raster/raster.py`** and **`src/assets/Texture.cpp`** derive the
  layer count by division with an 8-byte block. Both are the *differential
  reference*, and both are wrong here. They are left in place: `AGENTS.md`
  requires that the C++/Python half stays load-bearing, and the correct outcome is
  for their owners to re-derive against §2, not for this crate to pretend they
  agree.
