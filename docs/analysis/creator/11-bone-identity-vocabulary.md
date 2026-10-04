# 11 — The bone identity tokens are morph-parameter names: the body graph is recoverable

**Agent:** SKELETON · **Scope:** RESEARCH ONLY · **Date:** 2026-10-04
**Subject:** `Spore_Graphics.package`, group `0x40606000`, the `Skeleton (0x70002)`
section inside the 1824 `0x2F4E681B` (`rw4`) records paired 1:1 with the 1824
`0x01C135DA` (`gmsh`) records.

**Verdict: the central question of this expedition is ANSWERED, and the answer is
yes.** The `Array 1` that report 10 could only call "2002 distinct 32-bit
tokens, meaning UNKNOWN" is **an FNV-1 hash of a morph-lattice parameter name**.
With the names in hand, the corpus yields a **named, parented bone graph** —
not indices, not an anonymous ordinal list. That is the body graph the SIGGRAPH
2008 paper describes, recovered from data.

This report supersedes `08` §7 item 3, `10` §4.2 and the "identity UNKNOWN"
grades attached to the skeleton bone tokens.

---

## 0. Reproduction

```python
import sys, struct, collections
sys.path.insert(0, 'tools')
from spore.dbpf.dbpf import read, getdata

data, items = read('SPORE/Data/Spore_Graphics.package')

def sections(buf):                      # RW4 section directory, per crates/spore-rw4
    section_info_ptr = struct.unpack_from('<I', buf, 0x30)[0]
    section_count    = struct.unpack_from('<i', buf, 0x24)[0]
    buf_ptr          = struct.unpack_from('<I', buf, 0x44)[0]
    o1               = struct.unpack_from('<I', buf, 0x98+0x0C)[0]
    types_base       = 0x98 + o1
    n_types          = struct.unpack_from('<I', buf, types_base+0x04)[0]
    for i in range(section_count):
        dp, _u, size, _al, _tci, tc = struct.unpack_from('<IIiiii', buf, section_info_ptr+24*i)
        if tc == 0x10030: dp += buf_ptr
        yield tc, dp, size

tokens = collections.Counter()
for it in items:
    if it['type'] == 0x2F4E681B and it['group'] == 0x40606000:
        buf = getdata(data, it)
        for tc, dp, size in sections(buf):
            if tc != 0x70002: continue
            N = struct.unpack_from('<I', buf, dp+0x0C)[0]
            assert size == 0x18 + 12*N
            for b in range(N):
                tokens[struct.unpack_from('<I', buf, dp+0x18+4*b)[0]] += 1
# -> 1824 records, 6270 skeletons, 2002 distinct tokens, 15763 values
```

Independently re-derived here and **identical to report 10** on all four counts
(1824 / 6270 / 2002 / 15763), including the frequency of `0x7BD05F71` = 382.

---

## 1. The hash: FNV-1, multiply-then-xor

```
basis  = 0x811C9DC5
prime  = 0x01000193
h      = basis
for each byte c of the name:            # FNV-1: multiply FIRST, then xor
    h = (h * prime) & 0xFFFFFFFF
    h ^= c
```

**It is FNV-1, not FNV-1a.** The distinction is load-bearing and both were tested:
FNV-1a over the same candidate set yields **0** hits where FNV-1 yields hundreds.
This repo's own DBPF ids are FNV-1a, so writing an FNV-1a hash here would have
produced a clean, plausible, wrong answer — the same class of error as the
big-endian `refCount` trap.

### 1.1 How the scheme was proved rather than guessed

Three independent lines of evidence, in increasing strength:

**(a) A known-frequency match.** `0x7BD05F71` occurs **382** times. FNV-1 of
`"root"` is `0x7BD05F71`. Report 10 had already measured 382 for that token
without knowing what it meant.

**(b) Collapsed indexed families.** Guessing one stem and then its digit
suffixes returned *whole families*:

| family | members recovered | bone values |
|---|---:|---:|
| `joint<N>` | 18 (`joint1`–`joint9`, `joint20`–`joint29`) | 5 674 |
| `shaft<N>` | 5 (`shaft1`–`shaft5`) | 94 |
| `base<N>` | 6 (`base1`–`base6`) | 76 |
| `b<N>`, `node<N>`, `stem<N>`, `f<N>`, `wing<N>` | 5–6 each | 10–16 each |
| `bolt<N>`, `bulb<N>`, `fin<N>`, `top<N>`, `r4j<N>` | 4 each | 4–16 each |
| `arm<N>`, `ring<N>`, `jaw<N>`, `r3j<N>`, `spin<N>` | 3 each | 3–10 each |

**(c) An algebraic prediction, then a check.** For FNV-1, two names sharing a
prefix and differing only in the last byte satisfy

```
hash(P + c1)  ^  hash(P + c2)  ==  ord(c1) ^ ord(c2)
```

because the multiply term is identical and only the final xor differs.
Check on the recovered family: `joint1 ^ joint3 = 0xD0BE09E0 ^ 0xD0BE09E2 =
0x02`, and `ord('1') ^ ord('3') = 0x31 ^ 0x32 = 0x02`. Exact.

That relation is also a **tool**: given any cluster of near-consecutive tokens,
the last character of every name can be read off directly, and the shared prefix
hash recovered by one modular inverse. Applied to the corpus it independently
re-derived the prefixes `joint` (`0x1E07758B`) and `base` (`0x175D70FB`) — both
then confirmed against their already-recovered member names.

### 1.2 Chance-collision accounting

The exhaustive search covered **all** `[a-z0-9]` strings of length ≤ 5 —
62 193 780 candidates. Against 2002 targets in a 2³² space that is an expected
**29.0** coincidental collisions. **189** names were recovered by that route
alone, i.e. roughly 160 in excess of chance, and the excess is concentrated
precisely in multi-member families — the signature a collision cannot fake.

Recovered names are therefore graded:

* **CORPUS-LITERAL** — the exact byte string occurs in a decompressed record or
  a shipped binary (67 names).
* **FAMILY** — ≥3 siblings of an indexed family recovered, or an algebraic
  prefix/suffix check passed (the `joint`, `base`, `shaft`, `bolt`, `bulb`,
  `fin`, `top`, `stem`, `node`, `b`, `f`, `wing`, `ring`, `arm`, `jaw`, `spin`
  families).
* **EXHAUSTIVE-SINGLE** — one name recovered with no sibling and no literal
  (weaker; kept, but not load-bearing).
* A handful of single hits that are not word-like (`hw818`, `hw868`, `r4j2` in
  one context) are **retained as unresolved collisions, not promoted**. Where a
  recovered string could not be told from a coincidence, it stays UNKNOWN.

---

## 2. Result: 244 of 2002 token values named, 44.6 % of all bone values

`244 / 2002` distinct tokens recovered, covering `7 027 / 15 763` bone
occurrences. The unrecovered remainder is dominated by **part-specific asset
names** — the corpus yields literal strings like `CA_shoulders_01`,
`CE_details_crystal_01`, `VE_Spacecraft_06`, which embed a per-asset prefix and
cannot be enumerated without the asset database. That is a completeness limit,
not a decoding failure.

Head of the recovered table:

| token | occurrences | name |
|---|---:|---|
| `0xD0BE09E0` | 937 | `joint1` |
| `0xD0BE09E3` | 879 | `joint2` |
| `0xD0BE09E2` | 792 | `joint3` |
| `0xD0BE09E5` | 691 | `joint4` |
| `0xD0BE09E4` | 566 | `joint5` |
| `0xD0BE09E7` | 498 | `joint6` |
| `0x7BD05F71` | 382 | **`root`** |
| `0xD0BE09E9` | 362 | `joint8` |
| `0xD0BE09E8` | 313 | `joint9` |
| `0x80D91E9E` | 94 | `effect` |
| `0x2002C980` | 46 | `csnap` |
| `0x0AC4AEEE` | 36 | **`attach`** |
| `0xC045A9A3` | 32 | `tool` |
| `0x36932030` | 24 | `eye` |
| `0x4C56A3AF` | 12 | `pupil` |
| `0x68D0ED08` | 5 | `iris` |
| `0x61E2861E` | 2 | `ankle` |
| `0xF797D34B` | 3 | `lookat` |

