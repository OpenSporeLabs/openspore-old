//! Canonical persistence for a [`CreatorDocument`]: `spore-creator-doc/1`.
//!
//! A creator is not real if its changes only exist in RAM, so this is the
//! load/save round trip — and the whole point of it is that it is **verifiable**:
//! `load(save(doc))` must equal `doc` under this crate's own bitwise
//! [`PartialEq`], and `save(doc)` must be a pure function of `doc`. Both are
//! tested, the second over all 68 real creator documents.
//!
//! # This is not the in-game format
//!
//! Spore's own creator format is XML (`docs/CREATOR-CORE.md` §1), read by
//! [`crate::sporemodel::parse_sporemodel`]. This is a **canonical interchange
//! format**, and the difference is not cosmetic:
//!
//! * one canonical spelling per fact, where the game writes `modeltype` both as
//!   `0x9ea3031a` and as `2661483290` in the same corpus;
//! * the original element spelling and attribute order are **not** preserved.
//!   Nothing is lost *of the document* — both spellings are the same `u32` and
//!   the decoder has already normalised them — but this file cannot be used to
//!   reconstruct the game's bytes, and it is not trying to. Writing the game's
//!   XML is a separate concern and there is no XML writer in this crate.
//!
//! # The shape
//!
//! JSON Lines, mirroring the `spore-semantic` passport format
//! (`docs/tooling/semantic-exchange.md` §6.1):
//!
//! ```text
//! line 1      {"record":"metadata", "schema":"spore-creator-doc/1", ...}
//! line 2..N   one object per block, in document order
//! ```
//!
//! Line 1 carries the schema identity, the evidence grade of the facts, two
//! derived counts and `content_sha256`; the document-level fields
//! (`format_version`, `properties`) live inside it under `document`. Because
//! block order **is** the index space (`crate::document`), the body is a list and
//! each line also writes its own `index`, so a reordered file is detected rather
//! than silently accepted.
//!
//! # Determinism, and what it is made of
//!
//! Two saves of the same document produce identical bytes. That rests on four
//! things, each of which is a thing that could have gone wrong:
//!
//! * **No ambient state.** No timestamp, no PID, no hostname, no absolute path,
//!   no user name, no environment. `generator` is the constant crate name.
//! * **No map iteration.** The writer emits keys in a fixed order per type and
//!   holds no map ([`json::Out`]); every list is written in document order.
//! * **One spelling per number.** `u32` words are `0x` + 8 lowercase hex digits,
//!   the repository's canonical spelling, and are read back with
//!   [`spore_core::parse_id`] rather than a second hex parser.
//! * **One spelling per float.** See below.
//!
//! `content_sha256` covers **the content lines only** — everything after the
//! first newline, each line including its `\n`. It therefore cannot be
//! self-referential, and it is checked before the body is parsed, so a corrupted
//! file reports corruption rather than whichever field it happened to damage.
//!
//! # Floats round-trip bit-exactly, and here is why that needed a rule
//!
//! This crate's `PartialEq` for `Vec3`, `Matrix3`, `BlockTransform`, `Limb` and
//! `UnitRgb` is **bitwise**: `0.0` and `-0.0` are different documents, and a
//! derived comparison would call a sign-bit flip equal and ship a different
//! creature. So the encoding has to be exact, not approximately exact.
//!
//! * A finite `f32` is written as a JSON **number**: the shortest decimal that
//!   reads back to the same bits. Rust's `{:?}` for `f32` is exactly that
//!   spelling, which is why `-0.0` appears in the artifact as `-0.0` and `0.0` as
//!   `0.0` — the two are visibly different in the bytes *and* in the value. The
//!   [`tests`](mod@tests) below prove the property rather than assume it, and an
//!   `#[ignore]`d test sweeps the whole 2^32 bit space.
//! * `+inf`, `-inf` and **every NaN, including its sign bit and payload**, are
//!   written as JSON **strings**: `"inf"`, `"-inf"`, `"nan"`, and `"nan:0x…"` for
//!   any pattern that is not the default quiet NaN. This is the one place a
//!   decimal spelling would have been silently lossy — `-f32::NAN` formats as
//!   `"NaN"` and reads back as `0x7fc00000`, not `0xffc00000` — and a *string* is
//!   used rather than a bare token so that a standard JSON reader still sees a
//!   well-formed value and can tell it is not a number.
//! * On the way back, a JSON number is parsed **and checked for finiteness**.
//!   `"1e40".parse::<f32>()` is `Ok(f32::INFINITY)`, so a loader that trusted the
//!   parse would silently hand back an infinite transform; that is
//!   [`PersistError::FloatNotExact`].
//!
//! # Absent is not empty
//!
//! The document grammar has three distinct shapes that are easy to collapse and
//! that this format keeps apart, because each is a fact about the game:
//!
//! | fact | spelling |
//! |---|---|
//! | the element was not written (160 of 680 corpus blocks write no `<childlist>`) | **the key is absent** |
//! | an empty list *was* written (which no corpus block does) | `"children":[]` |
//! | one slot of a fixed-width triple was not written (7 of 68 documents write 2 of 3 `<skincolor…>`) | `"skin_colors":[…,null,…]` |
//!
//! Absence is **never** `[]`, and a written-empty list is never an absent key.
//! `null` appears only where a hole cannot be spelled any other way — inside a
//! fixed-width array, or for the explicitly-absent `<name>`-like slots of
//! [`DocumentProperties`], where the array is present but a slot is not.
//!
//! # What is *not* persisted, and why
//!
//! Two things, both deliberate and both in [`crate::claims`]:
//!
//! * **The record family.** [`CreatorDocument`] does not carry which of the six
//!   families it came from — [`parse_sporemodel`] takes a [`Family`] and does not
//!   store it — so a canonical save cannot record it. Inventing a field for it
//!   would be a fact about the *file* masquerading as a fact about the document.
//! * **The game's own bytes.** Spelled out above. The decoder is where the
//!   game's spellings become one spelling.
//!
//! Every other field of [`CreatorDocument`], [`Block`], [`BlockTransform`],
//! [`Limb`], [`PaintRegion`] and [`ChannelWeight`] is encoded, and the round-trip
//! fixture sets every one of them to a non-default value so a field that stops
//! being written shows up as a difference.
//!
//! The one thing that cannot be caught automatically: Rust has no reflection, so a
//! field *added* to [`Block`] and then not persisted would still round trip, because
//! it would be `None` on both sides. The defence is the fixture — a new field has to
//! be added there with a value, and [`the_key_set_of_a_block_line_is_pinned`] fails
//! when a *persisted* key changes — plus this list, which a reviewer reads.
//!
//! # What `load` does *not* check
//!
//! Document **validity** belongs to [`crate::validate`], and `load` does not call
//! it or duplicate any of it. A save file may legitimately hold a document that
//! `validate` reports problems about, and a loader that refused those would be a
//! second opinion about the same record. What `load` *does* check is the
//! file's own integrity, which is a different question: schema id, float
//! encoding, evidence grade, `block_count`, `root_count`, `content_sha256`, and
//! each block line's `index` against the position it occupies.

use crate::document::{
    Block, BlockId, BlockIndex, BlockTransform, ChannelWeight, CreatorDocument, DocumentProperties,
    Limb, Matrix3, PaintRegion, Vec3, DOCUMENT_EVIDENCE, DOCUMENT_EVIDENCE_STATE,
};
use crate::error::{PersistError, PersistResult, PersistSite};
use crate::sha256::Sha256;

use self::json::{Ctx, Fields, Json, Out};

mod json;

/// The schema id written into every line-1 header.
pub const FORMAT: &str = "spore-creator-doc/1";

/// The value of the header's `record` field.
const RECORD_METADATA: &str = "metadata";

/// The value of the header's `generator` field.
///
/// The crate name and **not** its version: a version string would make the bytes
/// of the same document change across a release, which is the opposite of what a
/// checksum is for.
const GENERATOR: &str = "spore-creator-core";

/// The float encoding this build writes and is willing to read.
///
/// Named in the file so that a future revision of the rule is a refusal rather
/// than a misreading. See the module docs for what the rule is.
pub const FLOAT_ENCODING: &str = "shortest-round-trip-decimal";

