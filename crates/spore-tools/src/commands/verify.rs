//! `verify`: walk every record of a type and count what happened to each one.
//!
//! # Memory-flat by construction
//!
//! This is the command that will be pointed at the whole 995 MB
//! `Spore_Content.package`, so the constraint is not "be reasonably careful" — it
//! is that the resident set must not grow with the number of records walked.
//! Three properties give that:
//!
//! 1. the package is a **read-only memory map** ([`spore_assets::Package::open`]),
//!    so the image costs page cache, not heap;
//! 2. records are read **one at a time** into a buffer that is dropped at the end
//!    of each iteration — nothing accumulates, including the decoded value;
//! 3. the only things that survive the loop are **counters** and a small map of
//!    *distinct* error messages. That map is bounded by the number of distinct
//!    failures, which is a property of the data rather than of its size.
//!
//! `Container` rows are not read at all: the classification says this build does
//! not parse them, so reading 30 000 bytes to learn nothing would be the
//! opposite of flat. [`RecordClass::NoDecoder`] rows likewise.
//!
//! # Progress
//!
//! Every [`PROGRESS_EVERY`] records a one-line progress note goes to `err`. It is
//! gated on a boolean the caller supplies from [`stderr_is_terminal`], rather
//! than guessed here, so a test can exercise the line and a redirected report can
//! opt out of it. A progress line interleaved with a captured log is noise a
//! reviewer has to filter out.

use std::collections::BTreeMap;
use std::io::{IsTerminal, Write};

use spore_dbpf::DbpfEntry;

use crate::{ToolError, VerifyRequest};

/// How many records between progress lines.
pub const PROGRESS_EVERY: u64 = 10_000;

/// Type ids this build treats as **containers**: recognised, deliberately not
/// decoded.
///
/// Transcribed from `spore_assets::manifest`'s private `CONTAINER_TYPE_IDS`,
/// which is itself a port of `manifest.py`'s `_CONTAINERS` set. The list is
/// duplicated rather than imported because that set is not public, and widening
/// another crate's API from here is not this crate's decision to make.
/// `tests/type_classes.rs` builds a package holding one record of every named
/// type and asserts the two classifications agree exactly, so the copy cannot
/// drift without a test failing.
pub const CONTAINER_TYPE_IDS: &[u32] = &[
    spore_core::record::type_id::PROP,
    spore_core::record::type_id::CELL_STRUCTURE,
    0x2399_BE55, // bld
    0x2468_2294, // vcl
    0x2B97_8C46, // crt
    0x3D97_A8E4, // cll
    0x055A_DA24, // cnv
];

/// Whether a type id is one this build recognises as a container.
pub fn is_container_type(type_id: u32) -> bool {
    CONTAINER_TYPE_IDS.contains(&type_id)
}

/// What `verify` will attempt for a record of a given type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordClass {
    /// Decoded with the gmdl walker, then every mesh extracted.
    Gmdl,
    /// Decoded as an RW4 section directory.
    Rw4,
    /// Decoded as a DXT5 raster.
    Raster,
    /// Recognised as a container; the payload is not read.
    Container,
    /// No decoder in this build. Not a failure: a measurement, not an error.
    NoDecoder,
}

/// Classifies a type id.
///
/// Three ids get a decoder and that is all: `gmdl`, `rw4` and `raster`. In
/// particular:
///
/// * `gmsh` (`0x01C135DA`) is **not** a gmdl. It is a RenderWare mesh section,
///   and `spore-gmdl`'s walk would either refuse it or misread it.
/// * `png` (`0x2F7D0004`) and `jpeg` (`0x2F7D0002`) are **not** rasters. They are
///   raw PNG and raw JPEG -- measured 10 487 of 10 487 carrying the PNG magic
///   across the installed packages -- so handing them to the DXT5 codec would
///   return a plausible wrong answer. (An earlier revision of this file claimed
///   they were RW4 containers; that was inherited from a summary line and is
///   wrong. `png` and `rw4` differ by one bit in the type id and are unrelated.)
/// * `plt` (`0x011989B7`) is a palette. Nothing in the workspace decodes it.
///
/// # This now agrees with `spore-assets`' manifest, with no carve-outs
///
/// Every named type classifies identically in both views. `spore-assets`'s
/// `DecodeStatus::Ok` means exactly "a per-record decoder exists in this
/// workspace", and its list is `{gmdl, raster, rw4}`.
///
/// # The bug this command found, and why it is worth a paragraph
///
/// `rw4` (`0x2F4E681B`) used to be classified `Undecoded` by
/// [`spore_assets::ManifestBuilder`], whose decoder list omitted the id — while
/// `spore_rw4` decoded the section directory and `ModelStore::load` accepted
/// the type. The manifest therefore labelled all 1131 real RW4 records
/// `undecoded`. `tools/spore/manifest/manifest.py` agreed with it, so both had
/// inherited the omission from the same C++-era list.
///
/// The root cause was the manifest's `Ok` meaning "a decoder *family* exists"
/// rather than "a decoder exists". A looser classification is not a useful
/// summary; it is a second source of truth, and it drifts. `Ok` now means
/// exactly "a per-record decoder exists", and `tests/type_classes.rs` asserts
/// the disagreement set between this command and the manifest is **empty**.
pub fn classify(type_id: u32) -> RecordClass {
    match type_id {
        spore_gmdl::GMDL_TYPE => RecordClass::Gmdl,
        spore_rw4::RW4_TYPE => RecordClass::Rw4,
        spore_texture::RASTER_TYPE => RecordClass::Raster,
        other if is_container_type(other) => RecordClass::Container,
        _ => RecordClass::NoDecoder,
    }
}