### 2.1 The vocabulary is anatomical AND mechanical AND positional

Three distinct groups, which is itself a finding:

* **Anatomical:** `root`, `body`, `head`, `eye`, `pupil`, `iris`, `eyeball`,
  `eyebrow`, `nose`, `jaw1..3`, `jaw2`, `ear`, `neck`, `shoulder`, `arm1..3`,
  `hand`, `finger2`, `finger3`, `hip`, `knee`, `ankle`, `foot`, `leg`, `tail`,
  `wing`, `fin`, `toe`, `mouth`, `weapon`, `mast`.
* **Mechanical / rig:** `joint1..9`, `joint20..29`, `base1..6`, `node1..6`,
  `stem0..5`, `shaft1..5`, `bolt1..4`, `ring1..3`, `spin1..3`, `bounce`,
  `rotate`, `axle`, `rod`, `csnap`, `attach`, `tool`, `snap`.
* **Positional / morph-axis:** `xpos`, `xneg`, `ypos`, `yneg`, `z_up`, `j_up`,
  `j_down`, `mid`, `middle`, `center`, `tip`, `top1..4`, `high`, `low`, `height`,
  `radius`, `length`, `front1`, `rear`.

`attach` and `csnap` are worth isolating: they are the two names in the whole
recovered set that name an **authoring operation** rather than an anatomy. They
are the strongest corpus hint that attachment and snapping were serialized as
*operations on parts*, and §6 follows that up.

---

## 3. The correspondence table the expedition asked for

