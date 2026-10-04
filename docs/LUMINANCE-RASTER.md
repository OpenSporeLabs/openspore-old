# The `0x15` raster family — hypothesis, measurement, evidence, conclusion

**Status:** decoded. `crates/spore-texture` decodes all 901 `0x15` records in
`SPORE/Data/Spore_Content.package`.
**Written:** 2026-10-04, Agent RASTER.
**Supersedes:** the "luminance, out of scope" reading in
`docs/MATERIALS-DESIGN.md` §1 and in `tools/spore/raster/raster.py`'s docstring.
**Implementation:** `crates/spore-texture/src/luminance.rs`, claims in
`crates/spore-texture/src/claims.rs`, tests in
`crates/spore-texture/tests/luminance.rs`.

---

## 0. Why this document exists

The repository's rule is that the **real corpus outranks a document**, and that
the derivation of a format is part of the format. Two committed sources
described this family before it was decoded and **both were wrong**:

| Source | Claim | Reality |
|---|---|---|
| `docs/MATERIALS-DESIGN.md` §1 | "the `0x15` records (`raster_1`/`_2`) are format `0x15` (luminance, not DXT5) — out of scope" | not luminance: a tangent-space normal map plus a fourth lane; decoded |
| `docs/MATERIALS-DESIGN.md` §1 | "`0x1c` format-specific (float 1.0 in `0x15` records)" | `0x1c` is **the first texel**. 670 of 901 records read `0x008080ff`, the rest are 231 further values tracking pixel content. `0x008080ff` is not `1.0f`. |
| `tools/spore/raster/raster.py:11` | "`0x15` is a luminance format (out of scope, hard error here)" | hard error was correct *given the knowledge of the day*; the name is wrong |

Nothing here deletes those statements. `crates/spore-texture/src/claims.rs`
records the correction on the same subjects, and
`claims::the_refuted_documents_are_named_rather_than_dropped` fails the build if a
refutation stops being visible in the claim itself.

---

## 1. The hypothesis, and how it was killed

**Initial hypothesis (from the mission brief):** the `0x15` family shares the
DXT5 framing — 32-byte envelope, `n` 16-byte layer headers, then block payloads —
and differs only in the block codec and/or the block size.

**Measurement that killed it.** The corpus census
(`Spore_Content.package`, all 2 954 records of type `0x2F4E681C`):

| `fourcc` | records | note |
|---|---|---|
| `0x35545844` `DXT5` | 1 747 | decoded by the existing block codec |
| `0x00000015` | 901 | this document |
| `0x31545844` `DXT1` | 306 | **not** decoded; see §7 |

The `0x15` family occurs in exactly **three** shapes — the census found no other
combination of dimensions and mip count in 901 records:

| width × height | mip_count | record_size | records |
|---|---|---|---|
| 32 × 32 | 6 | 5 508 | 67 |
| 256 × 256 | 9 | 349 584 | 294 |
| 512 × 512 | 10 | 1 398 164 | 540 |

Applying the DXT5 rule (`n = (size - 32) / (16 + chain)`, chain in 8-byte 4×4
blocks) to those three shapes yields **no integer layer count at any block
size from 1 to 128 bytes**, for any layer count 1–8, for any block edge 1/2/4/8.
A brute-force search over `(block edge, bytes per block, layers, header size)`
found **no** combination that fits all three shapes. The shared framing is
therefore false, and everything below is a fresh derivation from the bytes.

**The first real clue: `field_18`.** For all three shapes, the word at envelope
offset `0x18` is exactly `width * height * 4`:

| shape | `field_18` | `w*h*4` |
|---|---|---|
| 32² | `0x00001000` | 4 096 |
| 256² | `0x00040000` | 262 144 |
| 512² | `0x00100000` | 1 048 576 |

901 of 901 records, zero exceptions. In the DXT5 records `field_18` reads
`0x00040000` regardless of dimensions (a 512² DXT5 carries `0x40000`, not the
`0x100000` its `w*h*4` would give), so **this is not a shared header meaning** —
it is specific to this family, and it is a byte count. That points at four bytes
per texel, uncompressed.

**The second clue: the bytes at `0x1c` are not a constant.** Dumping the head of
a 32² record:

```
0000: 01 00 00 00 20 00 00 00 20 00 00 00 06 00 00 00   version w h mips
0010: 08 00 00 00 15 00 00 00 00 10 00 00 ff 80 80 09   f10 fourcc f18 "f1c"
0020: ff 7f 81 08 ff 80 7e 05 ff 80 7c 02 ff 80 82 02
```

