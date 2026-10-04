//! Comparison 5: the raster envelope, the layer count, and the base mip.
//!
//! Rust `spore_texture::parse_envelope` / `spore_texture::decode_raster` against
//! `tools/spore/raster/raster.py::parse_envelope` / `::layer_count` / `::decode` and
//! `tools/spore/dxt5/dxt5.py::decode_image`, for a deterministic bounded sample.
//!
//! # Three comparisons, three confidences
//!
//! * The 32-byte envelope is compared field by field (eight `u32`s). Both sides
//!   read the envelope without validating it, so this is a clean field
//!   comparison, and it is the one arm that still means "do these two
//!   implementations read the same bytes".
//! * The **layer count** is compared. `raster.py::layer_count` derives it by
//!   dividing `(record_size - 32)` by `16 + chain`, where `chain` charges **8**
//!   bytes per 4x4 block. A byte count over 4386 of 4386 raster records in the
//!   two content packages settles that this is wrong: a 256² `DXT5` base level
//!   occupies 65 536 bytes, which is 4096 blocks of *16*, and every recorded
//!   level prefix says 16. See `docs/RASTER-FRAMING.md` §2.
//! * The decoded base mip is compared by `sha256(rgba)[:16]`, which is exactly
//!   what `raster.py`'s own CLI prints. A hash is the right instrument here: the
//!   decoded image is megabytes, and "are these the same bytes" is the only
//!   question that matters. Only a small number of records are decoded, because
//!   decoding is the expensive part.
//!
//! # What the last two arms now measure: a refuted oracle, not a defect
//!
//! **The raster oracles on this side of the boundary implement a framing the
//! corpus refuted**, so every layer-count and base-mip disagreement here has the
//! same cause and the same verdict, and the report says so per record:
//!
//! * `raster.py` reports **2 layers** for a 512² `DXT5` record and refuses the
//!   256² ones outright ("payload bytes not a multiple of ..."). Under the
//!   measured framing there is exactly **one** chain in every record of all
//!   three formats -- 2954 of 2954 in `Spore_Content.package` and 1432 of 1432
//!   in `Spore_EP1_Content_01.package` -- and the record's length is exactly
//!   what its own envelope implies.
//! * `dxt5.py` decodes an 8-byte "Spore block". The records hold standard BC3.
//!   So a `DXT5` base-mip digest disagreement is the *expected* outcome, not a
//!   port defect, and the codec itself remains `spore-texture`'s `Inferred`
//!   claim: no decode of a real record has been compared against the game's own
//!   output.
//! * `raster.py::decode` refuses `fourcc` other than `DXT5`, so the 901 `0x15`
//!   and 306 `DXT1` records of this package produce an envelope comparison that
//!   agrees and a **one-sided** decode: the Rust side has a second
//!   implementation of the framing (the driver's `rasterframe` walk, which
//!   agrees over 4386 of 4386 records in `tests/luminance_corpus.rs`) and the
//!   oracle has none.
//!
//! None of that is worked around, and none of it is asserted clean. The
//! `ValueMismatch` class is kept for the digest disagreements because it is the
//! honest label -- two sides answering one question differently, one of them
//! wrong -- and the note on each divergence says which side the byte count
//! supports. The framing itself is checked against a *second transcription* of
//! the measured layout rather than against a refuted oracle; that check is
//! `tests/luminance_corpus.rs`, and it is where a framing claim belongs.
//!
//! # The old `0x15xx` guess, refuted
//!
//! This module used to define the family as the *range* `0x1500..=0x15FF` and
//! carry a `is_luminance` predicate over it. No record in the corpus is in that
//! range: the family's format word is the single value `0x00000015`. The range
//! came from a design document and from a Python comment, and it is gone.

use spore_assets::Package;
use spore_core::record::type_id;