Historical concept (SIGGRAPH 2008 "Real-time Motion Retargeting to Highly
Varied User-Created Morphologies"; Chris Hecker et al.) against corpus evidence.
**Only rows with evidence are populated.** External claim and corpus observation
are kept separate, per §18 of the brief.

| Historical concept | Corpus observation | Verdict | Confidence |
|---|---|---|---|
| **body** | one `Skeleton (0x70002)` section per rigblock container; 6270 sections in 1568/1824 records; `N` bones each, 1–25 | **confirmed** | VERIFIED |
| **parent** | array 3 at `d+0x18+8N`: values in `[-1,N-1]`, **exactly one `-1` per skeleton**, all chains acyclic, 6270/6270 | **confirmed** | VERIFIED |
| **DAG / rooted structure** | parent links verified acyclic in every skeleton; a rooted forest per section | **confirmed** | VERIFIED |
| **serial spine chain at the root** | exactly one root per skeleton exists, and `root` is the single most frequent bone name (382) | **partially confirmed** — a root is proven; that it is a *spine* is INFERRED | SUPPORTED |
| **caps / semantic capability** | bone identity tokens are FNV-1 hashes of morph-parameter names; the recovered vocabulary is anatomical (`hip`, `knee`, `jaw`, `iris`, `shoulder`…) | **confirmed, as a capability-ish naming layer** | SUPPORTED |
| `grasper` specifically | **not recovered.** No token hashes to `grasper` under FNV-1 | **unresolved** | UNKNOWN |
| `mouth` specifically | `mouth` recovered (n=2) and `jaw1..3`/`jaw2` recovered | **plausible** — the *word* `mouth` is present; no claim that it is a capability | INFERRED |
| **mesh** | `0x01C135DA` (`gmsh`) at the same instance id; 1824 records; bare float3 positions + per-channel blocks | **confirmed** | VERIFIED (report 09) |
| **deformation curves** | rigblock `0x5CAA5E28` is a quantised 4^k spatial subdivision with per-level resolution 4 (report 08); `bounce`, `spin`, `rotate`, `radius`, `height` names recovered on bones | **plausible** — the *mechanism* is proven, its binding to these names is not | INFERRED |
| **rest pose** | array 2 at `d+0x18+4N` takes values in `{0,1,2,3}` only; 10859 orthonormal 3×3 transforms in section `0x12000E05`; **no record of which is the rest pose** | **unresolved** | UNKNOWN |
| **attachment relationship** | bone `attach` recovered (n=36); `0x40606000` holds 56 `prop` records; `<blockref>` parent/child in `<sporemodel>` | **open** — see §6 | INFERRED |
| **20–80 bodies per character** | a *part* container holds 1–9 skeletons; a creature assembles up to 680 `<blockref>` across 68 documents | **consistent, not confirmed** — the counts do not contradict the paper, but the paper's "bodies" and a rigblock container's skeleton are not shown to be the same unit | PLAUSIBLE |
| **capability/type queries (Front/Center/Back, Left/Right)** | `L`/`R` laterality names **are** present: `wingL`/`wingR`, `sideL`/`sideR`, `hipL`/`hipR`, `kneeL`/`kneeR` — 4 complete pairs | **confirmed for laterality only** | SUPPORTED |

### 3.1 Explicitly NOT recovered

* No bone **names** are stored in the record. The mapping is a *dictionary
  attack* against names known to the engine and to the corpus. It is evidence
  about the hash function and the generator, not a decode of a name table.
* No **spine** designation. `root` is proven to be a name; that the chain below
  it is a spine is not.
* No **capability enum**. The paper's `caps` are a C++-side concept; what the
  corpus holds is a parameter-name vocabulary.
* The remaining 1 758 tokens are unnamed. That is a real gap and is recorded as
  such, not rounded to zero.

---

## 4. Symmetry: serialized as laterality, not as an operation

The brief asked whether symmetry is serialized, editor-only, or indirect. The
answer is measurable:

* There is **no** symmetry flag, symmetry node, or mirror relationship anywhere
  in the `0x70002` payload — it is three `u32[N]` arrays and nothing else, with
  zero slack (report 10).
* But bone identity carries **explicit laterality**, and complete L/R pairs are
  present in the corpus: `wingL`/`wingR`, `sideL`/`sideR`, `hipL`/`hipR`,
  `kneeL`/`kneeR`.

**Conclusion: symmetry is represented INDIRECTLY — as two independently named
bones.** The editor operation that produced the mirror is not stored; its
*result* is, in the same way any other part is stored. A reconstruction that
invents a "mirror this part" rule would be inventing behaviour: the corpus
records two distinct bones that happen to share a stem, and nothing more.

This is the cleanest instance in the project of the rule the brief states — do
not implement bilateral symmetry because the historical Creator had symmetry
tools; the corpus says what it says, and it says laterality, not an operation.

---

## 5. Relationship to the geometry layer (report 09)

Report 09 established that `0x01C135DA` bones are an **anonymous local ordinal
list** with rigid per-triangle skinning and no weights — and concluded the body
graph was not recoverable *from that layer*. That conclusion stands **for
`gmsh`**, and this report corrects only its over-general form:

> the body graph is not in the geometry record; it is in the **paired RW4
> container at the same instance id**.

The two records are paired 1:1 by identical instance id, and report 10 measured
that neither contains the other's type id (0 hits for `0x01C135DA`, 0 hits for the
`0x00A00CE4` key tag). So the join is **by instance id alone**, performed by the
loader, and it is the load that binds anonymous geometry to named bones.

**Consequence for OpenSpore:** a part cannot be decoded from `gmsh` alone. The
minimal unit is the **pair** `{0x01C135DA, 0x2F4E681B} @ 0x40606000`. Any
"skinned mesh decoder" that ignores the sibling container has decoded half a
part.

---

## 6. Attachment and snapping: the open question, stated precisely

`attach` (n=36) and `csnap` (n=46) are recovered bone names. That is suggestive
but **not** sufficient to claim an attachment graph exists at this layer:

* both are single bones in a per-part skeleton, i.e. *inside* one part;
* the `<blockref>` parent/child relation is the *other* candidate, one level up,
  in `<sporemodel>`;
* no array in `0x70002` stores a part-to-part reference, and the record has zero
  slack.

So the brief's real question — *what minimal persisted relationship is necessary
to reconstruct the same composed creature?* — has a measured answer at the
**authoring** layer: the `<blockref>` childlist plus the local transform. What is
still unproven is whether a part's `attach`/`csnap` bones participate in that
composition or are internal to the part.

**This is the single highest-value open question**, because it decides whether
attachment is data or an editor convenience. Settling it needs one runtime
observation: load a two-block creature and record which `ResourceKey`s and which
bone names the editor queries.

