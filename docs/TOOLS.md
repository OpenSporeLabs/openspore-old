# `osptool` — the offline inspector

`osptool` (`crates/spore-tools`) answers questions about a Spore DBPF package
without loading it into an engine: what records it holds, which one is this
identity, what does this record decode to, what does the whole corpus look like,
and is this package internally consistent.

It is a library first: the binary is a three-line shim over
`spore_tools::main_with`, so every command can be driven from a test with an
in-memory sink instead of a subprocess and a temporary file.

```
cargo run -p spore-tools -- --help
cargo run -p spore-tools -- manifest SPORE/Data/Spore_Content.package --stats
```

## The two rules every line obeys

**1. A statement about a record is either `decoded` or `type-level`, and it says
which.** "This build has a decoder family for `gmdl`" is a fact about the *type
table*; "this record decoded to one mesh of 50 vertices" is a fact about 1 266
bytes. `describe`, `verify --explain` and `stats` all label their sections, and a
type with **no decoder prints the index extent and says the payload was not
parsed** — never a summary that looks decoded. That is the tool's honesty
contract and it is enforced in three places: the wording, the `RecordClass`
partition in `commands::verify`, and tests that assert the absence of a summary
line.

**2. Unavailable is never zero, empty, or `false`.** Every optional fact is
carried in `spore_core::Fact` and printed in its envelope. A missing `value` key
is not `"UNKNOWN"`; a `null` fourcc is not fourcc `0`; a fourcc with no measured
framing rule is an **absence**, not a verdict on the record. An empty histogram
says *"no record of this scope produced a row here — this is not a measurement of
zero"* rather than printing `0`.

## Exit codes

Stable, and the same table the Python tools and `spore_engine::cli` use.

| code | meaning |
|---|---|
| `0` | ok. Also: a measurement whose *result* is a failure or a difference (`verify`, `stats`, `graph`, `diff`), and a record found whose type has no decoder. |
| `2` | bad command line. |
| `3` | record identity not present in any loaded package, or no package loaded. |
| `4` | found, but the container layer refused it — including a `texture --layer`/`--mip` that does not exist in the record read. |
| `5` | I/O failure, including a package that could not be opened. |

## Output discipline

**stdout carries only what was asked for.** Progress and diagnostics go to stderr.
With `--json`, stdout is the JSON document *alone* and the human report moves to
stderr, so `osptool stats p.package --json | jq` works.

## The subcommands

### `list` / `find` / `describe` / `extract`

Index rows in DBPF file order (`list`), identity resolution with near-miss
candidates (`find`), a decoded/type-level structural summary (`describe`), and a
raw byte copy into a directory the tool validates (`extract`).

### `texture` — decode a raster record and write a real PNG

```
osptool texture <package> <T:G:I> --out <file.png> [--layer N] [--mip N]
                                           [--channels auto|stored] [--json]
```

This is the command the workspace lacked: every raster decode used to be verified
by printing statistics, and statistics cannot tell a correct decode from a
plausible wrong one.

**What it had to do, and why.** The corpus stores its `0x15` family **Z first**.
`spore_texture::decode_raster` returns four bytes per texel in *file* order, so
writing that buffer straight out as RGBA gives an image with the right
dimensions, a valid PNG header, and its channels transposed. The flat-normal
records — the ones whose three first lanes are the literal constant
`(255, 128, 128)` — come out **red** where they should be blue. Nothing in the
file is malformed.

So `--channels auto` (the default) swizzles the `0x15` family from stored
`Z, X, Y, mask` to `R=X, G=Y, B=Z, A=mask`, and **says so on stderr and in
`channel_order_detail` in the JSON**. That swizzle is a *presentation* choice, not
a claim about the game: `spore_texture::claims::LUMINANCE_CHANNEL_SWIZZLE` is
deliberately `Unavailable` (what is observed is that the decoder emits the stored
bytes with no swizzle; whether a consumer *should* re-index is not established).
The JSON therefore carries

```json
"swizzle_should_be_applied": {"state":"unavailable","evidence_level":"UNKNOWN",
                              "evidence_state":"MISSING",
                              "reason":"luminance_0x15_channel_swizzle",
                              "provenance":[]}
```