use crate::corpus::{Corpus, LocatedPackage};
use crate::floats::ReportBuilder;
use crate::json::Json;
use crate::oracle::{classify_refusal, OracleConfig, OracleError, OracleSession};
use crate::report::{Agreement, DifferentialReport, Divergence, DivergenceClass};
use crate::sample::{describe_sample, sample_indices};

use super::{primary_package, skipped};

/// Report name for this comparison.
///
/// Renamed from `raster-envelope-dxt5`: the name used to promise agreement with
/// `dxt5.py`, and the corpus settled that this half of the comparison measures a
/// disagreement instead.
pub const NAME: &str = "raster-envelope-layer-count-and-base-mip";

/// How the oracle is invoked.
pub const INVOCATION: &str = "persistent `python3 -c` driver, one session per package; one \
                              NDJSON {cmd:\"raster\", i:N} request per sampled record, answered \
                              from raster.py::parse_envelope, raster.py::layer_count and \
                              raster.py::decode (which calls dxt5.py::decode_image). NOTE: the \
                              layer count and the block codec raster.py uses are the framing a \
                              byte count over 4386 of 4386 raster records REFUTED -- see this \
                              module's documentation and docs/RASTER-FRAMING.md section 2";

/// How many records the default sample draws for the envelope comparison.
pub const DEFAULT_SAMPLE: usize = 150;

/// How many of the sampled records are also decoded and hashed.
pub const DEFAULT_DECODE_SAMPLE: usize = 12;

/// Format word of the `DXT5` family, as the two sides each spell it.
pub const DXT5_FOURCC: u32 = 0x3554_5844;
/// Format word of the `DXT1` family.
pub const DXT1_FOURCC: u32 = 0x3154_5844;
/// Format word of the uncompressed four-lane family, `0x00000015`.
///
/// Not a FOURCC: the record carries one byte of format enum and three bytes of
/// padding. The old `0x1500..=0x15FF` guess for this family matched nothing in
/// the corpus.
pub const LUMINANCE_FOURCC: u32 = 0x0000_0015;

/// The three format words the corpus measures.
pub const MEASURED_FOURCCS: [u32; 3] = [DXT5_FOURCC, DXT1_FOURCC, LUMINANCE_FOURCC];

/// True when the corpus measures a framing for `fourcc`.
///
/// Absence is not a verdict: a `fourcc` outside this set would mean a family
/// nobody has characterised, and the caller should say so rather than refuse it
/// as malformed.
#[must_use]
pub fn has_measured_framing(fourcc: u32) -> bool {
    MEASURED_FOURCCS.contains(&fourcc)
}

/// Runs the raster comparison over the corpus's primary package.
pub fn compare(corpus: &Corpus, config: &OracleConfig) -> DifferentialReport {
    compare_with_budgets(corpus, config, DEFAULT_SAMPLE, DEFAULT_DECODE_SAMPLE)
}

/// Runs the raster comparison with explicit sample budgets.
pub fn compare_with_budgets(
    corpus: &Corpus,
    config: &OracleConfig,
    budget: usize,
    decode_budget: usize,
) -> DifferentialReport {
    super::run(NAME, || {
        let Some(package) = primary_package(corpus) else {
            return Ok::<_, OracleError>(skipped(
                NAME,
                format!(
                    "no package located (set {} to a checkout with SPORE/)",
                    crate::corpus::ROOT_ENV
                ),
            ));
        };
        compare_package(package, config, budget, decode_budget)
    })
}