// ---------------------------------------------------------------------------
// writing
// ---------------------------------------------------------------------------

/// Saves a document as `spore-creator-doc/1` text.
///
/// The result always ends in `\n`, has one line per block in document order, and
/// is a pure function of `document`: two calls with equal documents produce
/// equal bytes. The only input that is not a field of the document is
/// [`GENERATOR`], which is a constant.
pub fn save(document: &CreatorDocument) -> PersistResult<String> {
    let content = content_lines(document);
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.hex();

    let mut out = Out::new();
    out.object();
    out.key("record");
    out.string(RECORD_METADATA);
    out.key("schema");
    out.string(FORMAT);
    out.key("generator");
    out.string(GENERATOR);
    out.key("float_encoding");
    out.string(FLOAT_ENCODING);
    out.key("evidence_level");
    out.string(DOCUMENT_EVIDENCE.as_str());
    out.key("evidence_state");
    out.string(DOCUMENT_EVIDENCE_STATE.as_str());
    out.key("block_count");
    out.index(document.len());
    out.key("root_count");
    out.index(document.roots().len());
    out.key("content_sha256");
    out.string(&digest);
    out.key("document");
    write_document(&mut out, document);
    out.end('}');
    out.line();

    let header = out.finish();
    Ok(format!("{header}{content}"))
}

/// [`save`], as the bytes a file would hold.
pub fn save_bytes(document: &CreatorDocument) -> PersistResult<Vec<u8>> {
    save(document).map(String::into_bytes)
}

/// The `content_sha256` [`save`] would declare for `document`.
///
/// Exposed so a caller can check a file's declared digest against the document it
/// claims to hold without re-hashing the file, and so a test can assert the
/// header's claim and the body's hash are one thing rather than two. It hashes the
/// same bytes [`content_lines`] produces, through the same function, so the two
/// cannot drift apart.
pub fn content_digest(document: &CreatorDocument) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content_lines(document).as_bytes());
    hasher.hex()
}

/// The body of a save: one line per block, in document order, each newline-
/// terminated. The header is *not* here, which is what makes `content_sha256`
/// cover the content alone and therefore non-self-referential.
fn content_lines(document: &CreatorDocument) -> String {
    let mut content = Out::new();
    for (index, block) in document.blocks.iter().enumerate() {
        write_block(&mut content, index, block);
        content.line();
    }
    content.finish()
}

/// Writes `document` under the header's `document` key.
fn write_document(out: &mut Out, document: &CreatorDocument) {
    out.object();
    if let Some(version) = document.format_version {
        out.key("format_version");
        write_word(out, version);
    }
    out.key("properties");
    write_properties(out, &document.properties);
    out.end('}');
}

fn write_properties(out: &mut Out, properties: &DocumentProperties) {
    out.object();
    // Struct order, which is corpus order. Never a map's order.
    if let Some(model_type) = properties.model_type {
        out.key("model_type");
        write_word(out, model_type);
    }
    if let Some(name) = &properties.name {
        out.key("name");
        out.string(name);
    }
    if let Some(description) = &properties.description {
        // `Some("")` is a value, not an absence: 9 documents write an empty
        // `<description>` and that emptiness is a fact about them.
        out.key("description");
        out.string(description);
    }
    if let Some(effects) = &properties.skin_effects {
        out.key("skin_effects");
        write_word_slots(out, effects);
    }
    if let Some(seeds) = &properties.skin_effect_seeds {
        out.key("skin_effect_seeds");
        write_word_slots(out, seeds);
    }
    if let Some(colors) = &properties.skin_colors {
        out.key("skin_colors");
        out.array();
        for slot in colors {
            match slot {
                Some(colour) => write_colour(out, *colour),
                None => out.null(),
            }
        }
        out.end(']');
    }
    if let Some(required_pack) = properties.required_pack {
        out.key("required_pack");
        write_word(out, required_pack);
    }
    if let Some(zcorp_score) = properties.zcorp_score {
        out.key("zcorp_score");
        write_word(out, zcorp_score);
    }
    if let Some(has_thumbnail) = properties.has_thumbnail {
        out.key("has_thumbnail");
        out.flag(has_thumbnail);
    }
    if let Some(cache_id) = properties.cache_id {
        out.key("cache_id");
        write_word(out, cache_id);
    }
    out.end('}');
}

/// Writes a fixed-width array of optional words: `null` for an unwritten slot.
fn write_word_slots<const N: usize>(out: &mut Out, slots: &[Option<u32>; N]) {
    out.array();
    for slot in slots {
        match slot {
            Some(word) => write_word(out, *word),
            None => out.null(),
        }
    }
    out.end(']');
}

fn write_block(out: &mut Out, index: BlockIndex, block: &Block) {
    out.object();
    // Written first so every error below can name the block this line declares.
    out.key("index");
    out.index(index);
    if let Some(block_id) = block.block_id {
        out.key("block_id");
        out.object();
        out.key("group_id");
        write_word(out, block_id.group_id);
        out.key("instance_id");
        write_word(out, block_id.instance_id);
        out.end('}');
    }
    out.key("transform");
    write_transform(out, &block.transform);
    if let Some(snapped) = block.snapped {
        out.key("snapped");
        out.flag(snapped);
    }
    if let Some(children) = &block.children {
        // The distinction this crate exists to preserve: an absent key here is
        // "the element was not written" and `[]` is "an empty list was written".
        // Writing `[]` for a leaf would delete 160 corpus facts.
        out.key("children");
        out.array();
        for &child in children {
            out.index(child);
        }
        out.end(']');
    }
    if let Some(partner) = block.symmetric {
        out.key("symmetric");
        out.index(partner);
    }
    if let Some(paint) = &block.paint {
        out.key("paint");
        out.array();
        for region in paint {
            out.object();
            out.key("region");
            write_word(out, region.region);
            out.key("paint_id");
            write_word(out, region.paint_id);
            out.key("color1");
            write_colour(out, region.color1);
            out.key("color2");
            write_colour(out, region.color2);
            out.end('}');
        }
        out.end(']');
    }
    if let Some(handles) = &block.handles {
        out.key("handles");
        out.array();
        for handle in handles {
            out.object();
            out.key("channel");
            write_word(out, handle.channel);
            out.key("weight");
            out.string(&float_text(handle.weight));
            out.end('}');
        }
        out.end(']');
    }
    if let Some(limb) = &block.limb {
        out.key("limb");
        out.object();
        if let Some(limb_type) = limb.limb_type {
            out.key("limb_type");
            write_word(out, limb_type);
        }
        if let Some(muscle_scale) = limb.muscle_scale {
            out.key("muscle_scale");
            out.string(&float_text(muscle_scale));
        }
        if let Some(base_muscle_scale) = limb.base_muscle_scale {
            out.key("base_muscle_scale");
            out.string(&float_text(base_muscle_scale));
        }
        out.end('}');
    }
    if let Some(orientation) = &block.user_orientation {
        out.key("user_orientation");
        write_matrix(out, orientation);
    }
    if let Some((min, max)) = &block.bbox {
        out.key("bbox");
        out.object();
        out.key("min");
        write_vec3(out, min);
        out.key("max");
        write_vec3(out, max);
        out.end('}');
    }
    out.end('}');
}

fn write_transform(out: &mut Out, transform: &BlockTransform) {
    out.object();
    out.key("position");
    write_vec3(out, &transform.position);
    out.key("scale");
    out.string(&float_text(transform.scale));
    out.key("orientation");
    write_matrix(out, &transform.orientation);
    if let Some(direction) = &transform.triangle_direction {
        out.key("triangle_direction");
        write_vec3(out, direction);
    }
    if let Some(origin) = &transform.triangle_pick_origin {
        out.key("triangle_pick_origin");
        write_vec3(out, origin);
    }
    out.end('}');
}

fn write_vec3(out: &mut Out, value: &Vec3) {
    out.object();
    out.key("x");
    out.string(&float_text(value.x));
    out.key("y");
    out.string(&float_text(value.y));
    out.key("z");
    out.string(&float_text(value.z));
    out.end('}');
}

