# 10 — `0x2F4E681B` at `0x40606000`: the paired container is a RenderWare 4 **MODEL**, and it carries skeletons with parentage

**Agent:** RW4PAIRED · **Scope:** RESEARCH ONLY · **Date:** 2026-10-04
**Subject:** `SPORE/Data/Spore_Graphics.package`, type `0x2F4E681B` (`rw4`), group
`0x40606000` (`EditorRigblocks`), **1824 records**, 258 511 636 B decompressed.

**Verdict: RESOLVED (container + section directory + every section type named);
the payloads of the mesh/raster families remain out of scope by design.**

**The single most important fact: all 1824 records are ordinary RenderWare 4 MODEL
containers, and `SporeApp.exe` itself ships an 87-entry `{class-name, type-code}`
registry that names 16 of the 20 section types they contain — including the one
report 08 listed as UNKNOWN, `0x7000b` = `OBJECTTYPE_SKELETONSINK`.** A
`Skeleton (0x70002)` section is present in **1568/1824** records (**6270**
sections) and its layout is fully recovered: three parallel `u32[N]` arrays of
which the third is a **verified bone parent array** (every value in `[-1, N-1]`,
exactly one `-1` root per skeleton, every parent chain acyclic, 6270/6270).

Three claims carried in from report 08 §5 are **REFUTED with counts**:

| report 08 §5 claim | measurement |
|---|---|
| "binary/**text** container (ASCII fragments, CRLF, embedded NULs)" | the only CRLF in the whole family is the `0D 0A` **at offset 8 of the 28-byte magic**, 1824/1824. The only ASCII ≥ 6 outside section payloads is the literal `RW4w32`, 1824/1824. The only ASCII ≥ 4 *inside* payloads is `DXT1`/`DXT5`/`DXT3` (3 374 runs, 3 distinct). **0** shader sources, **0** scripts, **0** identifiers, **0** text blocks. |
| "its head is a mix of ASCII and binary" | every record begins with the 28-byte `RW4w32` magic followed by a `0x1C` file-type word. **0/1824** deviate. |
| sizes `206 B … 741 518 B` | that is the *paired mesh* type. These records are **1 856 B … 2 926 344 B** (median 91 340). |

And report 08 §7 item 3 ("identity and semantics of type `0x2F4E681B`") is now
**answered**: it is the **renderable, animatable part mesh** that sits 1:1 with
the rigblock's `0x01C135DA` collision/lower layer — **not** a behaviour or script
record. There is no script in it.

---

## 0. Reproduction

Everything below comes from these two snippets plus the throwaway walker in
`/tmp/opencode/rw4a/rw4walk.py` (a transcription of `crates/spore-rw4/src/lib.rs`;
**cross-checked byte-for-byte against the repository's independent oracle**
`tools/spore/rw4/rw4.py::describe` on **1824/1824** records, see §7).

```python
import sys, struct, collections
sys.path.insert(0, 'tools')
from spore.dbpf.dbpf import read, getdata

data, items = read('SPORE/Data/Spore_Graphics.package')
recs = [(it['inst'], getdata(data, it)) for it in items
        if it['type'] == 0x2F4E681B and it['group'] == 0x40606000]
# -> 1824 records, 258 511 636 B
```

```python
# section directory, transcribed from crates/spore-rw4/src/lib.rs
MAGIC = bytes([0x89,0x52,0x57,0x34,0x77,0x33,0x32,0x00,0x0D,0x0A,0x1A,0x0A,
               0x00,0x20,0x04,0x00,0x34,0x35,0x34,0x00,0x30,0x30,0x30,0x00,
               0x00,0x00,0x00,0x00])

def parse(b):
    assert b[:28] == MAGIC
    file_type   = u32(b, 0x1C); section_count = u32(b, 0x24)
    sec_info    = u32(b, 0x30); buffer_ptr   = u32(b, 0x44)
    o1          = u32(b, 0x98 + 0x0C)          # MANIFEST-RELATIVE
    types       = u32(b, 0x98 + o1 + 4)        # n_types, block is at 0x98+o1
    tcs         = [u32(b, 0x98 + o1 + 8 + 4*i) for i in range(types)]
    rows = []
    for i in range(section_count):
        r = sec_info + 24*i
        dp, _u4, sz, al, tci, tc = struct.unpack_from('<IIIiIi', b, r)
        rows.append(dict(dp=dp, size=sz, align=al, tci=tci, tc=tc,
                         addr=dp + buffer_ptr if tc == 0x10030 else dp))
    return dict(ft=file_type, sc=section_count, bp=buffer_ptr,
                bs=u32(b, 0x4C), tcs=tcs, rows=rows, n=len(b))
```

The section-type-name registry (§3) is read straight out of the shipped binary:

```python
# SporeApp.exe 3.1.0.22, image base 0x00400000
# table of 87 {u32 namePtr, u32 typeCode} entries at 0x015d0370 .. 0x015d0620
for v in range(0x015d0370, 0x015d0628, 8):
    p, tc = struct.unpack('<II', rd(v, 8))
    print('%#010x  %s' % (tc, cstr(p)))
```

---

## 1. Are these RW4 containers? — YES, 1824/1824, no exceptions