/// Runs the raster comparison over one package.
pub fn compare_package(
    package: &LocatedPackage,
    config: &OracleConfig,
    budget: usize,
    decode_budget: usize,
) -> Result<DifferentialReport, OracleError> {
    let mut builder = ReportBuilder::new(NAME, INVOCATION);
    builder.note(format!(
        "package {}; Rust side: spore_texture::parse_envelope, framing::record_size and \
         decode_raster; oracle: raster.py::parse_envelope, raster.py::layer_count and \
         raster.py::decode (dxt5.py::decode_image). The oracle's layer count and block codec are \
         the framing the corpus REFUTED: expect one-layer and two-layer to disagree on every \
         record it can divide, and expect DXT5 base-mip digests to disagree",
        package.name
    ));

    let opened = Package::open(package.name.clone(), &package.path).map_err(|error| {
        OracleError::Protocol(format!("spore-assets could not open the package: {error}"))
    })?;
    let image = opened.bytes();
    let targets: Vec<(usize, spore_dbpf::DbpfEntry)> = opened
        .index()
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.type_id == type_id::RASTER)
        .map(|(index, entry)| (index, *entry))
        .collect();

    if targets.is_empty() {
        builder.skip(format!(
            "{} holds no 0x{:08x} (raster) records, so there is nothing to compare",
            package.name,
            type_id::RASTER
        ));
        return Ok(builder.finish());
    }
    // The sample is over the FILTERED raster list; the oracle addresses records
    // by their PACKAGE row. See the note in `gmdl::compare_package` -- mixing
    // the two numbering systems compares two different records.
    let sampled = sample_indices(targets.len(), budget);
    let decoded: Vec<usize> = sampled.iter().copied().take(decode_budget).collect();
    builder.note(format!(
        "{} raster record(s) in {}; envelope compared on {}; base mip decoded and hashed on {}",
        targets.len(),
        package.name,
        sampled.len(),
        decoded.len()
    ));
    builder.note(describe_sample(targets.len(), budget, sampled.len()));

    let mut session = OracleSession::spawn(config)?;
    let info = session.open(&package.path)?;
    let mut layers_agreed = 0usize;
    let mut layers_disagreed = 0usize;
    let mut layers_refused = 0usize;
    let mut layers_unstated = 0usize;
    let mut base_mip_agreed = 0usize;
    let mut base_mip_disagreed = 0usize;
    let mut base_mip_one_sided = 0usize;
    let mut framing_refuted = 0usize;

    for sample in &sampled {
        let Some((package_index, entry)) = targets.get(*sample) else {
            continue;
        };
        let identity = info.identity(
            *package_index,
            entry.type_id,
            entry.group_id,
            entry.instance_id,
        );
        let bytes = match spore_dbpf::extract_record(image, entry) {
            Ok(bytes) => bytes,
            Err(error) => {
                builder.record(Agreement::RustOnly(format!(
                    "spore_dbpf::extract_record refused the record before the envelope was read: \
                     {error}"
                )));
                continue;
            }
        };

        let rust_envelope = spore_texture::parse_envelope(&bytes);
        // The PACKAGE row, not the position in the raster-filtered list.
        let oracle = session.request_record("raster", *package_index);

        let oracle_envelope = match oracle.as_ref().ok().and_then(|reply| reply.get("env")) {
            Some(env) => env.clone(),
            None => {
                let agreement = classify_refusal(
                    oracle.as_ref().map(|_| ()).map_err(|error| error.message()),
                    rust_envelope
                        .as_ref()
                        .map(|_| ())
                        .map_err(|error| error.to_string()),
                );
                builder.record(agreement);
                continue;
            }
        };

        let Ok(envelope) = rust_envelope else {
            let rust_error = rust_envelope.expect_err("the Ok arm was just handled");
            let why = oracle.as_ref().err().map_or_else(
                || "the oracle sent no envelope".to_owned(),
                OracleError::message,
            );
            builder.divergence(
                Divergence::value(
                    identity,
                    "envelope",
                    format!("<refused: {why}>"),
                    rust_error.to_string(),
                )
                .with_class(DivergenceClass::OneSidedDecode),
            );
            continue;
        };

        compare_envelope_fields(&mut builder, &identity, &oracle_envelope, &envelope);
        match compare_layer_count(
            &mut builder,
            &identity,
            oracle.as_ref().ok(),
            &bytes,
            &envelope,
        ) {
            LayerVerdict::Agreed => layers_agreed += 1,
            LayerVerdict::Disagreed => layers_disagreed += 1,
            LayerVerdict::OracleRefusedItsOwnDivision => layers_refused += 1,
            LayerVerdict::Unstated => layers_unstated += 1,
        }

        if !decoded.contains(sample) {
            continue;
        }
        let fourcc = envelope.fourcc;
        let rust_decode = spore_texture::decode_raster(&bytes);
        let oracle_sha: Result<String, String> = oracle
            .as_ref()
            .ok()
            .and_then(|reply| reply.get("mip0_sha16"))
            .and_then(Json::as_str)
            .map_or_else(
                || Err(oracle_reason_text(oracle.as_ref().ok())),
                |sha| Ok(sha.to_owned()),
            );

        if let Some(rust_error) = rust_decode.as_ref().err() {
            let agreement = classify_refusal(
                oracle_sha.clone().map(|_| ()),
                rust_decode
                    .as_ref()
                    .map(|_| ())
                    .map_err(|error| error.to_string()),
            );
            builder.record(agreement.clone());
            if let Agreement::BothFailed { oracle, rust } = agreement {
                builder.note(format!(
                    "{identity}: both sides refused to decode this raster; oracle={oracle:?} \
                     rust={rust:?}"
                ));
            }
            if !has_measured_framing(fourcc) {
                builder.note(format!(
                    "{identity}: fourcc 0x{fourcc:08x} has no measured framing, so the Rust side \
                     refused it by name while the oracle did not validate the format word at all; \
                     Rust said: {rust_error}"
                ));
            }
            continue;
        }
        let decoded_image = rust_decode.expect("checked above");
        let first = decoded_image.layers.first().and_then(|layer| layer.first());
        let rust_sha = first.map(|mip| spore_sha_hex16(&mip.pixels));
        match (rust_sha.clone(), oracle_sha.clone()) {
            (Some(rust_hex), Ok(oracle_hex)) => {
                if rust_hex == oracle_hex {
                    base_mip_agreed += 1;
                    builder.record(Agreement::Equal);
                } else {
                    base_mip_disagreed += 1;
                    framing_refuted += 1;
                    builder.divergence(
                        Divergence::value(identity, "mip0.sha256[:16]", oracle_hex, rust_hex)
                            .with_note(format!(
                                "{}x{} base mip, RGBA8, hashed after decode. The two sides read \
                                 the same envelope and disagree on the PIXELS, and the byte count \
                                 says which one is wrong: this record is {bytes_len} bytes, which \
                                 is exactly header {header} + a {base}-byte base level (16 bytes \
                                 per 4x4 block) + one u32 count per level past level 1, with \
                                 nothing left over, so the oracle's 8-byte-block chain and its \
                                 derived layer headers address bytes that are not there. The \
                                 codec inside the block is still spore-texture's INFERRED claim: \
                                 no decode of a real record has been compared against the game's \
                                 own output. Oracle decode path: raster.py::decode -> \
                                 dxt5.py::decode_image",
                                first.map_or(0, |mip| mip.width),
                                first.map_or(0, |mip| mip.height),
                                bytes_len = bytes.len(),
                                header = spore_texture::framing::frame_for(fourcc)
                                    .map_or(0, |frame| frame.header_size),
                                base = spore_texture::framing::level_size(
                                    spore_texture::framing::frame_for(fourcc)
                                        .unwrap_or(spore_texture::framing::DXT5_FRAME),
                                    envelope.width,
                                    envelope.height,
                                    0,
                                ),
                            )),
                    );
                }
            }
            _ => {
                base_mip_one_sided += 1;
                // Quoted here rather than above, because it is only meaningful
                // when the oracle produced no digest to disagree with.
                let why = oracle_reason_text(oracle.as_ref().ok());
                builder.divergence(
                    Divergence::value(
                        identity,
                        "mip0.sha256[:16]",
                        oracle_sha
                            .clone()
                            .err()
                            .unwrap_or_else(|| "<no digest>".to_owned()),
                        rust_sha.unwrap_or_else(|| "<no digest>".to_owned()),
                    )
                    .with_class(DivergenceClass::OneSidedDecode)
                    .with_note(format!(
                        "fourcc 0x{fourcc:08x}: only one side produced a digest for the base mip. \
                         raster.py::decode refuses every fourcc but DXT5 and derives its layer \
                         count by a division the corpus refuted, so the oracle has no answer here \
                         while the Rust side does. The Rust framing is not taken on trust: it is \
                         checked against a second transcription in Python over every raster \
                         record of both content packages (tests/luminance_corpus.rs, 4386 of \
                         4386). {why}",
                    )),
                );
            }
        }
    }

    builder.note(format!(
        "layer count over {sampled_len} sampled record(s): {layers_agreed} agreed, \
         {layers_disagreed} disagreed, {layers_refused} refused by the oracle's own division, \
         {layers_unstated} stated nothing. Every disagreement is raster.py deriving \
         8-byte-block layers against the measured one-chain framing \
         (docs/RASTER-FRAMING.md section 2), and a refusal is the same rule refusing a payload \
         it cannot divide -- not an absence of measurement",
        sampled_len = sampled.len()
    ));
    builder.note(format!(
        "base mip digest: {base_mip_agreed} agreed, {base_mip_disagreed} disagreed, \
         {base_mip_one_sided} one-sided; {framing_refuted} of those are a DXT5 decode on both \
         sides reading different bytes out of the same record"
    ));
    Ok(builder.finish())
}