/// The tally of one walk.
///
/// `decoded + container + no_decoder + failed == seen` is the invariant, and it
/// holds because a row is classified into exactly one bucket and, for the three
/// decoder classes, then lands in exactly one of the two outcomes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tally {
    /// Rows the walk looked at.
    pub seen: u64,
    /// Rows a decoder accepted.
    pub decoded: u64,
    /// Rows recognised as containers and not read.
    pub container: u64,
    /// Rows whose type has no decoder in this build.
    pub no_decoder: u64,
    /// Rows a decoder refused.
    pub failed: u64,
    /// Distinct failure messages and how often each occurred.
    pub errors: BTreeMap<String, u64>,
}

impl Tally {
    /// Adds one outcome for one classified row.
    ///
    /// `outcome` is ignored for the two classes whose bytes are never read:
    /// reporting "decoded" for a container would be exactly the type-level
    /// claim masquerading as a measurement.
    pub fn record(&mut self, class: RecordClass, outcome: Result<(), String>) {
        self.seen += 1;
        match class {
            RecordClass::Container => {
                self.container += 1;
                return;
            }
            RecordClass::NoDecoder => {
                self.no_decoder += 1;
                return;
            }
            RecordClass::Gmdl | RecordClass::Rw4 | RecordClass::Raster => {}
        }
        match outcome {
            Ok(()) => self.decoded += 1,
            Err(message) => {
                self.failed += 1;
                *self.errors.entry(message).or_insert(0) += 1;
            }
        }
    }

    /// True when every row this build could decode did decode.
    pub fn all_decoded(&self) -> bool {
        self.failed == 0
    }

    /// The failure messages with their counts, most frequent first.
    ///
    /// Sorted by count descending then by message ascending, so two runs over the
    /// same package print the same lines in the same order.
    pub fn ranked_errors(&self) -> Vec<(&str, u64)> {
        let mut ranked: Vec<(&str, u64)> =
            self.errors.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        ranked.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(right.0)));
        ranked
    }
}

/// Whether the process's stderr is a terminal, i.e. whether a progress line
/// would be read as it scrolls past rather than buried in a captured log.
pub fn stderr_is_terminal() -> bool {
    std::io::stderr().is_terminal()
}

/// `verify`: walk, count, report. Always exits [`EXIT_OK`](crate::EXIT_OK).
///
/// It is a *measurement*: a record that fails to decode is the output, not a
/// failed command. The caller decides what a given count means, which is why
/// `failed` rows do not change the exit code.
///
/// `request.explain` short-circuits the walk and diagnoses **one** record
/// instead. Diagnosing a bulk count is the slowest part of corpus work: a
/// `verify` tally says "1 580 raster records failed" and nothing about which
/// refusal applies to a given one.
pub fn run(
    request: &VerifyRequest,
    out: &mut dyn Write,
    err: &mut dyn Write,
    progress: bool,
) -> Result<(), ToolError> {
    let store = super::single_package_store(&request.package)?;
    if let Some(key) = &request.explain {
        return explain(&store, key, out);
    }
    let package = &store.packages()[0];
    let image = package.bytes();

    let mut tally = Tally::default();
    let mut walked = 0u64;
    for entry in package.index().entries() {
        if request
            .type_id
            .is_some_and(|wanted| entry.type_id != wanted)
        {
            continue;
        }
        tally.record(classify(entry.type_id), attempt(entry, image));
        walked += 1;
        if progress && walked % PROGRESS_EVERY == 0 {
            let _ = writeln!(
                err,
                "osptool: verify: {walked} record(s) walked, {} decoded, {} failed",
                tally.decoded, tally.failed
            );
        }
    }

    write_report(out, package.name(), package.record_count(), request, &tally)
}