| test | result |
|---|---|
| first 28 bytes `==` `RW4w32` magic byte-for-byte | **1824 / 1824** |
| parses under the `crates/spore-rw4` rules (all 9 structural checks) | **1824 / 1824**, **0** failures |
| `is_complete()` (rows fit, sizes ≥ 0, `addr+size ≤ len`) | **1824 / 1824** |
| `file_type` (`0x1C`) | **`1` = MODEL, 1824 / 1824** — *not* the `0xCAFED00D` "Special" of `Spore_Content` |
| `object_count` (`0x20`) `==` `section_count` (`0x24`) | **1824 / 1824** (the crate records but never validates this; here it always holds) |
| `buffer_pointer + buffer_size == record_size` | **1824 / 1824** |
| `unknown_28` / `unknown_2c` / `unknown_34` / `unknown_48` | `16` / `0` / `152` / `16`, each **1824/1824** constant |
| `manifest o1` (`0x98+0x0C`) | `0x1C` for every record (so `SectionTypes` always sits at `0xB4`) |
| negative section sizes | **0** |
| `unknown_04` on every row | `0` (51 904 / 51 904) |
| row `align` values | only `4` and `16` |

Record sizes: min **1 856**, p25 43 984, median **91 340**, p75 173 552,
max **2 926 344**, total **258 511 636**. `section_count` 10 … 150.
Section payloads cover **256 467 036 / 258 511 636 = 99.21 %** of all bytes; the
2 044 600 uncovered bytes are header + manifest + type table + row table +
alignment padding.

### 1.1 A caveat on `type_code_index` (new, falsifiable, 51 904/51 904)

Every record's `SectionTypes` block begins with the two codes
**`0x0000000c`, `0x00000000`** (slots 0 and 1) before the real codes.
Slot 0 is used by **no** row; slot 1 is used by **every** `BaseResource` row
(6 490/6 490) — and `type_codes[1] == 0`, not `0x10030`. So
`type_code_index` validates against the table's *length* (as `spore-rw4`
requires) but **does not resolve to the row's own type code**. Anyone using it as
a type resolver will read `0x00000000` for every base resource. Grade
**VERIFIED (negative)**.

---

## 2. Section census — 20 type codes, 51 904 sections

Over all 1824 records:

| type code | exe name (§3) | sections | records containing | size range (B) | align | payload bytes | % of all bytes |
|---|---|---:|---:|---|---:|---:|---:|
| `0x00010030` | `RWOBJECTTYPE_BASERESOURCE` | 6 490 | 1 824 | 6 … 699 088 | 4 | 84 071 014 | 32.52 % |
| `0x00070002` | `OBJECTTYPE_SKELETON` | 6 270 | 1 568 | 36 … 324 | 4 | 339 636 | 0.13 % |
| `0x00070001` | `OBJECTTYPE_KEYFRAMEANIM` | 4 613 | 1 564 | 80 … 163 008 | 16 | 19 288 976 | 7.46 % |
| `0x00020009` | `RWGOBJECTTYPE_MESH` | 3 887 | 1 824 | 40 (fixed) | 4 | 155 480 | 0.06 % |
| `0x0002000b` | `RWGOBJECTTYPE_COMPILEDSTATE` | 3 887 | 1 824 | 148 … 533 | 16 | 1 699 666 | 0.66 % |
| `0x0002001a` | `RWGOBJECTTYPE_MESHCOMPILEDSTATELINK` | 3 887 | 1 824 | 12 (fixed) | 4 | 46 644 | 0.02 % |
| `0x00ff0000` | *(no registry name)* `MorphHandle`† | 3 435 | 1 383 | 64 (fixed) | 4 | 219 840 | 0.09 % |
| `0x00020003` | `RWGOBJECTTYPE_RASTER` | 3 419 | 1 596 | 32 (fixed) | 4 | 109 408 | 0.04 % |
| `0x00080005` | `RWCOBJECTTYPE_BBOX` | 1 825 | 1 824 | 32 (fixed) | 16 | 58 400 | 0.02 % |
| `0x00080003` | `RWCOBJECTTYPE_TRIANGLEKDTREEPROCEDURAL` | 1 825 | 1 824 | 208 … 636 400 | 16 | 51 966 896 | 20.10 % |
| `0x00020004` | `RWGOBJECTTYPE_VDES` | 1 825 | 1 824 | 48 … 108 (6 sizes) | 4 | 150 108 | 0.06 % |
| `0x00020007` | `RWGOBJECTTYPE_IBUF` | 1 825 | 1 824 | 28 (fixed) | 4 | 51 100 | 0.02 % |
| `0x0007000b` | **`OBJECTTYPE_SKELETONSINK`** | 1 568 | 1 568 | 32 (fixed) | 4 | 50 176 | 0.02 % |
| `0x00070003` | `OBJECTTYPE_ANIMATIONSKIN` | 1 369 | 1 369 | 80 … 1 616 | 16 | 716 880 | 0.28 % |
| `0x0007000f` | `OBJECTTYPE_SKINMATRIXBUFFER` | 1 369 | 1 369 | 64 … 1 216 | 16 | 543 136 | 0.21 % |
| `0x0007000c` | **`OBJECTTYPE_SKINSINK`** | 1 369 | 1 369 | 20 (fixed) | 4 | 27 380 | 0.01 % |
| `0x00020005` | `RWGOBJECTTYPE_VBUF` | 1 246 | 1 245 | 28 (fixed) | 4 | 34 888 | 0.01 % |
| `0x00ff0001` | *(no registry name)* `Animations`† | 637 | 637 | 16 … 56 | 4 | 14 520 | 0.01 % |
| `0x000200af` | *(no registry name)* `BlendShapeBuffer`† | 579 | 579 | 5 504 … 961 684 | 16 | 96 891 660 | 37.48 % |
| `0x00ff0002` | *(no registry name)* `BlendShape`† | 579 | 579 | 36 … 100 | 4 | 31 228 | 0.01 % |

† name from the C++/SMFX table only; the exe's own registry has **no entry** for
`0x200af`, `0xff0000`, `0xff0001`, `0xff0002` — see §3.

