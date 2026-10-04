//! `stats`: corpus statistics, in the shape agents keep re-deriving by hand.
//!
//! # What this replaces
//!
//! Every number below has been produced at some point in this workspace by a
//! throwaway probe: a loop over an index, a `BTreeMap` in a scratch script, a
//! `grep | wc -l` over a decompiled listing. Each one was cheap to write and each
//! one risks disagreeing with the next, because none of them was a tool. This
//! command is the tool: deterministic, sorted, and running the same build twice
//! prints the same bytes.
//!
//! # The sections, and what each one is a measurement of
//!
//! | section | measured from | is a claim about |
//! |---|---|---|
//! | `types` | the index | the type table |
//! | `groups` | the index | the type table |
//! | `raster fourcc` | the first 32 bytes of each raster record | those records |
//! | `raster geometry` | the same 32 bytes | those records |
//! | `raster framing` | the record length and `field_18`, against the framing identity | those records |
//! | `decodes` | a full decode of each record of a decodable type | those records |
//! | `gmdl walk` | `spore_gmdl::parse_recovering` on each gmdl record | those records |
//! | `texture-set sizes` | the same recovering walk | those records |
//!
//! The distinction matters and is stated in the output: a count in `types` is
//! about the index, and one in `decodes` is about bytes that were actually
//! decoded. Neither is printed as the other.
//!
//! # `raster framing`, and the premise it replaced
//!
//! This command used to print a `raster layer splits` section whose rows were
//! either `derived` or the exact layer-split refusal, and a geometry histogram
//! keyed on a *derived* layer count. **A corpus byte count refuted both.** There
//! is no layer count in a raster record: every raster record measured is one
//! chain of mip levels, so the "split" closed on 2 954 of 2 954 records and the
//! section could print nothing but `derived`.
//!
//! What replaced it is the measurement that refuted it, so it is reproducible
//! from this tool rather than asserted in a comment:
//!
//! * **`record length`** — the record is exactly as long as
//!   `framing::record_size(fourcc, width, height, mip_count)` requires. That
//!   identity is *determined*, not approximate, which is why a disagreement is a
//!   refusal with both numbers rather than a remainder.
//! * **`field_18`** — the header word equals the base level's own byte count, in
//!   all three formats. It read as "format-specific" before only because the
//!   block size was wrong.
//!
//! A record whose fourcc has no frame gets its own row, because a format this
//! build cannot frame is an **absence**, not a refusal.
//!
//! # Unavailable is not zero
//!
//! A raster record too short to hold an envelope is counted under
//! `fourcc <envelope unreadable>` with its own row, not silently dropped from the
//! census and not folded into `DXT5`. A gmdl record whose walk stops before the
//! mesh table is counted as a failure with the decoder's own words attached. Every
//! section therefore covers every record it claims to cover, and where a count is
//! genuinely not applicable the section says so rather than printing `0`.
//!
//! # Memory
//!
//! One record is decoded at a time and dropped; what survives is a `BTreeMap` of
//! counters keyed by small tuples. Nothing accumulates proportional to the
//! package. The exception is the group histogram, which is bounded by the number
//! of distinct group ids — a property of the content, not of its size.
//!
//! Progress goes to `err` every [`PROGRESS_EVERY`] records and only when
//! `err` is a terminal, the same rule `verify` uses.

use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};

use spore_dbpf::DbpfEntry;

use crate::json::{hex_id, Json};
use crate::{StatsRequest, ToolError};

use super::verify::{classify, RecordClass};

/// How many records between progress lines, when stderr is a terminal.
pub const PROGRESS_EVERY: u64 = 10_000;

/// The default cap on a histogram's rows. `0` means "every row".
pub const DEFAULT_ROW_LIMIT: usize = 40;

/// What one raster record's envelope said.
///
/// Three states, kept apart because collapsing them invents data:
///
/// * `Shape` — the envelope was read, so a fourcc and three geometry words exist.
/// * `Short` — the record is shorter than a 32-byte envelope, so no fourcc and no
///   geometry exist to report at all.
///
/// # Why a refused framing still appears in the censuses
///
/// The fourcc and the three geometry words come *out of the envelope*, so they
/// are established even when the record length does not match the framing the
/// envelope implies. Dropping such a record from the fourcc census would make
/// the census cover fewer records than the type histogram says exist — a silent
/// hole in the very numbers this command exists to produce, and one the first run
/// over the real corpus opened: 373 of 2 954 `raster` records in
/// `Spore_Content.package` were exactly this case. So they are counted, with the
/// disagreement reported by name in the `raster framing` section.
///
/// That hole is now closed at the source rather than papered over here: the
/// framing is correct, so the refusal is gone too. The property above is
/// therefore no longer observable in the corpus, and is instead pinned by a
/// synthetic record whose length deliberately disagrees with its own envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RasterReading {
    /// The envelope was read.
    Shape {
        /// Offset `0x14`.
        fourcc: u32,
        /// Offset `0x04`.
        width: u32,
        /// Offset `0x08`.
        height: u32,
        /// Offset `0x0c`.
        mips: u32,
        /// The framing cross-check, keyed by label in the census.
        framing: Vec<(&'static str, String)>,
    },
    /// The record is shorter than a 32-byte envelope.
    Short,
}