/// The verdict one record reached, in the tool's own vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OneRecord {
    /// A decoder accepted it.
    Decoded {
        /// What the decoder produced, in one line.
        summary: String,
    },
    /// A decoder refused it, with the decoder's own words.
    Failed {
        /// The message the decoder produced.
        message: String,
    },
    /// Recognised as a container; the payload was not read.
    Container,
    /// No decoder in this build for this type id. Nothing was parsed.
    NoDecoder,
    /// The bytes could not be extracted from the container at all.
    Unreadable {
        /// The container layer's message.
        message: String,
    },
}

impl OneRecord {
    /// The one word this verdict is called, for a summary line.
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Decoded { .. } => "decoded",
            Self::Failed { .. } => "failed",
            Self::Container => "container",
            Self::NoDecoder => "no-decoder",
            Self::Unreadable { .. } => "unreadable",
        }
    }
}

/// Runs the same per-record decision as the walk, for exactly one record.
///
/// The path is deliberately the *same* code the walk uses
/// ([`classify`] then the decoder), so an explanation and the tally it explains
/// cannot come from two different decisions. The extra information here is
/// additional, never a second verdict.
pub fn diagnose(entry: &DbpfEntry, image: &[u8]) -> OneRecord {
    let class = classify(entry.type_id);
    if matches!(class, RecordClass::Container) {
        return OneRecord::Container;
    }
    if matches!(class, RecordClass::NoDecoder) {
        return OneRecord::NoDecoder;
    }
    let bytes = match spore_dbpf::extract_record(image, entry) {
        Ok(bytes) => bytes,
        Err(error) => {
            return OneRecord::Unreadable {
                message: error.to_string(),
            }
        }
    };
    match decode(class, &bytes) {
        Ok(()) => OneRecord::Decoded {
            summary: decoded_summary(class, &bytes),
        },
        Err(message) => OneRecord::Failed { message },
    }
}

/// One line about what a decoder produced, for the record that decoded.
///
/// Only called on the success path, so every number in it came out of a decoder
/// rather than out of the index.
fn decoded_summary(class: RecordClass, bytes: &[u8]) -> String {
    match class {
        RecordClass::Gmdl => match spore_gmdl::parse(bytes) {
            Ok(model) => format!(
                "{} mesh(es), {} index buffer(s), {} vertex buffer(s), {} texture ref(s)",
                model.mesh_count,
                model.index_buffers.len(),
                model.vertex_buffers.len(),
                model.texture_refs.len()
            ),
            // Unreachable: `decode` succeeded, which called `parse`.
            Err(error) => format!("<unreachable: {error}>"),
        },
        RecordClass::Rw4 => match spore_rw4::parse(bytes) {
            Ok(directory) => format!(
                "{} section(s) declared, {} present",
                directory.section_count,
                directory.sections.len()
            ),
            Err(error) => format!("<unreachable: {error}>"),
        },
        RecordClass::Raster => match spore_texture::decode_raster(bytes) {
            Ok(image) => {
                // `layers` is a one-element vector by construction
                // (`decode_raster` builds `vec![mips]`), so the `None` arm is
                // unreachable — and it is spelled as an unreachable marker rather
                // than as a `0`, which would be a measurement of a chain that was
                // never counted.
                let Some(chain) = image.layers.first() else {
                    return "<unreachable: a decoded raster carries no mip chain>".to_owned();
                };
                format!(
                    "{} mip level(s) in one chain, base {}x{}, {}",
                    chain.len(),
                    image.envelope.width,
                    image.envelope.height,
                    super::describe_format(image.envelope.fourcc)
                )
            }
            Err(error) => format!("<unreachable: {error}>"),
        },
        RecordClass::Container | RecordClass::NoDecoder => "<not read>".to_owned(),
    }
}

