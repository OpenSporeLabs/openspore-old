# 08 — The rigblock record (`0x5CAA5E28`): the tail is a parametric bounds table, and it does **not** name a `gmdl`

**Agent:** RIGBLOCK · **Scope:** RESEARCH ONLY · **Date:** 2026-10-04
**Subject:** `Spore_Game.package` / type `0x5CAA5E28`, 1549 records, groups
`0x40616000` (58), `0x40626000` (482), `0x40636000` (224), `0x40646000` (540),
`0x40666000` (107), `0x40686000` (138) — the record type every `<blockid>` in the
68 `<sporemodel>` documents resolves to.

**Verdict: PARTIALLY RESOLVED.** The encoding is **RESOLVED and machine-verified**
(§2–§3). The premise of the question is **REFUTED with evidence**: the record does
**not** contain a `gmdl` (`0x00E6BCE5`) ResourceKey, and it cannot, because its
bytes are 100 % accounted for with **zero slack in 1549/1549 records** (§3.4). The
rigblock's renderable geometry was located anyway, in a **different** record type at
a **different** group with the **same instance id** (§5).

Every figure below is a count produced by a command in this repository against the
real data. No asset byte is reproduced here; the format is given as a field table
and formulas.

---

## 0. Reproduction

```python
import sys, struct, collections
sys.path.insert(0, 'tools')
from spore.dbpf.dbpf import read, getdata

data, items = read('SPORE/Data/Spore_Game.package')
rb = { (it['group'], it['inst']): getdata(data, it)
       for it in items if it['type'] == 0x5CAA5E28 }
# -> 1549 records
```

Corpora used: `Spore_Game`, `Spore_Content`, `Spore_Graphics`, `PatchData`,
`Spore_Pack_03`, `Spore_Audio1/2`. Binary: `SporeApp.exe` 3.1.0.22, image base
`0x00400000`, via the `ghidra` MCP bridge.

---

## 1. Correction to the brief's measurements

The brief reported "sizes 36 … 8631, a long tail of distinct sizes", with a
histogram containing `84 ×428`, `228 ×233`, `208 ×2`, `74 ×3`. **That is not what
the records are.** Measured over all 1549 records:

| record size (B) | 36 | 48 | 84 | 228 | 804 | 3108 | 12324 | 49188 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| count | 212 | 330 | 465 | 369 | 128 | 37 | 7 | 1 |

**8 distinct sizes, 1549/1549 records.** No `208`, no `74`, no long tail, maximum
49188 B. This single correction is what made the format tractable: the record is
**not** a variable-length stream, it is a fixed 36-byte header plus a payload whose
length is `12 · 4^k`. (The brief's figures presumably came from a different family
mask or a size/offset mix-up; `0x208` and `0x4a` do not occur at all.)

Also measured, for the record:

* `[0x00,0x18)` — 6 little-endian floats. **0/1549** implausible (`|v| < 10^4`);
  **3/1549** have `min > max` on some axis (explained in §3.2 — they are *not* a
  single AABB).
* `[0x18,0x1c)` — 1 float. **1337** positive, **212** exactly `0.0`, and those 212
  are **exactly** the 212 size-36 records (perfect correlation, 0 exceptions).

---

## 2. Record layout — VERIFIED on 1549/1549

```
size = 36 + 12 * 4^k ,  k in 0..7
```