### 2.1 Comparison with `docs/RENDERWARE-RESEARCH.md` §11

§11 lists seven ids (`0x70001`, `0x70002`, `0x70003`, `0xff0002`, `0x200af`,
`0xff0000`, `0xff0001`). **All seven occur here.** The census also finds
**13 further codes** that §11 does not list:

`0x10030` BaseResource, `0x20003` Raster, `0x20004` VDES, `0x20005` VBUF,
`0x20007` IBUF, `0x20009` Mesh, `0x2000b` CompiledState, `0x2001a`
MeshCompiledStateLink, `0x7000b` SkeletonSink, `0x7000c` SkinSink, `0x7000f`
SkinMatrixBuffer, `0x80003` TriangleKDTreeProcedural, `0x80005` BBox.

Of those 13, four are **absent from the C++ known set** in
`crates/spore-rw4/src/type_codes.rs`: `0x20004` (1 825 sections),
`0x20005` (1 246), `0x7000c` (1 369), `0x7000f` (1 369). §3 names all four.

### 2.2 The `type_codes` table length

`n_types` per record: `13:110  14:146  16:1  18:5  19:196  20:93  21:624  22:481  23:168`.
Slots 0 and 1 are always `(0x0000000c, 0x00000000)` (§1.1).

---

## 3. The section types are named by the executable itself

`SporeApp.exe` 3.1.0.22 contains a **contiguous 87-entry table of
`{u32 namePtr, u32 typeCode}` at `0x015d0370 … 0x015d0620`** in `.rdata`
(`.rdata` VA base `0x00FCC000`). Each `namePtr` is a NUL-terminated
`OBJECTTYPE_*` / `RWOBJECTTYPE_*` / `RWGOBJECTTYPE_*` / `RWCOBJECTTYPE_*`
string. Grade **VERIFIED** — read from the shipped binary, no inference.

Entries matching this census (16 of 20):

| type code | string at the registry | table address |
|---|---|---|
| `0x00010030` | `RWOBJECTTYPE_BASERESOURCE_START` | `0x015d0400` |
| `0x00020003` | `RWGOBJECTTYPE_RASTER` | `0x015d04f0` |
| `0x00020004` | `RWGOBJECTTYPE_VDES` | `0x015d04f8` |
| `0x00020005` | `RWGOBJECTTYPE_VBUF` | `0x015d0500` |
| `0x00020007` | `RWGOBJECTTYPE_IBUF` | `0x015d0510` |
| `0x00020009` | `RWGOBJECTTYPE_MESH` | `0x015d0520` |
| `0x0002000b` | **`RWGOBJECTTYPE_COMPILEDSTATE`** | `0x015d0530` |
| `0x0002001a` | `RWGOBJECTTYPE_MESHCOMPILEDSTATELINK` | `0x015d05a8` |
| `0x00070001` | `OBJECTTYPE_KEYFRAMEANIM` | `0x015d0418` |
| `0x00070002` | `OBJECTTYPE_SKELETON` | `0x015d0420` |
| `0x00070003` | `OBJECTTYPE_ANIMATIONSKIN` | `0x015d0428` |
| `0x0007000b` | **`OBJECTTYPE_SKELETONSINK`** | `0x015d0468` |
| `0x0007000c` | **`OBJECTTYPE_SKINSINK`** | `0x015d0470` |
| `0x0007000f` | **`OBJECTTYPE_SKINMATRIXBUFFER`** | `0x015d0488` |
| `0x00080003` | `RWCOBJECTTYPE_TRIANGLEKDTREEPROCEDURAL` | `0x015d04b8` |
| `0x00080005` | `RWCOBJECTTYPE_BBOX` | `0x015d04c8` |

Two corrections fall out of this, both **VERIFIED**:

1. `docs/RENDERWARE-RESEARCH.md` §7.5 and `crates/spore-rw4/src/type_codes.rs`
   record `0x2000b` as *"(Spore-specific, unmapped)"*. It is
   **`RWGOBJECTTYPE_COMPILEDSTATE`**. (It is still deliberately unnamed in the
   crate, which is correct — the crate reproduces the C++ table, not the exe.)
2. `docs/RENDERWARE-RESEARCH.md` §11 calls `0x7000b` an "undumped Spore-specific
   section" and guesses "candidate: per-frame blend-shape weights".
   It is **`OBJECTTYPE_SKELETONSINK`**, a 32-byte fixed-size record (§5). The
   guess is **REFUTED**.

Also in the registry but **not** present in any of the 1824 records (so they
cannot be what is being asked about here): `OBJECTTYPE_INTERPOLATOR`,
`FEATHERINTERPOLATOR`, `ONEBONEIK`, `TWOBONEIK`, `BLENDER`, `WEIGHTEDBLENDER`,
`REMAPPER`, `LIGHTSINK`, `CAMERASINK`, `TWEAKCONTROLLER`, `SHADERSINK`,
`OBJECTTYPE_PART`, `PARTDEFINITION`, `JOINTEDPAIR`, `JOINTLIMIT`,
`JOINTSKELETON`, `ASSEMBLY`, `ASSEMBLYDEFINITION`, `RWGOBJECTTYPE_SHADER`,
`PIXELSHADER`, `VERTEXSHADER`, `TEXTUREOVERRIDE`-adjacent `RASTERDATA`, etc.
Grade **VERIFIED (negative)**.

### 3.1 A second, name-less factory table

A **separate** cluster of tiny `push <typecode>; call alloc; mov [eax+slot], fn`
stubs exists at `0x007c56e0`–`0x007c57d0` for `0x00ff0000`, `0x00ff0001`,
`0x00ff0002` and at `0x01200a70`–`0x012010d0` for `0x70003`, `0x7000b`,
`0x7000c`, `0x7000f`. Each installs 1–3 virtual-function pointers into the fresh
object. This proves the morph/blend family and the sink family are real
first-class Spore serialisable classes; it supplies **no names**. Grade
**VERIFIED (structure) / UNKNOWN (identity)**.