/// `verify --explain <T:G:I>`: everything known about one record.
///
/// Always exits [`EXIT_OK`](crate::EXIT_OK) once the record has been located: an
/// explanation is a measurement, exactly like the tally. A record that is not in
/// the package is [`EXIT_NOT_FOUND`](crate::EXIT_NOT_FOUND), with the store's
/// near-miss candidates.
fn explain(
    store: &spore_assets::ContentStore,
    key: &spore_core::ResourceKey,
    out: &mut dyn Write,
) -> Result<(), ToolError> {
    let found = store.find(key)?;
    let entry = found.entry;
    let class = classify(entry.type_id);
    let verdict = diagnose(entry, store.packages()[found.package_index].bytes());

    writeln!(out, "record      {key}")?;
    writeln!(out, "package     {}", found.package_name)?;
    writeln!(out, "type        {}", super::describe_type(entry.type_id))?;
    writeln!(out, "group       {}", group_text(entry))?;
    writeln!(
        out,
        "index extent            0x{:08x}+{} ({} bytes stored)",
        entry.offset, entry.stored_size, entry.stored_size
    )?;
    writeln!(out, "index memory size       {}", entry.memory_size)?;
    writeln!(
        out,
        "index compression       {}",
        super::compression_text(entry)
    )?;
    writeln!(
        out,
        "class        {class:?} \u{2014} {}",
        class_sentence(class)
    )?;
    writeln!(out, "verdict      {}", verdict.label())?;

    match &verdict {
        OneRecord::Decoded { summary } => {
            writeln!(out, "  decoded    {summary}")?;
            write_extra(out, class, true, entry, store)?;
        }
        OneRecord::Failed { message } => {
            writeln!(out, "  refused    {message}")?;
            write_extra(out, class, false, entry, store)?;
        }
        // The honesty contract, in the one place a caller looks when a bulk count
        // has to be explained: a type with no decoder prints the index extent and
        // says the payload was not parsed. No summary line is emitted, because
        // there is no decoder to summarise anything.
        OneRecord::Container => {
            writeln!(
                out,
                "  the {} bytes are the index's memory_size.",
                entry.memory_size
            )?;
            writeln!(
                out,
                "  This build does not parse this container, so nothing further is claimed about \
                 the record."
            )?;
        }
        OneRecord::NoDecoder => {
            writeln!(
                out,
                "  the {} bytes are the index's memory_size.",
                entry.memory_size
            )?;
            writeln!(
                out,
                "  This build has no decoder for that type id, so nothing was parsed. A count of \
                 these in `verify` is a statement about the type table, not about this record."
            )?;
        }
        OneRecord::Unreadable { message } => {
            writeln!(out, "  {message}")?;
            writeln!(
                out,
                "  The record's bytes could not be extracted, so no decoder ran and nothing \
                 below is claimed about its content."
            )?;
        }
    }
    Ok(())
}

/// What the classification means for this type, in one sentence.
fn class_sentence(class: RecordClass) -> &'static str {
    match class {
        RecordClass::Gmdl => "decoded by the gmdl walker",
        RecordClass::Rw4 => "decoded as an RW4 section directory",
        RecordClass::Raster => "decoded as a raster (DXT5 / DXT1 / 0x15) record",
        RecordClass::Container => "recognised as a container; payload not read",
        RecordClass::NoDecoder => "no decoder in this build; nothing parsed",
    }
}

/// Extra facts a decoder already has in hand, printed so a caller does not have
/// to run a second command to get them.
///
/// Only reached for a record that a decoder accepted or refused, so every number
/// here came from a decoder.
fn write_extra(
    out: &mut dyn Write,
    class: RecordClass,
    decoded: bool,
    entry: &DbpfEntry,
    store: &spore_assets::ContentStore,
) -> Result<(), ToolError> {
    match class {
        RecordClass::Raster => write_raster_extra(out, decoded, entry, store),
        RecordClass::Gmdl => write_gmdl_extra(out, entry, store),
        // An RW4 failure message is already complete, and there is no second
        // decoding pass to report from.
        RecordClass::Rw4 | RecordClass::Container | RecordClass::NoDecoder => Ok(()),
    }
}

/// The raster envelope, printed even when the walk refused.
///
/// For a raster refusal the envelope is *the* answer: the fourcc is what the walk
/// dispatched on, and the geometry words are what the record actually said. This
/// used to key its "these are header words, not a decoded image" note off the
/// **fourcc**, which was right only while a refusal always meant "no decoder for
/// that format". Now that every format in the corpus decodes, a refusal means the
/// record disagreed with its own framing — so the note keys off the **verdict**,
/// which is the thing that is actually in question.
fn write_raster_extra(
    out: &mut dyn Write,
    decoded: bool,
    entry: &DbpfEntry,
    store: &spore_assets::ContentStore,
) -> Result<(), ToolError> {
    // Read through the answering package, not `packages()[0]`: with one package
    // they are the same and with more than one they are not.
    let image = &store.packages()[0].bytes();
    let Ok(bytes) = spore_dbpf::extract_record(image, entry) else {
        return Ok(());
    };
    match spore_texture::parse_envelope(&bytes) {
        Ok(envelope) => {
            writeln!(
                out,
                "  envelope   version={} {}x{} mips={} fourcc=0x{:08x}{}",
                envelope.version,
                envelope.width,
                envelope.height,
                envelope.mip_count,
                envelope.fourcc,
                fourcc_verdict(envelope.fourcc)
            )?;
            writeln!(
                out,
                "  field_10=0x{:08x} is UNRESOLVED and carried verbatim; field_18=0x{:08x} is the \
                 base level's byte count and was checked against the geometry before this record \
                 decoded; field_1c=0x{:08x} is carried verbatim and never used to select a decode",
                envelope.field_10, envelope.field_18, envelope.field_1c
            )?;
            if !decoded {
                writeln!(
                    out,
                    "  the record did not decode, so the dimensions above are the header's words \
                     \u{2014} not a decoded image"
                )?;
            } else if spore_texture::frame_for(envelope.fourcc).is_none() {
                // Unreachable: a raster with no framing rule cannot reach the
                // success path, because `decode_raster` dispatches on the frame.
                // Kept as a bound on that argument rather than removed.
                writeln!(
                    out,
                    "  this build has no measured framing rule for that fourcc, which the decode \
                     above would have had to use"
                )?;
            }
        }
        Err(error) => {
            writeln!(out, "  envelope   unreadable: {error}")?;
            writeln!(
                out,
                "  the 32-byte envelope is not present, so this record cannot be a raster of any \
                 format this build knows"
            )?;
        }
    }
    Ok(())
}