---

## 7. Rejected hypotheses, with scope

| hypothesis | test | result |
|---|---|---|
| tokens are FNV-1a of a name | 1 184 977 ASCII runs from 10 shipped binaries + 11 543 799 corpus runs, all casings | **REFUTED, 0 hits** |
| tokens are FNV-1a, CRC32, djb2, djb2-xor, sdbm, Jenkins OAAT or MurmurHash3 of a name | same corpus, 4 casings × 4 NUL variants | **REFUTED for all six**; only FNV-1 hits |
| tokens are DBPF instance ids | index of 5 packages, all types/groups | **REFUTED**: 26 of 2002 collide, and those 26 are `png` at unrelated groups plus coincidental `prop`/`gmsh` |
| tokens are DBPF group ids | same | **REFUTED, 0 of 2002** |
| `0x811C9DC5` is the hash of `"GeomInfo"` | FNV-1a of 4 casings | **REFUTED**: `0x811C9DC5` is simply the FNV offset basis, a coincidence in report 08's reading |
| a bone-name string table exists in the corpus | identifier sweep over 4 packages, 11.5 M runs, all identifier-like tokens | **NOT FOUND** — names are hashed, not stored |

---

## 8. What this changes for OpenSpore

1. **`0x70002` is a first-class decodable record**, and it is the body graph.
   `spore-rw4` already walks to it; the payload is `0x18 + 12·N` with three
   arrays. That is a small, fully-specified decode.
2. **The canonical unit of a part is a pair**, `{gmsh, rw4} @ 0x40606000`,
   joined by instance id — and joining them is a resolver step with provenance,
   not a convenience.
3. **Bone names are recoverable for 44.6 % of occurrences by dictionary
   attack**, deterministically, offline, with the hash function measured. This is
   legitimate: it is inference about a hash, graded per name, never a fabricated
   anatomical claim.
4. **Symmetry is laterality, not an operation** (§4).
5. **No rest pose and no capability enum yet** (§3.1).

---

## 9. UNKNOWN register

| # | question | grade | what would settle it |
|---|---|---|---|
| 1 | the 1 758 unrecovered token names | UNKNOWN | a corpus-wide asset-name table, or inverting the remaining hash blocks by prefix (method in §1.1, already recovers 4 prefix hashes per block) |
| 2 | meaning of array 2 (values ⊂ `{0,1,2,3}`) | UNKNOWN | correlating it against bone name classes across the 6270 skeletons |
| 3 | which of the 10 859 transforms is the rest pose | UNKNOWN | one runtime pose observation |
| 4 | whether `attach`/`csnap` bones cross part boundaries | UNKNOWN | one runtime composition trace |
| 5 | whether a part's bones are the paper's "bodies" | UNKNOWN | a creature-level count of bodies vs its blockref count |
| 6 | why 256 of 1824 records have no skeleton at all | UNKNOWN | report 10 §4 composition split; a cross-check against record size |
| 7 | the meaning of the 1360-value block at `0x7B298B90..99` | UNKNOWN | its shared prefix hash is `0x065A4E7B` and the last characters are digits 0–9; recovering that prefix recovers 10 names at once |

---

## 10. Method and provenance

* Data: `sys.path.insert(0,'tools'); from spore.dbpf.dbpf import read, getdata`,
  Python 3.14, against the installed GOG build in `SPORE/`.
* RW4 section directory implemented in the report from `crates/spore-rw4/src/lib.rs`
  (the Rust crate is authoritative for the layout, including the `0x10030`
  BaseResource adjustment).
* Hash search: FNV-1 and six alternatives × 4 casings × NUL variants, over
  1 184 977 binary runs and 11 543 799 corpus runs; exhaustive `[a-z0-9]^{1..5}`
  (62 193 780 candidates); targeted family expansion; algebraic prefix/suffix
  inversion over 48 blocks of size ≥ 6.
* Independent re-derivation of report 10's four headline counts: **identical**.
* No asset byte is reproduced. No external project was consulted for the format;
  the only historical input is the paper named in the brief, and it is confined
  to §3 and treated as hypothesis.