/// Compares the derived layer count, and reports the oracle's own arithmetic when
/// it disagrees.
///
/// The framing is an identity, not a division: `record_size == header + base +
/// SUM(4 + level)`, over 4386 of 4386 raster records in the two content
/// packages. `raster.py::layer_count` divides `(record_size - 32)` by
/// `16 + chain_at_8_bytes_per_block` and reported 2 for every 512² DXT5 record
/// it could divide at all. Both numbers are quoted on the divergence so a reader
/// can check the claim rather than take it: the payload the oracle divided, and
/// the stride it divided by, both come from `raster.py`'s own constants and are
/// reported by the driver.
fn compare_layer_count(
    builder: &mut ReportBuilder,
    identity: &crate::report::RecordIdentity,
    reply: Option<&Json>,
    bytes: &[u8],
    envelope: &spore_texture::RasterEnvelope,
) -> LayerVerdict {
    let Some(reply) = reply else {
        builder.note(format!(
            "{identity}: no oracle reply, so its layer count was not compared"
        ));
        return LayerVerdict::Unstated;
    };
    if let Some(reason) = reply.get("layers_error").and_then(Json::as_str) {
        builder.note(format!(
            "{identity}: raster.py::layer_count refused its own division here ({reason}); a \
             refusal is not a layer count, so there is nothing to compare and no disagreement is \
             recorded for this record. The Rust side read this record's envelope and framing"
        ));
        return LayerVerdict::OracleRefusedItsOwnDivision;
    }
    let Some(oracle_layers) = reply.get("layers").and_then(Json::as_u32) else {
        builder.note(format!(
            "{identity}: the oracle stated no layer count and no reason for not stating one"
        ));
        return LayerVerdict::Unstated;
    };
    if oracle_layers == 1 {
        return LayerVerdict::Agreed;
    }
    let payload = reply.get("payload").and_then(Json::as_u32).unwrap_or(0);
    let stride = reply.get("stride").and_then(Json::as_u32).unwrap_or(0);
    let frame = spore_texture::framing::frame_for(envelope.fourcc);
    let expected = frame.map_or(0, |frame| {
        spore_texture::framing::record_size(
            frame,
            envelope.width,
            envelope.height,
            envelope.mip_count,
        )
    });
    let base = frame.map_or(0, |frame| {
        spore_texture::framing::level_size(frame, envelope.width, envelope.height, 0)
    });
    builder.divergence(
        Divergence::value(
            identity.clone(),
            "layer count",
            format!("{oracle_layers} layers ({payload} payload bytes / {stride}-byte stride)"),
            format!("1 chain (record is exactly {expected} bytes: header + a {base}-byte base level + one u32 count per level)"),
        )
        .with_class(DivergenceClass::Structural)
        .with_note(format!(
            "REFUTED ORACLE, not a port defect. The record is {} bytes and its own envelope \
             implies exactly {expected}, so there is nothing left over for a second layer; the \
             oracle's stride comes from charging 8 bytes per 4x4 block where the record holds 16 \
             (measured: field_18 states {base} = the base level at 16 bytes per block), and its \
             16-byte 'layer header' is the first two blocks of the base level. Verified over 4386 \
             of 4386 raster records in Spore_Content.package and Spore_EP1_Content_01.package; \
             see docs/RASTER-FRAMING.md section 2",
            bytes.len()
        )),
    );
    LayerVerdict::Disagreed
}