/// Everything one walk of every loaded package measured.
#[derive(Debug, Clone, Default)]
pub struct Census {
    /// Per package: name, index rows, and stored bytes.
    pub packages: Vec<(String, usize, u64)>,
    /// Type id to record count.
    pub types: BTreeMap<u32, u64>,
    /// Group id to record count.
    pub groups: BTreeMap<u32, u64>,
    /// Raster fourcc to record count, `None` when the envelope was not read.
    pub fourcc: BTreeMap<Option<u32>, u64>,
    /// `(width, height, mip_count)` to raster record count. There is **no**
    /// layer-count column, because a raster record has no layer count and none
    /// is derived; see the module documentation for the refutation.
    pub raster_geometry: BTreeMap<(u32, u32, u32), u64>,
    /// Which framing cross-check, and with what outcome, to record count.
    ///
    /// Keys are the cross-check's name and a verdict: `("record length", "exact")`
    /// when the record is exactly as long as its own envelope implies, or the
    /// decoder's own words when it is not. Two axes rather than one map, because
    /// "the identity closes" and "`field_18` is the base level's byte count" are
    /// two independent claims and a single verdict would let one hide the other.
    pub raster_framing: BTreeMap<(String, String), u64>,
    /// `(class name, outcome)` to count, where outcome is the decoder's message
    /// or `"decoded"`.
    ///
    /// Keyed by the class *name* rather than by `RecordClass` so the map has a
    /// total order this crate owns; `RecordClass` deliberately derives no `Ord`,
    /// and inventing one here would be a second, weaker ordering rule.
    pub decodes: BTreeMap<(String, String), u64>,
    /// `(walk stage, error text)` to gmdl record count. `stage` is `complete`
    /// when the walk closed on the record end.
    pub gmdl_walk: BTreeMap<(String, String), u64>,
    /// Number of texture refs per gmdl record to record count.
    pub texture_set_sizes: BTreeMap<u64, u64>,
    /// How many raster records held no readable envelope.
    pub raster_unreadable: u64,
}

impl Census {
    /// One walk's summary lines, in the documented order.
    pub fn totals(&self) -> (u64, u64) {
        let index_rows: u64 = self.types.values().sum();
        let stored_bytes: u64 = self.packages.iter().map(|(_, _, bytes)| *bytes).sum();
        (index_rows, stored_bytes)
    }
}

/// `stats`: walk every loaded package once and report.
pub fn run(
    request: &StatsRequest,
    out: &mut dyn Write,
    err: &mut dyn Write,
    progress: bool,
) -> Result<(), ToolError> {
    let store = super::open_store(&request.packages)?;
    let mut census = Census::default();

    for package in store.packages() {
        census.packages.push((
            package.name().to_owned(),
            package.record_count(),
            package
                .index()
                .entries()
                .iter()
                .map(|e| e.memory_size as u64)
                .sum(),
        ));
        let image = package.bytes();
        let mut walked = 0u64;
        for entry in package.index().entries() {
            if request
                .type_id
                .is_some_and(|wanted| entry.type_id != wanted)
            {
                continue;
            }
            walk_record(entry, image, &mut census);
            walked += 1;
            if progress && walked % PROGRESS_EVERY == 0 {
                let _ = writeln!(
                    err,
                    "osptool: stats: {walked} record(s) walked in `{}`",
                    package.name()
                );
            }
        }
    }

    if request.json {
        writeln!(out, "{}", census_json(&census))?;
    } else {
        write_report(out, &census, request)?;
    }
    Ok(())
}