and `--channels stored` writes the file order unchanged for anyone who wants to
see it.

**The `0x15` lane claims are scoped to the `0x15` family.** Both block formats are
written exactly as decoded, and for them the report says the lane layout is *not
applicable* (`not_applicable_to_this_fourcc`) rather than printing the normal-map
claim or staying silent. Silence would read as "settled for this format"; it is
the opposite of the truth. `stored_lane_layout` is an `unavailable` fact in the
JSON for those records, so it carries no `value` a caller could mistake for a
claim.

**All three formats in the corpus decode, and the report names each one.**

| `fourcc` | codec | block | corpus records in `Spore_Content.package` |
|---|---|---|---|
| `0x35545844` | DXT5/BC3 | **16** bytes per 4x4 block | 1 747 |
| `0x31545844` | DXT1/BC1 | **8** bytes per 4x4 block | 306 |
| `0x00000015` | uncompressed | four bytes per texel | 901 |

`DXT5` is never swizzled. Its documented decode produces red in `{0, 8}` with
green and blue `0` on every texel, because Spore's blocks depart from BC3 in four
documented ways. That image looks broken and is not, so the report says so every
time rather than leaving a reader to conclude the tool is wrong.

`DXT1` is also never swizzled, and it is **not** a mask: BC1 carries no alpha, so
every texel is written `A = 255`, and a real corpus record expands to thousands of
distinct RGBA values. Until a corpus byte count corrected the framing this tool
named the format *"present in the corpus, refused by name"* while decoding all 306
of its records — a census that calls a decodable format refused is a confident
falsehood, and `stats`, `describe` and `verify --explain` now all say `DXT1/BC1`
with its 8-byte block.

**Measured over the corpus.** `osptool stats SPORE/Data/Spore_Content.package`
reports 2 954 raster records: 1 747 DXT5, 901 `0x15`, 306 DXT1, and **2 954
decoded, zero refusals**. For `0x2f4e681c:0x40652902:0x0e5c7ee1` (a 512² `0x15`
record) the stored-order PNG is red-dominant on 262 144 of 262 144 texels and the
swizzled PNG is blue-dominant on 262 144 of 262 144 — the whole point of the
command, and the number a re-run should reproduce.

**Where it writes.** `--out` is required and `validate_out_file` refuses any path
that resolves inside this repository (found by walking up for the ancestor
holding both a `Cargo.toml` and a `crates/`). There is no default destination.

**What the PNG is.** 8-bit RGBA, colour type 6, filter `0` on every scanline,
non-interlaced, deflate **stored** blocks inside a zlib wrapper — see
`src/png.rs`. Correct and readable everywhere, and large: a 512² level is 1 MiB
where a real compressor would manage a fraction of that. The tool exists to make
a decode inspectable, not to ship assets, and `spore-tools` will not grow a
compression dependency for a size nobody needs. `png::verify` re-reads the file
through an independent path (chunk walk, every CRC-32, the zlib Adler-32) so the
encoder is checked rather than asserted.

### `graph` — the resource dependency graph

```
osptool graph <T:G:I> --package <p> [--package <p>...] [--depth N]
                          [--kinds a,b,...] [--dependants] [--limit N] [--json]
```

No graph logic lives here. `spore_assets::ResourceGraph` builds the edges, grades
each kind with `EvidenceLevel`, counts dangling references and reports every
record it could not read with the reason. `--kinds` narrows
`GraphOptions` itself, so a caller who wants one kind's edges does not pay for
the other three; the report prints the kinds it actually built.

Dangling references are reported **per edge kind with counts**, because for
`gmdl->texture` and `gmdl->reference` — references some record actually names —
a dangling edge is the normal case when the referring package is not loaded. A
count on an adjacency kind would mean the graph is inconsistent, and the report
says so if it ever sees one.

An **absent root exits 0** and prints `root present no`: a record can be named
by loaded records while itself being absent, and "which records reference this
missing texture?" is worth answering. The loaded package set is printed so the
answer can be read with its context.

### `stats` — corpus statistics

```
osptool stats <package...> [--type 0x..] [--limit N] [--json]
```

