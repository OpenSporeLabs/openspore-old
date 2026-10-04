# 09 — The `gmsh` record (`0x01C135DA`): a fully-accounted skinned mesh with an **anonymous, ordinal-only** bone list

**Agent:** GMSH · **Scope:** RESEARCH ONLY · **Date:** 2026-10-04
**Subject:** `Spore_Graphics.package`, group `0x40606000` (`EditorRigblocks`, per `02` §2.1),
1824 records, 107 708 474 decompressed bytes, sizes 206 B … 741 518 B (1254 distinct sizes).

**Verdict: RESOLVED (byte layout), with one deliberate negative that matters more than the
decoding: the record's bones are an anonymous ordered list.** Every record is
`24-byte header + 12·nv float3 positions + a typed block chain + a bone tail`, and the whole
of it is accounted for with **zero unaccounted bytes in 1823 / 1824 records** (one record is a
multi-section variant, §6). The single most important fact found is **negative and
falsifiable**: there is no bone name, no parent index, no external key anywhere in the record
or reachable from it. Skeletons here are *rigid* — each bone owns a **contiguous
`[start,end)` range of the triangle-index array**, and the 1824/1824 ranges tile that array
exactly — and bones are identified only by their position in a local list. **Spore's creature
"body graph" is therefore NOT recoverable from this layer.** §8.