fn write_matrix(out: &mut Out, value: &Matrix3) {
    out.object();
    for (name, row) in [
        ("row0", value.row0),
        ("row1", value.row1),
        ("row2", value.row2),
    ] {
        out.key(name);
        out.array();
        for component in row {
            out.string(&float_text(component));
        }
        out.end(']');
    }
    out.end('}');
}

fn write_colour(out: &mut Out, value: crate::color::UnitRgb) {
    out.object();
    out.key("red");
    out.string(&float_text(value.red));
    out.key("green");
    out.string(&float_text(value.green));
    out.key("blue");
    out.string(&float_text(value.blue));
    out.end('}');
}

/// Writes a `u32` word as `0x` + 8 lowercase hex digits.
///
/// The repository's canonical spelling, because "bare address digits are hex
/// everywhere in this repo" — and 8 digits with a prefix means one word cannot be
/// read as a decimal number by accident.
fn write_word(out: &mut Out, value: u32) {
    out.string(&format!("0x{value:08x}"));
}

// ---------------------------------------------------------------------------
// floats
// ---------------------------------------------------------------------------

/// The canonical text this format writes for `value`.
///
/// * a finite `f32` becomes the shortest decimal that reads back to the same
///   bits, so `0.0` is `"0.0"` and `-0.0` is `"-0.0"` — distinguishable in the
///   artifact itself, not only in the value;
/// * `+inf`, `-inf` and the default quiet `NaN` become `"inf"`, `"-inf"` and
///   `"nan"`;
/// * any other NaN becomes `"nan:0x…"` with the full 32-bit pattern, because a
///   decimal spelling cannot carry a payload or a sign bit and losing either
///   would be exactly the silent loss this format exists to prevent.
///
/// A **string** is used for the non-finite cases so the line stays valid JSON: a
/// standard reader sees a well-formed value and can tell it is not a number,
/// rather than choking on a bare `NaN`.
fn float_text(value: f32) -> String {
    if value.is_finite() {
        return format!("{value:?}");
    }
    match value {
        v if v == f32::INFINITY => "inf".to_owned(),
        v if v == f32::NEG_INFINITY => "-inf".to_owned(),
        v if v.to_bits() == f32::NAN.to_bits() => "nan".to_owned(),
        v => format!("nan:0x{:08x}", v.to_bits()),
    }
}

/// Reads a float this format wrote, refusing anything that is not the same `f32`.
///
/// The finiteness check on the numeric branch is the point of the function:
/// `"1e40"` is a perfectly good JSON number and `parse::<f32>()` answers
/// `Ok(f32::INFINITY)` for it, so a loader that only looked at the `Result`
/// would widen a document without saying so.
fn read_float(site: PersistSite, field: &str, value: &Json) -> PersistResult<f32> {
    let text = match value {
        Json::Number(text) => text.clone(),
        Json::String(text) => return parse_float_text(site, field, text),
        other => {
            return Err(json::wrong_type(
                Ctx::from(site),
                field,
                "number or string",
                other,
            ))
        }
    };
    match text.parse::<f32>() {
        Ok(parsed) if parsed.is_finite() => Ok(parsed),
        _ => Err(not_exact(site, field, &text)),
    }
}

/// Reads one of the non-finite spellings, or a finite decimal in quotes.
fn parse_float_text(site: PersistSite, field: &str, text: &str) -> PersistResult<f32> {
    match text {
        "inf" => Ok(f32::INFINITY),
        "-inf" => Ok(f32::NEG_INFINITY),
        "nan" => Ok(f32::NAN),
        payload if payload.starts_with("nan:0x") => u32::from_str_radix(&payload[6..], 16)
            .map(f32::from_bits)
            .map_err(|_| not_exact(site, field, text)),
        // A quoted finite decimal is accepted so a hand-edited file may quote a
        // number without changing its value; it goes through exactly the same
        // finiteness check as the bare form, so `"1e40"` in quotes is refused too.
        quoted => match quoted.parse::<f32>() {
            Ok(parsed) if parsed.is_finite() => Ok(parsed),
            _ => Err(not_exact(site, field, quoted)),
        },
    }
}