---

## 4. `Skeleton (0x70002)` — layout fully recovered, parentage VERIFIED

**6 270 sections in 1 568 records.** Per-record counts:
`0:256  1:3  2:235  3:357  4:433  5:329  6:137  7:66  8:7  9:1`.

### 4.1 Field table

Let `d` = the section's adjusted `data_address` and `N` = `u32(d+0x0C)`.

| offset | width | name | grade |
|---|---:|---|---|
| `d+0x00` | u32 | pointer → `d+0x18+4N` (start of array 2) | VERIFIED 6270/6270 |
| `d+0x04` | u32 | pointer → `d+0x18+8N` (start of array 3) | VERIFIED 6270/6270 |
| `d+0x08` | u32 | pointer → `d+0x18` (start of array 1) | VERIFIED 6270/6270 |
| `d+0x0C` | u32 | **`N`** — bone count, 1 … 25 | VERIFIED 6270/6270 |
| `d+0x10` | u32 | id/hash — 1 352 distinct, **never** the record's own instance id (0/6270) | UNKNOWN |
| `d+0x14` | u32 | `== N` in 6270/6270 | VERIFIED |
| `d+0x18` | u32×N | **bone identity tokens** | VERIFIED as an array; UNKNOWN as semantics |
| `d+0x18+4N` | u32×N | per-bone small field, values ⊂ `{0,1,2,3}` only | UNKNOWN |
| `d+0x18+8N` | u32×N | **bone parent index**, `-1` = root | VERIFIED 6270/6270 |
| — | — | `size == 0x18 + 12·N` | **VERIFIED 6270/6270** |

All three pointers are **record-absolute** and always land inside the section's
own payload; they are self-relative offsets and are exact in every one of the
6 270 sections. This is a *three-array SoA* container, not a pointer-chase
structure.

### 4.2 Bone identity (array 1)

* **15 763** values, **2 002 distinct** across the whole family.
* **Duplicate-free inside every one of the 6 270 skeletons** (6 270/6 270) — so
  it is a genuine per-skeleton key set, not a position index.
* The vocabulary is strongly **block-structured** — long runs of consecutive
  values, which is what a generator emitting per-mesh-family bone blocks
  produces:

| value | count | value | count |
|---|---:|---|---:|
| `0x12e167fa` | 1 124 | `0xd0be09e5` | 691 |
| `0xd0be09e0` | 937 | `0xb2d6aebf` | 630 |
| `0xd0be09e3` | 879 | `0xd0be09e4` | 566 |
| `0xd0be09e2` | 792 | `0xd0be09e7` | 498 |
| `0xb2d6aebc` | 488 | `0x7bd05f71` | 382 |

`0xd0be09e0 … 0xd0be09e9` (10 consecutive), `0xb2d6aeb8 … 0xb2d6aebf`
(8 consecutive), `0x7b298b90 … 0x7b298b99`+ — these are **32-bit ids, not
indices and not strings**. Grade **VERIFIED (a per-bone 32-bit identity exists)**
/ **UNKNOWN (what the identity means)**.

### 4.3 Bone parentage (array 3) — this is the real payload of the question

Four independent falsifiable tests, all over **6 270/6 270** sections:

```
every parent value ∈ [-1, N-1]                     : 6270 / 6270
exactly one -1 (root) per skeleton                  : 6270 / 6270
no parent chain visits a node twice (acyclic)       : 6270 / 6270
size == 0x18 + 12*N                                 : 6270 / 6270
```

Parent-value histogram (15 763 values): `-1`: 6 270 (one per skeleton),
`0`: 2 730, `1`: 1 470, `2`: 864, `3`: 702, `4`: 622, `5`: 569, `6`: 452, …

**What this is and is not.** Bone *topology* is present and machine-checkable: a
rooted forest per skeleton with integer parent links. **Bone names are not
present** — there is no text anywhere in a `0x70002` payload (§6), so these
records do **not** name bones. A numeric parent array plus an opaque id array is
**not** a semantic rig: it gives you the shape of the skeleton and nothing about
what the bones are for. Grade **VERIFIED (structure) / UNKNOWN (identity)**.

Array 2's per-bone field takes only `{0,1,2,3}` (counts `0`:6 028, `1`:7 172,
`2`: 902, `3`: 1 661). It is **not** a child count (0/6 270), **not** a bone
index (it takes values `≥ N` in 4 979 cases). It is a 2-bit-wide per-bone
attribute of unknown meaning — plausibly an influence/weight count or a LOD tag.
Grade **UNKNOWN**.

---

## 5. The `0x7000b` section is `OBJECTTYPE_SKELETONSINK`

`0x7000b` appears **exactly 0 or 1 times per record**: 1 568 records have one,
256 have none. **Size is 32 bytes in 1 568/1 568** — a fixed-size record, so the
"per-frame blend-shape weights" reading is impossible on size grounds alone.

| offset | width | measurement | grade |
|---|---:|---|---|
| `+0x00` | u32 | `0x00400000` (= `SporeApp.exe` image base) — 1 568/1 568 | VERIFIED |
| `+0x04` | u32 | a **code address inside `SporeApp.exe`**, only **3 distinct** values: `0x0063ffb0` (1 565), `0x00617b60` (2), `0x00659c30` (1) | VERIFIED |
| `+0x08` | u32 | `== data_address + 24` — a self-relative pointer to the word at `+0x18`, 1 568/1 568 | VERIFIED |
| `+0x0C` | u32 | `0x00400000` again — 1 568/1 568 | VERIFIED |
| `+0x10` | u32 | small integer, **37 distinct** values (2 … ~40) | UNKNOWN |
| `+0x14` | u32 | `1` — 1 568/1 568 | VERIFIED |
| `+0x18` | u32 | small integer, **40 distinct** values | UNKNOWN |
| `+0x1C` | u32 | `0` in 1 565, `1` in 3 | VERIFIED |