Extends `08-rigblock-tail.md` §5 (which had only "24-byte header, then float3 positions, then a
trailing bone-index/weight block") and closes `08` §7 item 2. `08` §6 item 1's "decode
`0x01C135DA`" is now done; `08` §6 item 2 (`0x2F4E681B`) is untouched.

Every number below is a count produced by a command in this repository against the real data.
**No asset byte is reproduced**; the format is given as a field table and formulas.

---

## 0. Reproduction

Everything below is reproducible from a clean checkout with the game installed. Python 3.14.7.

### 0.1 Load the corpus

```python
import sys, struct, pickle
sys.path.insert(0, 'tools')                      # repo root on the path
from spore.dbpf.dbpf import read, getdata

data, items = read('SPORE/Data/Spore_Graphics.package')
gm = [it for it in items if it['type'] == 0x01C135DA]
recs = {(it['group'], it['inst']): getdata(data, it) for it in gm}
sub  = {k[1]: v for k, v in recs.items() if k[0] == 0x40606000}
# -> 1824 records; group histogram {0x40606000: 1824, 0x40606100: 595, 0x00000000: 3}
# -> no duplicate (group, instance) keys
```

Observed on this machine (reading the 943 MB package takes ≈10 s):

```
# Spore_Graphics.package magic=DBPF v3.0 idxCount=24707 idxSize=691804 idxOff=0x3828abd1 ...
gmsh items 2422 unique keys 2422 dups []
sub count 1824 insts unique 1824
total decompressed bytes 107708474
min 206 max 741518 distinct 1254
```

The other two groups (`0x40606100` ×595, `0x0` ×3) are **not** analysed here. That is an
unavailability, not a zero.

### 0.2 Learn the block vocabulary, then find the vertex array (self-bootstrapping)

The parser is *derived from the data*, not assumed. The recipe, in order:

1. Find the trailing `ff×8` (all 1824 records end that way) → the record ends with a
   `… u16BE v, FF FF FF FF FF FF FF FF` record (§4.4).
2. Learn candidate block ids/element sizes from the records that parse under a *guess*; then
   **grow** the id set until no record's chain stops early:

```python
IDS, ELS = seed_ids, {2,4,8,12,16,48}           # 43 ids from the 972 that parsed exactly
for round in range(6):                            # 2 rounds were needed
    new = { u32BE(d,off) at a chain stop point
            if elem in ELS and 0 < count and the next offset begins a known id or EOF }
    if not new: break
    IDS |= new
# -> round 0 learned exactly 3 more ids: 0x0C080300, 0x0D080300, 0x0E080300 (elem 12)
# -> final vocabulary: 46 ids, element sizes {2,4,8,12,16,48}
```

3. The **vertex-array end** is found without any assumption about the count: scan every
   4-aligned offset `o ≥ 0x18`, keep those where a *well-formed block* starts, and take the
   unique offset whose forward block chain is longest.

```python
def chain(d, o):
    n=len(d); cur=o; steps=[]
    while cur+10 <= n:
        i4 = u32be(d,cur); c = u32be(d,cur+4); e = u16be(d,cur+8)
        if i4 not in IDS or e not in ELS or c == 0 or cur+10+c*e > n: break
        steps.append((cur,i4,c,e)); cur += 10 + c*e
    return steps, cur

best = []
o = 0x18
while o + 10 <= len(d):
    if u32be(d,o) in IDS:
        c, e = u32be(d,o+4), u16be(d,o+8)
        if e in ELS and 0 < c and o+10+c*e <= len(d):
            best.append((len(chain(d,o)[0]), o))
    o += 4
mx   = max(b[0] for b in best)
tops = [b for b in best if b[0] == mx]
assert len(tops) == 1          # holds for 1824 / 1824 records
nv   = (tops[0][1] - 0x18) // 12
```

```
unique best: 1824    multi: 0    none: 0
```

4. **Independent confirmation of `nv`.** The header's 6th word carries it (§1):
   `nv == byteswap16(header[0x14] & 0xFFFF)` in **1824 / 1824** records — a check that shares
   no code path with the chain search.

### 0.3 The three falsifiable identities quoted throughout

```
nv  == byteswap16(u32le(d,0x14) & 0xffff)                        1824 / 1824
block-chain end  ==  tail start                                    1824 / 1824
len(d) == 24 + 12*nv + Σ_blocks(10 + count*elem) + 14 + 4*K1 + 34*nBones
        slack histogram: { 0 bytes : 1823, 130290 : 1 }           (§6)
```

---

## 1. The 24-byte header

`08` reported `{0x01000000, 0x01000000, 0x400, X, 3, Y}` as six LE `u32`. **Confirmed**, with
two corrections of interpretation and one field resolved.

| off | size | field | value / meaning | grade |
|---|---:|---|---|---|
| `0x00` | 4 | `u32` LE | `0x01000000` in **1824/1824** | VERIFIED (constant) |
| `0x04` | 4 | `u32` LE | `0x01000000` in **1823/1824**; one record has `0x02000000` | VERIFIED (near-constant; grade on the single exception: see §7) |
| `0x08` | 4 | `u32` LE | `0x00000400` in **1824/1824** | VERIFIED (constant) |
| `0x0C` | 4 | `u32` LE | **format/recipe version.** 16 distinct values; see §1.1 | VERIFIED (field); INFERRED (meaning) |
| `0x10` | 4 | `u32` LE | `3` in **1824/1824** | VERIFIED (constant; meaning UNKNOWN) |
| `0x14` | 4 | — | `{ u16 BE = nv, u8 0x00, u8 0x0C }` | VERIFIED |

**Field `0x14` is not one integer.** Read as a little-endian `u32` it looks like a single
opaque value (`0x0c00xxxx`, 827 distinct); read as **`u16` big-endian + two constant bytes**
it is the vertex count, and that reading holds in 1824/1824 records with
`byte[2] == 0x00` and `byte[3] == 0x0C` also 1824/1824. `nv` ranges 3 … 18 800
(median 482, sum 1 222 646), which fits `u16` and does not fit the "single LE word" reading.
Grade **VERIFIED**.

`0x08 = 0x400` and `0x10 = 3` were tested for correlation against record size, against `nv`,
and against the first block count: **no correlation survives** (constant by construction —
there is nothing to correlate). Their meaning is **UNKNOWN**. Grade
**VERIFIED (constant) / UNKNOWN (meaning)**.

### 1.1 What header word `0x0C` is

16 distinct values, all of the form `0x0001_yy00`:

| value | n | value | n | value | n | value | n |
|---|---:|---|---:|---|---:|---|---:|
| `0x00010300` | 50 | `0x00010800` | **892** | `0x00010E00` | 178 | `0x00011A00` | 17 |
| `0x00010400` | 90 | `0x00010900` | 1 | `0x00011100` | 170 | `0x00011D00` | 8 |
| `0x00010500` | 117 | `0x00010B00` | 68 | `0x00011400` | 92 | `0x00012000` | 5 |
| `0x00010600` | 90 | | | `0x00011700` | 34 | `0x00012300` | 1 |
| `0x00010700` | 11 | | | | | | |

**It is a format version, and it alone determines the block recipe.** The number of blocks in
the chain is a pure function of it — the cross-tabulation is exact, with **zero exceptions**:

| `0x0C` | records | blocks | | `0x0C` | records | blocks |
|---|---:|---:|---|---|---:|---:|
| `0x00010300` | 50 | 3 | | `0x00010E00` | 178 | 14 |
| `0x00010400` | 90 | 4 | | `0x00011100` | 170 | 17 |
| `0x00010500` | 117 | 5 | | `0x00011400` | 92 | 20 |
| `0x00010600` | 90 | 6 | | `0x00011700` | 34 | 23 |
| `0x00010700` | 11 | 7 | | `0x00011A00` | 17 | 26 |
| `0x00010800` | 892 | 8 | | `0x00011D00` | 8 | 29 |
| `0x00010900` | 1 | 9 | | `0x00012000` | 5 | 32 |
| `0x00010B00` | 68 | 11 | | `0x00012300` | 1 | 35 |

The block *sequence* is not fully determined by the version (1–6 variants per version, §3.2)
— only its length is. Grade **VERIFIED** (length), **SUPPORTED** (version semantics).

---

## 2. Block grammar

```
block := u32 BE  id            (46 distinct values, §3.1)
       | u32 BE  count         1 … 49734
       | u16 BE  elemSize      ∈ {2, 4, 8, 12, 16, 48}
       | count * elemSize bytes of payload
```

Grade **VERIFIED**. Evidence, all over 1824 records:

* the chain starting at `0x18 + 12·nv` lands exactly on the tail in **1824/1824**;
* every `count` equals `nv` for every *per-vertex* block, and no payload ever overruns the
  record;
* `1823/1824` records are consumed to the last byte by header + vertex array + blocks + tail
  (§6).

**Endianness is mixed and this is measured, not assumed.** The 24-byte header and the block
`id` read naturally little-endian; `count` and `elemSize` read **big-endian**. The
discriminator is `elemSize = 48` (`0x0030` on disk): LE would give 12 288 and the chain would
immediately overrun, BE gives 48 and it lands exactly. Grade **VERIFIED**.

> Note for future work: this is the AGENTS.md address/byte-order trap in a new place. A
> "just read it as `u32`" parser fails on every `elemSize ≥ 256` block and silently
> mis-sizes every `u32` field.

---

## 3. Block inventory

### 3.1 All 46 block ids, with occurrence counts over 1824 records

`count == nv` is marked ✓ (verified for every record carrying the block).

| id (BE) | elem | n | count | interpretation | grade |
|---|---:|---:|---|---|---|
| `0x00000001` | 2 | 1824 | varies | **triangle index list** (`u16` vertex indices) | VERIFIED |
| `0x15000608` | 4 | 1823 | = nBones | **bone table**; payload is literally `u32 LE 0,1,2,…,nBones-1` | VERIFIED |
| `0x14000608` | 4 | 1 | = nBones | same role, different id (one record) | SUPPORTED |
| `0x12000E05` | 48 | 1369 | varies | **bone bind matrices**, 12 floats each | SUPPORTED |
| `0x02000701` | 4 | 1232 | ✓ nv | packed bytes, **not** float (0/1232 plausible as float) | INFERRED (packed 4-byte attribute) |
| `0x03000702` | 4 | 1081 | ✓ nv | packed bytes, **not** float (0/1081) | INFERRED |
| `0x04000702` | 4 | 1 | ✓ nv | packed bytes | INFERRED |
| `0x08000202` | 8 | 1616 | ✓ nv | **2 floats/vertex** (1616/1616 plausible) | INFERRED (UV-like) |
| `0x09000700` | 4 | 987 | ✓ nv | packed bytes (959/987 implausible as float) | INFERRED |
| `0x09000800` | 4 | 161 | ✓ nv | mixed (150/161 implausible as float) | INFERRED |
| `0x09000900` | 8 | 221 | ✓ nv | mixed (221/221 implausible as float) | INFERRED |
| `0x0A000100` | 4 | 107 | ✓ nv | float (107/107 plausible) | INFERRED |
| `0x0A000200` | 8 | 54 | ✓ nv | float (54/54) | INFERRED |
| `0x0A000300` | 12 | 48 | ✓ nv | float (48/48) | INFERRED |
| `0x0A000400` | 16 | 173 | ✓ nv | 4 floats/vertex (173/173) | INFERRED |
| `0x0A000A00` | 4 | 987 | ✓ nv | packed bytes (987/987 implausible) | INFERRED |
| `0x02000301` | 12 | 581 | ✓ nv | **3 floats/vertex** (550/581 plausible) | INFERRED |
| `0x03000301` | 12 | 579 | ✓ nv | 3 floats (516/579) | INFERRED |
| `0x03000302` | 12 | 2 | ✓ nv | 3 floats (2/2) | INFERRED |
| `0x0C00_0300` … `0x0C07_0300` | 12 | 579→11 | ✓ nv | **float3 channels**, 8 generations | INFERRED |
| `0x0D00_0300` … `0x0D07_0300` | 12 | 579→11 | ✓ nv | float3 channels | INFERRED |
| `0x0E00_0300` … `0x0E07_0300` | 12 | 579→11 | ✓ nv | float3 channels | INFERRED |
| `0x0C080300`/`0x0D080300`/`0x0E080300` | 12 | 1 each | ✓ nv | 9th float3 generation (one record) | INFERRED |

Per-generation counts (each of `0x0C/0x0D/0x0E`): gen0 579, gen1 537, gen2 379, gen3 220,
gen4 87, gen5 43, gen6 20, gen7 11, gen8 1. The three families always appear as a matched
triple (`0x0C`/`0x0D`/`0x0E` with the same generation), 579/579/579 at generation 0 down to
1/1/1 at generation 8 — i.e. they are three parallel float3 attribute streams that grew one
generation at a time.

Element-size census: `12` ×6841, `4` ×6380, `8` ×1891, `2` ×1824, `48` ×1369, `16` ×173.

### 3.2 Vertex layout, stated exactly

There is **no interleaved vertex structure**: the vertex attribute is a **bare `float3`
position array** at `0x18`, stride 12, followed by *one block per attribute channel*. This is
the direct refutation of the working assumption in `08` §6 item 1 ("the vertex stream
interleaves other attributes"), and of the naive "all float3 from `0x18`" probe: the probe is
right about the first `nv` triples and wrong about everything after them, because the
attributes are *separate blocks*, not interleaved.

```
0x18                nv × { f32 x, f32 y, f32 z }      position, 12 B/vertex
0x18 + 12·nv        block[0] … block[m-1]            one block per channel (§3.1)
tail_start          bone tail (§4)
```

The bounding test that settles it: `nv` is derived independently from header word `0x14`
(§0.3) and equals `(chain_start − 0x18)/12` in **1824/1824** records. If the vertex region were
interleaved, its length would not be an exact multiple of 12 and could not equal a header
field. Grade **VERIFIED**.

Vertex-position sanity over the whole corpus (the reason the array is accepted as positions):
min/max per axis are O(0.1 … 10) and the median span is `[0.469, 0.555, 0.412]`; the median
centre is `[0.000, −0.094, 0.000]`, i.e. parts are modelled around the origin.

### 3.3 Triangle index list — VERIFIED

Block `0x00000001`, `elemSize = 2`, `count` = number of `u16` indices.

```
exactly one index block per record                      1824 / 1824
count % 3 == 0                                         1824 / 1824
max(index) < nv                                         1824 / 1824
min(index) == 0                                         1824 / 1824
total indices 4 106 853  (= 1 368 951 triangles)
indices per vertex: median 3.31, min 1.00, max 5.98
```

Grade **VERIFIED**. Note this corrects the `08` characterisation "trailing bone-index/weight
block": the big `u16` array is **not** bone data, it is the index buffer.

---

## 4. Bone data

### 4.1 Bone count

`nBones` = `count` of block `0x15000608` (or `0x14000608`).

| nBones | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 | 11 | 20 | 41 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| records | **973** | 337 | 206 | 153 | 55 | 57 | 18 | 11 | 7 | 4 | 1 | 1 | 1 |

**53.3 % of all rigblock meshes are bound to exactly one bone.** Grade **VERIFIED**.

### 4.2 The bone table is an identity list — so bone indices are LOCAL

Payload of `0x15000608`, `nBones × u32 LE`, measured on `0x00046BAE` (`nBones = 8`):

```
00 00 00 00 | 01 00 00 00 | 02 00 00 00 | 03 00 00 00 |
04 00 00 00 | 05 00 00 00 | 06 00 00 00 | 07 00 00 00
```

i.e. exactly `[0, 1, 2, …, nBones−1]`. Grade **VERIFIED**. Consequently every bone reference
in this record is a **small ordinal in `[0, nBones)`**, never a global or cross-part id. The
triangle indices are likewise local (all `< nv`).

### 4.3 Bone → triangle range table (this is the actual skinning model)

The tail carries `nBones` entries of **20 bytes**:

```
u32 LE   0            always 0                       (1824/1824)
u32 LE   start        first index owned by this bone
u32 LE   end          one past the last index owned by this bone
u32 LE   localBone    0, 1, …, nBones-1, in order     (1824/1824)
u32 LE   4            constant                       (1824/1824, all but the last entry)
```

Worked example, `0x00046BAE` (`nv = 671`, `nIndices = 2712`, `nBones = 8`):

| local bone | start | end | triangles |
|---:|---:|---:|---:|
| 3 | 0 | 1008 | 336 |
| 0 | 1008 | 1152 | 48 |
| 7 | 1152 | 1548 | 132 |
| 6 | 1548 | 1908 | 120 |
| 1 | 1908 | 2472 | 188 |
| 2 | 2472 | 2532 | 20 |
| 4 | 2532 | 2676 | 48 |
| 5 | 2676 | 2712 | 12 |

```
sorted [start,end) ranges == exact tiling of [0, nIndices)      1824 / 1824
boneIdx column == [0,1,…,nBones-1]                              1824 / 1824
leading pad field == 0                                          1824 / 1824
constant field == 4                                             1824 / 1824
```

**This is rigid skinning.** There are no per-vertex weights anywhere in the record: influence
is decided per *triangle*, and the constant `4` is the only hint of a weighting capacity that
the data never uses. Grade **VERIFIED** (structure and tiling), **SUPPORTED** (interpretation
as rigid skinning — no weight array exists to contradict it, and none was found: §8).

This is the load-bearing answer to `08` §7 item 2's "trailing bone-index/weight block": there
are **bone indices and no weights**.

### 4.4 Tail, in full

```
tailLen = 14 + 4*K1 + 34*nBones                              (zero-slack fit, §6)
```

| region | offset | size | content | grade |
|---|---|---:|---|---|
| T0 | `tail+0` | 4 | `u32 BE = nv` | VERIFIED (1824/1824) |
| T0 | `tail+4` | 2 | `u8 K1`, `u8 0` | VERIFIED |
| T0 payload | `tail+6` | `4·K1` | `K1 × { u8 index, u16 0xFFFF, u8 0 }`, `index == 0…K1−1` | VERIFIED (1824/1824 for both) |
| T1 | `tail+6+4K1` | 4 | `u32 BE = nBones` | VERIFIED (1824/1824) |
| T1 | `+4` | 2 | `u8 4`, `u8 0` | VERIFIED (1824/1824) |
| T1 payload | `+6` | 2 | two zero bytes (part of entry 0's pad) | VERIFIED |
| T1 table | `tail+14+4K1` | `20·nBones` | §4.3 range table | VERIFIED |
| T2 | `tail+14+4K1+20·nBones` | `14·nBones` | `nBones × { u32 BE localBone, u16 BE v, 0xFFFFFFFF ×2 }` | VERIFIED (1823/1824) |
| end | | | record ends exactly at `tail + tailLen` | VERIFIED (1823/1824) |

`K1` histogram (`u8`, 2 … 33):

```
2:50  3:90  4:207  5:11  6:886  7:7  9:36  10:32  12:126  13:52  15:107
16:63  18:70  19:22  21:22  22:12  24:11  25:6  27:3  28:5  30:5  33:1
```

`K1` is **not** `nBones`: the contingency table over 1824 records is

```
K1=6 , nBones=1  : 368      K1=6 , nBones!=1 : 518
K1!=6, nBones=1  : 605      K1!=6, nBones!=1: 333
```

T0's index list runs `0 … K1−1` with no reference to the bone table, so **T0 is a second,
independent ordinal index space whose referent is unknown**. Grade: structure **VERIFIED**,
referent **UNKNOWN** (§9).

T2's `v` field increases by exactly **2 per bone** and never resets within a record
(checked on the sampled and aggregated set; e.g. `(0,31)…(7,45)`, `(0,17)…(2,21)`). It is a
per-record constant base plus `2·bone`. It is **not** a name, an offset into the index array
(its values are far below `nIndices` in general and odd), or a matrix index. Grade
**VERIFIED** (the arithmetic progression), **UNKNOWN** (meaning).

### 4.5 Bone bind matrices — SUPPORTED, not VERIFIED

Block `0x12000E05`, `elemSize = 48` = 12 floats. Present in **1369/1824** records (455 have
none), **10 859** matrices in total. Read as a 3×4 row-major transform, the first three
entries of `0x00046BAE` are exact identities and entry 3 is
`(1,0,0,0 / 0,1,0,0 / 0,0,1,−0.5341)`.

```
3x3 part orthonormal to 1e-4 (rows unit + mutually perpendicular)   10859 / 10859
row3 == exactly (0,0,1,0)                                            2103 / 10859
full matrix == identity                                              417 / 10859
row3 == (0,0,0,1)                                                        0 / 10859
```

So the 12 floats are a **rigid transform — a 3×3 rotation plus a 3×1 translation**, with no
perspective row (the fourth component of the last stored vector is always `0`, never `1`).
The `row3` values that are not `(0,0,1,0)` are the other axis-aligned basis rows
(`(−1,0,0,0)`, `(0,0,−1,0)`, `(1,0,0,0)`, `(0,1,0,0)`), i.e. 90°/180° rotations — the
signature of a yaw/pitch/roll-assembled bone frame rather than a free quaternion.

Relation to `nBones`:

```
nMatrices > nBones  1204      nMatrices == nBones  100
nMatrices < nBones    65      no matrix block      455
nMatrices - nBones histogram: -5:1 -4:3 -3:7 -2:12 -1:42 0:100 1:135 2:113 3:170 4:123 5:83 6:105 …
```

`nMatrices` is **not** `nBones`; it is larger in most records and the difference is
unbounded (up to +8 and beyond in the tail). So these are **more** transforms than bones —
consistent with a per-bone bind pose plus per-bone extra frames, but that reading is
**INFERRED** and no field in the record maps a matrix to a bone. Grade **SUPPORTED**.

---

## 5. Correlation of `nv` with the two non-constant header words

The brief asked for correlations of the header's `X` and `Y` words against record size and
against the first following `u32`. Both questions are now closed, and one of them resolves a
mystery:

* **`Y` (word `0x14`)**: it *is* the vertex count (§1). Correlation with record size is
  therefore strong but not structural — size is `12·nv` plus per-channel `4…16 B` plus the
  tail, so `nv` explains most of the variance but the channel set varies. The exact statement
  is the size formula in §6, not a correlation.
* **`X` (word `0x0C`)**: **no** correlation with size (`nv` and `X` are independent: `X =
  0x00010800` spans 840 B … 264 416 B). Its exact correlation is with the **block sequence
  length**, a perfect one (§1.1). Correlation with the *first* following `u32` — which is the
  first block's `id` — is the same fact: the first block id is a function of the recipe, not of
  size.

---

## 6. Size algebra

Let `nv` = vertex count, `S` = the ordered block list, `nB` = `nBones`, `K1` = `u8` at
`tail+4`.

```
size = 24                                  header
     + 12 * nv                             positions
     + Σ_{(id,count,elem) ∈ S} (10 + count*elem)     every block: 10 B header + payload
     + 14 + 4*K1 + 34*nB                   tail (§4.4)

slack = size_measured − size_predicted
```

Result over all 1824 records:

```
slack histogram: { 0 : 1823 , 130290 : 1 }
```

**1823 / 1824 records are consumed to the last byte — zero unaccounted bytes**, matching the
standard `08` set for the rigblock record (1549/1549). There is no padding, no trailer, no
spare word anywhere.

The single exception is `instance 0x01DCAF31` (len 194 250, `nv = 1613`, `nBones = 1`), whose
chain and tail both terminate correctly but which carries **130 290 bytes of additional
structure between the tail's palette and its bone records**. Its tail ends with **five**
`0x…/u16/FF×8` records while `nBones = 1`. The most likely reading is a **multi-section
record** (a second mesh sharing one header), but that is **INFERRED**: the extra region was not
parsed. Everything else about that record (header, `nv`, chain start, palette) validates.

Second and third exceptions in an earlier iteration (`0x49D50DA5`, `0x81B00223`) were **not**
structural failures: they used block ids outside the first-pass vocabulary and resolved once
`0x0C080300` / `0x0D080300` / `0x0E080300` were learned (§0.2 step 2). After that, 1821/1823 →
1823/1823 → 1823/1824.

---

## 7. Relationship to the paired `0x2F4E681B` record, and to identity

The instance-id sets are identical (1824 each, re-measured), so the pairing holds. But the
`gmsh` record contains **no pointer to its partner and no self-identification at all**:

```
records containing the type id 0x01C135DA, LE or BE, any offset   0 / 1824
records containing their OWN instance id as LE u32, any offset     0 / 1824
records containing the group id 0x40606000 as LE u32, any offset   0 / 1824
```

So the link is **addressing-only** (the same `(type, group, instance)` triple names both), and
**nothing inside the `gmsh` bytes** corroborates it. This *strengthens* `08` §5's conclusion
that the per-rigblock `prop` (`0x00B1B104`) is the real pointer, and it means a `gmsh` decoded
in isolation cannot discover its `0x2F4E681B` sibling. Grade **VERIFIED** (negative).

---

## 8. Skeleton / bone identity — the important negative

**Nothing in this record says which bone is which.** Measured, over all 1824 records:

| searched for | scope | result |
|---|---|---|
| printable-ASCII runs ≥ 4 | whole record | 886 139 — **all artefacts of packed attribute bytes** (`0x30–0x39`, `0x61–0x7a`) |
| alphanumeric/`_`/`-` runs ≥ 11 | whole record | 527; longest is a 1027-char run of the literal 4-byte pattern `f333` repeated in one record's vertex data — **not text** |
| a `ResourceKey`-shaped 12-byte triple | whole record | 0 |
| a parent / hierarchy index | bone table `0x15000608`, tail range table, tail records | **none found**; the only non-local values are the `0xFFFFFFFF` sentinels in T2 (1823/1824) |
| a bone name table anywhere | whole record | **0** |
| any mapping from matrix (`0x12000E05`) to bone | whole record | **none** — `nMatrices ≠ nBones` and no index field links them |

**Therefore: a `parent index` array is not even present, let alone a semantic capability.**
What the layer provides is:

1. an **ordered** list of `nBones` bones, identified only by position `0 … nBones−1`;
2. a per-bone bind (or bind-plus-frames) transform block that is **not** indexable by bone;
3. a per-bone **contiguous triangle range** — the only functional bone↔geometry link;
4. an unexplained second ordinal palette `K1` with no referent.

**Consequence for the wider project.** Spore's creature *body graph* is **not** recoverable
from `0x01C135DA`. What *is* recoverable is the **deformation topology**: how many bones a
part has, their ordinal order, and exactly which triangles each one moves. That is enough to
re-skin and to diff poses, and it is not enough to name a bone ("head", "tail", "left leg") or
to walk a hierarchy. Naming must come from another layer — candidates, in order of promise:
the per-rigblock `prop` (`0x00B1B104`), the `0x2F4E681B` sibling, and a runtime observation of
the loader. `08` §6 items 2 and 3 stand unchanged.

---

## 9. Rigblock bounds cross-check (`08` §6 item 1, second half)

Rigblock `0x5CAA5E28` records were re-decoded from `Spore_Game.package` using `08`'s §2 layout
and §3.3's *interp B* nibble mapping (`lo = f[j] − qHi·s`, `hi = f[j+3] + qLo·s`), which gives
**0 / 1549** envelopes with `lo > hi` — the positive control that the decoder is oriented
correctly. **1316** instance ids are shared with the `gmsh` layer.

| test | pairs | result |
|---|---:|---|
| rigblock envelope == `gmsh` vertex AABB, per-axis, `< 1e-4` | 1316 | **45** |
| per-axis span ratio `gmsh/rig` (median; IQR) | 1316 | x **1.127** (0.949–1.667); y **1.251** (1.000–1.744); z **1.150** (0.910–1.757) |
| single isotropic scale + free translation, 6 residuals | 1316 | median max-residual **0.0465**; only **10.0 %** below `1e-3`, 21.8 % below `1e-2`; fitted `s` median 1.218, range 3.7e-31 … 21.98 |
| per-axis scale + translation | 1316 | max residual 8.9e-16 for **all** — **this test is vacuous** (2 free parameters per axis for 2 residuals per axis) and is reported only so nobody mistakes it for confirmation |
| median span | 1316 | `gmsh` `[0.469, 0.555, 0.412]`, rigblock `[0.342, 0.366, 0.281]` |
| median centre | 1316 | `gmsh` `[0.000, −0.094, 0.000]`, rigblock `[0.000, −0.058, 0.000]` |

**Interpretation.** The two envelopes are *consistent* — same order of magnitude, both centred
on the origin, and the `gmsh` hull is uniformly **larger** by a factor 1.13–1.25, which is the
expected direction (a vertex hull strictly contains a quantised cell lattice, and `08`'s
lattice is reconstructed to ±`s/2`). But there is **no transformation that makes them equal**,
so this test **does not confirm** "these are the same part" and I decline to call it a pass.
Grade: **SUPPORTED** (same part, same frame, hull containment) / **REFUTED** (exact or
scale-translation equivalence).

The strong version of this test is *not available from the data alone* and would need the
identity link from §7 — i.e. the per-rigblock `prop` — plus a runtime trace to know whether
the engine renders `gmsh` with the rigblock's own lattice or with a decimated one.

---

## 10. Negative results

| # | hypothesis | what was searched | scope | result |
|---|---|---|---|---|
| 1 | the vertex stream is **interleaved** (the `08` §6 working assumption, and the reason its "all float3 from `0x18`" probe was invalid) | is `(chain_start − 0x18) % 12 == 0`, and does the chain start equal `0x18 + 12·nv` for the header's `nv`? | 1824 | **REFUTED** — bare `float3` array, 1824/1824 |
| 2 | attributes are **interleaved per vertex** rather than split into blocks | do any two attribute blocks share an offset, or does any block's count differ from `nv`? | 1824, all blocks | **REFUTED** — every per-vertex block has `count == nv` and a distinct offset |
| 3 | the big `u16` array is **bone** data | `count % 3`, `max < nv`, `min == 0` | 1824 | **REFUTED** — it is the triangle index list, 1824/1824 on all three |
| 4 | per-vertex **skin weights** exist | scan every block for a channel that is neither a position, nor `u16` indices, nor the bone table; look for float channels summing to 1 per vertex | 1824, 46 ids | **NOT FOUND** — the only per-bone data is a triangle range; no weight array exists |
| 5 | bone indices are **global** / cross-referenced | `max(index)` over the `u16` array vs `nBones` | 1824 | **REFUTED** — the array holds vertex indices `< nv`; the bone table holds `0…nBones−1` |
| 6 | the record names its bones | ASCII / alphanumeric runs, `ResourceKey` triples, own instance id, own group id | 1824 | **REFUTED (negative)** — 0 hits, §8 |
| 7 | the `0x…` header word `0x08 = 0x400` is a size/offset | correlation with `size`, `nv`, first block count | 1824 | **UNRESOLVED** — constant, so nothing to correlate; meaning UNKNOWN |
| 8 | header word `0x14` is one opaque LE integer | re-read as `u16 BE` + 2 constant bytes and compared to `nv` | 1824 | **REFUTED** — it is `nv` (1824/1824) |
| 9 | header words `0x00`/`0x04` are perfectly constant | re-count | 1824 | `0x00` 1824/1824; `0x04` **1823/1824** (one record `0x02000000`) — a real, unexplained exception |
| 10 | `K1` is the bone count | contingency table over 1824 | 1824 | **REFUTED** — `K1 = 6` with `nBones = 1` in 368 records, `K1 ≠ 6` with `nBones = 1` in 605; `K1` up to 33 |
| 11 | `0x12000E05` count == `nBones` | compare | 1824 | **REFUTED** — `>` in 1204, `<` in 65, absent in 455 |
| 12 | block ids / elem sizes beyond the first-pass vocabulary | iterative vocabulary growth | 1824 | 3 further ids found and folded in (`0x0C/0D/0E080300`); the growth then converged in 1 extra round |

---

## 11. UNKNOWN table

| # | open question | grade | the specific evidence that would settle it |
|---|---|---|---|
| 1 | semantics of the 8 attribute channels (`0x02000701`, `0x03000702`, `0x08000202`, `0x0900070x`, `0x0A000A00`, `0x0A0001–400`, `0x02000301`, `0x03000301`, `0x0C/0D/0E·0300`) | UNKNOWN (float-vs-packed known; names not) | the shader/`VertexFormat` that consumes them, or `gmdl`'s equivalent channel table for a matched part |
| 2 | what T0's `K1`-entry index space (`0…K1−1`, `K1` ≠ `nBones`) refers to | UNKNOWN | the renderer code that reads T0; it is the only unexplained ordinal space in the record |
| 3 | meaning of T2's `u16 v` (constant base + `2·bone`) | UNKNOWN | same; one 32-bit decompile of the tail reader |
| 4 | why `nMatrices` (`0x12000E05`) exceeds `nBones` and how matrices map to bones | UNKNOWN | the writer; or a record where `nMatrices == nBones` **and** a known pose, to fix the correspondence |
| 5 | whether the skinning is *exactly* rigid or a 4-influence blend that happens to be contiguous here | SUPPORTED (rigid) / UNKNOWN (why the constant `4`) | a mesh with `nBones ≥ 2` whose triangles are **not** contiguously owned — searched for, none exists in 1824 |
| 6 | semantic meaning of header words `0x08` (`0x400`) and `0x10` (`3`) | UNKNOWN | the writer; or a version sweep where either changes |
| 7 | the single record `0x01DCAF31` with 130 290 unexplained bytes | UNKNOWN | parse the extra region; it most likely holds a second mesh section |
| 8 | the one record whose header word `0x04` is `0x02000000` | UNKNOWN | identify it and diff it against a sibling with `0x01000000` |
| 9 | groups `0x40606100` (595) and `0x0` (3) | **NOT LOOKED AT** | run §0.2 on them; whether they share this layout is an availability question, not a negative |
| 10 | whether any part-level channel is shared between this layer and `gmdl` (`0x00E6BCE5`) | UNKNOWN | `08` §4.1 showed the instance-id spaces are disjoint; a channel-name comparison needs `gmdl` decoded |

---

## 12. Field table (consolidated)

Byte offsets are relative to the record start. `tail` = `chain_end`. "BE" fields are marked;
everything else in the header and the block `id` is LE.

| offset | size | field | grade |
|---|---:|---|---|
| `0x00` | 4 | `u32` = `0x01000000` | VERIFIED |
| `0x04` | 4 | `u32` = `0x01000000` (1 exception) | VERIFIED / exception UNKNOWN |
| `0x08` | 4 | `u32` = `0x00000400`, meaning unknown | VERIFIED (value) / UNKNOWN (meaning) |
| `0x0C` | 4 | `u32` format version, 16 values; determines the block recipe length | VERIFIED |
| `0x10` | 4 | `u32` = `3`, meaning unknown | VERIFIED (value) / UNKNOWN (meaning) |
| `0x14` | 2 | `u16 BE` = `nv` | VERIFIED (1824/1824) |
| `0x16` | 1 | `0x00` | VERIFIED (1824/1824) |
| `0x17` | 1 | `0x0C` | VERIFIED (1824/1824) |
| `0x18` | `12·nv` | `nv` × `float3` vertex positions | VERIFIED |
| `chain` | 10 per block | `u32 BE id`, `u32 BE count`, `u16 BE elemSize` | VERIFIED |
| … | `count·elemSize` | channel payloads, §3.1 | VERIFIED (structure) / INFERRED (meanings) |
| `tail+0` | 4 | `u32 BE` = `nv` | VERIFIED |
| `tail+4` | 2 | `u8 K1`, `u8 0` | VERIFIED |
| `tail+6` | `4·K1` | `K1 × { u8 idx, `u16` 0xFFFF, `u8` 0 }`, `idx = 0…K1−1` | VERIFIED |
| `tail+6+4K1` | 4 | `u32 BE` = `nBones` | VERIFIED |
| `tail+10+4K1` | 2 | `u8 4`, `u8 0` | VERIFIED |
| `tail+14+4K1` | `20·nBones` | bone → triangle-range table (§4.3) | VERIFIED |
| `tail+14+4K1+20·nB` | `14·nBones` | `{ u32 BE localBone, `u16 BE` v, `0xFFFFFFFF`×2 }` | VERIFIED (1823/1824) |

---

## 13. Claims to carry forward

* **`0x01C135DA` = a skinned, indexed triangle mesh with an anonymous ordinal bone list.**
  `24 + 12·nv + Σ(10 + count·elem) + 14 + 4·K1 + 34·nBones`, **zero slack in 1823/1824**.
  VERIFIED.
* **Header word `0x14` is `{u16 BE nv, 0x00, 0x0C}` — the vertex count, twice over.**
  VERIFIED (1824/1824, and it independently agrees with the chain-derived `nv`).
* **Header word `0x0C` is a format version whose value alone fixes the number of blocks**
  (3 … 35, one per version, zero exceptions). VERIFIED.
* **Block headers are `{u32 BE id, u32 BE count, u16 BE elemSize}` — big-endian counts inside
  a little-endian file.** VERIFIED. This is the single most reusable fact here.
* **Vertex attributes are separate per-channel blocks, never interleaved.** VERIFIED; this
  retires the assumption that invalidated `08`'s probe.
* **`0x00000001` is the triangle index list, not bone data.** VERIFIED.
* **Skinning is rigid and per-triangle**: `nBones` contiguous `[start,end)` index ranges that
  tile `[0, nIndices)` exactly, 1824/1824. VERIFIED. **53.3 % of meshes have one bone.**
* **Bone identity is ordinal only. No names, no parents, no keys.** VERIFIED (negative).
  The creature body graph is **not** recoverable here; the deformation topology is.
* **`nMatrices` (48-byte transforms) ≠ `nBones` and is unindexed.** SUPPORTED.
* **The rigblock-envelope cross-check is consistent but not exact**: hull containment with a
  1.13–1.25 span ratio, 45/1316 exact matches, median max-residual 0.047 under a single
  isotropic scale. SUPPORTED / exact-equivalence REFUTED.
* **Do not use per-axis scale+translation as a bounds test** — 2 free parameters per axis for
  2 residuals per axis is an identity. Recorded so it is not mistaken for a pass later.
* **Trap, new instance of the byte-order family:** the block section is big-endian while the
  header, the `float3` positions and the bone table's `u32` payloads are little-endian. Any
  single-endianness parser of this record is wrong somewhere.

---

## 14. Method / provenance

* **Data.** `SPORE/Data/Spore_Graphics.package` (942 881 325 B, DBPF v3.0, idxCount 24 707),
  read whole with `sys.path.insert(0,'tools'); from spore.dbpf.dbpf import read, getdata`;
  0 decompression failures on the 1824 records. `SPORE/Data/Spore_Game.package`
  (20 210 108 B, idxCount 11 529) for the 1549 rigblock records of §9.
* **Python** 3.14.7, stdlib only (`struct`, `pickle`, `collections`, `statistics`). No
  external package, no network. GOG digital build, no DRM.
* **Binary (`SporeApp.exe` 3.1.0.22, image base `0x00400000`, via the `ghidra` MCP bridge).**
  Used only for orientation, **not** for any field in §12. Full instruction-wide scan
  (4 671 374 instructions) for the scalar `0x01C135DA`: **9 hits** —
  `FUN_004021A0 @0x4021a0`, `FUN_00404660 @0x404660`, `FUN_0072FFF0 @0x72fff0`,
  `FUN_007666A0`, `MTDISC_00769200`, `FUN_00769D80`, `FUN_0076A720`, `FUN_01002EE0`,
  `FUN_0100A960`. Decompiled `FUN_004021A0`, `FUN_00404660`, `FUN_00404430`, `FUN_00404460`,
  `FUN_007666A0`, `FUN_00769D80`, `FUN_0076A720`, `MTDISC_00769200`, `FUN_00767C80`,
  `FUN_00764F60`, `FUN_00763100`, `FUN_0076A4D0`, `FUN_01002EE0`, `FUN_0100A960`.
  Two useful facts, both **corroborative only**: `FUN_004021A0` loads type `0x2F4E681B` and
  then `0x1C135DA` for any group with `(g>>30)==1 && (g>>16 & 0xFF)==0x60 && (g>>24 & 0x1F)==0`
  — i.e. exactly `0x40606000` — confirming the §7 pairing from the code side; and
  `FUN_007666A0` allocates a 44-byte `"RenderAsset"` for `0x1C135DA` versus 320 bytes for
  `0x00E6BCE5`, confirming `08` §8's "`gmsh` is a mesh *section*, not a `gmdl`". **The
  on-disk reader for these bytes was not located in the binary**, which is why §12 rests
  entirely on data-side proof.
* **Corpus cross-check.** Every count in this document was taken over the full 1824 records at
  group `0x40606000`. Where a claim is narrower, the narrower count is quoted.
* **No file outside this one was created or modified.** No asset byte is reproduced.

> **CORRECTION (2026-10-04, from the Rust decoder in `spore-rigblock::gmsh`).**
> Two statements in §4.3 are wrong as written, and both would have produced a
> decoder that rejects real records:
>
> 1. **"sorted `[start,end)` ranges == exact tiling of `[0, nIndices)` — 1824/1824"
>    is only true after sorting.** The table is stored **unsorted in 599 of 1824
>    records**; 1225 happen to be ascending. Validated in file order the tiling
>    check fails on a third of the corpus. Re-measured: tiling holds 1824/1824
>    once ordered by `start`.
> 2. **"constant field == 4 — 1824/1824 (all but the last entry)" is false.** The
>    fifth word is `4` in most entries but `0x02000000` and `0x03000000` occur too.
>    It is now carried verbatim as `BoneRange::tag` and claimed nothing.
>
> What stands: the `bone` column **is** the entry index (`0…nBones` in order,
> 1824/1824), the pad word is 0, the ranges tile `[0, nIndices)` once sorted, and
> there are no weights. §2's endianness claim needed one word corrected too: the
> block `id` is big-endian like `count` and `elemSize`, not little-endian as §1's
> opening implies — read LE the ids come out as consecutive offsets and the index
> block never matches.
>
> The layout, the byte accounting and every §3 census figure reproduced exactly in
> Rust: 1 222 646 vertices, 1 368 951 triangles, 973 single-bone, 16 recipes,
> 46 channel ids, 18 800 max vertices.