`field_1c` reads `0x098080ff`, i.e. bytes `ff 80 80 09` — and the payload
continues `ff 7f 81 08`, `ff 80 7e 05`, … That is a run of *four-byte texels*,
starting **four bytes before** offset `0x20`. This family has **no 16-byte layer
header and a 28-byte envelope**, which is the whole of the framing difference.

---

## 2. The framing

Dumping the **tail** of one record per shape located the missing length prefixes:

```
32x32 tail:  ... fc 85 86 0a | 10 00 00 00 | fd 83 7e 0a fc 81 88 09
                      f9 8a 81 08 fc 84 85 0a | 04 00 00 00 | fc 85 83 0a
```

`10 00 00 00` is 16 = the byte size of a **2×2 RGBA8** mip, `04 00 00 00` is 4 =
the byte size of a **1×1** mip, and each is followed by exactly that many payload
bytes. The 512² tail shows `10 00 00 00` then 16 bytes then `04 00 00 00` then 4
bytes; the 256² tail likewise.

So the layout is:

```text
off  size  field
0x00   4   version       u32 LE, always 1        (901/901)
0x04   4   width         u32 LE
0x08   4   height        u32 LE
0x0c   4   mip_count     u32 LE
0x10   4   field_10      u32 LE, always 8        (901/901) -- UNKNOWN, never read
0x14   4   format        u32 LE, always 0x15     (901/901)
0x18   4   field_18      u32 LE == width*height*4 (901/901) -- carried, never read
0x1c   ..  mip 0 payload, width*height*4 bytes, NO length prefix
       4   mip 1 byte count, u32 LE == mip1_w*mip1_h*4
       ..  mip 1 payload
       4   mip 2 byte count
       ..  mip 2 payload
       ...
```

### 2.1 The exact-consume identity

```text
record_size == 28 + width*height*4
              + SUM over mips 1..mip_count of (4 + max(1,w>>m)*max(1,h>>m)*4)
```

Checked three ways:

| shape | mips | `28 + base` | prefix bytes | **total** | **measured** |
|---|---|---|---|---|---|
| 32×32 | 6 | 4 124 | 5 × 4 = 20 | **5 508** | **5 508** |
| 256×256 | 9 | 262 172 | 8 × 4 = 32 | **349 584** | **349 584** |
| 512×512 | 10 | 1 048 604 | 9 × 4 = 36 | **1 398 164** | **1 398 164** |

Expanded per shape, so the arithmetic is checkable by hand:

```text
32x32, 6 mips: 28 + 4096 + (4+1024) + (4+256) + (4+64) + (4+16) + (4+4) = 5508
256x256, 9:    28 + 262144 + (4+65536) + (4+16384) + (4+4096) + (4+1024)
                   + (4+256) + (4+64) + (4+16) + (4+4)                = 349584
512x512, 10:   28 + 1048576 + (4+262144) + (4+65536) + (4+16384) + (4+4096)
                   + (4+1024) + (4+256) + (4+64) + (4+16) + (4+4)      = 1398164
```

**Result: 901 of 901 records close exactly** — zero bytes left over, zero bytes
unconsumed, and every one of the 901 `mip_count - 1` per-level prefixes equals its
own mip's `w*h*4`. That is an exact identity over the whole corpus, not a
plausible fit over one record. It is also **strictly stronger** than the DXT5
rule, which divides a payload to *find* a layer count: here the record size is
fully determined by the envelope, so a mismatch is a refusal
(`LuminanceRecordSize` / `LuminanceLengthPrefix`) rather than a rounding.

### 2.2 Why the base level has no prefix

It does not need one: `field_18` already states its length, and the derived chain
states it independently, and the two agree in 901 of 901 records. Both are
redundant. The decoder **uses the derived value and leaves `field_18` unread** —
the crate's existing "derived, never read" rule, applied to one more header word.
A consequence, asserted by test: a record whose `field_18` is corrupted still
decodes, because nothing depends on it. Making a decode depend on a word whose
*cross-format* meaning is not shared would be a step backwards.

### 2.3 What is therefore different from DXT5

| | DXT5 | `0x15` |
|---|---|---|
| envelope | 32 bytes (8 words) | **28 bytes (7 words)** |
| layer headers | 16 bytes each, all before all payloads | **none** |
| layer count | derived by division, `n ≥ 1` | always **1** |
| payload | 4×4 blocks, 8 bytes each | **4 bytes per texel, uncompressed** |
| per-mip length | derived from block count | **`u32` prefix before every mip past the base** |

---

## 3. The texel layout — hypothesis, then measurement

**Hypothesis from the census:** four bytes per texel, uncompressed. Which four
channels, and in what order, was **not** assumed. The candidate readings were
tested against the data rather than chosen.

### 3.1 The flat records (decisive)

Seven of the 901 records have lanes 0, 1 and 2 **constant over the entire base
level**:

| record | shape | lanes 0-2 | lane 3 values |
|---|---|---|---|
| `0x40652902:0x04bafc1e` | 512² | `(255, 128, 128)` | `{0, 255}` |
| `0x40642902:0x0fbd1396` | 512² | `(255, 128, 128)` | `{0, 255}` |
| `0x40642902:0x19ed05ad` | 512² | `(255, 128, 128)` | `{0, 255}` |
| `0x40632902:0x0675f7af` | 512² | `(255, 128, 128)` | `{0, 255}` |
| `0x40632902:0x06804c10` | 512² | `(255, 128, 128)` | `{0, 255}` |
| `0x40632902:0x06804c15` | 512² | `(255, 128, 128)` | `{0, 255}` |
| `0x40632902:0x06804c1a` | 512² | `(255, 128, 128)` | `{0, 255}` |

Seven records that are *nothing but* a constant quad, with 540 of 901 records
giving lane 3 only **two** distinct values (`0` and `255`).

`(255, 128, 128)` with 128 as zero is the textbook encoding of the **flat
tangent normal `(0, 0, 1)`** — in an order that puts **Z first**. No other
reading of a constant triple is so standard. This is the single strongest piece
of evidence in the whole derivation, and it is what rules out "luminance".

### 3.2 The population statistics

Over all 901 records' base levels (four bytes per texel, called *lane 0..3* in
**file order**):

| statistic | lane 0 | lane 1 | lane 2 | lane 3 |
|---|---|---|---|---|
| mean, min over records | 232.5 | 116.7 | 116.7 | 0.2 |
| mean, median over records | 251.8 | 127.9 | 127.8 | 169.7 |
| mean, max over records | 255.0 | 133.0 | 132.8 | 229.8 |
| records with mean > 230 | **901 / 901** | — | — | — |
| records with mean in (120, 136) | — | **899 / 901** | **899 / 901** | — |

Lane 0 saturates near 255; lanes 1 and 2 are centred on 127.5.

### 3.3 The functional test

If lane 0 is `Z`, it must satisfy `Z = sqrt(1 - nx² - ny²)` where
`n = (2·lane - 255)/255`. Measured on `0x40622901:0x06808aae` (256², 65 536
texels):

* `corr(lane 0, Z derived from lanes 1 and 2)` = **0.954**
* byte-exact match against `round(Z·255)`: **52.79 %** on that record, and over
  all 901 records the median is **74.1 %**, with 274 records ≥ 80 % and 27 ≥ 95 %.
  The tail is quantisation noise at high gradient, which is exactly where an
  8-bit-encoded normal map must lose it.
* `corr(lane 1, lane 2)` = **0.004** — two *independent* components, as two normal
  axes are, and as two derived-from-a-common-source channels would not be.
* `corr(lane 0, lane 3)` = **0.032** — lane 3 is not part of the normal map.

### 3.4 The picture

Rendering the mission's record as `(X, Y, Z) = (lane 1, lane 2, lane 0)` gives an
unmistakable tangent-space normal map: lavender where flat, pink/cyan along the
creases. Rendering it as `(lane 0, lane 1, lane 2)` gives a washed-out salmon
image — which is the visual statement that **the Z component is first on disk**.

### 3.5 Conclusion — graded

| claim | grade | why that grade |
|---|---|---|
| The family is uncompressed, 4 bytes per texel | **VERIFIED** | the exact-consume identity in §2.1 over 901/901 |
| Framing: 28-byte header, no layer headers, per-mip `u32` prefixes | **VERIFIED** | same identity; prefixes independently equal `w*h*4` |
| Lanes 0-2 are `Z, X, Y` with 128 as zero | **VERIFIED** | §3.1's constant-`(255,128,128)` records plus §3.3's functional fit over 901/901 |
| `field_18 == w*h*4` in this family | **OBSERVED** | 901/901, but it is a measurement of a word, not a derivation |
| `field_1c` is payload, not a field | **VERIFIED** | the identity only closes when those four bytes are charged to the base level |
| Lane 3 is a per-texel mask | **INFERRED** | only its *independence* from the normal map was measured; "mask" is a reading |
| The decoded bytes should be swizzled for rendering | **UNKNOWN** | see §5 |

---

## 4. What the decoder does

There is **no codec**. `decode_luminance_mip` validates that it has
`width*height*4` bytes and copies them. Writing a block decoder here would invent
a structure the corpus does not have, so the `luminance` module has no
`BLOCK_BYTES` and no palette, and says so in its own documentation.

Output is `RasterImage { envelope, layers }` with **one** layer, matching the
crate's existing type shape. `layer_count_for` dispatches on `fourcc`: DXT5 keeps
the division rule; `0x15` returns 1 and enforces the exact size.