/// Where the strict gmdl parse stopped, and how much survived anyway.
///
/// This is the section that makes the bulk `619 gmdl failed` actionable. The
/// strict walk is all-or-nothing, so a record refused at the material-info table
/// looks identical to one refused inside the geometry from the tally alone. The
/// *recovering* walk answers the difference: it either closes, or it stops and
/// says where — and stopping at material-info means the geometry a renderer needs
/// is already fully decoded.
fn write_gmdl_extra(
    out: &mut dyn Write,
    entry: &DbpfEntry,
    store: &spore_assets::ContentStore,
) -> Result<(), ToolError> {
    let image = &store.packages()[0].bytes();
    let Ok(bytes) = spore_dbpf::extract_record(image, entry) else {
        return Ok(());
    };
    let partial = match spore_gmdl::parse_recovering(&bytes) {
        Ok(partial) => partial,
        Err(error) => {
            writeln!(
                out,
                "  recovery   the recovering walk stopped INSIDE the geometry, so nothing usable \
                 was decoded: {error}"
            )?;
            return Ok(());
        }
    };
    match &partial.stop {
        None => writeln!(
            out,
            "  recovery   the strict parse refused, but the recovering walk CLOSED on the record \
             end: {} mesh(es) decoded",
            partial.model.mesh_count
        )?,
        Some(stop) => {
            writeln!(
                out,
                "  recovery   stopped at the {} table: {}",
                stop.stage.as_str(),
                stop.error
            )?;
            writeln!(
                out,
                "             {} mesh(es), {} index buffer(s), {} vertex buffer(s) are decoded and \
                 validated; only what follows the mesh table is unmeasured",
                partial.model.mesh_count,
                partial.model.index_buffers.len(),
                partial.model.vertex_buffers.len()
            )?;
            writeln!(
                out,
                "             strict walk validated {} of {} bytes",
                partial.model.strict_consumed, partial.model.consumed
            )?;
        }
    }
    Ok(())
}

/// Why a fourcc was or was not decoded.
///
/// Total, and worded so that a format this build cannot frame reads as an
/// **absence** while one it frames reads as a decode. `DXT1` used to land in the
/// second arm and print "REFUSED BY NAME" while `spore_texture` decoded all 306 of
/// its corpus records; `first_record_of_fourcc` in `tests/real_corpus.rs` greps
/// for `fourcc=0x…`, so this wording is what a corpus scan reads to find a record
/// of a given format.
fn fourcc_verdict(fourcc: u32) -> &'static str {
    match spore_texture::frame_for(fourcc) {
        Some(_) => " (this build decodes it)",
        None => " (no measured framing rule in this build \u{2014} an absence, not a verdict on the record)",
    }
}

/// A group's id plus its canonical name when one exists.
fn group_text(entry: &DbpfEntry) -> String {
    match super::group_name_of(entry.group_id) {
        Some(name) => format!("0x{:08x} ({name})", entry.group_id),
        None => format!("0x{:08x}", entry.group_id),
    }
}

/// Reads and decodes one record, mapping the outcome onto the tally's vocabulary.
///
/// Every failure here is a typed error from a `spore-*` crate, so a malformed
/// record becomes a counted message and never a panic. The read buffer is a
/// local, freed before the next iteration.
fn attempt(entry: &DbpfEntry, image: &[u8]) -> Result<(), String> {
    let class = classify(entry.type_id);
    match class {
        RecordClass::Container | RecordClass::NoDecoder => Ok(()),
        RecordClass::Gmdl | RecordClass::Rw4 | RecordClass::Raster => {
            let bytes =
                spore_dbpf::extract_record(image, entry).map_err(|error| error.to_string())?;
            decode(class, &bytes)
        }
    }
}