/// Measures one record into every section it belongs to.
///
/// One pass, one read: a raster record's envelope is read once and used by both
/// the fourcc census and the geometry census, so the two cannot disagree about
/// how many records each covers.
fn walk_record(entry: &DbpfEntry, image: &[u8], census: &mut Census) {
    *census.types.entry(entry.type_id).or_insert(0) += 1;
    *census.groups.entry(entry.group_id).or_insert(0) += 1;

    let class = classify(entry.type_id);
    match class {
        RecordClass::Raster => {
            match read_raster(entry, image) {
                Ok(RasterReading::Shape {
                    fourcc,
                    width,
                    height,
                    mips,
                    framing,
                }) => {
                    *census.fourcc.entry(Some(fourcc)).or_insert(0) += 1;
                    *census
                        .raster_geometry
                        .entry((width, height, mips))
                        .or_insert(0) += 1;
                    for (check, verdict) in framing {
                        *census
                            .raster_framing
                            .entry((check.to_owned(), verdict))
                            .or_insert(0) += 1;
                    }
                }
                Ok(RasterReading::Short) => {
                    // Read and counted: the envelope was not there. Not a
                    // fourcc and not a shape, so neither census covers it.
                    *census.fourcc.entry(None).or_insert(0) += 1;
                    census.raster_unreadable += 1;
                }
                // The bytes could not be extracted at all, so there is no
                // envelope to census. `record_decode` counts it as a per-record
                // decode failure with the container's words.
                Err(_) => {}
            }
            record_decode(census, class, entry, image);
        }
        RecordClass::Gmdl => {
            // Two measurements from one walk: the decode outcome, and the walk's
            // stopping point. `parse_recovering` is the walk that can stop and
            // still report where, which is the only way to count the `0x218`
            // stops without calling them failures.
            let bytes = match extract(entry, image) {
                Ok(bytes) => bytes,
                Err(message) => {
                    *census
                        .decodes
                        .entry((class_name(class).to_owned(), message.clone()))
                        .or_insert(0) += 1;
                    *census
                        .gmdl_walk
                        .entry(("not-read".to_owned(), message))
                        .or_insert(0) += 1;
                    return;
                }
            };
            match gmdl_decode(&bytes) {
                Ok(()) => {
                    *census
                        .decodes
                        .entry((class_name(class).to_owned(), "decoded".to_owned()))
                        .or_insert(0) += 1;
                }
                Err(error) => {
                    *census
                        .decodes
                        .entry((class_name(class).to_owned(), error))
                        .or_insert(0) += 1;
                }
            }
            // One walk, two measurements: the decode above and the recovering
            // walk here read the same bytes and answer different questions, so
            // they are recorded in different sections rather than merged into one
            // verdict.
            match spore_gmdl::parse_recovering(&bytes) {
                Ok(partial) => {
                    record_partial(census, &partial);
                    *census
                        .texture_set_sizes
                        .entry(partial.model.texture_refs.len() as u64)
                        .or_insert(0) += 1;
                }
                Err(error) => {
                    // The walk stopped inside the geometry, so the record has no
                    // texture-set size to report either: the material-info table
                    // was never reached.
                    *census
                        .gmdl_walk
                        .entry(("geometry".to_owned(), error.to_string()))
                        .or_insert(0) += 1;
                }
            }
        }
        RecordClass::Rw4 => record_decode(census, class, entry, image),
        RecordClass::Container | RecordClass::NoDecoder => {
            // A container and a no-decoder type are type-level facts: their
            // payloads were not read, so this command has no measurement to
            // report about them and prints none.
        }
    }
}

