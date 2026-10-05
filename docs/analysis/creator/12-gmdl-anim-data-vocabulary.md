# 12 — The gmdl anim data holds no anatomy: it is a pointer to a whole-body skeleton group

**Agent:** VOCAL · **Scope:** RESEARCH ONLY · **Date:** 2026-10-05
**Subject:** the animation trailer of all 4 209 `gmdl` records in
`Spore_Content.package`, histogrammed against the 2 002-token morph-parameter-name
vocabulary recovered in report 11.

**Verdict: NO.** Not one of Spore's anatomical bone names — `hip`, `knee`, `eye`,
`iris`, `jaw`, `shoulder`, `body`, `root`, `wing`, or any L/R pair — occurs
anywhere in the gmdl anim-data / baked-deform region. 0 hits out of
**11 236 365** trailer words, where 0.45 hits are expected by chance. This closes
`docs/CREATOR-CORE.md` §13.2 candidate 1 and **rules out the whole-body `gmdl`
layer as the store of the creature anatomy**, on a measurement rather than on an
argument.

**And the pointer was worth following.** The same histogram produced a pointer map
of everything the anim data actually references, and one of its four `rw4` targets
— group **`0x41606100`** — carries `Skeleton (0x70002)` sections whose identity
tokens are the *same* FNV-1 name vocabulary (88.4 % shared with the part layer),
with 96 of 623 already named, including the complete `joint1..joint9` family plus
`root`, `eye`, `hip`, `knee`, `jaw`, `shoulder` and `body`. **The whole-body
anatomy exists; it is reached through a resource key, not stored inline.** §6
measures that group; §7 states where that leaves the expedition.

---

## 0. Reproduction

```text
cargo test -p spore-gmdl --release --test anim_vocabulary -- --ignored --nocapture
```

Two `#[ignore]`d tests, ~2.3 s, ~600 MB peak RSS. Both skip cleanly when the game
is not installed. The measurement is a committed test, not a script: every number
below is an assertion or a `--nocapture` line of
`crates/spore-gmdl/tests/anim_vocabulary.rs`.

## 1. The region, and how it was located

**The region is every byte after the validated geometry and material-info
sections** — the bone-range table, the `GameModelAnimData` array, and the
trailing 12-byte resource key. 8 900 180 bytes over 3 590 records; median 20
bytes, largest 25 228 bytes.

It is located by a **new additive function**, `spore_gmdl::trailer_region`:

```rust
pub fn trailer_region(data: &[u8]) -> Option<&[u8]> {
    if data.is_empty() { return None; }
    let mut walk = Walk::new(data);
    walk.strict_sections().ok()?;
    Some(&data[walk.r.position()..])
}
```

Two design points, both load-bearing:

* **`strict_consumed` is the wrong anchor and using it would have been wrong.**
  It is the best-effort walk's *stop*, not the trailer's *start*. On a truncated
  walk — and the real creature records truncate — `data[strict_consumed..]` is the
  un-validated remainder, not the trailer. §2 uses `strict_consumed` only for the
  619 records where no trailer can be located at all, and says so when it does.
* **It is a function, not a field on `GmdlModel`.** `spore-material`,
  `spore-differential` and `spore-tools` all build `GmdlModel` literals; adding a
  field breaks crates this workstream does not own. A free function is additive.

**Alignment is swept, not assumed.** A table whose true boundary sits two bytes
earlier than the walk believes is invisible to a phase-0-only scan. Every region
is read at all four byte phases. Measured per-phase universe hits: `[58, 0, 0, 0]`
— phases 1–3 contributed nothing, so the swept baseline did no hidden work, and
the 4× word count that follows from it is conservative rather than flattering.

## 2. Denominator — measured over the whole corpus, in two regions

| | records | bytes | words (4 phases) |
|---|---:|---:|---:|
| **Region A** — located trailer | 3 590 / 4 209 | 8 900 180 | 8 889 410 |
| **Region B** — no located trailer, wider span | 619 / 4 209 | 2 348 812 | 2 346 955 |
| **total** | **4 209 / 4 209** | — | **11 236 365** |