Refusals, each named (never a partial image, never a blank one):

| variant | when |
|---|---|
| `LuminanceRecordSize` | `record_size != 28 + chain` |
| `LuminanceLengthPrefix` | a prefix ≠ its own mip's `w*h*4` |
| `LuminanceSlicePastRecord` | a prefix or payload runs past the record |
| `ShortLuminanceMip` | a mip payload is short (there is no codec to absorb it) |
| `UnsupportedFourcc` | anything but `DXT5` and `0x15` |

---

## 5. The swizzle: a question, not an answer

The bytes show **Z in lane 0**, so the file order is *not* the conventional
normal-map order `R=X, G=Y, B=Z`. The decoder emits the stored bytes in file
order (R = lane 0, G = lane 1, B = lane 2, A = lane 3) and **records the
question as unresolved**.

Reasoning, because this is the one place where convenience could have won:

* Swizzling would change decoded bytes. The only argument for it is "Z is blue,
  and Z is first, so it must be BGRA" — which is a real argument, but it is a
  statement about **a consumer's expectations**, not about the record.
* The crate is a decoder. Its DXT5 decoder already reproduces the oracle's channel
  assignment literally and records the *interpretation* of those channels
  (`claims::DXT5_SPEC_DEVIATIONS`, `claims::LUMINANCE_FOURTH_LANE`) rather than
  re-ordering anything. Doing the same here is consistent.
* No shader, material or renderer in this workspace has been read. Guessing
  `BGRA` because it "looks like DirectX" is an inference wearing a fact's
  clothes.

**Consequence for the renderer, stated plainly:** the creature preset now decodes
both its textures (`textures=2/2`), but the `0x15` one is applied as-is, so on
screen its Z component lands in red. If the game expects the conventional order,
the swizzle belongs in `crates/spore-material` or `crates/spore-engine` — files
this work did not touch — and should be made there *with the shader as evidence*.

---

## 6. Reproduction

The census and the identity check, from the repository root:

```bash
# 2954 raster records; the fourcc table and the three shapes
python3 tools/spore/raster/raster.py        # does NOT read this family (see §0)

# The decoder, hermetically
cargo test -p spore-texture --test luminance

# End to end, real asset, real window
cargo run -p spore-engine -- --preset creature \
    --package SPORE/Data/Spore_Content.package --texture --info
```

There is **no committed fixture**: the real records live only in the git-ignored
`SPORE/` tree. `crates/spore-texture/tests/common/mod.rs` builds a synthetic
`0x15` record byte by byte and transcribes the *measurements* — the three shapes,
their record sizes, the constant `(255, 128, 128)` flat texel — as numbers.

---

## 7. Still UNKNOWN

These are non-findings, not gaps in effort.

1. **What lane 3 is.** Measured: independent of the normal map (`corr 0.032`),
   per-record mean from 0.2 to 229.8, binary (`{0, 255}`) in 540 of 901 records.
   Not established: whether it is ambient occlusion, a specular term, a paint
   channel or something else. `LUMINANCE_FOURTH_LANE` is graded **INFERRED**.
2. **Whether the decoded bytes should be swizzled.** §5.
3. **`field_10` (always 8).** Unchanged from before this work; still unread, still
   `UNKNOWN`.
4. **`field_18` in the DXT5 records.** It is *not* `w*h*4` there, and nothing
   explains its `0x00040000`. The word is format-specific, which is all that is
   now claimed.
5. **The 306 `DXT1` (`0x31545844`) records.** Measured, not guessed: they do
   **not** fit the DXT5 framing (a 256²/9-mip record leaves 12 bytes over the DXT5
   layer stride: `43732` available against a `43720` stride). They are refused by
   name. Given what `0x15` turned out to be, the obvious next hypothesis is that
   DXT1 uses the same 28-byte header plus per-mip prefixes with a 4-byte block —
   `43732 - 43704 - 8·4 = -4` — which is close but does not close, so it is a
   hypothesis and **not** a finding. Out of scope here.
6. **Whether `0x15` and `DXT1` are two variants of one uncompressed format.**
   Both are uncompressed-looking, and `0x15` is a format *enum* value while
   `DXT1` is a FOURCC, which suggests two different vocabularies rather than one.
   Untested.
7. **Whether the identity holds outside `Spore_Content.package`.** One corpus, one
   build. The exactness of the fit over 901 records is strong evidence but it is
   not a cross-build differential.
8. **The role of the family in the game's shading.** The DXT5 sibling of every
   `0x15` record decodes to the "black/red alpha-mask signature", so the pair
   looks like a mask plus a normal/detail map — but no shader has been read, and
   this is an impression from pixels, not a finding.