/// What one record's layer-count comparison found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayerVerdict {
    /// Both sides derived one chain.
    Agreed,
    /// The oracle derived more than one, and a structural divergence was recorded.
    Disagreed,
    /// `raster.py::layer_count` refused its own division: it read a count of
    /// nothing, which is a fact about the rule and not about the record.
    OracleRefusedItsOwnDivision,
    /// There was no count to compare and no reason for its absence.
    Unstated,
}

/// The eight envelope words, field by field.
fn compare_envelope_fields(
    builder: &mut ReportBuilder,
    identity: &crate::report::RecordIdentity,
    oracle: &Json,
    envelope: &spore_texture::RasterEnvelope,
) {
    for (field, rust_value) in [
        ("version", envelope.version),
        ("width", envelope.width),
        ("height", envelope.height),
        ("mipCount", envelope.mip_count),
        ("field_10", envelope.field_10),
        ("fourcc", envelope.fourcc),
        ("field_18", envelope.field_18),
        ("field_1c", envelope.field_1c),
    ] {
        // The oracle spells mipCount where Rust spells mip_count; both are
        // reported under one name so the field path means one thing.
        match oracle.get(field).and_then(Json::as_u32) {
            None => builder.note(format!(
                "{identity}: the oracle envelope carried no `{field}` field, so it was not compared"
            )),
            Some(value) if value == rust_value => {}
            Some(value) => builder.divergence(Divergence::value(
                identity.clone(),
                format!("envelope.{field}"),
                format!("0x{value:08x} ({value})"),
                format!("0x{rust_value:08x} ({rust_value})"),
            )),
        }
    }
    builder.record(Agreement::Equal);
}