This is **100 % of the gmdl corpus**, not "300 of 4 209". The 619 are the records
whose material info names shader-data id `0x218`, which this crate has no size
for — 619 of 619, one single cause — and that set includes **all 194 records of
the creature-model groups `0x40627100`/`0x40627101`**. Region B spans from the
last validated byte to the record end, which *over-includes* the unsized material
info. That can only add false positives, never hide a hit, so it is the safe
direction for a negative result. It found **zero**.

Within region A: **1 771 records carry anim data** (trailer > 20 bytes) and
**1 819 are the bare 20-byte floor** — a zero bone-range count, a zero anim count,
and the key, i.e. no anim table at all. Splitting these matters: "8.9 M words
scanned" reads very differently if most of them are the bytes of an absent table.

## 3. The vocabulary, rebuilt rather than transcribed

The 2 002-token universe was **re-measured from the part layer**, not copied out
of report 11, because a 2 002-entry constant is a claim nobody could check:

```
universe: 1824 group rows, 1568 records with a validated skeleton,
          6270 skeletons, 2002 distinct tokens, 15763 bone values
```

The three counts that carry the identity universe — **6 270 skeletons, 2 002
distinct tokens, 15 763 bone values** — match report 11 §0 exactly. The record
count is 1 568 rather than 1 824 because this reader counts only rows whose
`0x70002` payload matched the `0x18 + 12·N` shape before reading any of it; report
11 counted every index row it iterated. Different question, same corpus.

The 244 recovered names are read through `spore_rigblock`'s own `fnv1`, reached as
a **dev-dependency**: a second FNV-1 in the test would be one transposed line away
from FNV-1a and would report a confident zero about nothing. The 17 names the
question names outright are transcribed as `(name, hash)` pairs and re-hashed
through `fnv1` in the test, so the check is against the *hash function*, not
against the table that the hash produced.

| grade | count |
|---|---:|
| witnessed (`Family` / `BinaryLiteral` / `RecordLiteral`) | **170** |
| `Unconfirmed` (bare exhaustive-search hit) | **74** |
| total | 244 |

## 4. The measurement

### Region A (3 590 records, 8 889 410 words)

| set | hits | distinct | chance expectation | ratio |
|---|---:|---:|---:|---:|
| full 2 002-token universe | 58 | **3** | 4.14 words | 14.0× words / **0.72× distinct** |
| 170 witnessed names | **0** | **0** | 0.352 | **0×** |
| 74 `Unconfirmed` names | 1 | 1 (`pipes`) | 0.153 | 6.5× |
| *control:* words that are a live index instance id | 6 484 | 768 | 4.70 | 1380× |

### Region B (619 records, 2 346 955 words)

| set | hits | distinct | chance expectation |
|---|---:|---:|---:|
| full universe | **0** | **0** | 1.094 |
| witnessed names | **0** | **0** | 0.093 |
| *control:* index instance ids | 1 750 | 270 | — |

### The 17 anatomical names, asked one by one

`root` `body` `hip` `hipL` `hipR` `knee` `kneeL` `kneeR` `eye` `iris` `jaw`
`shoulder` `head` `wing` `wingL` `wingR` `joint1` — **0 of 17 present**, in either
region, at any of the four alignments.

## 5. Why the 58 hits are not anatomy

A raw hit count of 58 against a baseline of 4.14 is **14× chance**, and reporting
it as a finding would have been the exact failure this repository warns about. It
is not a finding, and the reason is measured rather than argued.

Three distinct tokens account for all 58 words, and **every one of them is the
instance word of a resource key**:

| token | recovered name | part-layer freq | occurrences | what it is |
|---|---|---:|---|---|
| `0x43c67520` | `pipes` (weak) | 2 | 1 word, 1 record | the containing `gmdl` record's **own instance id**, in its own trailing key — the index holds `t=0x00E6BCE5 g=0x40656200 i=0x43c67520` |
| `0x4e84bd28` | unnamed | 2 | 24 words, 24 records | instance word of `t=0x2F4E681B g=0x41606100` |
| `0x4e84bd29` | unnamed | 2 | 21 words, 21 records | instance word of `t=0x2F4E681B g=0x41606100` |

Verified by reading the bytes around each occurrence. A gmdl trailer holds keys
as three consecutive words — **instance, type, group**:

```text
0xbfe5 2ea5 0xffffffff 0x4e84bd28 0x2f4e681b 0x41606100 0x00000001 0x00000006 0x0 …
```