/// Decodes a gmdl record under **the same definition `verify` uses**: the strict
/// walk must close *and* every mesh must be extractable.
///
/// # Why not just `parse`
///
/// Measuring the corpus with `parse` alone reported 6 472 of 7 gmdl records as
/// failing; measuring it with parse plus per-mesh extraction reported 737. Both
/// numbers were correct and they answered different questions, which is exactly
/// the hazard this repository's own rule warns about — two commands, two
/// answers, one corpus. So `stats` now uses the stricter definition and the two
/// commands agree; the second measurement this section would have given (the
/// parse alone) is available from `verify --explain`, which reports both.
///
/// The `gmdl walk outcomes` section below deliberately keeps the *other*
/// definition, because "where did the recovering walk stop" is a question about
/// the walk and not about mesh extraction.
fn gmdl_decode(bytes: &[u8]) -> Result<(), String> {
    let model = spore_gmdl::parse(bytes).map_err(|error| error.to_string())?;
    for index in 0..model.mesh_count {
        spore_gmdl::mesh_from_gmdl(&model, index).map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Records where a recovering walk stopped.
fn record_partial(census: &mut Census, partial: &spore_gmdl::PartialGmdl) {
    let (stage, detail) = match &partial.stop {
        None => ("complete".to_owned(), "-".to_owned()),
        Some(stop) => (stop.stage.as_str().to_owned(), stop.error.to_string()),
    };
    *census.gmdl_walk.entry((stage, detail)).or_insert(0) += 1;
}

/// Decodes a record whose class has a decoder, counting the outcome.
fn record_decode(census: &mut Census, class: RecordClass, entry: &DbpfEntry, image: &[u8]) {
    let outcome = match extract(entry, image) {
        Err(message) => Err(message),
        Ok(bytes) => match class {
            RecordClass::Raster => spore_texture::decode_raster(&bytes)
                .map(|_| "decoded".to_owned())
                .map_err(|error| error.to_string()),
            RecordClass::Rw4 => spore_rw4::parse(&bytes)
                .map(|_| "decoded".to_owned())
                .map_err(|error| error.to_string()),
            RecordClass::Gmdl => spore_gmdl::parse(&bytes)
                .map(|_| "decoded".to_owned())
                .map_err(|error| error.to_string()),
            RecordClass::Container | RecordClass::NoDecoder => {
                unreachable!("walk_record only calls this for decoder classes")
            }
        },
    };
    // `decoded` and the decoder's own message are the same shape of outcome:
    // a per-record verdict, with the words attached. The two arms below are
    // written as one because they *are* one.
    let _ = outcome;
    match outcome {
        Ok(message) | Err(message) => {
            *census
                .decodes
                .entry((class_name(class).to_owned(), message))
                .or_insert(0) += 1;
        }
    }
}

/// Extracts one record's bytes, mapping the failure onto a message.
///
/// Used for the decodes section only: the gmdl section needs the bytes anyway and
/// counts its own read failure, so it does not go through here twice.
fn extract(entry: &DbpfEntry, image: &[u8]) -> Result<Vec<u8>, String> {
    spore_dbpf::extract_record(image, entry).map_err(|error| error.to_string())
}

/// Reads a raster record's envelope and runs the two framing cross-checks.
///
/// # The two checks, and why they are separate
///
/// * **Record length.** `framing::record_size(frame, width, height, mip_count)`
///   *determines* the record's length — header, bare base level, bare level 1 for
///   the block formats, then a `u32` count and that many bytes for every level
///   from [`FrameFormat::first_prefixed_level`] on. So a record that differs is a
///   record whose framing this build does not understand, and the refusal names
///   both numbers.
/// * **`field_18`.** The header word is the base level's own byte count in all
///   three formats. Checked separately, because "the length closes" and "the
///   header agrees with the length" are independent claims and a single verdict
///   would let one mask the other.
///
/// A `fourcc` with **no** frame is not a refusal: it is a format this build
/// cannot frame, which is an absence, and it gets its own row rather than being
/// folded into either check.
fn read_raster(entry: &DbpfEntry, image: &[u8]) -> Result<RasterReading, String> {
    let bytes = extract(entry, image)?;
    let envelope = match spore_texture::parse_envelope(&bytes) {
        Ok(envelope) => envelope,
        Err(spore_texture::TextureError::TruncatedEnvelope { .. }) => {
            return Ok(RasterReading::Short);
        }
        Err(error) => return Err(error.to_string()),
    };

    let mut framing = Vec::with_capacity(2);
    // The record-length verdict is the crate's own, including its zero-dimension
    // and zero-mip-count refusals: re-deriving them here would be a second,
    // weaker copy of the geometry check this section exists to reproduce.
    framing.push((
        "record length",
        match spore_texture::layer_count_for(bytes.len(), &envelope) {
            // `layer_count_for` answers a chain count, which is not the question
            // any more — there is one chain and it is not derived. What it still
            // owns is the framing identity, so only that verdict is kept.
            Ok(_) => "exact".to_owned(),
            Err(spore_texture::TextureError::UnsupportedFourcc { .. }) => format!(
                "no framing rule for fourcc {:#010x} (an absence, not a refusal)",
                envelope.fourcc
            ),
            Err(error) => error.to_string(),
        },
    ));

    if let Some(frame) = spore_texture::frame_for(envelope.fourcc) {
        // `field_18` is only comparable when the base level is addressable as a
        // `u32`, which is every record the corpus holds; a base level above
        // `u32::MAX` bytes has already been refused as a size mismatch, and
        // reporting it as a `field_18` failure would name the wrong problem.
        let base = spore_texture::framing::level_size(frame, envelope.width, envelope.height, 0);
        framing.push((
            "field_18",
            match u32::try_from(base) {
                Ok(base) if base == envelope.field_18 => "the base level's byte count".to_owned(),
                Ok(base) => format!(
                    "declares {} where the base level needs {base}",
                    envelope.field_18
                ),
                Err(_) => {
                    format!("base level of {base} bytes does not fit a u32 header word")
                }
            },
        ));
    }
    Ok(RasterReading::Shape {
        fourcc: envelope.fourcc,
        width: envelope.width,
        height: envelope.height,
        mips: envelope.mip_count,
        framing,
    })
}

/// One histogram row.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    label: String,
    count: u64,
}

/// The human report.
fn write_report(
    out: &mut dyn Write,
    census: &Census,
    request: &StatsRequest,
) -> Result<(), ToolError> {
    let (walked, stored_bytes) = census.totals();
    // `walked` is the count *of this scope*, which is not the package's index row
    // count when `--type` narrows it. Naming it "records walked" keeps the two
    // numbers from looking like a contradiction.
    writeln!(out, "records walked {walked}")?;
    writeln!(
        out,
        "stored bytes   {stored_bytes}  (whole index, not the scope)"
    )?;
    match request.type_id {
        Some(type_id) => writeln!(out, "scope          type {}", super::describe_type(type_id))?,
        None => writeln!(out, "scope          every record of every loaded package")?,
    }
    writeln!(out, "packages      (each package's whole index)")?;
    for (name, rows, bytes) in &census.packages {
        writeln!(out, "  {name:<24} {rows:>9} row(s)  {bytes:>12} byte(s)")?;
    }

    write_section(
        out,
        "types  (counted from the index; not a decode)",
        ranked(
            census
                .types
                .iter()
                .map(|(id, count)| (super::describe_type(*id), *count)),
        ),
        request.limit,
    )?;

    write_section(
        out,
        "groups  (counted from the index)",
        ranked(
            census
                .groups
                .iter()
                .map(|(id, count)| (group_label(*id), *count)),
        ),
        request.limit,
    )?;

    write_section(
        out,
        "raster fourcc  (read from the 32-byte envelope of every raster record)",
        ranked(census.fourcc.iter().map(|(fourcc, count)| {
            (
                match fourcc {
                    Some(word) => fourcc_label(*word),
                    None => "<envelope unreadable>".to_owned(),
                },
                *count,
            )
        })),
        0,
    )?;

    write_section(
        out,
        "raster geometry  (width x height x mip_count, all three read from the envelope;\n             there is no layer count in a raster record and none is derived)",
        ranked(
            census
                .raster_geometry
                .iter()
                .map(|((width, height, mips), count)| {
                    (format!("{width}x{height} mips={mips}"), *count)
                }),
        ),
        0,
    )?;

    // The corpus measurement that refuted the two-layer reading, made
    // reproducible here instead of asserted in a comment. Capped like `decodes`,
    // because a refusal carries a record-specific length.
    //
    // The title names the *identity* rather than a frozen count: the rows below it
    // carry whatever this package holds, and a caption that pinned "2954 of 2954"
    // would be a second place to go stale the moment the corpus did. The count that
    // established the rule is recorded in `docs/RASTER-FRAMING.md` and in this
    // crate's module documentation.
    write_section(
        out,
        "raster framing  (record length vs framing::record_size, and field_18 vs the base\n             level's own byte count \u{2014} the cross-check that refuted the derived\n             layer count; both axes closed on every raster record of this scope is\n             the expected reading, and anything else is named here in full)",
        ranked(
            census
                .raster_framing
                .iter()
                .map(|((check, verdict), count)| (format!("{check}: {verdict}"), *count)),
        ),
        request.limit,
    )?;

    // Capped, unlike the small censuses above: a decoder message can carry a
    // record-specific offset, so this section can end up with one row per failing
    // record — 118 near-identical rows on `Spore_Content.package` — which is
    // noise. The cap says how many it dropped.
    write_section(
        out,
        "decodes  (one full decode per record of a DECODABLE type; container and\n             no-decoder types are absent because their payloads were not read)",
        ranked(
            census
                .decodes
                .iter()
                .map(|((class, outcome), count)| (format!("{class} {outcome}"), *count)),
        ),
        request.limit,
    )?;

    write_section(
        out,
        "gmdl walk outcomes  (spore_gmdl::parse_recovering; a stop is not a failure)",
        ranked(
            census
                .gmdl_walk
                .iter()
                .map(|((stage, detail), count)| (format!("{stage}: {detail}"), *count)),
        ),
        0,
    )?;

    write_section(
        out,
        "texture-set sizes  (entries per gmdl record, from the same walk)",
        ranked(
            census
                .texture_set_sizes
                .iter()
                .map(|(size, count)| (format!("{size} texture ref(s)"), *count)),
        ),
        0,
    )?;
    Ok(())
}

/// Sorts a histogram deterministically: count descending, then label ascending.
///
/// The tie-break is the point. Two rows with the same count must not be ordered
/// by a `BTreeMap`'s key order in one section and by something else in another.
fn ranked(rows: impl Iterator<Item = (String, u64)>) -> Vec<Row> {
    let mut rows: Vec<Row> = rows.map(|(label, count)| Row { label, count }).collect();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    rows
}

/// One histogram section, capped and honest about the cap.
fn write_section(
    out: &mut dyn Write,
    title: &str,
    rows: Vec<Row>,
    limit: usize,
) -> Result<(), ToolError> {
    writeln!(out)?;
    writeln!(out, "{title}")?;
    if rows.is_empty() {
        writeln!(
            out,
            "  (no record of this scope produced a row here \u{2014} this is not a measurement of zero)"
        )?;
        return Ok(());
    }
    let shown = if limit == 0 {
        rows.len()
    } else {
        limit.min(rows.len())
    };
    for row in rows.iter().take(shown) {
        writeln!(out, "  {:>9}  {}", row.count, row.label)?;
    }
    if shown < rows.len() {
        writeln!(
            out,
            "  ({} further row(s) not shown \u{2014} raise --limit, or pass 0 for every row)",
            rows.len() - shown
        )?;
    }
    Ok(())
}

/// The canonical name of a record class, as a stable sort key.
///
/// `RecordClass` deliberately derives no `Ord`: the order of "which classes to
/// list" belongs to this report, and the `ALL`-style declaration order-style declaration order
/// is not something the enum exposes. Spelling the names here gives the census a
/// key it can sort on and a label it can print from the same value.
pub fn class_name(class: RecordClass) -> &'static str {
    match class {
        RecordClass::Gmdl => "gmdl",
        RecordClass::Rw4 => "rw4",
        RecordClass::Raster => "raster",
        RecordClass::Container => "container",
        RecordClass::NoDecoder => "no-decoder",
    }
}