Every number here was, at some point, produced by a throwaway probe in this
workspace. Sections: `types`, `groups`, `raster fourcc`, `raster geometry`,
`raster framing`, `decodes`, `gmdl walk outcomes`, `texture-set sizes`.

Two properties are structural rather than hopeful:

* **Coverage.** The fourcc census sums to the raster count in the type histogram,
  and the gmdl-walk section sums to the gmdl count. A record whose envelope was
  read is in the fourcc *and* geometry censuses even when its framing does not
  close, because its fourcc and its three geometry words are established by the
  envelope itself; the disagreement is reported by name in `raster framing`.
  (The first run over `Spore_Content.package` caught a hole in exactly this
  property: 373 records were falling out of the census, and the census then read
  "1374 DXT5" where the truth is 1747. The framing is now correct so the corpus
  can no longer exercise the hole, and `tests/stats.rs` pins the property with a
  synthetic record whose length deliberately disagrees with its own envelope.)
* **Determinism.** Every histogram is sorted by `(count descending, label
  ascending)`, so equal counts cannot be ordered differently in two sections, and
  two runs are byte-identical.

### `raster framing`, and the premise it replaced

This section **replaced** `raster layer splits`. That section's rows were either
`derived` or the exact layer-split refusal, and its geometry histogram was keyed on
a *derived* layer count — and once the framing was corrected it could print nothing
but `derived 2954`. A section that can only ever print one thing is not a
measurement.

What is there now is the measurement that refuted it, so it is reproducible from
the tool rather than asserted in a comment. Two independent axes, because "the
length closes" and "the header agrees with the length" are separate claims and one
verdict would let either mask the other:

* **`record length: exact`** — the record is exactly as long as
  `framing::record_size(fourcc, width, height, mip_count)` requires. That identity
  is *determined* (header, bare base level, bare level 1 for the block formats,
  then a `u32` count and that many bytes per level from
  `first_prefixed_level` on), which is why a disagreement is a refusal with both
  numbers rather than a remainder.
* **`field_18: the base level's byte count`** — verified in **all three** formats.
  It read as "format-specific" before only because the block size was wrong.

Over `Spore_Content.package` both close on 2 954 of 2 954. A fourcc with no
measured framing rule gets its own row worded as an **absence**, and gets no
`field_18` verdict at all, because with no frame there is no base level to size.

One record is decoded at a time and dropped; only counters survive. Progress goes
to stderr every 10 000 records and only when stderr is a terminal.

### `diff` — record-level differences between two packages

```
osptool diff <packageA> <packageB> [--bytes] [--limit N] [--json]
```

Compares **revision sets**, not first-row-wins: every identity is mapped to the
multiset of index rows carrying it in each package, and the multisets are
compared. The stock install's globals record `0x2a3ce5b7:0x00000000:0xa426730b`
is 264 bytes in `Spore_Game.package` and 276 in `PatchData.package`; a per-key
lookup that keeps only the first row would call it unchanged.

Index rows only by default — a pure function of the index, so it runs over the
largest package in the install in milliseconds. `--bytes` also extracts and
compares content, and reports records whose index rows are identical but whose
bytes are not: a non-zero count means the index does not determine the content,
which matters to any cache keyed by `(type, group, instance, size)`.

Differences are the output, so the exit code is 0.

### `verify` and `verify --explain`

`verify` walks every record and counts `decoded / container / no decoder /
failed`, streaming one record at a time.

`--explain <T:G:I>` replaces the walk with a diagnosis of **one** record.
Diagnosing a bulk count is the slowest part of corpus work, and the diagnosis
reuses the same `classify`/`decode` path as the tally, so the two cannot disagree.
Beyond the index row and the verdict it prints:

* for a **raster** refusal, the envelope — the fourcc is what the walk dispatched
  on, so it is the answer;
* for a **gmdl** refusal, where the *recovering* walk stopped and how much
  survived. A record refused at the material-info table looks identical to one
  refused inside the geometry from the tally alone; the recovering walk says
  `stopped at the material-info table` and `1 mesh(es) ... decoded and validated`,
  which is the difference between "619 records failed" and "619 records have
  complete validated geometry and an unmeasured tail";