The token is followed, within twelve bytes, by both the type and the group of the
real index row whose instance it is. The test asserts that for every token it
finds, so "this is a pointer" is checked, not claimed.

Two supporting observations, both of which a coincidence cannot fake:

* The 6 484 index-instance-id words in region A outnumber the 58 universe hits
  **112 : 1**. The trailer is pointer-dense; a pointer has a 1-in-2^32 chance of
  landing in the 2 002-token set, and 6 484 such chances produce a handful of
  collisions. That is what 58 is.
* `0x4e84bd28` and `0x4e84bd29` differ **only in the low byte** — the FNV-1
  prefix signature of two names sharing a prefix, which is the strongest possible
  coincidence bait. They are also both real `rw4` instance ids at group
  `0x41606100`. This is the 26-token "coincidental `prop`/`gmsh`" class report 11
  §already refuted as DBPF instance ids; these two are members of it.

**The honest summary: 58 words, 3 distinct tokens, 0.72× the chance expectation on
distinct tokens, 0 witnessed names, and 100 % of the three explained as resource
pointers.**

## 6. What the trailer *does* contain: a pointer to a whole-body skeleton

The same scan produced the pointer map. Distinct `(type, group)` targets behind
the trailer's instance ids: **94**. Four of them are `rw4` containers:

| `rw4` group | distinct instance ids | section codes found | `0x70002` Skeletons |
|---|---:|---|---:|
| `0x40646200` | 285 | `{0x80003}` | 0 |
| `0x40636200` | 164 | `{0x80003}` | 0 |
| `0x40656200` | 91 | `{0x80003}` | 0 |
| **`0x41606100`** | **255** | `0x70001`, `0x70002`, `0x70003`, `0x70004`, `0x7000C`, `0x7000F`, `0x100001` | **8 in 4 probed records** |

`0x41606100` is **not** the part layer's `0x40606000`. It is a whole-body skeleton
group, and it is where the creature anatomy lives:

```
group 0x41606100: 591 rw4 rows, 591 records with a validated skeleton,
                  1182 skeletons, 623 distinct tokens, 3074 bone values
token overlap with the part layer: 551 of 623 (88.4%)
whole-body tokens with a recovered name: 96
```

**13 of the 17 anatomical names are present in this group**, with bone-value
frequencies:

| name | hash | bones | name | hash | bones |
|---|---|---:|---|---|---:|
| `root` | `0x7BD05F71` | **190** | `joint1` | `0xD0BE09E0` | **375** |
| `eye` | `0x36932030` | 24 | `joint2` | `0xD0BE09E1` | 222 |
| `body` | `0x6DFE676D` | 2 | `joint3` | `0xD0BE09E2` | 146 |
| `hip` / `hipL` / `hipR` | `0x428AE404` / `0xC4A4F200` / `0xC4A4F21E` | 2 / 2 / 2 | `joint4`…`joint9` | `0xD0BE09E3`…`0xD0BE09E8` | 126 / 83 / 70 / 51 / 54 / 48 |
| `knee` / `kneeL` / `kneeR` | `0x9004000C` / `0xC24C12A8` / `0xC24C12B6` | 2 / 2 / 2 | `attach` | `0x0AC4AEEE` | 34 |
| `jaw` | `0x2E8647BD` | 2 | `tool` | `0xC045A9A3` | 32 |
| `shoulder` | `0x8B6725F9` | 2 | `effect` | `0x80D91E9E` | 23 |
| `iris` | `0x68D0ED08` | 2 | `Mesh` | `0x1372D766` | 12 |

Plus, unnamed in this report because they are not on the question's list and
because their grades are `Unconfirmed`: `pupil` (9), `eyebrow` (2), `eyeball` (4),
`Iris` (3), `toe`, `ankle`, `hand`, `finger2`, `finger3`, `arm1`, `arm2`, `arm3`,
`wing1`–`wing4`, `lookat`, `weapon`, `mast`, `lever`, `axle`, `shaft`, `leaf1`,
`leaf2`, `tip1`, `tip2`, `root1`, `rotate`, `bounce`, `spin`, `bar`, `center`,
`base`, `top`/`Top1`/`Top2`, `L1`/`L2`, `R1`/`R2`, `r1j1`, `r2j1`, `r2j2`,
`r3j1`, `r3j2`, `r4j1`, `r4j3`, `pipe1`, `pipe2`, `pipes`, `flags`, `spike`,
`left`, `right`, `upper`, `bottom`, `roof`, `Root`, `mid1`, `OG`, `dzsbu`,
`arm1a`, `arm1b`, `arm2a`, `arm2b`.