/// A group id with its canonical name when one exists.
fn group_label(group_id: u32) -> String {
    match super::group_name_of(group_id) {
        Some(name) => format!("{:#010x} ({name})", group_id),
        None => format!("{group_id:#010x}"),
    }
}

/// A fourcc in the words the corpus supports, with the decode verdict attached.
///
/// The wording is [`super::describe_format`]'s, plus whether this build decodes
/// it — which today is true of every format the corpus contains. The block size
/// is in every label because it is the load-bearing number: the whole "two
/// layers" reading was 8 bytes charged per 4x4 block where DXT5 holds 16.
fn fourcc_label(fourcc: u32) -> String {
    if spore_texture::frame_for(fourcc).is_some() {
        format!("{} (decoded)", super::describe_format(fourcc))
    } else {
        format!(
            "{} \u{2014} not decoded by this build",
            super::describe_format(fourcc)
        )
    }
}

/// The `--json` document.
fn census_json(census: &Census) -> Json {
    let (index_rows, stored_bytes) = census.totals();
    Json::obj()
        .push("index_rows", Json::from(index_rows))
        .push("stored_bytes", Json::from(stored_bytes))
        .push(
            "packages",
            Json::Arr(
                census
                    .packages
                    .iter()
                    .map(|(name, rows, bytes)| {
                        Json::obj()
                            .push("name", name.clone())
                            .push("index_rows", Json::from(*rows))
                            .push("stored_bytes", Json::from(*bytes))
                    })
                    .collect(),
            ),
        )
        .push(
            "types",
            Json::Arr(
                census
                    .types
                    .iter()
                    .map(|(id, count)| {
                        Json::obj()
                            .push("type_id", hex_id(*id))
                            .push("name", Json::from(super::type_name_of(*id)))
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
        .push(
            "groups",
            Json::Arr(
                census
                    .groups
                    .iter()
                    .map(|(id, count)| {
                        Json::obj()
                            .push("group_id", hex_id(*id))
                            .push("name", Json::from(super::group_name_of(*id)))
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
        .push(
            "raster_fourcc",
            Json::Arr(
                census
                    .fourcc
                    .iter()
                    .map(|(fourcc, count)| {
                        Json::obj()
                            // `null`, never 0: an unreadable envelope has no
                            // fourcc, and fourcc zero is a different, false claim.
                            .push(
                                "fourcc",
                                match fourcc {
                                    Some(word) => hex_id(*word),
                                    None => Json::Null,
                                },
                            )
                            .push("label", Json::from(fourcc.map(fourcc_label)))
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
        .push(
            "raster_geometry",
            Json::Arr(
                census
                    .raster_geometry
                    .iter()
                    .map(|((width, height, mips), count)| {
                        Json::obj()
                            .push("width", Json::from(*width))
                            .push("height", Json::from(*height))
                            .push("mip_count", Json::from(*mips))
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
        .push(
            "raster_framing",
            Json::Arr(
                census
                    .raster_framing
                    .iter()
                    .map(|((check, verdict), count)| {
                        Json::obj()
                            .push("check", check.clone())
                            .push("verdict", verdict.clone())
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
        .push(
            "decodes",
            Json::Arr(
                census
                    .decodes
                    .iter()
                    .map(|((class, outcome), count)| {
                        Json::obj()
                            .push("class", class.clone())
                            .push("outcome", outcome.clone())
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
        .push(
            "gmdl_walk",
            Json::Arr(
                census
                    .gmdl_walk
                    .iter()
                    .map(|((stage, detail), count)| {
                        Json::obj()
                            .push("stage", stage.clone())
                            .push("detail", detail.clone())
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
        .push(
            "texture_set_sizes",
            Json::Arr(
                census
                    .texture_set_sizes
                    .iter()
                    .map(|(size, count)| {
                        Json::obj()
                            .push("texture_refs", Json::from(*size))
                            .push("count", Json::from(*count))
                    })
                    .collect(),
            ),
        )
}

/// Whether the process's stderr is a terminal.
pub fn stderr_is_terminal() -> bool {
    std::io::stderr().is_terminal()
}

/// Reads one top-level key out of a rendered document.
///
/// A test-side convenience so an assertion about the JSON can name the key it
/// cares about instead of matching on a whole rendered string. Implemented with
/// the crate's own strict reader, so a document that does not parse fails here
/// rather than passing a substring check.
#[cfg(test)]
trait PipeKey {
    fn pipe_key(self, key: &str) -> Json;
}

#[cfg(test)]
impl PipeKey for Json {
    fn pipe_key(self, key: &str) -> Json {
        let Json::Obj(pairs) = self else {
            panic!("a document's top level is an object")
        };
        pairs
            .into_iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("no key {key} in the document"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ranking_tie_break_is_by_label_so_two_runs_agree() {
        let rows = ranked(
            [
                (1u64, "b".to_owned()),
                (2, "a".to_owned()),
                (2, "c".to_owned()),
            ]
            .into_iter()
            .map(|(count, label)| (label, count)),
        );
        assert_eq!(
            rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(),
            vec!["a", "c", "b"],
            "equal counts sort by label, not by insertion or map order"
        );
    }

    #[test]
    fn an_empty_section_says_it_is_not_a_measurement_of_zero() {
        let mut out = Vec::new();
        write_section(&mut out, "title", Vec::new(), 0).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("title"));
        assert!(
            text.contains("not a measurement of zero"),
            "an empty histogram must not read as a count of zero: {text}"
        );
        assert!(!text.contains("         0"), "{text}");
    }

    #[test]
    fn a_capped_section_says_how_many_rows_it_dropped() {
        let rows: Vec<Row> = (0..5)
            .map(|index| Row {
                label: format!("row{index}"),
                count: (5 - index) as u64,
            })
            .collect();
        let mut out = Vec::new();
        write_section(&mut out, "title", rows, 2).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("row0") && text.contains("row1"));
        assert!(!text.contains("row2"), "{text}");
        assert!(text.contains("3 further row(s) not shown"), "{text}");
    }

    #[test]
    fn limit_zero_means_every_row_not_no_rows() {
        let rows: Vec<Row> = (0..5)
            .map(|index| Row {
                label: format!("row{index}"),
                count: 1,
            })
            .collect();
        let mut out = Vec::new();
        write_section(&mut out, "title", rows, 0).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("row4"), "{text}");
        assert!(!text.contains("further row"), "{text}");
    }

    #[test]
    fn a_fourcc_label_names_every_format_the_corpus_has_and_refuses_none_of_them() {
        // The corpus refuted the old premise. All three fourcc values present in
        // `Spore_Content.package` decode, so a label calling any of them
        // "refused" is a confident falsehood and this asserts against one.
        for fourcc in [
            spore_texture::DXT5_FOURCC,
            spore_texture::DXT1_FOURCC,
            spore_texture::LUMINANCE_FOURCC,
        ] {
            let label = fourcc_label(fourcc);
            assert!(label.contains("decoded"), "{fourcc:#010x}: {label}");
            assert!(
                !label.contains("refus"),
                "a decodable format must not be labelled refused: {label}"
            );
        }
        // The block sizes are the load-bearing numbers, and they are the ones the
        // refuted reading got wrong: DXT5 is 16 bytes per 4x4 block.
        assert!(fourcc_label(spore_texture::DXT5_FOURCC).contains("16 bytes per 4x4 block"));
        assert!(fourcc_label(spore_texture::DXT1_FOURCC).contains("8 bytes per 4x4 block"));
        assert!(fourcc_label(spore_texture::LUMINANCE_FOURCC).contains("four bytes per texel"));
        // Anything with no framing rule gets no decode claim at all, and the
        // absence is worded as an absence rather than as a verdict on the record.
        let unknown = fourcc_label(0xdead_beef);
        assert!(unknown.contains("no measured framing rule"), "{unknown}");
        assert!(!unknown.contains("(decoded)"), "{unknown}");
    }

    #[test]
    fn the_geometry_histogram_has_no_layer_column_to_be_wrong_about() {
        // There is no layer count in a raster record, so there is nothing to
        // render as `layers=unavailable` — and nothing to render as `layers=1`
        // either, which would be a measurement of a field that does not exist.
        let mut census = Census::default();
        census.raster_geometry.insert((512, 512, 10), 2);
        census
            .raster_framing
            .insert(("record length".to_owned(), "exact".to_owned()), 2);
        census.raster_framing.insert(
            (
                "field_18".to_owned(),
                "the base level's byte count".to_owned(),
            ),
            2,
        );
        let Json::Arr(rows) = census_json(&census).pipe_key("raster_geometry") else {
            panic!("expected an array")
        };
        let Json::Obj(row) = &rows[0] else {
            panic!("expected an object")
        };
        let keys: Vec<&str> = row.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, vec!["width", "height", "mip_count", "count"]);
        assert!(
            !row.iter().any(|(k, _)| k.contains("layer")),
            "a derived layer count is the refuted premise: {keys:?}"
        );

        // The framing section is a measurement, not a status line: both axes are
        // present, each with its own count.
        let Json::Arr(checks) = census_json(&census).pipe_key("raster_framing") else {
            panic!("expected an array")
        };
        assert_eq!(checks.len(), 2, "{checks:?}");
    }

    #[test]
    fn an_unreadable_envelope_is_a_json_null_never_a_zero_fourcc() {
        let mut census = Census::default();
        census.fourcc.insert(None, 3);
        let Json::Arr(rows) = census_json(&census).pipe_key("raster_fourcc") else {
            panic!("expected an array")
        };
        let Json::Obj(row) = &rows[0] else {
            panic!("expected an object")
        };
        assert_eq!(
            row.iter().find(|(k, _)| k == "fourcc").map(|(_, v)| v),
            Some(&Json::Null),
            "a missing fourcc is null; fourcc 0 is a different, false claim"
        );
    }

    #[test]
    fn a_fourcc_with_no_framing_rule_is_censused_and_labelled_an_absence() {
        // A record whose envelope is readable but whose format this build cannot
        // frame. Its fourcc and geometry are still established by the envelope, so
        // it is counted; the framing section records the absence.
        let mut census = Census::default();
        census.fourcc.insert(Some(0xdead_beef), 1);
        census.raster_geometry.insert((4, 4, 1), 1);
        census.raster_framing.insert(
            (
                "record length".to_owned(),
                "no framing rule for fourcc 0xdeadbeef (an absence, not a refusal)".to_owned(),
            ),
            1,
        );
        let document = census_json(&census);
        let Json::Arr(rows) = document.clone().pipe_key("raster_fourcc") else {
            panic!("expected an array")
        };
        let Json::Obj(row) = &rows[0] else {
            panic!("expected an object")
        };
        let label = row
            .iter()
            .find(|(k, _)| k == "label")
            .map(|(_, v)| v.clone())
            .expect("a label");
        assert_eq!(label, Json::from(fourcc_label(0xdead_beef)));
        assert!(
            !label.to_string().contains("refused"),
            "an absent framing rule is not a refusal of the record: {label}"
        );
        // No `field_18` verdict for a format this build cannot size: there is no
        // base level to compare against, and inventing one would be the old bug.
        let Json::Arr(checks) = document.pipe_key("raster_framing") else {
            panic!("expected an array")
        };
        assert_eq!(checks.len(), 1, "{checks:?}");
    }

    #[test]
    fn an_unreadable_envelope_is_counted_and_never_folded_into_a_format() {
        let mut census = Census::default();
        census.fourcc.insert(None, 5);
        census.fourcc.insert(Some(spore_texture::DXT5_FOURCC), 9);
        census.raster_unreadable = 5;
        assert_eq!(census.fourcc[&None], 5);
        assert_eq!(census.fourcc[&Some(spore_texture::DXT5_FOURCC)], 9);
        assert_eq!(census.raster_unreadable, 5);
        // The geometry census covers only the records whose envelope was read,
        // which is why it is a separate map and not a derived one.
        assert!(census.raster_geometry.is_empty());
        // And a record with no envelope has no framing to cross-check.
        assert!(census.raster_framing.is_empty());
    }

    #[test]
    fn the_census_json_parses_back_as_the_document_it_is() {
        let mut census = Census::default();
        census.types.insert(1, 2);
        census.fourcc.insert(Some(spore_texture::DXT5_FOURCC), 2);
        let document = census_json(&census);
        assert_eq!(crate::json::parse(&document.render()), Ok(document));
    }

    #[test]
    fn totals_are_the_sum_of_the_type_histogram_and_the_package_stored_bytes() {
        let mut census = Census::default();
        census.types.insert(1, 3);
        census.types.insert(2, 4);
        census.packages.push(("a".to_owned(), 7, 100));
        census.packages.push(("b".to_owned(), 0, 20));
        assert_eq!(census.totals(), (7, 120));
    }
}