/// The oracle's own words for why it has no digest, quoted rather than
/// paraphrased: `layers_error` when its division refused, `decode_error` when
/// `raster.py::decode` refused (which it does for every `fourcc` but `DXT5`), and
/// the driver's own error otherwise.
fn oracle_reason_text(reply: Option<&Json>) -> String {
    match reply {
        None => "The driver returned no reply for this record.".to_owned(),
        Some(reply) => {
            let mut parts: Vec<String> = Vec::new();
            if let Some(reason) = reply.get("layers_error").and_then(Json::as_str) {
                parts.push(format!("raster.py::layer_count said: {reason}"));
            }
            if let Some(reason) = reply.get("decode_error").and_then(Json::as_str) {
                parts.push(format!("raster.py::decode said: {reason}"));
            }
            if parts.is_empty() {
                "The oracle's reply carried no digest and no reason for carrying none.".to_owned()
            } else {
                parts.join("; ")
            }
        }
    }
}

/// A minimal SHA-256, so the decoded mip can be compared against the digest
/// `raster.py` prints without adding a dependency.
///
/// Only the 16 hex characters the oracle prints are needed, but the full digest
/// is computed anyway: a truncated hash would be a weaker comparison than the
/// one being claimed.
mod spore_sha {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];

    /// A streaming SHA-256.
    #[derive(Clone)]
    pub struct Sha256 {
        state: [u32; 8],
        buffer: [u8; 64],
        buffered: usize,
        length_bits: u64,
    }

    impl Sha256 {
        #[must_use]
        pub fn new() -> Self {
            Self {
                state: [
                    0x6a09_e667,
                    0xbb67_ae85,
                    0x3c6e_f372,
                    0xa54f_f53a,
                    0x510e_527f,
                    0x9b05_688c,
                    0x1f83_d9ab,
                    0x5be0_cd19,
                ],
                buffer: [0u8; 64],
                buffered: 0,
                length_bits: 0,
            }
        }

        pub fn write(&mut self, byte: u8) {
            self.buffer[self.buffered] = byte;
            self.buffered += 1;
            self.length_bits = self.length_bits.wrapping_add(8);
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }

        pub fn hex16(mut self) -> String {
            let bits = self.length_bits;
            self.write_raw(0x80);
            while self.buffered != 56 {
                self.write_raw(0x00);
            }
            // The length is a big-endian u64 in the last 8 bytes, so the MOST
            // significant byte goes first. Writing it little-endian happens to
            // agree with the reference only when the length is zero, which is
            // exactly the case a test that only hashes the empty string would
            // miss.
            for shift in (0..64).step_by(8).rev() {
                self.write_raw(((bits >> shift) & 0xFF) as u8);
            }
            let mut out = String::with_capacity(16);
            for word in self.state {
                out.push_str(&format!("{word:08x}"));
            }
            // The oracle prints `sha256(rgba).hexdigest()[:16]`.
            out.truncate(16);
            out
        }

        fn write_raw(&mut self, byte: u8) {
            self.buffer[self.buffered] = byte;
            self.buffered += 1;
            if self.buffered == 64 {
                let block = self.buffer;
                self.compress(&block);
                self.buffered = 0;
            }
        }

        fn compress(&mut self, block: &[u8; 64]) {
            let mut w = [0u32; 64];
            for (index, word) in w.iter_mut().take(16).enumerate() {
                let base = index * 4;
                *word = u32::from_be_bytes([
                    block[base],
                    block[base + 1],
                    block[base + 2],
                    block[base + 3],
                ]);
            }
            for index in 16..64 {
                let s0 = w[index - 15].rotate_right(7)
                    ^ w[index - 15].rotate_right(18)
                    ^ (w[index - 15] >> 3);
                let s1 = w[index - 2].rotate_right(17)
                    ^ w[index - 2].rotate_right(19)
                    ^ (w[index - 2] >> 10);
                w[index] = w[index - 16]
                    .wrapping_add(s0)
                    .wrapping_add(w[index - 7])
                    .wrapping_add(s1);
            }
            let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
            for index in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ ((!e) & g);
                let temp1 = h
                    .wrapping_add(s1)
                    .wrapping_add(ch)
                    .wrapping_add(K[index])
                    .wrapping_add(w[index]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b) ^ (a & c) ^ (b & c);
                let temp2 = s0.wrapping_add(maj);
                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(temp1);
                d = c;
                c = b;
                b = a;
                a = temp1.wrapping_add(temp2);
            }
            for (slot, value) in self.state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
                *slot = slot.wrapping_add(value);
            }
        }
    }

    impl Default for Sha256 {
        fn default() -> Self {
            Self::new()
        }
    }

    /// The first 16 hex characters of `sha256(bytes)`, which is exactly what
    /// `raster.py` prints.
    #[must_use]
    pub fn hex16_of(bytes: &[u8]) -> String {
        let mut hash = Sha256::new();
        for byte in bytes {
            hash.write(*byte);
        }
        hash.hex16()
    }
}