* for a type with **no decoder**, the index extent and the sentence "nothing was
  parsed" — and no summary line.

### `manifest` / `types`

`manifest` is the canonical one-row-per-record report, byte-identical across
runs; `types` prints the type and group tables plus a package histogram.

## Testing

```bash
cargo fmt --all
cargo test  -p spore-tools
cargo clippy -p spore-tools --all-targets -- -D warnings
```

323 tests, all hermetic: synthetic packages built byte by byte in
`tests/common/mod.rs`, synthetic raster records built by
`spore_texture::framing`'s own sizing functions (the builder asserts its output is
exactly the length the framing identity requires, so a builder that disagreed with
the decoder would fail at build time rather than produce a record the decoder
refuses), synthetic `0x15` records carrying the corpus's own flat normal, and no
game install, no window and no GPU. `crates/spore-tools` is Bevy-free and must stay
so.

Corpus tests live in `tests/real_corpus.rs`, `#[ignore]`d, and **return early
with a printed note** when `SPORE/` is absent:

```bash
cargo test -p spore-tools --release --test real_corpus -- --ignored --nocapture
```

## Cross-crate findings

Each was found while building these commands. **2 is resolved** — the owning crate
fixed it and this crate stopped reporting the old model; **1, 3 and 4 are open**
and each belongs to the crate that owns the format.

1. **`tests/fixtures/mini.gmdl` encodes its `refCount` key words
   little-endian.** `spore-gmdl` now reads the whole table big-endian, which the
   corpus settles (7 989 of 7 989 declared keys resolve big-endian, 0
   little-endian). Against the committed fixture the type word decodes as
   `0xe5bce600` instead of `0x00e6bce5`, so the fixture needs regenerating by
   whoever owns `tests/fixtures/`. `crates/spore-tools/tests/describe.rs` no
   longer asserts the byte order — it compares `describe`'s rendering against
   `spore_gmdl::parse`'s own output, because the byte order is not this crate's
   claim.

2. **The old raster layer model is gone, and `osptool` no longer reports it.**
   The previous build charged 8 bytes per 4x4 block and divided the payload by
   `16 + chain`, which is how it derived *two layers* for every DXT5 record — two
   errors that cancelled exactly: a 16-byte block read at 8 bytes per block is two
   half-pictures, and two of them make one picture. It refused 373 of the 2 954
   `raster` records in `Spore_Content.package` for "payload bytes is not a multiple
   of the 43720-byte layer stride", and `describe` printed a derived layer count of
   2 with two identical-looking per-layer dimension lists.

   **Fixed in `spore-texture`**, whose corpus measurement is the authority: one
   chain, no layer headers, 16 bytes per block for DXT5 and 8 for DXT1, and
   `field_18` as the base level's own byte count in all three formats. This crate's
   job was to stop *reporting* the refuted model:

   * `stats` no longer has a `raster layer splits` section — it could only print
     `derived` — and its geometry histogram has no layer column;
   * `describe` prints `mip chains 1` with the reason inline, says `field_18` is
     read and checked (it used to say "UNRESOLVED, never read by any decoder" about
     the field that gates every raster decode), and says `field_1c` is not a header
     field at all in the 28-byte `0x15` header;
   * `find`, `verify` and `verify --explain` print "one chain" rather than a
     layer count;
   * `texture --layer N` still exists, refuses `N != 0` by name, and says a raster
     record is one chain.

   `osptool stats SPORE/Data/Spore_Content.package --type 0x2f4e681c` now reports
   `2954 raster decoded` with no refusal row, and `raster framing` closes on
   2 954 of 2 954 on both axes.

3. **All 132 dangling `gmdl->texture` references in `Spore_Content.package` name
   the same target**, `0x2f4e681c:0x00000000:0x552a5077` — group id `0`. Either
   one record is missing from the package, or the group word of that texture-set
   entry is read as `0`. Spore does use group `0x00000000` for at least one real
   record (the globals record above), so both readings are open.

4. `graph`'s numbers match `spore-assets`' documented measurements exactly
   (16 628 declared texture-set entries → 11 363 edges; 7 989 reference-table
   keys → 6 076; 132 dangling texture references), which is the cross-check that
   the tool is not deriving them a second way.