**And those identities do not appear inline in the trailer that points at them**:
the identical 8 889 410-word scan against this group's own 623-token vocabulary
gives 58 words over 3 distinct (chance 1.29) — the same three pointers — and
**0 witnessed hits**. Both facts are asserted together in the test, because
reporting only the first would read as "found in the anim data", which is the
finding this report exists to prevent.

## 7. Where this leaves `docs/CREATOR-CORE.md` §13.2

| candidate | status after this report |
|---|---|
| 1. `gmdl` anim data / baked deforms | **REFUTED**, on 11 236 365 words over 4 209 / 4 209 records: 0 witnessed names, 0 of 17 anatomical names. Its role is a **pointer**, not a store. |
| 2. `BlendShape (0xff0002)` / `MorphHandle (0xff0000)` parameter name tables | **untouched.** Still open, still the cheapest untried option. |
| 3. rigblock 4^k lattice indexing morph parameters | **untouched.** Still open. |

**Candidate 1 is answered, and its answer relocated the question**: the anatomy is
in `rw4` group `0x41606100`'s `Skeleton (0x70002)` sections, using the same
morph-parameter-name vocabulary as the part layer, reached by a resource key the
gmdl anim data carries. The `gmdl` layer is a *reference* to the body graph, not a
copy of it — which is consistent with `docs/CREATOR-CORE.md` §13.1 having already
found that a `gmdl` record is not an RW4 container and therefore cannot carry a
`0x70002` section of its own. That structural fact predicted this negative result
before it was measured.

## 8. The next single cheapest probe

**Read the `0x41606100` skeleton's parent structure — `spore-rigblock`'s RW4
skeleton body graph — over group `0x41606100` instead of `0x40606000`.**

It is cheapest because the target is already measured and named: 591 records,
1 182 skeletons, 623 tokens, 96 names. The existing `spore-rigblock` skeleton
reader needs only its group parameter widened, and the report-11 vocabulary keys
it directly, so the body graph — parented, named bones — should fall out with no
new format work at all. It is the difference between "the anatomy exists and is
named" and "the anatomy exists, is named, and is a graph".

The two open alternatives both cost more: candidate 2 needs an `0xff0002` section
reader that does not exist, and candidate 3 needs a correlational measurement
against the `gmsh` channel counts with no established link to test.

A cheaper *confirmation* of this report, if wanted: extend `is_key_shaped` to
require the pointed-at `rw4` group to be `0x41606100` and assert that the 255
distinct instance ids account for the 24 + 21 hits. That would make the pointer
routing itself an assertion rather than a `println!`.

## 9. Claim recorded

`crates/spore-gmdl/src/claims.rs`, subject **`anim_data_bone_identities`**, grade
**`OBSERVED`**. It states the counts that justify it: both word totals, both
record totals, the 0/170 and 0/17 results, the three explained tokens, and the
`0x41606100` pointer with its 623-token / 88.4 % / 96-named payload.

**The claims-count assertion was not touched, because there is none.**
`crates/spore-gmdl/src/claims.rs` has four tests; `every_claim_carries_a_grade_and_provenance`
asserts `is_available()` *per claim* and `subjects_are_unique_and_resolvable`
asserts uniqueness and resolvability. Neither asserts a total, and
`rg 'all_claims\(\)\.len\(\)|claims\(\)\.len\(\)'` over the workspace returns
nothing — `spore-gmdl::all_claims` has exactly one consumer, its own `lib.rs`
re-export. (The workspace *does* have fixed claim-count tests elsewhere —
`spore-semantic-bridge` asserts `ALL_CLAIMS.len() == 24`, `spore-cellcontent`
asserts its confirmed set — which is presumably the memory this instruction
refers to. None of them covers `spore-gmdl`.) Adding the claim moved no count
because no count exists, and nothing outside `crates/spore-gmdl/` was modified.