/// Convenience alias for [`spore_sha::hex16_of`].
fn spore_sha_hex16(bytes: &[u8]) -> String {
    spore_sha::hex16_of(bytes)
}

#[cfg(test)]
mod tests {
    use super::spore_sha::Sha256;
    use super::*;

    #[test]
    fn the_hand_rolled_sha256_matches_the_known_vectors() {
        // If this is wrong every mip comparison is wrong, and it would be wrong
        // quietly: both sides would produce a different-but-stable hex string.
        let hex = |text: &str| {
            let mut hash = Sha256::new();
            for byte in text.as_bytes() {
                hash.write(*byte);
            }
            hash.hex16()
        };
        assert_eq!(hex(""), "e3b0c44298fc1c14");
        assert_eq!(hex("abc"), "ba7816bf8f01cfea");
        assert_eq!(
            hex("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8"
        );
        // Longer than one block, so the buffering and the length encoding are
        // exercised too.
        assert_eq!(hex(&"a".repeat(1000)), "41edece42d63e8d9");
    }

    #[test]
    fn the_measured_fourccs_are_three_words_and_the_old_range_matched_nothing() {
        // The old predicate took `0x1500..=0x15FF` for the uncompressed family,
        // on the strength of a design document and a Python comment. The corpus
        // says the family's format word is the single value `0x00000015`, and no
        // record in either package carries a word in that range -- so the
        // predicate silently classified nothing and the range is deleted rather
        // than kept as a second, wrong answer.
        assert!(has_measured_framing(0x0000_0015));
        assert!(has_measured_framing(0x3554_5844));
        assert!(has_measured_framing(0x3154_5844));
        for never in [
            0u32,
            0x1500,
            0x15FF,
            0x14FF,
            0x1600,
            0x3554_5843,
            0xDEAD_BEEF,
        ] {
            assert!(
                !has_measured_framing(never),
                "0x{never:08x} must not be treated as a measured family"
            );
        }
        // The crate and this module must agree on the words themselves, or the
        // "no measured framing" note above would name a format the crate reads.
        assert_eq!(LUMINANCE_FOURCC, spore_texture::LUMINANCE_FOURCC);
        assert_eq!(DXT5_FOURCC, spore_texture::DXT5_FOURCC);
        assert_eq!(DXT1_FOURCC, spore_texture::DXT1_FOURCC);
        for fourcc in MEASURED_FOURCCS {
            assert!(
                spore_texture::framing::frame_for(fourcc).is_some(),
                "0x{fourcc:08x} has no frame in the crate under test"
            );
        }
    }

    #[test]
    fn the_oracle_reason_is_quoted_not_paraphrased() {
        // A note that says "the oracle refused" without saying why is the shape
        // of a finding that cannot be checked, so the driver's own words are
        // carried through verbatim.
        let both = crate::json::object(vec![
            (
                "layers_error",
                Json::Str(
                    "ValueError: raster: 87436 payload bytes not a multiple of 43720".to_owned(),
                ),
            ),
            (
                "decode_error",
                Json::Str("ValueError: raster: unsupported fourcc 0x00000015".to_owned()),
            ),
        ]);
        let reason = oracle_reason_text(Some(&both));
        assert!(reason.contains("87436 payload bytes"), "{reason}");
        assert!(reason.contains("unsupported fourcc 0x00000015"), "{reason}");
        let neither = crate::json::object(vec![]);
        assert!(
            oracle_reason_text(Some(&neither)).contains("no digest"),
            "{}",
            oracle_reason_text(Some(&neither))
        );
        assert!(
            oracle_reason_text(None).contains("no reply"),
            "a missing reply is an absence, not an empty reason"
        );
    }
}