**Classification asked for by the brief:** it is **not text, not a table, not
floats, not indices**. It is a *serialised C++ object header*: two pinned
constants equal to the executable's image base, one link-time code address
identifying the handler class, a self-relative pointer, and two small counters.
Byte histogram over all 1 568 records × 32 B: `0x00` 34 334, `0x40` 3 149,
`0x01` 1 737, `0xB0` 1 577, `0x63` 1 570, `0xFF` 1 568, … 241 distinct byte
values, dominated by the three constants.

**Grade INFERRED** for "serialised object header naming an in-exe handler class";
the *semantics* of `+0x10` and `+0x18` are **UNKNOWN**. Refuted hypotheses, with
counts: `+0x10 == number of skeletons in the record` → 124/1568;
`+0x10 == total bone count` → 103/1568; `+0x18 == skeletons` → 0/1568;
`+0x18 == total bones` → 25/1568; `+0x18 == +0x10 − 1` → 0/1568.

Its sibling **`0x7000c` = `OBJECTTYPE_SKINSINK`** (20 B, 0/1 per record, 1 369
records) has the same shape: `+0x00 = 0x00400000` (1 369/1 369),
`+0x04` = a code address with **3 distinct** values (`0x008d6da0` ×1 366,
`0x008b73b0` ×2, `0x00900e50` ×1), then three small integers where
`+0x10 == +0x08 − 1` in **1 369/1 369**.

---

## 6. The ASCII — it is incidental, with one exception

Full scan of every byte of all 1824 records for printable runs ≥ 4
(2 157 400 runs total):

| where | runs | distinct | what |
|---|---:|---:|---|
| outside section payloads (2 044 600 B) | 1 824 | **1** | literally `RW4w32` — the magic itself |
| `0x20003` Raster payloads | 3 374 | **3** | `DXT1` **2710**, `DXT5` **655**, `DXT3` **9** |
| `0x10030` BaseResource | 1 409 500 | 394 919 | 4–5 byte accidental runs (`UUUU` 98 306, `@@@@` 17 776, …) |
| `0x200af` BlendShapeBuffer | 529 320 | 300 447 | same |
| `0x80003` KDTree | 177 935 | 82 275 | same |
| `0x70001` KeyframeAnim | 57 127 | 13 288 | same |
| `0x70003` AnimationSkin | 2 753 | 2 040 | same |
| `0xff0000` MorphHandle | 169 | 93 | same |
| `0x70002` Skeleton | 186 | 86 | same |
| `0x80005` BBox | 448 | 345 | same |
| `0x2000b` CompiledState | 5 | 4 | same |
| `0x7000b` / `0x7000c` / `0x7000f` / `0xff0001` / `0xff0002` / `0x20004` / `0x20005` / `0x20007` / `0x20009` / `0x2001a` | **0** | 0 | none |

Run-length histogram (all types pooled), showing the 4–5 byte cliff that is the
signature of coincidence rather than text:

```
len:  4     5     6     7     8     9    10    11    12    13   ...  max
   1212504 604986 146224 146718 31351 13398 5615 5186 6257 2097 ... 1029
```

### 6.1 "CRLF" is the magic

```
CRLF pairs outside section payloads : 1824   <-- exactly 1 per record
  …all 1824 of them at record offset 8 : 1824
CRLF pairs inside section payloads  : 2006   (coincidental; e.g. 0x55555555 runs)
```

Offset 8 of the 28-byte magic is `0D 0A 1A 0A`. **The CRLF that report 08 §5
observed is four bytes of the container's own signature.**

### 6.2 There is no text block anywhere

* Printable runs ≥ 8 immediately followed by CR/LF: **936**, only **625 distinct**,
  and they are 4-byte-aligned repetitions of one 4-byte pattern
  (`-s.sUUUU-s.sUUUU-…`, `R)))-1|nc-=-`, `kncxxxxnc`). A file-format string never
  repeats one 4-byte group 40 times.
* Longest such run: **123** bytes, and it is the same repeated pattern.
* Shader-name search: **no** `HLSL`/`vs_`/`ps_`/`technique`/`d3d` token appears in
  any of the 1824 records as a printable run. Grade **VERIFIED (negative)**.

**Answers to the brief's four options:** (a) shader source — **no**;
(b) script — **no**; (c) identifiers — **no**; (d) incidental — **yes**, with the
single exception of the **DXT format tag**, which is a real field, not text.

### 6.3 The Raster tag is a real field

`0x20003 RWGOBJECTTYPE_RASTER`, 32 B, 3 419 sections:

| offset | measurement | grade |
|---|---|---|
| `+0x00` | 4CC: `DXT1` 2710, `DXT5` 655, `DXT3` 9, **`0x00000015` 45** (not a 4CC) | VERIFIED |
| `+0x04` | `8` — 3 419/3 419 (mip count) | VERIFIED as a constant; INFERRED as mip count |
| `+0x08` | 3 118 distinct addresses in `SporeApp.exe` `.text` — a per-format handler | VERIFIED |
| `+0x0C` | packs `{height<<16 \| width}`, 14 observed values: `2×2`, `4×4`, `8×8`, `16×16`, `32×32`, `32×64`, `64×64` (2 801), `128×128`, `256×256`, `256×512`, `512×512`, `512×1024`, `1024×512`, `1024×1024` | INFERRED (14/14 values are plausible extents) |
| `+0x10` | 20 distinct values; `0x704` (1 796) in 2 350 sections, `0x708` 472, `0x904` 156, … | UNKNOWN |
| `+0x14`,`+0x18` | `0` — 3 419/3 419 | VERIFIED |
| `+0x1C` | 76 distinct values (27, 30, 22, …) | UNKNOWN |