fn not_exact(site: PersistSite, field: &str, value: &str) -> PersistError {
    PersistError::FloatNotExact {
        site,
        field: field.to_owned(),
        value: value.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// reading
// ---------------------------------------------------------------------------

/// Reads a `spore-creator-doc/1` text back into a document.
///
/// Fail-closed and line-precise. The checks run in this order, and the order is
/// the answer to "what is wrong with this file":
///
/// 1. `record`, `schema`, `float_encoding` and the two evidence grades — before
///    anything is interpreted, so a file that is a different format is reported as
///    a different format;
/// 2. `content_sha256` — before the body is parsed, so corruption is reported as
///    corruption rather than as whichever field it happened to damage;
/// 3. the body, one line at a time, each failure naming its line and block;
/// 4. `block_count` and `root_count` — cross-checks on the header once the body
///    it heads has been read.
///
/// Document **validity** is not checked here; that is [`crate::validate`]'s job,
/// and duplicating it would be a second opinion about the same record.
pub fn load(text: &str) -> PersistResult<CreatorDocument> {
    // Split off the header's terminating newline so the digest covers exactly
    // the bytes a second save would write for the same body.
    let (header_text, content_text) = match text.find('\n') {
        Some(at) => (&text[..at], &text[at + 1..]),
        None => (text, ""),
    };
    let content_lines: Vec<&str> = {
        let mut lines: Vec<&str> = content_text.split('\n').collect();
        // A trailing newline yields one empty tail element; that is the format's
        // normal ending, not an empty line. Any *other* empty line fails to parse
        // below, which is the fail-closed answer.
        if lines.last().is_some_and(|line| line.is_empty()) {
            lines.pop();
        }
        lines
    };

    let mut header = Fields::open(
        json::parse(header_text, 1)?,
        Ctx::document(1),
        "metadata record",
    )?;

    match json::text(Ctx::document(1), "record", &header.require("record")?)?.as_str() {
        RECORD_METADATA => {}
        other => {
            return Err(PersistError::Schema {
                site: PersistSite::document(1),
                reason: format!("this is a {other:?} record, expected {RECORD_METADATA:?}"),
            })
        }
    }
    match json::text(Ctx::document(1), "schema", &header.require("schema")?)?.as_str() {
        FORMAT => {}
        other => {
            return Err(PersistError::Schema {
                site: PersistSite::document(1),
                reason: format!("schema is {other:?}, this build reads {FORMAT:?}"),
            })
        }
    }
    // Read for presence, not for equality: which build wrote a file is history,
    // not a correctness question. The float encoding is the opposite — it
    // decides how the digits are read, so an unknown one is a refusal.
    json::text(Ctx::document(1), "generator", &header.require("generator")?)?;
    let encoding = json::text(
        Ctx::document(1),
        "float_encoding",
        &header.require("float_encoding")?,
    )?;
    if encoding != FLOAT_ENCODING {
        return Err(PersistError::UnsupportedFloatEncoding {
            site: PersistSite::document(1),
            found: encoding,
        });
    }
    read_grade(&mut header)?;

    let declared_blocks = json::index(
        Ctx::document(1),
        "block_count",
        &header.require("block_count")?,
    )?;
    let declared_roots = json::index(
        Ctx::document(1),
        "root_count",
        &header.require("root_count")?,
    )?;
    let declared_digest = json::text(
        Ctx::document(1),
        "content_sha256",
        &header.require("content_sha256")?,
    )?;

    let document_value = header.require("document")?;
    // The header must hold nothing else: an unrecognised field here is a
    // revision this build has not adopted.
    header.finish()?;

    let computed = {
        let mut hasher = Sha256::new();
        hasher.update(content_text.as_bytes());
        hasher.hex()
    };
    if declared_digest != computed {
        return Err(PersistError::Checksum {
            site: PersistSite::document(1),
            declared: declared_digest,
            computed,
        });
    }
    let mut document = read_document(document_value)?;
    // Body first, counts second, on purpose. The digest already rules out
    // truncation and corruption, so `block_count` is a cross-check on the header
    // rather than the first line of defence — and reading the body before it
    // means a blank or malformed line is reported *as the line it is*, which is
    // the answer a caller can open the file with.
    for (position, line) in content_lines.iter().enumerate() {
        let line_number = position + 2;
        document
            .blocks
            .push(read_block(json::parse(line, line_number)?, position)?);
    }
    if content_lines.len() != declared_blocks {
        return Err(PersistError::HeaderMismatch {
            site: PersistSite::document(1),
            field: "block_count".to_owned(),
            declared: declared_blocks,
            found: content_lines.len(),
        });
    }

    let roots = document.roots().len();
    if roots != declared_roots {
        return Err(PersistError::HeaderMismatch {
            site: PersistSite::document(1),
            field: "root_count".to_owned(),
            declared: declared_roots,
            found: roots,
        });
    }
    Ok(document)
}

/// [`load`], from the bytes a file would hold.
pub fn load_bytes(bytes: &[u8]) -> PersistResult<CreatorDocument> {
    let text = core::str::from_utf8(bytes).map_err(|_| PersistError::Syntax {
        site: PersistSite::document(1),
        reason: "the file is not valid UTF-8".to_owned(),
    })?;
    load(text)
}

/// Checks the two evidence grades against the ones this build writes.
///
/// A file that calls itself this schema with a **stronger** grade than
/// [`DOCUMENT_EVIDENCE`] is making a claim the corpus does not support, and a
/// weaker one is a claim this build cannot vouch for either. Both are refusals
/// rather than a quiet overwrite: the grade is the whole reason the fields exist.
fn read_grade(header: &mut Fields) -> PersistResult<()> {
    let site = PersistSite::document(1);
    let level = json::text(
        site.into(),
        "evidence_level",
        &header.require("evidence_level")?,
    )?;
    if spore_core::EvidenceLevel::parse(&level) != DOCUMENT_EVIDENCE {
        return Err(PersistError::Schema {
            site,
            reason: format!(
                "evidence_level is {level:?}; this build's facts are {}",
                DOCUMENT_EVIDENCE.as_str()
            ),
        });
    }
    let state = json::text(
        site.into(),
        "evidence_state",
        &header.require("evidence_state")?,
    )?;
    if spore_core::EvidenceState::parse(&state) != DOCUMENT_EVIDENCE_STATE {
        return Err(PersistError::Schema {
            site,
            reason: format!(
                "evidence_state is {state:?}; this build's facts are {}",
                DOCUMENT_EVIDENCE_STATE.as_str()
            ),
        });
    }
    Ok(())
}

fn read_document(value: Json) -> PersistResult<CreatorDocument> {
    let site = PersistSite::document(1);
    let mut fields = Fields::open(value, site.into(), "document")?;
    let format_version = match fields.take("format_version") {
        Some(value) => Some(json::word(site.into(), "format_version", &value)?),
        None => None,
    };
    let properties = read_properties(&fields.require("properties")?)?;
    fields.finish()?;
    Ok(CreatorDocument {
        format_version,
        properties,
        blocks: Vec::new(),
    })
}

fn read_properties(value: &Json) -> PersistResult<DocumentProperties> {
    let site = PersistSite::document(1);
    let mut fields = Fields::open(value.clone(), site.into(), "properties")?;
    let mut out = DocumentProperties::default();

    if let Some(value) = fields.take("model_type") {
        out.model_type = Some(json::word(site.into(), "model_type", &value)?);
    }
    if let Some(value) = fields.take("name") {
        out.name = Some(json::text(site.into(), "name", &value)?);
    }
    if let Some(value) = fields.take("description") {
        out.description = Some(json::text(site.into(), "description", &value)?);
    }
    if let Some(value) = fields.take("skin_effects") {
        out.skin_effects = Some(read_word_slots(&value, "skin_effects")?);
    }
    if let Some(value) = fields.take("skin_effect_seeds") {
        out.skin_effect_seeds = Some(read_word_slots(&value, "skin_effect_seeds")?);
    }
    if let Some(value) = fields.take("skin_colors") {
        let items = json::list::<3>(site.into(), "skin_colors", &value)?;
        let mut colours = [None; 3];
        for (slot, item) in colours.iter_mut().zip(items) {
            *slot = match item {
                Json::Null => None,
                colour => Some(read_colour(&colour, "skin_colors", site)?),
            };
        }
        out.skin_colors = Some(colours);
    }
    if let Some(value) = fields.take("required_pack") {
        out.required_pack = Some(json::word(site.into(), "required_pack", &value)?);
    }
    if let Some(value) = fields.take("zcorp_score") {
        out.zcorp_score = Some(json::word(site.into(), "zcorp_score", &value)?);
    }
    if let Some(value) = fields.take("has_thumbnail") {
        out.has_thumbnail = Some(json::flag(site.into(), "has_thumbnail", &value)?);
    }
    if let Some(value) = fields.take("cache_id") {
        out.cache_id = Some(json::word(site.into(), "cache_id", &value)?);
    }
    fields.finish()?;
    Ok(out)
}

fn read_word_slots<const N: usize>(value: &Json, field: &str) -> PersistResult<[Option<u32>; N]> {
    let site = PersistSite::document(1);
    let items = json::list::<N>(site.into(), field, value)?;
    let mut out = [None; N];
    for (slot, item) in out.iter_mut().zip(items) {
        *slot = match item {
            Json::Null => None,
            word => Some(json::word(site.into(), field, &word)?),
        };
    }
    Ok(out)
}

fn read_block(value: Json, position: BlockIndex) -> PersistResult<Block> {
    // First pass, only for `index`: every error below needs to name the block the
    // line declares, and `index` is the first key every save writes.
    let mut probe = Fields::open(value.clone(), Ctx::document(position + 2), "block")?;
    let declared = json::index(
        Ctx::document(position + 2),
        "index",
        &probe.require("index")?,
    )?;
    if declared != position {
        return Err(PersistError::BlockOrder {
            site: PersistSite::block(position + 2, declared),
            declared,
            found: position,
        });
    }
    drop(probe);

    let site = PersistSite::block(position + 2, declared);
    let mut fields = Fields::open(value, site.into(), "block")?;
    let mut out = Block::default();

    if let Some(value) = fields.take("block_id") {
        out.block_id = Some(read_block_id(&value, site)?);
    }
    out.transform = read_transform(&fields.require("transform")?, site)?;
    if let Some(value) = fields.take("snapped") {
        out.snapped = Some(json::flag(site.into(), "snapped", &value)?);
    }
    if let Some(value) = fields.take("children") {
        let items = json::items(site.into(), "children", &value)?;
        let mut children = Vec::with_capacity(items.len());
        for item in items {
            children.push(json::index(site.into(), "children", &item)?);
        }
        out.children = Some(children);
    }
    if let Some(value) = fields.take("symmetric") {
        out.symmetric = Some(json::index(site.into(), "symmetric", &value)?);
    }
    if let Some(value) = fields.take("paint") {
        out.paint = Some(read_paint(&value, site)?);
    }
    if let Some(value) = fields.take("handles") {
        out.handles = Some(read_handles(&value, site)?);
    }
    if let Some(value) = fields.take("limb") {
        out.limb = Some(read_limb(&value, site)?);
    }
    if let Some(value) = fields.take("user_orientation") {
        out.user_orientation = Some(read_matrix(&value, "user_orientation", site)?);
    }
    if let Some(value) = fields.take("bbox") {
        out.bbox = Some(read_bbox(&value, site)?);
    }
    // `index` is already consumed above; take it again so a file cannot carry it
    // twice (the JSON reader refuses that) and so it does not trip `finish`.
    let _ = fields.take("index");
    fields.finish()?;
    Ok(out)
}

fn read_block_id(value: &Json, site: PersistSite) -> PersistResult<BlockId> {
    let mut fields = Fields::open(value.clone(), site.into(), "block_id")?;
    let group_id = json::word(
        site.into(),
        "block_id.group_id",
        &fields.require("group_id")?,
    )?;
    let instance_id = json::word(
        site.into(),
        "block_id.instance_id",
        &fields.require("instance_id")?,
    )?;
    fields.finish()?;
    Ok(BlockId::new(group_id, instance_id))
}

fn read_transform(value: &Json, site: PersistSite) -> PersistResult<BlockTransform> {
    let mut fields = Fields::open(value.clone(), site.into(), "transform")?;
    // `position`, `scale` and `orientation` are values, not absences, so they are
    // required — the same rule the XML reader applies to `<transform>`.
    let position = read_vec3(&fields.require("position")?, "transform.position", site)?;
    let scale = read_float(site, "transform.scale", &fields.require("scale")?)?;
    let orientation = read_matrix(
        &fields.require("orientation")?,
        "transform.orientation",
        site,
    )?;
    let triangle_direction = match fields.take("triangle_direction") {
        Some(value) => Some(read_vec3(&value, "transform.triangle_direction", site)?),
        None => None,
    };
    let triangle_pick_origin = match fields.take("triangle_pick_origin") {
        Some(value) => Some(read_vec3(&value, "transform.triangle_pick_origin", site)?),
        None => None,
    };
    fields.finish()?;
    Ok(BlockTransform {
        position,
        scale,
        orientation,
        triangle_direction,
        triangle_pick_origin,
    })
}

fn read_vec3(value: &Json, field: &str, site: PersistSite) -> PersistResult<Vec3> {
    let mut fields = Fields::open(value.clone(), site.into(), field)?;
    let x = read_float(site, &format!("{field}.x"), &fields.require("x")?)?;
    let y = read_float(site, &format!("{field}.y"), &fields.require("y")?)?;
    let z = read_float(site, &format!("{field}.z"), &fields.require("z")?)?;
    fields.finish()?;
    Ok(Vec3 { x, y, z })
}

fn read_matrix(value: &Json, field: &str, site: PersistSite) -> PersistResult<Matrix3> {
    let mut fields = Fields::open(value.clone(), site.into(), field)?;
    let mut rows = [[0.0f32; 3]; 3];
    for (index, row) in rows.iter_mut().enumerate() {
        let path = format!("{field}.row{index}");
        let items = json::list::<3>(site.into(), &path, &fields.require(&format!("row{index}"))?)?;
        for (component, item) in row.iter_mut().zip(items) {
            *component = read_float(site, &path, &item)?;
        }
    }
    fields.finish()?;
    Ok(Matrix3 {
        row0: rows[0],
        row1: rows[1],
        row2: rows[2],
    })
}

fn read_colour(
    value: &Json,
    field: &str,
    site: PersistSite,
) -> PersistResult<crate::color::UnitRgb> {
    let mut fields = Fields::open(value.clone(), site.into(), field)?;
    let red = read_float(site, &format!("{field}.red"), &fields.require("red")?)?;
    let green = read_float(site, &format!("{field}.green"), &fields.require("green")?)?;
    let blue = read_float(site, &format!("{field}.blue"), &fields.require("blue")?)?;
    fields.finish()?;
    // `UnitRgb::new` is the range check the corpus grammar already applies, so this
    // is not a second opinion: a colour outside 0.0 ..= 1.0 is refused here exactly
    // as `UnitRgb::parse` refuses it. Clamping instead would make the save read
    // back and then print a corpus spelling the grammar itself refuses.
    crate::color::UnitRgb::new(red, green, blue).ok_or(PersistError::ColourOutOfRange {
        site,
        field: field.to_owned(),
        value: format!("{red},{green},{blue}"),
    })
}

fn read_paint(value: &Json, site: PersistSite) -> PersistResult<Vec<PaintRegion>> {
    let items = json::items(site.into(), "paint", value)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let mut fields = Fields::open(item, site.into(), "paint")?;
        let region = json::word(site.into(), "paint.region", &fields.require("region")?)?;
        let paint_id = json::word(site.into(), "paint.paint_id", &fields.require("paint_id")?)?;
        let color1 = read_colour(&fields.require("color1")?, "paint.color1", site)?;
        let color2 = read_colour(&fields.require("color2")?, "paint.color2", site)?;
        fields.finish()?;
        out.push(PaintRegion {
            region,
            paint_id,
            color1,
            color2,
        });
    }
    Ok(out)
}

fn read_handles(value: &Json, site: PersistSite) -> PersistResult<Vec<ChannelWeight>> {
    let items = json::items(site.into(), "handles", value)?;
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let mut fields = Fields::open(item, site.into(), "handles")?;
        let channel = json::word(site.into(), "handles.channel", &fields.require("channel")?)?;
        let weight = read_float(site, "handles.weight", &fields.require("weight")?)?;
        fields.finish()?;
        out.push(ChannelWeight { channel, weight });
    }
    Ok(out)
}