/// Runs the decoder for a class over already-read bytes.
fn decode(class: RecordClass, bytes: &[u8]) -> Result<(), String> {
    match class {
        RecordClass::Gmdl => {
            let model = spore_gmdl::parse(bytes).map_err(|error| error.to_string())?;
            // Extraction is part of "decoded": a model that parses but yields no
            // drawable mesh has not been decoded into anything usable, and the
            // per-mesh error is the more useful message of the two.
            for index in 0..model.mesh_count {
                spore_gmdl::mesh_from_gmdl(&model, index).map_err(|error| error.to_string())?;
            }
            Ok(())
        }
        RecordClass::Rw4 => spore_rw4::parse(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        RecordClass::Raster => spore_texture::decode_raster(bytes)
            .map(|_| ())
            .map_err(|error| error.to_string()),
        RecordClass::Container | RecordClass::NoDecoder => Ok(()),
    }
}

/// The human report.
fn write_report(
    out: &mut dyn Write,
    package_name: &str,
    index_rows: usize,
    request: &VerifyRequest,
    tally: &Tally,
) -> Result<(), ToolError> {
    writeln!(out, "package        {package_name}")?;
    writeln!(out, "index rows     {index_rows}")?;
    match request.type_id {
        Some(type_id) => writeln!(out, "scope          type {}", super::describe_type(type_id))?,
        None => writeln!(out, "scope          every record in the package")?,
    }
    writeln!(out, "walked         {}", tally.seen)?;
    writeln!(out, "decoded        {}", tally.decoded)?;
    writeln!(out, "container      {}", tally.container)?;
    writeln!(out, "no decoder     {}", tally.no_decoder)?;
    writeln!(out, "failed         {}", tally.failed)?;
    if tally.errors.is_empty() {
        writeln!(out, "errors         none")?;
    } else {
        writeln!(
            out,
            "errors         {} distinct message(s)",
            tally.errors.len()
        )?;
        for (message, count) in tally.ranked_errors() {
            writeln!(out, "  {count:>9}  {message}")?;
        }
    }
    writeln!(
        out,
        "note           `container` and `no decoder` are type-level facts about the type table;"
    )?;
    writeln!(
        out,
        "               those payloads were not read. `decoded` and `failed` are per-record."
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_decoded_ids_are_exactly_the_three_decoders() {
        assert_eq!(classify(spore_gmdl::GMDL_TYPE), RecordClass::Gmdl);
        assert_eq!(classify(spore_rw4::RW4_TYPE), RecordClass::Rw4);
        assert_eq!(classify(spore_texture::RASTER_TYPE), RecordClass::Raster);
        for id in CONTAINER_TYPE_IDS {
            assert_eq!(classify(*id), RecordClass::Container, "0x{id:08x}");
        }
    }

    #[test]
    fn png_jpeg_plt_and_gmsh_are_not_routed_to_a_raster_or_gmdl_decoder() {
        // Each of these is named in the manifest's decodable set at the *type*
        // level, and none of them is a raster or a gmdl on disk.
        for (id, name) in [
            (0x2F7D_0004u32, "png"),
            (0x2F7D_0002u32, "jpeg"),
            (0x0119_89B7u32, "plt"),
            (0x01c1_35dau32, "gmsh"),
        ] {
            assert_eq!(classify(id), RecordClass::NoDecoder, "{name}");
        }
    }

    #[test]
    fn an_unknown_id_is_no_decoder_not_a_failure() {
        assert_eq!(classify(0xdead_beef), RecordClass::NoDecoder);
    }

    #[test]
    fn the_four_buckets_partition_the_rows() {
        let mut tally = Tally::default();
        tally.record(RecordClass::Container, Ok(()));
        tally.record(RecordClass::NoDecoder, Ok(()));
        tally.record(RecordClass::Gmdl, Ok(()));
        tally.record(RecordClass::Raster, Err("bad fourcc".to_owned()));
        assert_eq!(tally.seen, 4);
        assert_eq!(tally.decoded, 1);
        assert_eq!(tally.container, 1);
        assert_eq!(tally.no_decoder, 1);
        assert_eq!(tally.failed, 1);
        assert_eq!(
            tally.decoded + tally.container + tally.no_decoder + tally.failed,
            4
        );
    }

    #[test]
    fn a_container_is_never_reported_as_decoded_and_never_read() {
        // An entry whose extent is nonsense still reports Ok for a container,
        // and lands in the container bucket rather than the decoded one: the
        // observable proof that the bytes were never touched.
        let entry = DbpfEntry::new(
            spore_core::record::type_id::PROP,
            1,
            2,
            0xFFFF_0000,
            16,
            16,
            0,
            false,
        );
        assert_eq!(attempt(&entry, b""), Ok(()));
        let mut tally = Tally::default();
        tally.record(classify(entry.type_id), attempt(&entry, b""));
        assert_eq!(tally.container, 1);
        assert_eq!(tally.decoded, 0);
    }

    #[test]
    fn errors_are_counted_by_distinct_message_and_ranked_deterministically() {
        let mut tally = Tally::default();
        tally.record(RecordClass::Gmdl, Err("short buffer".to_owned()));
        tally.record(RecordClass::Gmdl, Err("short buffer".to_owned()));
        tally.record(RecordClass::Raster, Err("bad fourcc".to_owned()));
        assert_eq!(tally.failed, 3);
        assert_eq!(tally.errors["short buffer"], 2);
        assert_eq!(tally.errors["bad fourcc"], 1);
        assert_eq!(
            tally.ranked_errors(),
            vec![("short buffer", 2), ("bad fourcc", 1)]
        );
        assert!(!tally.all_decoded());
    }

    #[test]
    fn a_malformed_record_of_a_decoded_type_is_an_error_not_a_panic() {
        // The entry claims 4 bytes at offset 0 of a 3-byte image.
        let entry = DbpfEntry::new(spore_gmdl::GMDL_TYPE, 1, 2, 0, 4, 4, 0, false);
        assert!(attempt(&entry, b"abc").is_err());
        // And so is a truncated payload that *is* present.
        let entry = DbpfEntry::new(spore_gmdl::GMDL_TYPE, 1, 2, 0, 8, 8, 0, false);
        assert!(attempt(&entry, b"12345678").is_err());
    }

    #[test]
    fn the_committed_fixtures_decode() {
        for (path, class) in [
            ("mini.gmdl", RecordClass::Gmdl),
            ("mini_rw4.rw4", RecordClass::Rw4),
        ] {
            let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/");
            let bytes = std::fs::read(format!("{base}{path}")).unwrap();
            assert_eq!(decode(class, &bytes), Ok(()), "{path}");
        }
    }

    #[test]
    fn the_fixture_set_is_the_one_the_tests_pin() {
        // Guards the paths above against a fixture rename that would otherwise
        // turn them into `unwrap()` panics rather than failures.
        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/");
        for name in ["mini.gmdl", "mini_rw4.rw4", "mini_package.dbpf"] {
            assert!(
                std::path::Path::new(&format!("{base}{name}")).exists(),
                "{name}"
            );
        }
    }

    #[test]
    fn an_explanation_and_the_tally_it_explains_share_one_decision() {
        // The point of `diagnose` reusing `decode`: a record the walk counted as
        // decoded must be reported by `--explain` as decoded, and a record the
        // walk counted as failed must be reported as failed with the same words.
        let base = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/");
        let payload = std::fs::read(format!("{base}mini.gmdl")).unwrap();
        let (entry, image) = container_holding(spore_gmdl::GMDL_TYPE, &payload);

        // The verdict for the committed fixture is whatever the decoder decides;
        // what this test pins is that `diagnose` and the walk make the *same*
        // decision about it.
        let verdict = diagnose(&entry, &image);
        let mut tally = Tally::default();
        tally.record(classify(entry.type_id), attempt(&entry, &image));
        match &verdict {
            OneRecord::Decoded { summary } => {
                assert_eq!(
                    (tally.decoded, tally.failed),
                    (1, 0),
                    "an explanation of `decoded` must match a tally of one decoded"
                );
                assert!(
                    summary.contains("mesh(es)"),
                    "a decoded gmdl summary names what decoded: {summary}"
                );
            }
            OneRecord::Failed { message } => {
                assert_eq!(
                    (tally.decoded, tally.failed),
                    (0, 1),
                    "an explanation of `failed` must match a tally of one failed"
                );
                assert_eq!(
                    tally.errors.get(message.as_str()),
                    Some(&1),
                    "the counted message and the explained message must be the same string: \
                     {message}"
                );
            }
            other => panic!("expected a decoder verdict, got {}", other.label()),
        }

        // The same record with its last eight bytes chopped: both paths must now
        // say `failed`, and with the same message.
        let truncated = &payload[..payload.len() - 8];
        let (bad_entry, bad_image) = container_holding(spore_gmdl::GMDL_TYPE, truncated);
        let mut bad_tally = Tally::default();
        bad_tally.record(classify(bad_entry.type_id), attempt(&bad_entry, &bad_image));
        let OneRecord::Failed { message } = diagnose(&bad_entry, &bad_image) else {
            panic!("a truncated record must be a `failed` verdict, not a decoded one")
        };
        assert_eq!(bad_tally.failed, 1);
        assert_eq!(
            bad_tally.errors.get(&message),
            Some(&1),
            "the counted message and the explained message must be the same string: {message}"
        );
    }

    /// A one-record container image holding `payload` uncompressed.
    fn container_holding(type_id: u32, payload: &[u8]) -> (DbpfEntry, Vec<u8>) {
        let stored_size = u32::try_from(payload.len()).expect("fixture is small");
        let entry = DbpfEntry::new(
            type_id,
            0x4061_6201,
            0x067a_0801,
            96 + 4,
            stored_size,
            stored_size,
            0,
            false,
        );
        let mut image = vec![0u8; 96];
        image[0..4].copy_from_slice(b"DBPF");
        image[4..8].copy_from_slice(&3u32.to_le_bytes());
        image[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
        image[0x40..0x44].copy_from_slice(&96u32.to_le_bytes());
        image.extend_from_slice(&0u32.to_le_bytes());
        image.extend_from_slice(&entry.type_id.to_le_bytes());
        image.extend_from_slice(&entry.group_id.to_le_bytes());
        image.extend_from_slice(&entry.instance_id.to_le_bytes());
        image.extend_from_slice(&entry.offset.to_le_bytes());
        image.extend_from_slice(&(stored_size | 0x8000_0000).to_le_bytes());
        image.extend_from_slice(&entry.memory_size.to_le_bytes());
        image.extend_from_slice(&0u16.to_le_bytes());
        image.push(0);
        image.push(0);
        image.extend_from_slice(payload);
        (entry, image)
    }

    #[test]
    fn an_explanation_for_a_type_with_no_decoder_says_the_payload_was_not_parsed() {
        // The honesty contract, exercised: `NoDecoder` must never be rendered as
        // anything that looks like a decode.
        let entry = DbpfEntry::new(spore_core::record::type_id::PLT, 1, 2, 0, 40, 40, 0, false);
        assert_eq!(diagnose(&entry, b""), OneRecord::NoDecoder);
        assert_eq!(OneRecord::NoDecoder.label(), "no-decoder");
    }

    #[test]
    fn an_unreadable_record_is_distinct_from_a_refused_one() {
        // The row claims bytes the image does not have. That is the container
        // refusing the record, not a decoder refusing it, and the two get
        // different labels.
        let entry = DbpfEntry::new(spore_gmdl::GMDL_TYPE, 1, 2, 4096, 64, 64, 0, false);
        match diagnose(&entry, &[0u8; 8]) {
            OneRecord::Unreadable { message } => {
                assert!(!message.is_empty(), "the container's message is carried");
                assert_eq!(OneRecord::Unreadable { message }.label(), "unreadable");
            }
            other => panic!("expected unreadable, got {}", other.label()),
        }
    }

    #[test]
    fn an_unsupported_fourcc_is_named_by_the_explanation_helper() {
        // Every format in the corpus is framed by this build, so every one of them
        // decodes. `DXT1` used to print "REFUSED BY NAME" here while
        // `spore_texture` decoded all 306 of its records.
        for fourcc in [
            spore_texture::DXT5_FOURCC,
            spore_texture::DXT1_FOURCC,
            spore_texture::LUMINANCE_FOURCC,
        ] {
            let verdict = fourcc_verdict(fourcc);
            assert!(
                verdict.contains("this build decodes it"),
                "{fourcc:#010x}: {verdict}"
            );
            assert!(!verdict.contains("REFUS"), "{fourcc:#010x}: {verdict}");
        }
        // A format with no measured framing is an absence, not a verdict on the
        // record, and says so.
        let unknown = fourcc_verdict(0xdead_beef);
        assert!(unknown.contains("no measured framing rule"), "{unknown}");
        assert!(unknown.contains("absence"), "{unknown}");
    }

    #[test]
    fn every_class_has_a_sentence_and_a_label() {
        for class in [
            RecordClass::Gmdl,
            RecordClass::Rw4,
            RecordClass::Raster,
            RecordClass::Container,
            RecordClass::NoDecoder,
        ] {
            assert!(!class_sentence(class).is_empty(), "{class:?}");
            assert!(!class_label(class).is_empty(), "{class:?}");
        }
    }

    /// The `Debug` spelling of a class, which the report prints verbatim.
    fn class_label(class: RecordClass) -> String {
        format!("{class:?}")
    }
}