---

## 7. `AnimationSkin`, `SkinMatrixBuffer`, morph and blend shapes

### 7.1 Presence and sizes (asked for; no payload decode attempted)

| section | records | size formula | verified |
|---|---:|---|---|
| `0x70003 ANIMATIONSKIN` | 1 369 | `16 + 64·n`, `n ∈ 1…25` | **1369/1369** |
| `0x7000f SKINMATRIXBUFFER` | 1 369 | `16 + 48·n`, `n ∈ 1…25` | **1369/1369** |
| `0x70003` count == `0x7000f` count | — | — | **1369/1369** |
| `0x7000f` count ∈ the record's skeleton `N` set | — | — | **1369/1369** |
| `0x7000f` count == max(skeleton `N`) | — | — | **1368/1369** |
| `0x70003` count == total bones over all skeletons | — | — | 0/1369 |
| `0x7000c SKINSINK` present ⇔ `0x70003` present | — | — | 1 369/1 369 (455 records have none of the three) |
| `0x70003`+`0x7000f` first matrix is identity | — | — | **0/1369** |

Both carry `count` at `+0x04` after a self pointer at `+0x00`, then `count`
4×4 (`0x70003`) or 3×4 (`0x7000f`) float matrices. The matrices are **real
rotations**, not identity — e.g. one record's first `0x70003` matrix row set is
`(0.0118, −0.9999, 0, 0), (1, 0, 0, 0), (−0.1523, 0.0137, 0.9882, 0)`. So the
`AnimationSkin` **does** carry bind-pose matrices, and the skeleton count
matching `max(N)` rather than `Σ N` is a real, unexplained observation.

### 7.2 Morph / blend family

| section | sections | records | size | notes |
|---|---:|---:|---|---|
| `0xff0000` (MorphHandle†) | 3 435 | 1 383 | **64 fixed** | `+0x00` = a 32-bit id, **only 77 distinct** over 3 435 sections (top: `0xd02751de` 1 014, `0x2e6521f4` 883, `0x7cc96c02` 725, `0x9bfe8bf8` 219); `+0x04 … +0x3C` = 15 float-ish words whose value sets are dominated by `±1.0/±2.0/0.0` |
| `0xff0001` (Animations†) | 637 | 637 | `8 + 8·n`, `n ∈ 1…6` — **637/637** | `+0x00 = 0x00800000` (637/637); `n` pairs of `{u32 id, u32 id2}`. **Neither word of any pair matches a morph-handle id in the same record: 0/1 178.** An earlier guess that it maps anim→morph is **REFUTED** |
| `0xff0002` (BlendShape†) | 579 | 579 | `28 + 8·n` — **579/579** | 4CC **`PFI\0`** at `+0x04` in **577/579** (the other two hold `0x493580`, `0x40d85a`); `+0x08 = 0x800000\|n`, `+0x10 = 0x800001\|n`, `+0x18` = a 32-bit id, `+0x20` ∈ {`0`, `0x050c5d2f`} |
| `0x200af` (BlendShapeBuffer†) | 579 | 579 | 5 504 … 961 684 | `+0x00 = 1`, **`+0x04 = 0x40` = 64, 579/579** (bytes per vertex), then three strictly increasing byte offsets at `+0x08/+0x0C/+0x10`; the payload is `0x00000000`-led float3 displacement triples. **37.48 % of every byte in this whole record family.** |

`0xff0002` and `0x200af` occur in **exactly the same 579 records**, and
`0x20009 MESH` / `0x2000b COMPILEDSTATE` / `0x2001a MESHCOMPILEDSTATELINK` occur
**3 887 times each with identical per-record histograms** (1×972, 2×337, 3×206,
4×153, 5×56, 6×57, 7×18, 8×11, 9×7, 10×4, 11×1, 20×1, 41×1). Those are hard
per-mesh and per-blend-shape couplings.

`PFI` appears **0** times as a 4CC (`PFI\0`) anywhere in `SporeApp.exe`;
the only `PFI` substrings are `PFRecordRead`, `PFIndexModifiable`, `PFIndex`.
Grade **UNKNOWN** for `PFI`.

### 7.3 `0x2001a MESHCOMPILEDSTATELINK` — resolved

12 bytes, 3 887 sections, and every field resolves:

| offset | measurement | grade |
|---|---|---|
| `+0x00` | a **section-row index**; `sections[+0x00].type_code == 0x20009` (MESH) in **3887/3887** | VERIFIED |
| `+0x04` | `1` — 3 887/3 887 | VERIFIED |
| `+0x08` | a section-row index; `sections[+0x08].type_code == 0x2000b` (COMPILEDSTATE) in **3887/3887** | VERIFIED |

Both indices are `< section_count` in 3 887/3 887. So every mesh owns exactly
one compiled state. (Earlier suspicion that these were indices into the
`SectionTypes` table is **REFUTED**: the table is only 13–23 entries long while
these run to 26.)

---

## 8. Per-record composition — 17 presence signatures, 257 exact multisets

Grouping by **which** section types are present (multiplicities collapsed):