fn read_limb(value: &Json, site: PersistSite) -> PersistResult<Limb> {
    let mut fields = Fields::open(value.clone(), site.into(), "limb")?;
    let mut out = Limb::default();
    if let Some(value) = fields.take("limb_type") {
        out.limb_type = Some(json::word(site.into(), "limb.limb_type", &value)?);
    }
    if let Some(value) = fields.take("muscle_scale") {
        out.muscle_scale = Some(read_float(site, "limb.muscle_scale", &value)?);
    }
    if let Some(value) = fields.take("base_muscle_scale") {
        out.base_muscle_scale = Some(read_float(site, "limb.base_muscle_scale", &value)?);
    }
    fields.finish()?;
    Ok(out)
}

fn read_bbox(value: &Json, site: PersistSite) -> PersistResult<(Vec3, Vec3)> {
    let mut fields = Fields::open(value.clone(), site.into(), "bbox")?;
    // Both corners are required: the XML reader requires both `<blockbbox_min>`
    // and `<blockbbox_max>`, and a half bbox is not a shape this corpus has.
    let min = read_vec3(&fields.require("min")?, "bbox.min", site)?;
    let max = read_vec3(&fields.require("max")?, "bbox.max", site)?;
    fields.finish()?;
    Ok((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every field the format encodes, with a value that is not a default.
    ///
    /// This is the anti-regression list, and it is a list rather than a claim in
    /// prose because a claim in prose does not fail. A field added to [`Block`]
    /// **must** be added here with a value that is not its `Default`, or nothing
    /// will notice that it is not persisted — Rust has no reflection, and an
    /// unpersisted `None` round trips perfectly. The corollary is that a change to
    /// this fixture is a change to what the format claims about a document.
    fn every_field_document() -> CreatorDocument {
        let colour = crate::color::UnitRgb::new(0.341176, 0.360784, 0.576470).unwrap();
        CreatorDocument {
            format_version: Some(11),
            properties: DocumentProperties {
                model_type: Some(0x9ea3_031a),
                name: Some("a \"quoted\" name\nwith a newline".to_owned()),
                description: Some(String::new()),
                skin_effects: Some([Some(1), None, Some(0x9ea3_031a)]),
                skin_effect_seeds: Some([None, Some(7), None]),
                skin_colors: Some([
                    Some(colour),
                    None,
                    Some(crate::color::UnitRgb::new(1.0, 1.0, 1.0).unwrap()),
                ]),
                required_pack: Some(0x0241_0000),
                zcorp_score: Some(0),
                has_thumbnail: Some(true),
                cache_id: Some(0xffff_ffff),
            },
            blocks: vec![
                Block {
                    block_id: Some(BlockId::new(0x4062_6000, 0xd05c_53a3)),
                    transform: BlockTransform {
                        position: Vec3 {
                            x: -0.0,
                            y: 0.450979,
                            z: 7.26504e-9,
                        },
                        scale: 2.254181,
                        orientation: Matrix3 {
                            row0: [1.0, -0.0, 0.0],
                            row1: [0.0, 1.0, -1.0],
                            row2: [3.4028235e38, f32::MIN_POSITIVE, 0.0],
                        },
                        triangle_direction: Some(Vec3 {
                            x: f32::INFINITY,
                            y: f32::NEG_INFINITY,
                            z: 0.1,
                        }),
                        triangle_pick_origin: None,
                    },
                    snapped: Some(false),
                    children: Some(vec![1, 2, 3]),
                    // Reciprocal with block 2, so the fixture writes every optional
                    // field of `Block` at least once.
                    symmetric: Some(2),
                    paint: Some(vec![PaintRegion {
                        region: 0x1a,
                        paint_id: 0xf596_7f30,
                        color1: colour,
                        color2: crate::color::UnitRgb::new(0.0, 0.0, 0.0).unwrap(),
                    }]),
                    handles: Some(vec![
                        ChannelWeight {
                            channel: 3,
                            weight: -0.0,
                        },
                        ChannelWeight {
                            channel: 0,
                            weight: 0.75,
                        },
                    ]),
                    limb: Some(Limb {
                        limb_type: Some(2),
                        muscle_scale: Some(1.5),
                        base_muscle_scale: None,
                    }),
                    user_orientation: Some(Matrix3::IDENTITY),
                    bbox: Some((
                        Vec3 {
                            x: -1.0,
                            y: 0.0,
                            z: 0.0,
                        },
                        Vec3 {
                            x: 1.0,
                            y: 0.0,
                            z: 0.0,
                        },
                    )),
                },
                // A symmetry partner with the two triangle fields absent, which
                // is what 10 of the 680 corpus blocks look like.
                Block {
                    block_id: Some(BlockId::new(0x4062_6000, 0x1111_1111)),
                    transform: BlockTransform {
                        position: Vec3::default(),
                        scale: 0.28,
                        orientation: Matrix3::default(),
                        triangle_direction: Some(Vec3::default()),
                        triangle_pick_origin: Some(Vec3::default()),
                    },
                    snapped: Some(true),
                    children: None,
                    symmetric: Some(0),
                    paint: None,
                    handles: None,
                    limb: None,
                    user_orientation: None,
                    bbox: None,
                },
                Block {
                    block_id: Some(BlockId::new(0x4062_6000, 0x2222_2222)),
                    transform: BlockTransform {
                        position: Vec3::default(),
                        scale: 1.0,
                        orientation: Matrix3::IDENTITY,
                        triangle_direction: None,
                        triangle_pick_origin: None,
                    },
                    snapped: None,
                    // The other half of the absent/empty distinction: an empty
                    // list that *was* written.
                    children: Some(Vec::new()),
                    symmetric: Some(0),
                    paint: Some(Vec::new()),
                    handles: Some(Vec::new()),
                    limb: Some(Limb::default()),
                    user_orientation: None,
                    bbox: None,
                },
                Block {
                    block_id: Some(BlockId::new(0x4062_6000, 0x3333_3333)),
                    transform: BlockTransform {
                        position: Vec3::default(),
                        scale: 1.0,
                        orientation: Matrix3::IDENTITY,
                        triangle_direction: None,
                        triangle_pick_origin: None,
                    },
                    // Every optional field absent.
                    ..Block::default()
                },
            ],
        }
    }

    #[test]
    fn every_field_round_trips_bit_exactly() {
        let document = every_field_document();
        let text = save(&document).unwrap();
        let reloaded = load(&text).unwrap();
        assert_eq!(
            reloaded, document,
            "a save that reads back as a different document is not a save"
        );
        // And the second save is the same file, so "equal documents" and
        // "equal bytes" are not two different properties.
        assert_eq!(save(&reloaded).unwrap(), text);
    }

    /// The keys of one block line, in written order.
    fn block_line_keys(text: &str, content_line: usize) -> Vec<String> {
        let line = text.lines().nth(content_line).expect("a content line");
        match json::parse(line, content_line + 1).expect("one JSON object") {
            Json::Object(entries) => entries.into_iter().map(|(key, _)| key).collect(),
            other => panic!("content line {content_line} is a {}", other.type_name()),
        }
    }

    #[test]
    fn the_key_set_of_a_block_line_is_pinned() {
        // Renaming a persisted key, adding one, or reordering them is a **format**
        // change and has to move one of these lists on purpose. That is the friction
        // a versioned format is supposed to have.
        let document = every_field_document();
        assert_eq!(
            block_line_keys(&save(&document).unwrap(), 1),
            [
                "index",
                "block_id",
                "transform",
                "snapped",
                "children",
                "symmetric",
                "paint",
                "handles",
                "limb",
                "user_orientation",
                "bbox",
            ],
            "block 0 writes every optional field, so every key is present, in struct order"
        );
        // Block 3 writes none of its optional fields, so its line carries only the
        // three it must: `index`, `block_id` and `transform`.
        assert_eq!(
            block_line_keys(&save(&document).unwrap(), 4),
            ["index", "block_id", "transform"],
            "an absent field is an absent key, never a null and never an empty value"
        );
        assert_ne!(
            Block::default(),
            document.blocks[0],
            "the fixture has to differ from a default block, or it proves nothing"
        );
    }

    #[test]
    fn an_absent_child_list_and_a_written_empty_one_are_different_bytes() {
        let document = every_field_document();
        // Block 1 writes none; block 2 writes an empty one.
        assert!(!document.blocks[1].writes_children());
        assert!(document.blocks[2].writes_children());

        let text = save(&document).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert!(
            !lines[2].contains("\"children\""),
            "block 1 wrote no <childlist>, so its save must carry no key: {}",
            lines[2]
        );
        assert!(
            lines[3].contains("\"children\":[]"),
            "block 2 wrote an empty <childlist>, which is not the same as none: {}",
            lines[3]
        );

        // Both directions: the reloaded document keeps the distinction, and
        // flipping one produces different bytes and a different document.
        let reloaded = load(&text).unwrap();
        assert_eq!(reloaded, document);
        assert!(!reloaded.blocks[1].writes_children());
        assert!(reloaded.blocks[2].writes_children());

        let mut flipped = reloaded.clone();
        flipped.blocks[1].children = Some(Vec::new());
        let other = save(&flipped).unwrap();
        assert_ne!(
            other, text,
            "one absent-vs-empty change must change the bytes"
        );
        assert_ne!(load(&other).unwrap(), reloaded);
    }

    #[test]
    fn a_negative_zero_survives_as_a_negative_zero() {
        // `0.0 == -0.0` under IEEE, so a derived comparison would call this a pass
        // on a document whose sign bit moved. Both halves are asserted: the value
        // and the bytes, because only the value would not notice a format that
        // normalises the spelling while keeping the sign.
        let mut document = CreatorDocument::default();
        document.blocks.push(Block {
            block_id: Some(BlockId::new(1, 2)),
            transform: BlockTransform {
                position: Vec3 {
                    x: -0.0,
                    y: 0.0,
                    z: -0.0,
                },
                scale: -0.0,
                ..BlockTransform::default()
            },
            ..Block::default()
        });
        let text = save(&document).unwrap();
        assert!(text.contains("\"x\":\"-0.0\""), "{text}");
        assert!(text.contains("\"y\":\"0.0\""), "{text}");
        assert!(text.contains("\"scale\":\"-0.0\""), "{text}");

        let reloaded = load(&text).unwrap();
        assert_eq!(reloaded, document);
        assert_eq!(
            reloaded.blocks[0].transform.position.x.to_bits(),
            (-0.0f32).to_bits()
        );
        assert_eq!(
            reloaded.blocks[0].transform.position.y.to_bits(),
            (0.0f32).to_bits()
        );
        assert_ne!(
            reloaded.blocks[0].transform.position,
            Vec3 {
                x: 0.0,
                y: 0.0,
                z: 0.0
            },
            "the sign bit is the fact"
        );
    }

    #[test]
    fn a_nan_keeps_its_sign_bit_and_payload() {
        // The one case a decimal spelling cannot carry: `-f32::NAN` formats as
        // `NaN` and reads back as the *positive* default quiet NaN.
        let mut document = CreatorDocument::default();
        document.blocks.push(Block {
            block_id: Some(BlockId::new(1, 2)),
            transform: BlockTransform {
                position: Vec3 {
                    x: -f32::NAN,
                    y: f32::NAN,
                    z: f32::from_bits(0x7fc0_0001),
                },
                scale: f32::INFINITY,
                ..BlockTransform::default()
            },
            ..Block::default()
        });
        let reloaded = load(&save(&document).unwrap()).unwrap();
        assert_eq!(reloaded, document);
        assert_eq!(
            reloaded.blocks[0].transform.position.x.to_bits(),
            0xffc0_0000,
            "-NaN must not come back as +NaN"
        );
    }

    #[test]
    fn saving_twice_gives_identical_bytes() {
        let document = every_field_document();
        assert_eq!(save(&document).unwrap(), save(&document).unwrap());
        assert_eq!(
            save_bytes(&document).unwrap(),
            save_bytes(&document).unwrap()
        );
    }

    #[test]
    fn the_header_carries_no_timestamp_no_path_and_no_pid() {
        let text = save(&every_field_document()).unwrap();
        let header = text.lines().next().unwrap();
        for forbidden in [
            "time", "date", "pid", "path", "user", "host", "cwd", "dir", "file",
        ] {
            assert!(
                !header.to_ascii_lowercase().contains(forbidden),
                "the header names {forbidden:?}, which is not a property of the document: {header}"
            );
        }
    }

    #[test]
    fn the_digest_covers_the_content_lines_and_is_not_self_referential() {
        let document = every_field_document();
        let text = save(&document).unwrap();
        let split = text.find('\n').expect("a header line");
        let mut hasher = Sha256::new();
        hasher.update(&text.as_bytes()[split + 1..]);
        let digest = hasher.hex();
        assert_eq!(content_digest(&document), digest);

        // Non-self-referential means exactly this: the header's own text is not
        // part of what it hashes, so nothing the header says can change the
        // digest it declares. Editing a header field leaves the digest alone.
        let edited_header = text.replacen(
            "\"generator\":\"spore-creator-core\"",
            "\"generator\":\"someone-else\"",
            1,
        );
        assert!(edited_header != text);
        let edited_split = edited_header.find('\n').expect("a header line");
        let mut edited_hasher = Sha256::new();
        edited_hasher.update(&edited_header.as_bytes()[edited_split + 1..]);
        assert_eq!(
            edited_hasher.hex(),
            digest,
            "the header cannot reach its own digest"
        );
        // An empty document's content is empty, and the empty digest is a known
        // value — which is what makes "no content lines" checkable rather than a
        // special case.
        let empty = save(&CreatorDocument::default()).unwrap();
        assert_eq!(empty.lines().count(), 1);
        assert!(empty.contains("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"));
    }

    #[test]
    fn the_wrong_schema_is_refused_rather_than_best_guessed() {
        let text = save(&every_field_document()).unwrap();
        let text = text.replace(FORMAT, "spore-creator-doc/2");
        assert!(matches!(
            load(&text),
            Err(PersistError::Schema {
                site: PersistSite {
                    line: 1,
                    block: None
                },
                ..
            })
        ));
    }

    #[test]
    fn an_unknown_float_encoding_is_refused() {
        let text = save(&every_field_document())
            .unwrap()
            .replace(FLOAT_ENCODING, "shortest-decimal-please");
        assert!(matches!(
            load(&text),
            Err(PersistError::UnsupportedFloatEncoding {
                site: PersistSite {
                    line: 1,
                    block: None
                },
                ..
            })
        ));
    }

    #[test]
    fn a_stronger_evidence_grade_than_the_corpus_supports_is_refused() {
        for grade in ["VERIFIED", "CONFIRMED", "UNKNOWN"] {
            let text = save(&every_field_document())
                .unwrap()
                .replace(DOCUMENT_EVIDENCE.as_str(), grade);
            assert!(
                matches!(
                    load(&text),
                    Err(PersistError::Schema {
                        site: PersistSite {
                            line: 1,
                            block: None
                        },
                        ..
                    })
                ),
                "evidence_level {grade:?} must be refused"
            );
        }
    }

    #[test]
    fn an_unknown_field_is_refused_with_its_line_and_never_dropped() {
        let text = save(&every_field_document()).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        // Line 1 is the header, line 2 is content line 0.
        lines[1] = lines[1].replace("{\"index\":0", "{\"surprise\":1,\"index\":0");
        // The digest is checked before the body, so re-sign to reach the reader.
        let text = resign(&format!("{}\n", lines.join("\n")));
        assert!(matches!(
            load(&text),
            Err(PersistError::UnexpectedField {
                site: PersistSite {
                    line: 2,
                    block: Some(0)
                },
                ..
            })
        ));
    }

    #[test]
    fn a_required_field_that_is_missing_is_refused() {
        // A renamed required field is `MissingField`, not a defaulted value: the
        // reader asks for it by name and refuses when it is not there.
        let text = save(&every_field_document()).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        lines[1] = lines[1].replace("\"transform\":{", "\"transform_without_scale\":{");
        let text = resign(&format!("{}\n", lines.join("\n")));
        assert!(matches!(
            load(&text),
            Err(PersistError::MissingField {
                site: PersistSite {
                    line: 2,
                    block: Some(0)
                },
                field,
                ..
            }) if field == "transform"
        ));

        // `index` is required too, and it is read before anything else so that
        // every later failure can name the block.
        let text = save(&every_field_document()).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        lines[3] = lines[3].replace("\"index\":2,", "");
        let text = resign(&format!("{}\n", lines.join("\n")));
        assert!(matches!(
            load(&text),
            Err(PersistError::MissingField {
                site: PersistSite {
                    line: 4,
                    block: None
                },
                ..
            })
        ));
    }

    #[test]
    fn a_malformed_line_names_its_line_number() {
        // Two good lines and then a broken one, which is content line 2 and so
        // file line 4. The failure has to say so: "the file is malformed" would
        // not tell a caller which line to open.
        let good = save(&every_field_document())
            .unwrap()
            .lines()
            .skip(1)
            .take(2)
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let body = format!("{}\n{{\"index\":2,\"transform\":}}\n", good.join("\n"));
        let text = signed(&body, 3, 2);
        assert!(matches!(
            load(&text),
            Err(PersistError::Syntax {
                site: PersistSite {
                    line: 4,
                    block: None
                },
                ..
            })
        ));
    }

    #[test]
    fn a_field_that_is_not_an_object_says_which_field_and_where() {
        // "expected object, found array" without a path is not an answer, so every
        // object in this grammar is opened with the path it was reached by.
        let text = save(&every_field_document()).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        // The line stays valid JSON, so the failure is the field's *type* and not
        // the line's syntax.
        lines[1] = lines[1].replace(
            "\"block_id\":{\"group_id\":\"0x40626000\",\"instance_id\":\"0xd05c53a3\"}",
            "\"block_id\":[\"0x40626000\",\"0xd05c53a3\"]",
        );
        assert_ne!(lines[1], text.lines().nth(1).expect("content line 0"));
        let text = resign(&format!("{}\n", lines.join("\n")));
        let error = load(&text).unwrap_err();
        assert_eq!(
            error,
            PersistError::WrongType {
                site: PersistSite::block(2, 0),
                field: "block_id".to_owned(),
                expected: "object".to_owned(),
                found: "array".to_owned(),
            },
            "{error}"
        );
    }

    #[test]
    fn a_blank_line_in_the_body_is_a_syntax_error_and_not_an_empty_block() {
        let document = every_field_document();
        let text = save(&document).unwrap();
        let (header, body) = text.split_once('\n').expect("a header line");
        let broken = resign(&format!("{header}\n\n{}", body.replacen('\n', "\n\n", 1)));
        assert!(
            matches!(
                load(&broken),
                Err(PersistError::Syntax {
                    site: PersistSite { line: 2, .. },
                    ..
                })
            ),
            "{:?}",
            load(&broken)
        );
    }

    #[test]
    fn a_json_number_that_overflows_to_infinity_is_refused() {
        // `"1e40".parse::<f32>()` is `Ok(f32::INFINITY)`. Trusting it would hand
        // back a document with an infinite transform and no error.
        let mut document = one_block_document();
        document.blocks[0].transform.scale = 1.0;
        let text = save(&document).unwrap();
        let text = text.replacen("\"scale\":\"1.0\"", "\"scale\":1e40", 1);
        let text = resign(&text);
        assert!(matches!(
            load(&text),
            Err(PersistError::FloatNotExact {
                site: PersistSite {
                    line: 2,
                    block: Some(0)
                },
                ..
            })
        ));
    }

    #[test]
    fn an_error_inside_a_block_names_that_blocks_line_and_index() {
        // The attribution is the point of the typed errors, so it is checked for a
        // *nested* value rather than only for a top-level one: a bad colour inside
        // block 0\'s `<paint>` entry must say `line 2, block 0` and not `line 1`.
        // `UnitRgb::new` is the range check the corpus grammar already applies, so
        // a file claiming 9.0 for a colour component is refused here rather than
        // clamped into something this build would draw.
        let text = save(&every_field_document()).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        lines[1] = lines[1].replacen("\"blue\":\"0.0\"", "\"blue\":\"9.0\"", 1);
        assert_ne!(lines[1], text.lines().nth(1).expect("content line 0"));
        let text = resign(&format!("{}\n", lines.join("\n")));
        assert_eq!(
            load(&text).unwrap_err(),
            PersistError::ColourOutOfRange {
                site: PersistSite::block(2, 0),
                field: "paint.color2".to_owned(),
                value: "0,0,9".to_owned(),
            },
            "the site and the field must both be reported"
        );
    }

    #[test]
    fn an_overflowing_number_inside_a_block_is_refused_at_that_blocks_line() {
        // `"1e40".parse::<f32>()` is `Ok(f32::INFINITY)`, so a reader that trusted
        // the parse would silently widen a handle weight to infinity.
        let text = save(&every_field_document()).unwrap();
        let text = text.replacen("\"weight\":\"0.75\"", "\"weight\":1e40", 1);
        let text = resign(&text);
        let error = load(&text).unwrap_err();
        assert_eq!(
            error,
            PersistError::FloatNotExact {
                site: PersistSite::block(2, 0),
                field: "handles.weight".to_owned(),
                value: "1e40".to_owned(),
            },
            "{error}"
        );
    }

    #[test]
    fn a_corrupted_body_reports_the_digest_not_a_stray_field() {
        let text = save(&every_field_document())
            .unwrap()
            .replace("\"scale\":\"2.254181\"", "\"scale\":\"2.254182\"");
        assert!(matches!(
            load(&text),
            Err(PersistError::Checksum {
                site: PersistSite {
                    line: 1,
                    block: None
                },
                ..
            })
        ));
    }

    #[test]
    fn a_reordered_body_is_refused_because_order_is_the_index_space() {
        let document = every_field_document();
        let text = save(&document).unwrap();
        let mut lines: Vec<&str> = text.lines().collect();
        lines.swap(1, 2);
        let text = resign(&format!("{}\n", lines.join("\n")));
        assert!(matches!(
            load(&text),
            Err(PersistError::BlockOrder {
                site: PersistSite {
                    line: 2,
                    block: Some(1)
                },
                declared: 1,
                found: 0,
            })
        ));
    }

    #[test]
    fn a_header_count_that_disagrees_with_the_body_is_refused() {
        let document = every_field_document();
        let text = save(&document).unwrap();
        let text = text.replacen("\"block_count\":4", "\"block_count\":5", 1);
        assert!(matches!(
            load(&text),
            Err(PersistError::HeaderMismatch {
                field,
                declared: 5,
                found: 4,
                ..
            }) if field == "block_count"
        ));
    }

    #[test]
    fn a_non_utf8_file_is_refused_rather_than_lossy_decoded() {
        let mut bytes = save(&every_field_document()).unwrap().into_bytes();
        bytes[5] = 0xff;
        assert!(matches!(
            load_bytes(&bytes),
            Err(PersistError::Syntax {
                site: PersistSite {
                    line: 1,
                    block: None
                },
                ..
            })
        ));
    }

    #[test]
    fn a_hex_index_is_refused_because_an_index_is_a_position() {
        let mut document = every_field_document();
        document.blocks[0].children = Some(vec![0x10]);
        let text = save(&document).unwrap();
        let text = text.replacen("\"children\":[16]", "\"children\":[\"0x10\"]", 1);
        let text = resign(&text);
        assert!(matches!(
            load(&text),
            Err(PersistError::WrongType {
                site: PersistSite {
                    line: 2,
                    block: Some(0)
                },
                ..
            })
        ));
    }

    #[test]
    #[ignore = "sweeps all 2^32 f32 patterns; run it with --release"]
    fn every_float_round_trips_bit_exactly_across_the_whole_bit_space() {
        // The exhaustive proof, and the reason the hermetic sweep above is allowed
        // to be a sweep. 2^32 patterns, sharded across the available cores, each
        // written and read back and compared **by bits** — because `0.0 == -0.0`
        // and `NaN != NaN`, so a numeric comparison would pass a format that
        // dropped a sign bit and fail on one that faithfully carried a NaN.
        const PATTERNS: u64 = 1 << 32;
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(16);
        let shard = PATTERNS / workers as u64;
        std::thread::scope(|scope| {
            for worker in 0..workers {
                let start = worker as u64 * shard;
                let end = if worker + 1 == workers {
                    PATTERNS
                } else {
                    start + shard
                };
                scope.spawn(move || {
                    let site = PersistSite::document(1);
                    let mut out = Out::new();
                    for at in start..end {
                        let bits = at as u32;
                        let text = float_text(f32::from_bits(bits));
                        // Round-trip through the real writer/reader pair: the
                        // value is a JSON *string* either way, and the reader has
                        // to see the quotes to know that.
                        out.string(&text);
                        let written = out.take();
                        let value = json::parse(&written, 1).expect("the writer emits one value");
                        let back = read_float(site, "probe", &value).unwrap_or_else(|error| {
                            panic!("bits {bits:08x} wrote {text:?}: {error}")
                        });
                        assert_eq!(back.to_bits(), bits, "bits {bits:08x} wrote {text:?}");
                    }
                });
            }
        });
    }

    fn one_block_document() -> CreatorDocument {
        CreatorDocument {
            format_version: Some(16),
            properties: DocumentProperties::default(),
            blocks: vec![Block {
                block_id: Some(BlockId::new(0x4062_6000, 0xd05c_53a3)),
                transform: BlockTransform {
                    scale: 1.0,
                    orientation: Matrix3::IDENTITY,
                    ..BlockTransform::default()
                },
                snapped: Some(true),
                ..Block::default()
            }],
        }
    }

    /// A valid header wrapped around an arbitrary `body`, so a test can reach the
    /// body reader instead of stopping at the integrity check.
    fn signed(body: &str, blocks: usize, roots: usize) -> String {
        let mut hasher = Sha256::new();
        hasher.update(body.as_bytes());
        let mut out = Out::new();
        out.object();
        out.key("record");
        out.string(RECORD_METADATA);
        out.key("schema");
        out.string(FORMAT);
        out.key("generator");
        out.string(GENERATOR);
        out.key("float_encoding");
        out.string(FLOAT_ENCODING);
        out.key("evidence_level");
        out.string(DOCUMENT_EVIDENCE.as_str());
        out.key("evidence_state");
        out.string(DOCUMENT_EVIDENCE_STATE.as_str());
        out.key("block_count");
        out.index(blocks);
        out.key("root_count");
        out.index(roots);
        out.key("content_sha256");
        out.string(&hasher.hex());
        out.key("document");
        write_document(&mut out, &CreatorDocument::default());
        out.end('}');
        out.line();
        let mut text = out.finish();
        text.push_str(body);
        text
    }

    /// Recomputes the header's `content_sha256` for a hand-edited body, so a test
    /// can reach the *body* reader instead of stopping at the integrity check.
    ///
    /// Deliberately a test helper rather than a public `resign`: a caller that
    /// could re-sign a file could also produce a file that passes the digest and
    /// says something false, and the digest's value is that nobody does.
    fn resign(text: &str) -> String {
        let split = text.find('\n').expect("a header line");
        let mut hasher = Sha256::new();
        hasher.update(&text.as_bytes()[split + 1..]);
        let digest = hasher.hex();
        let (header, content) = text.split_at(split + 1);
        let at = header
            .find("\"content_sha256\":\"")
            .expect("a digest field")
            + "\"content_sha256\":\"".len();
        let end = header[at..].find('"').expect("a closing quote") + at;
        format!("{}{}{}", &header[..at], digest, &header[end..]).to_owned() + content
    }
}