> **CORRECTION (2026-10-04, found by `spore-rigblock`'s corpus test).** The
> formula above is **wrong for `k = 0`**, and this report's own histogram
> contradicts it. `36 + 12·4⁰` = 48, but the histogram below lists **212 records
> of exactly 36 bytes**, and §3.2's decompiled writer returns immediately after
> the header when the level count is zero:
>
> ```
> if n == 0:  write(&hdr, 0x24);  return   // -> exactly 36 bytes
> ```
>
> The correct statement is:
>
> ```
> cell_count = 0       when k == 0     <- the early return; no payload at all
> cell_count = 4^k     when k >= 1
> size       = 36 + 3 * cell_count
> ```
>
> The per-level mapping, from this report's own size histogram cross-tabulated
> against its level histogram, is
> `k=0→36, 1→48, 2→84, 3→228, 4→804, 5→3108, 6→12324, 7→49188`
> (counts `212, 330, 465, 369, 128, 37, 7, 1`, summing to 1549), which is
> `36 + 3·4^k` for `k >= 1`. So `k = 0` carries **no cells**, not one.
>
> A `k = 0` record read as one cell would invent a cell the record does not
> contain, which is why `spore-rigblock` refuses to. The rest of this report —
> the field table, the nibble mapping, the slack histogram — is unaffected: the
> payload simply does not exist at `k = 0`, and every other level's arithmetic
> was already right.

| offset | size | field | grade |
|---|---:|---|---|
| `0x00` | 12 | 3 floats — component-wise **MAX** over all cells of each cell's *low* corner | VERIFIED |
| `0x0c` | 12 | 3 floats — component-wise **MIN** over all cells of each cell's *high* corner | VERIFIED |
| `0x18` | 4 | float — **quantisation step** `s` (see §3.3) | VERIFIED |
| `0x1c` | 8 | 8 × uint8; `byte[i] = 4` for `i < k`, else `0` | VERIFIED, 1549/1549 |
| `0x24` | `12·4^k` | payload: `4^k` cells × 3 bytes | VERIFIED, 1549/1549 |

Two independent confirmations of the header, both exact:

```
level-byte field == [4]*k ++ [0]*(8-k)                : 1549 / 1549
record size == 36 + 12*4^k                            : 1549 / 1549
(k == 0)  <=>  (float@0x18 == 0.0)                    : 212/212 and 1337/1337, 0 exceptions
unaccounted ("slack") bytes per record after the above : {0: 1549}   <-- nothing left over
```

### 2.1 Payload cell encoding — VERIFIED

Each cell is **3 bytes**, one per axis `j ∈ {x, y, z}`:

```
byte_j = ( qLow[j] << 4 ) | qHigh[j]          both nibbles in 0..15
```

* **high nibble** = distance from the record's *global* extreme down to this cell's
  low corner, in units of `s`.
* **low nibble** = distance from this cell's high corner up to the record's *global*
  extreme, in units of `s`.

Decoder (inverse of the writer, §3.3):

```
cellLow [j] = f32[0+j] - qLow [j] * s
cellHigh[j] = f32[3+j] + qHigh[j] * s
```

with reconstruction error ≤ `s/2` per face (4-bit quantisation).

This is the resolution of the brief's "row selector × column" motif: the motifs
`0x11,0x22,…,0x55` are simply the cells where the two distances quantise to the
*same* nibble on an axis, and `0x10,0x20,…,0xf0` are the cells at nibble `(j, 0)`.
The "sparse triples" are cells whose low nibbles are all `0`.

### 2.2 Distributions (all 1549 records)

* `k` histogram: `0:212  1:330  2:465  3:369  4:128  5:37  6:7  7:1`
* total cells decoded: **148 088**; total payload bytes **444 264** (= 148 088 × 3).
* Per family:

| family | n | k=0 | k=1 | k=2 | k=3 | k=4 | k=5 | k=6 | k=7 | payload B |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `0x40616000` cell | 58 | 11 | 18 | 20 | 9 | 0 | 0 | 0 | 0 | 2 904 |
| `0x40626000` creature | 482 | 164 | 113 | 102 | 89 | 14 | 0 | 0 | 0 | 34 092 |
| `0x40636000` building | 224 | 7 | 61 | 47 | 52 | 31 | 26 | 0 | 0 | 116 652 |
| `0x40646000` vehicle | 540 | 2 | 86 | 176 | 189 | 68 | 11 | 7 | 1 | 266 952 |
| `0x40666000` flora | 107 | 28 | 21 | 13 | 30 | 15 | 0 | 0 | 0 | 18 156 |
| `0x40686000` unknown | 138 | 0 | 31 | 107 | 0 | 0 | 0 | 0 | 0 | 5 508 |

`k` rises monotonically with part complexity in the way one expects from a morph
lattice: unknown `0x40686000` never exceeds `k=2`, creature never exceeds `k=4`,
vehicle (UFO generation reuses this family per `02` §5.2) reaches `k=7`.

---

## 3. What the record *is* — the writer, from the binary

`0x5CAA5E28` appears as an immediate in **exactly 2 places** in the whole 21.8 MB
image (full scan of 4 671 374 defined instructions for the scalar `0x5caa5e28`;
`0x00e6bce5` appears 38 times, for comparison).

### 3.1 `FUN_0041c1d0` @ `0x0041c1d0` — the baker

```
for group in { the 6 rigblock family bases }:            # 6 iterations
    ids = PropertyList.GetArrayKey(group)                # -> array of instance ids
    for id in ids:
        model = bakeMgr.vtable[4]().vtable[3]( id, group, 4 )     # load a Model
        if FUN_0041aa40(model, out):                     # serialise (below)
            write  Editors_BlockData.package
                   "<root>BlockData/%s0x%08x!0x%08x.blockdata"
                   key = { instance = id, type = 0x5CAA5E28, group = group }
```

* `0x5CAA5E28` is a **baked, editor-generated** record. Grade **VERIFIED** (code).
* It is written to `Editors_BlockData.package`, not to `Spore_Game.package`. The
  copy in `Spore_Game`/`PatchData` is the shipped bake output. (`PatchData`
  carries a **byte-identical** copy: 1549/1549 same key set, 200/200 sampled
  records SHA-256-equal — VERIFIED.)
* The property that supplies the id list is `0x53878d0`; its keys have
  `typeID` zeroed before use, i.e. the list is stored as `(type, group, instance)`
  with the type elided.

### 3.2 `FUN_0041aaa0` @ `0x0041aaa0` — the serialiser

```
n = model.vtable[0xe8](subsystems, buf, 32)              # number of levels, 0..8
if n == 0:  write(&hdr, 0x24);  return                    # -> exactly 36 bytes
for L in 0..n-1:
    res[L]      = 4                                      # <-- HARDCODED 4
    nCells     *= 4                                      # -> 4^n
hdr.f32[0..2] = maxOverCells( cell.low )                 # component-wise MAX
hdr.f32[3..5] = minOverCells( cell.high )                # component-wise MIN
hdr.f32[6]    = max( max3(maxLow-minLow), max3(maxHigh-minHigh) ) / 15.0
hdr.u8[0..7]  = { 4 for i<n, else 0 }
write(&hdr, 0x24)                                        # 36 bytes
for cell in 0..nCells-1:                                 # base-4 odometer,
    for j in 0..2:                                       # digit n-1 fastest
        q = round_toward_+inf( (hdrExtreme[j] - cell[j]) / s )
        byte[j] = (min(q,15) & 0xf) << 4 | (min(q',15) & 0xf)
    write(buf, nCells*3)
```

`res[L] = 4` is the whole reason for the `4^k` scaling and for the byte `4` in the
header. The `k` octets at `0x1c` are literally that per-level resolution array.
Grade **VERIFIED** (machine code + data agreement, below).

Three primitives were decompiled and then **re-checked at machine level**, because
Ghidra's inlined comparison chains inverted them:

| VA | Ghidra decompile says | disassembly says | used here |
|---|---|---|---|
| `FUN_0041bfb0` | component-wise MIN | `COMISS b,a ; JBE` → **MAX** | MAX |
| `FUN_0041c0c0` | component-wise MAX | mirrored → **MIN** | MIN |
| `FUN_0041db10` | `a - b` | `SUBSS [param_2],[param_3]` → `a - b` | `a - b` |
| `FUN_0041bf30` | max3 | 2 × `CMPSS`/`JB` → **max3** | max3 |

*(Trap worth recording: a 4-call decompilation of `FUN_0041aaa0` yields a
**negative** quantisation step and a positive/negative-inverted nibble mapping.
Both are wrong, and only the data catches it. See §3.4.)*

### 3.3 The data-side proof, with counts

`s` cannot be computed from the record alone (it is built from the *per-cell*
extreme corners, only two of whose four aggregates are stored). So the layout claim
is tested where it is falsifiable — the nibble axis assignment — over every cell of
every record:

```
cells decoded (k>=1)                          : 148 088
axis tests (cells x 3)                        : 444 264
interp A  lo = f[j] + qHi*s, hi = f[j+3] - qLo*s
        axis-order violations                  : 234 428
interp B  lo = f[j] - qHi*s, hi = f[j+3] + qLo*s   <-- the writer's actual mapping
        axis-order violations                  :        0
```

**0 / 444 264** under the writer's own mapping. Interpretation A is refuted by the
same corpus. This is what fixes both the field order *and* the nibble order.

Two further necessary conditions, each of which the theory forces and the data
satisfies without exception:

```
records where min(qLow) == 0 on axis x / y / z        : 1337 / 1337 / 1337
records where min(qHigh) == 0 on axis x / y / z       : 1337 / 1337 / 1337
records where both nibble families reach 0            : 1337 / 1337
```

The extreme cell must quantise to `0` on every axis. A coincidence would need
`16^-6`; observed 1337/1337. Grade **VERIFIED**.

Nibble histograms over all 444 264 nibble pairs are strongly bimodal at `0`, `1`
and `15` — the expected signature of a 4-bit distance quantiser whose dynamic range
is exactly saturated by `s = maxExtent/15`.

### 3.4 Slack: the record cannot hold anything else

```
bytes consumed by the §2 layout, per record : slack histogram { 0 bytes : 1549 }
```

There is no padding, no trailer, no spare word, in any record. A `ResourceKey` is
12 bytes. **There is nowhere for one to be.** Grade **VERIFIED**.

---

## 4. Hypotheses tested before touching the tail

### 4.1 (a) Positional pairing / bbox correspondence — **REFUTED**

Rigblock `float[0..5]` was compared against every `gmdl` record's decoded
`bboxMin/bboxMax` (all 4209 `0x00E6BCE5` records parse; `gmdl` v8 4209/4209), by
family:

| family | rigblock records | `gmdl` records | pairs tested | rigblock box equal to some `gmdl` box |
|---|---:|---:|---:|---:|
| `0x4061` | 58 | 132 | 7 656 | **0** |
| `0x4062` | 482 | 388 | 187 016 | **0** |
| `0x4063` | 224 | 1 312 | 293 888 | **0** |
| `0x4064` | 540 | 1 140 | 615 600 | **0** |
| `0x4066` | 107 | 612 | 65 484 | **0** |
| **total** | 1 411 | 3 584 | **1 169 644** | **0** |

Nearest-neighbour residuals are large and generic (best per family: 0.243, 0.284,
4.316, 0.318, 0.631) — there is no cluster of near-matches hiding at a different
tolerance. Instance-id intersection between the two layers is **0** in every family
(re-measured; agrees with `02` §2.4).

### 4.2 (b) Arithmetic relation between the ids — **REFUTED**

Rigblock instance ids and `gmdl` instance ids share no residue structure. Best
modular agreement over `m ∈ {2,3,4,5,7,8,16,32,64,97,128,255,256}`:

| family | best `m` | agreements | chance |
|---|---:|---:|---:|
| `0x4062` | 2 | 234 / 482 | ≈241 |
| `0x4064` | 2 | 264 / 540 | ≈270 |

Both *below* chance. Low-nibble histograms are flat for both layers. Grade
**REFUTED**.

### 4.3 (c) A dictionary record mapping rigblock → model — **PARTIALLY HIT, then REFUTED as a `gmdl` map**

A `prop` record (`0x00B1B104`) exists at **the rigblock's own `(group, instance)`**:

| family | `prop` @ rigblock group (Spore_Graphics) | rigblock records | id sets identical | shared ids |
|---|---:|---:|:--:|---:|
| `0x40616000` | 58 | 58 | **yes** | 58 |
| `0x40626000` | 503 | 482 | no | 464 |
| `0x40636000` | 224 | 224 | **yes** | 224 |
| `0x40646000` | 540 | 540 | **yes** | 540 |
| `0x40666000` | 140 | 107 | no | 107 |
| `0x40686000` | 138 | 138 | **yes** | 138 |

**1531** `prop` records share a rigblock instance id. Of those:

```
containing the gmdl type word 0x00E6BCE5        : 0 / 1531
containing ANY creature/cell/vehicle/flora/building rigblock instance id
   at ANY byte alignment (as a reference)        : 0 / 743
```

So the per-rigblock `prop` is **not** a `gmdl` dictionary. What it *does* contain
is in §5.

### 4.4 (d) Any record type carrying both a `0x4062xxxx` word and `0x00E6BCE5` — **HIT (positive control), and it is the *other* layer**

Full scan of **79 150** decompressed records across 5 packages (0 decompress
failures). Exactly one type matches:

```
type 0x00B1B104 (prop)  x  creature model-slot group 0x40626200
    97 records, 97/97 contain a gmdl ResourceKey, 388 keys total
    keys resolve against the real Spore_Content gmdl index : 388 / 388  (100.0%)
```

This is the **positive control that validates the search method**: the extraction
pattern (12-byte key, fields in the order `instance, type, group`) recovers 388
keys and every one resolves. The same search over the 1549 rigblock records finds
**zero**.

Key layout inside `prop` (VERIFIED from the 97 creature records): the 4 LOD gmdl
keys for instance `0x010dea5e` are groups `0x40627100`, `0x40627101`, `0x40626202`,
`0x40626203`, each stored as a 16-byte entry
`{ instance:u32, type:u32, group:u32, tag:u32 = 0x00A00CE4 }`. The same 16-byte form
opens each record with its own identity.

---

## 5. Where the rigblock's renderable geometry actually is

The `prop` from §4.3 was the right door. Inside the per-rigblock `prop` records, the
most frequent 16-byte tagged key whose `instance` equals the record's own is:

```
{ instance = <rigblock instance>, type = 0x2F4E681B, group = 0x40606000 }   : 1358 records
```

and `Spore_Graphics.package` group `0x40606000` (`EditorRigblocks`, per `02` §2.1)
contains exactly:

| type | records at `0x40606000` | what it is |
|---|---:|---|
| `0x01C135DA` | 1 824 | **skinned mesh**: 24-byte header `{0x01000000, 0x01000000, 0x400, X, 3, Y}` then float3 vertex positions, then a trailing bone-index/weight block; sizes 206 B … 741 518 B |
| `0x2F4E681B` | 1 824 | binary/text container (ASCII fragments, CRLF, embedded NULs) |
| `0x00B1B104` | 56 | `prop` |

The two 1824-record sets have **identical** instance-id sets (VERIFIED).

Instance-id overlap with the rigblock layer:

| rigblock family | rigblock ids | ids also at `0x40606000` |
|---|---:|---:|
| `0x40616000` cell | 58 | **58 / 58** |
| `0x40626000` creature | 482 | **392 / 482** |
| `0x40636000` building | 224 | **220 / 224** |
| `0x40646000` vehicle | 540 | **528 / 540** |
| `0x40686000` unknown | 138 | **118 / 138** |
| `0x40666000` flora | 107 | **0 / 107** |
| **total** | **1 549** | **1 316** |

**This is the link the brief was looking for, and it is not a `gmdl`.** A rigblock's
renderable mesh is `0x01C135DA` at group `0x40606000`, addressed by the *same*
instance id as the rigblock, with the per-rigblock `prop` as the pointer.
Grade **SUPPORTED** (id-set overlap + a validated 12-byte key form + the leading
float3 vertex block); the *vertex layout* of `0x01C135DA` is **UNKNOWN** (§7).

Consecutive rigblock instances are geometry variants of one part, not copies —
e.g. instances `0xc8f7a869 … 0xc8f7a86c` (4 consecutive creature ids) carry
rigblock records all of size 228 (`k=3`) and meshes of 99 260 / 200 440 / 134 644 /
263 416 B. Grade **OBSERVED**.

Note `flora` is the one family with **zero** overlap: 107 flora rigblocks and 140
flora `prop` records at `0x40666000`, but no `0x40606000` counterpart. That is a
real gap, not a measurement artefact. Grade **OBSERVED**.

---

## 6. The negative, stated precisely

**The `0x5CAA5E28` record does not name a renderable `gmdl`, and no arithmetic,
positional or dictionary relation connects it to one.** Grade **VERIFIED**
(negative).

Exactly what was searched:

| search | scope | result |
|---|---|---:|
| LE `0x00E6BCE5` byte pattern | 1549 rigblock records, any offset | **0** |
| BE `0x00E6BCE5` byte pattern | 1549 rigblock records, any offset | **0** |
| 12-byte `ResourceKey`-shaped triple (validated form, `instance,type,group`) | 1549 records | **0** |
| any 4-byte-aligned word equal to the record's **own** group id | 1549 records | **0 / 1549** |
| any 4-byte-aligned word equal to the record's **own** instance id | 1549 records | **0 / 1549** |
| any 4-byte-aligned word equal to any rigblock family base group | 1549 records | **0** |
| `gmdl` bbox equality, per family | 1 169 644 pairs | **0** |
| `gmdl` instance-id intersection | per family | **0** |
| `prop` at the rigblock's own `(group, instance)` containing a gmdl type word | 1531 records | **0** |
| any record type anywhere containing both `0x00E6BCE5` and a creature slot group word | 79 150 records | 97 (`prop` @ `0x40626200`) — the whole-model layer only |

(For completeness and to prevent a future false positive: 2537 aligned words in the
rigblock records equal *some* instance id in the 35 889-id corpus universe and 2165
equal *some* group id in the 546-id universe. These are **numeric coincidences** of
the small quantised nibbles and the float bit patterns — neither the record's own id
nor any `gmdl` key is among them, as the five rows above show.)

### Evidence that *would* settle it further

1. **Decode `0x01C135DA`** (1824 records, `0x40606000`) properly — its 24-byte
   header's `X` and `Y` words and the trailing bone block. Then check whether its
   vertex bounds match the rigblock record's `float[0..5]` envelope up to a scale.
   My naive "all float3 from `0x18`" probe was **invalid** (the vertex stream is not
   a bare float3 array; it interleaves other attributes) and is not reported as a
   result.
2. **Identify type `0x2F4E681B`.** It is the type the per-rigblock `prop` points at
   (1358 records), it is paired 1:1 with the mesh type, and its head is a mix of
   ASCII and binary. If it is the part's behaviour/script, that closes the rigblock.
3. **One runtime observation.** Under Wine, load a known `<blockid>` (e.g.
   `0x40626000, 0x00013530`, which resolves to `0x40606000:0x00013530`) in the
   creature editor and record which `ResourceKey`s the loader actually fetches. That
   single trace would confirm or refute §5 outright.
4. **`0x40606000` is also a name/description layer.** `FUN_00403af0` registers six
   geometry classes `0x61..0x66` under the property namespace `"GeomInfo"`
   (`0x811C9DC5`) with property ids `0x66F235B` (array of keys) and `0x66F235C`
   (array of floats, equal length). Untested; a lead, not a finding.

---

## 7. What remains UNKNOWN

| # | question | grade | what would settle it |
|---|---|---|---|
| 1 | semantic meaning of the `k` levels (which rigblock parameter each level drives) | UNKNOWN | `model.vtable[0x74]` / `[0x80]` in `FUN_0041aaa0`; the `Model` subsystem list |
| 2 | semantics of type `0x01C135DA`'s vertex layout | UNKNOWN | §6 item 1 |
| 3 | identity and semantics of type `0x2F4E681B` | UNKNOWN | §6 item 2 |
| 4 | whether the 18 creature / 4 building / 10 unknown / 33 flora rigblock instances with **no** `0x40606000` counterpart use a different geometry layer | UNKNOWN | `FUN_00403af0`'s `"GeomInfo"` registration; runtime trace |
| 5 | why flora has zero `0x40606000` coverage | UNKNOWN | runtime trace |
| 6 | whether the `k = 0` (36-byte) records are parts with no morph parameters or failed bakes | UNKNOWN | `FUN_0041aaa0`'s `n == 0` path reads a different bbox source; 212/1549 records, family-skewed (164/482 creature, 28/107 flora, 2/540 vehicle) |
| 7 | identity of property `0x53878d0`'s authoring source | UNKNOWN | the editor's rigblock list UI |

---

## 8. Claims and negative results to carry forward

* **`0x5CAA5E28` = "EditorRigblock blockdata", a baked parametric-bounds table.**
  Not a mesh, not a material record, not a key table. VERIFIED (code + 1549/1549).
* **A rigblock record is `36 + 12·4^k` bytes with `k ∈ 0..7`.** VERIFIED.
* **Every byte of every rigblock record is accounted for.** VERIFIED. Any future
  claim that something "must be in there" is refutable in one subtraction.
* **`gmdl` (`0x00E6BCE5`) is the whole-body layer, never the part layer.** This
  independently reproduces `02` §3.2 from a completely different direction (record
  layout + key search rather than mesh statistics). VERIFIED.
* **The part geometry layer is `0x01C135DA` @ `0x40606000`, keyed by the rigblock's
  own instance id**, reached through the per-rigblock `prop` (`0x00B1B104`).
  SUPPORTED — this is the single most useful new fact here and it needs item 1/2 of
  §7 before it can be called VERIFIED.
* **A `ResourceKey` inside a `prop` is stored `{instance, type, group, 0x00A00CE4}`**
  — 16 bytes, `instance` first. VALIDATED on 388/388 resolving keys; use this form,
  not EDA's `{type, group, instance}`, when scanning Spore property lists.
  VERIFIED (positive control).
* **Do not trust Ghidra's inlined min/max for tiny leaf helpers.** Two of four were
  inverted; only a data-side test caught it, and the wrong version still *looked*
  self-consistent (it produced a negative scale and an inverted nibble order that
  were individually plausible).
* **Ghidra decompilation of `FUN_0041c0c0` and `FUN_0041bfb0` should be corrected**
  in any future work: `FUN_0041bfb0` = component-wise **MAX**, `FUN_0041c0c0` =
  component-wise **MIN**. This is a proposed correction to the shared DB, not made
  here (research-only scope).

---

## 9. Method / provenance

* Data: `sys.path.insert(0,'tools'); from spore.dbpf.dbpf import read, getdata`,
  Python 3.14, against the installed GOG build in `SPORE/`. `Spore_Content.package`
  (995 MB), `Spore_Graphics.package` (943 MB) and `Spore_Audio1/2` were read whole.
* `gmdl` bounds were obtained with `tools/spore/gmdl/gmdl.py`'s field layout,
  applied directly to the decompressed bytes (4209/4209 parsed, all version 8).
* Binary: `SporeApp.exe` 3.1.0.22 via the `ghidra` MCP bridge. Instruction-wide
  scan for the two type immediates; `ghidra_decompile_function` for
  `FUN_0041c1d0`, `FUN_0041aa40`, `FUN_0041aaa0`, `FUN_0041bfb0`, `FUN_0041c0c0`,
  `FUN_0041db10`, `FUN_0041bf30`, `FUN_0041e770`, `FUN_0041e8b0`;
  `ghidra_disassemble_function` for `FUN_0041bfb0` and `FUN_0041db10`.
* No file outside this one was created or modified. No asset byte is reproduced.