| # | records | profile |
|---:|---:|---|
| 1 | 455 | full: mesh+skel+keyanim+animsink+skinsink+skinmat+animskin+morph, **no** blend/animations |
| 2 | 282 | #1 **+ `0xff0001` Animations** |
| 3 | 191 | blend/animations variant, **no** `0x20005` VBUF, **no** `0x70003/0x7000c/0x7000f`, **no** `0xff0001` |
| 4 | 173 | #3 **+ the animskin/skinsink/skinmat trio** |
| 5 | 168 | #4 **+ `0xff0001`** |
| 6 | 149 | full, **no** blend, **with** `0xff0001` |
| 7 | 146 | **static mesh only**: VDES+VBUF+IBUF+MESH+COMPILEDSTATE+LINK+2 KDTree/BBox, **no** raster, **no** skeleton, **no** anim |
| 8 | 110 | as #7 but **no** `0x20003` raster |
| 9 | 89 | skeleton-bearing, **no** blend, **no** `0xff0001` |
| 10 | 26 | blend + `0xff0001`, **no** VBUF, **no** animskin trio |
| 11 | 14 | blend, **no** VBUF, **no** raster, trio present, **no** `0xff0001` |
| 12 | 6 | skeleton+`0xff0001`, no raster/VBUF/blend |
| 13 | 5 | blend, no VBUF/raster/trio/`0xff0001` |
| 14 | 4 | skeleton+`0xff0001`, no raster/blend |
| 15 | 3 | skeleton+animskin trio, **no** keyframe anim, no morph/blend |
| 16 | 2 | blend+`0xff0001`, no VBUF/trio |
| 17 | 1 | static + skeleton + skeletonSink only |

Key derived counts:

```
records with 0x70002 SKELETON          : 1568
records with 0x7000b SKELETONSINK      : 1568   (identical set)
records with 0x70003/0x7000c/0x7000f   : 1369   (identical set for all three)
records with 0x20003 RASTER            : 1596
records with 0x20005 VBUF              : 1245
records with 0xff0000 MorphHandle      : 1383
records with 0xff0001 Animations       :  637
records with 0xff0002 BlendShape       :  579
records with NO animation/morph/sink section at all : 256
```

The **256 skeleton-free records** are exactly the records with zero `0x70002`
sections — a clean split between "rigblock part with a bone rig" and "rigblock
part that is a rigid mesh". Grade **VERIFIED**.

If multiplicities are kept, the number of distinct multisets is **257** and of
distinct *ordered* section sequences **429** — i.e. the *content* of a record is
one of 17 shapes and only the *counts* vary. Grade **VERIFIED**.

---

## 9. Does it reference the paired `0x01C135DA` mesh? — NO

Searched every byte of all 1824 decompressed records (258 511 636 B):

| search | scope | result |
|---|---|---:|
| LE `0x01C135DA` byte pattern | 1824 records, any offset | **0 hits** |
| LE `0x00A00CE4` byte pattern (the validated 16-byte-key tag) | 1824 records, any offset | **0 hits** |
| 16-byte `{instance, 0x01C135DA, group, 0x00A00CE4}` at any 4-byte offset | 1824 records | **0** |
| any 4-byte-aligned word equal to the record's own group `0x40606000` | 1824 records | 4 (numeric coincidences) |
| any 4-byte-aligned word equal to the record's own instance id | 1824 records | 16 (numeric coincidences) |

Grade **VERIFIED (negative)**. There is **no** embedded `ResourceKey` to the
paired mesh — and, given the 0-hit result for the `0x00A00CE4` tag, **no**
`ResourceKey` of *any* kind in the validated 16-byte form.

**The pairing is purely by identity.** Independently re-measured:
the `0x01C135DA` and `0x2F4E681B` instance-id sets at `0x40606000` are
**identical** (1 824 ids each, set equality `True`). And the rigblock overlap
from report 08 §5 reproduces exactly, per family:

| rigblock family | rigblocks | also present at `0x40606000` |
|---|---:|---:|
| `0x40616000` cell | 58 | **58** |
| `0x40626000` creature | 482 | **392** |
| `0x40636000` building | 224 | **220** |
| `0x40646000` vehicle | 540 | **528** |
| `0x40666000` flora | 107 | **0** |
| `0x40686000` unknown | 138 | **118** |
| **total** | **1 549** | **1 316** |

**508** of the 1 824 ids here match no rigblock at all. Grade **OBSERVED**
(independent re-measurement of report 08 §5, identical numbers).

**Inference, stated as such:** since this record is an RW4 **MODEL** with meshes,
a KD-tree, per-mesh compiled states, rasters and skeletons, and the paired
`0x01C135DA` is the flat index/geometry buffer, the natural reading is
`0x01C135DA` = the *editor's* compact mesh, `0x2F4E681B` = the *engine's*
`Model` for the same part. Grade **SUPPORTED**, not VERIFIED — the loader's
fetch order was not observed.

---

## 10. Negative results

| # | claim / search | scope | result |
|---|---|---|---|
| 1 | `RW4w32` magic | 1824 records | **1824/1824 present**, 0 failures |
| 2 | any non-`MODEL` `file_type` | 1824 records | **0** |
| 3 | truncated container (`is_complete()==false`) | 1824 records | **0** |
| 4 | negative section size | 51 904 sections | **0** |
| 5 | non-zero row `unknown_04` | 51 904 sections | **0** |
| 6 | text block (≥8 printable + newline) that is not a repeated 4-byte pattern | 1824 records | **0** |
| 7 | shader source / script / identifier strings | 1824 records | **0** |
| 8 | bone names in `0x70002` | 6 270 sections | **0** |
| 9 | `0x00A00CE4` key tag anywhere | 258 511 636 B | **0** |
| 10 | `0x01C135DA` anywhere | 258 511 636 B | **0** |
| 11 | exe registry names for `0x200af`, `0xff0000`, `0xff0001`, `0xff0002` | 87-entry registry | **absent** |
| 12 | `MORPH`/`BLENDSHAPE` as a type-name string anywhere in `SporeApp.exe` | 20 454 960 B | **0** |
| 13 | `0x70002` `word@0x10` == record's own instance id | 6 270 sections | **0** |
| 14 | `0xff0001` pair words matching a same-record morph id | 1 178 pairs | **0** |
| 15 | `0x70003` first matrix is identity | 1 369 sections | **0** |
| 16 | skeleton array 2 == child count | 6 270 sections | **0** |
| 17 | skeleton parents out of range / cyclic / no unique root | 6 270 sections | **0** |

---

## 11. UNKNOWN table

| # | question | grade | what would settle it |
|---|---|---|---|
| 1 | what the 2 002 distinct bone-identity tokens **mean** (names? property ids? morph-lattice handles?) | UNKNOWN | a bone-name table elsewhere in the corpus keyed by these tokens; `OBJECTTYPE_JOINTSKELETON` (`0x40005`, in the exe registry but absent here) |
| 2 | meaning of skeleton array 2 (values ⊂ {0,1,2,3}) | UNKNOWN | the `SkeletonSink`'s handler at `0x0063ffb0`; `OBJECTTYPE_BLENDER`/`WEIGHTEDBLENDER` consumers |
| 3 | meaning of skeleton `word@0x10` (1 352 distinct) | UNKNOWN | ditto |
| 4 | `SKELETONSINK +0x10` / `+0x18` (37 / 40 distinct small ints) | UNKNOWN | decompile the reader for `0x7000b`; `FUN_01200f60` only *constructs* it |
| 5 | why `AnimationSkin`/`SkinMatrixBuffer` count == `max(skeleton N)` and not `Σ N` (1368/1369 vs 0/1369) | UNKNOWN | one runtime trace: load a rigblock with ≥2 skeletons and log which skeleton feeds the skin |
| 6 | `BlendShapeBuffer` `+0x04 = 64` (bytes/vertex?) and the three offsets at `+0x08/+0x0C/+0x10` | UNKNOWN | payload decode; 37.5 % of the family's bytes live here |
| 7 | the 45 rasters whose `+0x00` is `0x00000015` rather than a DXT 4CC | UNKNOWN | format enum in the exe; the 3 distinct `+0x08` handler addresses |
| 8 | raster `+0x10` (20 distinct) and `+0x1C` (76 distinct) | UNKNOWN | ditto |
| 9 | `VBUF`/`IBUF` 28-byte descriptor fields — `IBUF +0x10 = 101` and `+0x14 = 4` are **constant in 1825/1825**, and `+0x00` points into `.text` | UNKNOWN | the static tables at those `.text` addresses; they look like shared templates, which would mean the actual vertex/index data is in the exe, not the record |
| 10 | the `0xff0000`/`0xff0001`/`0xff0002`/`0x200af` class names | UNKNOWN | only the factory stubs at `0x007c56e0`–`0x007c57d0` exist, with no registry entry and no name string |
| 11 | the 4CC `PFI` in `BlendShape` | UNKNOWN | `PFRecordRead`/`PFIndex` in `.rdata` are the only `PF*` strings; the relationship is unmeasured |
| 12 | which of the two paired types the engine actually loads first | UNKNOWN | one Wine trace (report 08 §6 item 3) |

---

## 12. Provenance

* **Data.** `SPORE/Data/Spore_Graphics.package` (942 881 325 B, 24 707 index
  entries, DBPF v3.0 idxMinor 3, read whole). Filter
  `type == 0x2F4E681B and group == 0x40606000` → **1824** records,
  **258 511 636 B** decompressed, 0 decompress failures. Cross-check:
  `Spore_Game.package` (20 210 108 B) for the `0x5CAA5E28` rigblock id sets.
* **Reader.** `sys.path.insert(0,'tools'); from spore.dbpf.dbpf import read,
  getdata`, CPython **3.14.7**, Linux/CachyOS.
* **Section walker.** Transcribed from `crates/spore-rw4/src/lib.rs`
  (`MAGIC`, `MANIFEST_OFFSET=0x98`, manifest-relative `o1`, `SECTION_ROW_SIZE=24`,
  and the `BaseResource (0x10030)` arena-relative address rule).
  **Cross-validated against the repository's independent oracle
  `tools/spore/rw4/rw4.py::describe`: byte-identical on 1824/1824 records.**
* **Binary.** `SPORE/SporeBin/SporeApp.exe` 3.1.0.22, 20 454 960 B,
  image base `0x00400000`, sections parsed from the PE headers
  (`.text` va `0x1000`/raw `0x400`, `.rdata` va `0x00FCC000`/raw `0xFCB400`).
  The type-code registry was located by scanning for 4-byte little-endian
  immediates (`0x7000b` → 4 VAs, `0x7000c` → 7, `0x7000f` → 4) and reading the
  `{namePtr, typeCode}` run at `0x015d0370`. Ghidra MCP (`SporeApp.exe`,
  93 995 functions) used only to confirm that `0x0063ffb0` lies inside the code
  of `VTDISC_0063FDA0` rather than in a vtable.
* **Scratch.** Throwaway analysis under `/tmp/opencode/rw4a/` and a 258 MB record
  cache under `/var/tmp/openspore-scratch/rw4a/` (`/tmp` was full at 16 G).
  **No file in this repository was created or modified except this report.**
  No Spore asset byte is reproduced anywhere above: only field offsets, formulas,
  histograms, counts, symbol names read out of the shipped executable, and
  non-asset identifier tokens.