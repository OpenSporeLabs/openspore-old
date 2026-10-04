//! `texture`: decode a raster record and write a real PNG file.
//!
//! # Why this command exists
//!
//! Until it did, every raster decode in this workspace was checked by printing
//! statistics: a width, a height, a mean. That cannot tell a correct decode from
//! a plausible wrong one, and the corpus contains a decode where the difference
//! is invisible in numbers and glaring in an image — see the section below.
//!
//! # The trap this command is built around: `0x15` is stored Z-first
//!
//! `spore_texture::decode_raster` returns RGBA8 for both raster families, and
//! for the `0x15` family those four bytes are **not** in red-green-blue order.
//! The stored lane order is `Z, X, Y, fourth` — `spore_texture::claims` grades
//! that layout `VERIFIED` (lane 0 tracks the derived `Z = sqrt(1-nx²-ny²)` with
//! correlation 0.954, and the flat records hold the literal `(255, 128, 128)`).
//!
//! So writing the decoded buffer straight out as RGBA produces an image that
//! *looks* like a texture, has the right dimensions, and has its channels
//! transposed: the flat-normal records come out red where they should be blue.
//! Nothing in the file is malformed. That is exactly the failure a numeric check
//! cannot see.
//!
//! What this command does, and what it refuses to claim:
//!
//! * **Default (`--channels auto`)**: the `0x15` family is swizzled from
//!   `Z, X, Y, mask` to `R=X, G=Y, B=Z, A=mask`, and the report says so in those
//!   words on stderr and in `swizzled_from` in the JSON document.
//! * **The swizzle is a presentation choice, not a claim about the game.**
//!   `spore_texture::claims::LUMINANCE_CHANNEL_SWIZZLE` is deliberately
//!   `Unavailable`: what is *observed* is that `decode_luminance_mip` emits the
//!   four stored bytes with no swizzle; whether any consumer *should* read them
//!   as `X, Y, Z` is not established. So the report carries that question as an
//!   [`Fact`] whose state is `unavailable`, never as a `false` and never as a
//!   value. `--channels stored` shows the file order unchanged.
//! * **DXT5 is never swizzled.** Its documented decode produces the black/red
//!   alpha-mask signature (red in `{0, 8}`, green and blue `0`) on every texel,
//!   because Spore's blocks depart from BC3 in four places. An image of that is
//!   correct and looks broken; the report says so in the same words every time,
//!   because a reader who has not been told will assume the tool is wrong.
//!
//! # It writes only where it was told to
//!
//! `--out` is required, and [`validate_out_file`] refuses any path that resolves
//! inside this repository. There is no default destination, so there is nothing
//! for a command to create by accident, and the guard means even a plausible
//! invocation cannot drop megabytes of expanded texture next to the sources.

use std::io::Write;

use spore_core::{EvidenceLevel, Fact, Provenance};
use spore_texture::{MipImage, RasterImage};

use crate::json::{hex_id, Json};
use crate::{png, TextureRequest, ToolError};

use super::describe_type;

/// The `0x15` family's fourcc, named in reports rather than printed bare.
const LUMINANCE_FOURCC: u32 = spore_texture::LUMINANCE_FOURCC;

/// The DXT5 family's fourcc.
const DXT5_FOURCC: u32 = spore_texture::DXT5_FOURCC;

/// What the tool did to the decoded channel order, in words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppliedOrder {
    /// The decoded bytes were written unchanged.
    Stored,
    /// The decoded bytes were re-indexed from `Z, X, Y, mask` to
    /// `X, Y, Z, mask`.
    SwizzledFromZXY,
}

impl AppliedOrder {
    /// One phrase for a human line.
    pub const fn describe(self) -> &'static str {
        match self {
            Self::Stored => "stored byte order, unchanged (as decoded)",
            Self::SwizzledFromZXY => {
                "swizzled from the stored Z, X, Y, mask into R=X, G=Y, B=Z, A=mask"
            }
        }
    }

    /// The JSON spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::SwizzledFromZXY => "swizzled_from_zxy",
        }
    }
}

/// The one mip level that was written, and everything the report needs about it.
#[derive(Debug)]
struct Selection<'a> {
    layer: usize,
    mip: usize,
    image: &'a MipImage,
}

/// `texture`: resolve, read, decode, write one PNG.
pub fn run(
    request: &TextureRequest,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), ToolError> {
    let store = super::single_package_store(&request.package)?;
    let found = store.find(&request.key)?;
    let entry = found.entry;

    // The honesty contract, stated before any decode is attempted: a type with
    // no decoder in this build reports its index extent and says so, and never
    // reaches the image writer.
    if entry.type_id != spore_texture::RASTER_TYPE {
        return Err(refuse_non_raster(
            &request.key,
            entry.type_id,
            entry.memory_size,
        ));
    }

    let bytes = store.read_from(found.package_index, &request.key)?;
    let image = spore_texture::decode_raster(&bytes).map_err(|error| {
        ToolError::Decode(format!("record `{}`: texture: {error}", request.key))
    })?;

    let selection = select(&image, request.layer, request.mip)
        .map_err(|message| ToolError::Decode(format!("record `{}`: {message}", request.key)))?;
    let applied = applied_order(image.envelope.fourcc, request.channels);
    let rgba = png::Rgba8::new(
        selection.image.width,
        selection.image.height,
        to_rgba(selection.image, applied)?,
    )
    .map_err(|error| ToolError::Decode(format!("record `{}`: {error}", request.key)))?;

    let written = png::write_to(&rgba, &request.out_file, err)?;

    if request.json {
        writeln!(
            out,
            "{}",
            report_json(
                request,
                found.package_name,
                entry.memory_size,
                &image,
                &selection,
                applied,
                written
            )
        )?;
    } else {
        write_report(
            err,
            request,
            found.package_name,
            entry.memory_size,
            &image,
            &selection,
            applied,
            written,
        )?;
    }
    Ok(())
}

/// The refusal for a record this command will not decode, in one sentence.
fn refuse_non_raster(key: &spore_core::ResourceKey, type_id: u32, memory_size: u32) -> ToolError {
    let message = match type_id {
        spore_core::record::type_id::PNG | spore_core::record::type_id::JPEG => format!(
            "record `{key}` is a {} record, not a raster record. Its {} index bytes are a raw \
             image in that format already \u{2014} there is nothing to decode, so use \
             `osptool extract` to copy it. `osptool texture` decodes the raster container \
             (type 0x{:08x}) only.",
            spore_core::RecordType::new(type_id)
                .name()
                .unwrap_or("image"),
            memory_size,
            spore_texture::RASTER_TYPE
        ),
        _ => format!(
            "record `{key}` is {} and this build has no raster decoder for that type id, so \
             nothing was parsed. The {memory_size} bytes below are the index's memory_size. Use \
             `osptool describe` for the same refusal in context.",
            describe_type(type_id)
        ),
    };
    ToolError::Decode(message)
}

/// Picks the layer and mip the caller asked for, refusing an index that does not
/// exist by naming the range that does.
fn select<'a>(image: &'a RasterImage, layer: usize, mip: usize) -> Result<Selection<'a>, String> {
    let chain = image.layers.get(layer).ok_or_else(|| {
        format!(
            "layer {layer} was requested; this record has {} layer(s)",
            image.layers.len()
        )
    })?;
    let level = chain.get(mip).ok_or_else(|| {
        format!(
            "mip {mip} was requested; layer {layer} has {} mip level(s)",
            chain.len()
        )
    })?;
    Ok(Selection {
        layer,
        mip,
        image: level,
    })
}

/// What `--channels auto` decided for one fourcc.
///
/// The dispatch is on the format word alone, and it is the *whole* of the
/// decision: DXT5's decode is already in the order its bytes are in.
pub fn applied_order(fourcc: u32, requested: ChannelOrder) -> AppliedOrder {
    match requested {
        ChannelOrder::Stored => AppliedOrder::Stored,
        ChannelOrder::Auto => match fourcc {
            LUMINANCE_FOURCC => AppliedOrder::SwizzledFromZXY,
            _ => AppliedOrder::Stored,
        },
    }
}

/// The decoded mip's bytes, in the applied channel order.
///
/// A swizzle is a re-index of four-byte texels, so it is total: every output
/// texel is exactly four input bytes and no byte is read twice or dropped. The
/// result length is therefore unchanged, which is the invariant a test can
/// check without looking at a pixel.
fn to_rgba(mip: &MipImage, applied: AppliedOrder) -> Result<Vec<u8>, ToolError> {
    match applied {
        AppliedOrder::Stored => Ok(mip.pixels.clone()),
        AppliedOrder::SwizzledFromZXY => {
            debug_assert_eq!(mip.pixels.len() % 4, 0, "RGBA8 is four bytes per texel");
            let mut out = Vec::with_capacity(mip.pixels.len());
            for texel in mip.pixels.chunks_exact(4) {
                out.push(texel[1]); // stored lane 1 is X -> R
                out.push(texel[2]); // stored lane 2 is Y -> G
                out.push(texel[0]); // stored lane 0 is Z -> B
                out.push(texel[3]); // the fourth lane -> A
            }
            Ok(out)
        }
    }
}

/// The evidence this tool attaches to its own channel-order choice.
///
/// Two separate facts, kept separate because they have different grades:
///
/// * what the stored lanes *are* — `VERIFIED`, from `spore_texture`'s claims;
/// * whether a consumer *should* read them as `X, Y, Z` — `Unavailable`, because
///   `spore_texture::claims::LUMINANCE_CHANNEL_SWIZZLE` says so explicitly.
fn swizzle_facts(fourcc: u32) -> (Fact<&'static str>, Fact<&'static str>) {
    let layout_grade: EvidenceLevel =
        spore_texture::claims::grade_of(spore_texture::claims::LUMINANCE_TEXEL_LAYOUT);
    let layout = Fact::new(
        layout_grade,
        spore_core::EvidenceState::Persisted,
        vec![Provenance::committed(
            "crates/spore-texture/src/claims.rs (luminance_0x15_texel_layout)",
        )],
        "the three first lanes are one tangent-space normal map, stored Z first, 128 as zero",
    );
    // The question this tool must not answer is recorded as a question. An
    // `Unavailable` here is not a `false`: nobody has established that the game
    // reads these bytes as X, Y, Z, and saying "no" would invent the same error
    // with a sign on it.
    let should = if fourcc == LUMINANCE_FOURCC {
        Fact::unavailable(spore_texture::claims::LUMINANCE_CHANNEL_SWIZZLE)
    } else {
        Fact::unavailable("not_applicable_to_this_fourcc")
    };
    (layout, should)
}

/// What a fourcc is, in words this repository has evidence for.
fn fourcc_text(fourcc: u32) -> String {
    match fourcc {
        DXT5_FOURCC => "DXT5 \u{2014} block family, decoded in stored order".to_owned(),
        LUMINANCE_FOURCC => {
            "0x15 family \u{2014} uncompressed tangent-space normal map plus a fourth lane, \
             stored Z, X, Y, mask"
                .to_owned()
        }
        other => format!("0x{other:08x}"),
    }
}

/// The human report. Goes to `err` so that **stdout stays empty** unless
/// `--json` asked for a document — the write confirmation and the decode facts
/// are diagnostics, and a caller piping stdout gets nothing but what it asked for.
#[allow(clippy::too_many_arguments)]
fn write_report(
    err: &mut dyn Write,
    request: &TextureRequest,
    package_name: &str,
    memory_size: u32,
    image: &RasterImage,
    selection: &Selection<'_>,
    applied: AppliedOrder,
    written: usize,
) -> Result<(), ToolError> {
    let envelope = &image.envelope;
    writeln!(err, "record        {}", request.key)?;
    writeln!(err, "package       {package_name}")?;
    writeln!(
        err,
        "type          {}",
        describe_type(spore_texture::RASTER_TYPE)
    )?;
    writeln!(err, "index extent            {memory_size} bytes")?;
    writeln!(
        err,
        "decoder       raster (decoded \u{2014} every number below came out of spore_texture)"
    )?;
    writeln!(err, "  fourcc      0x{:08x}", envelope.fourcc)?;
    writeln!(err, "              {}", fourcc_text(envelope.fourcc))?;
    writeln!(
        err,
        "  base level  {}x{}, {} mip level(s), {} layer(s) (layer count derived from the record \
         size; the record stores none)",
        envelope.width,
        envelope.height,
        envelope.mip_count,
        image.layers.len()
    )?;

    let (layout, should) = swizzle_facts(envelope.fourcc);
    writeln!(err, "channel order {}", applied.describe())?;
    if let Some(value) = layout.value() {
        writeln!(err, "  lanes       {value} [{}]", layout.level())?;
    }
    if let Some(reason) = should.reason() {
        writeln!(
            err,
            "  swizzle     this tool re-indexes the lanes so the image reads R=X, G=Y, B=Z. That \
             is a PRESENTATION choice: whether a consumer should read them that way is \
             unestablished ({reason}). Use `--channels stored` for the file order unchanged."
        )?;
    }
    if envelope.fourcc == DXT5_FOURCC {
        writeln!(
            err,
            "  note        Spore's DXT5 blocks depart from BC3 in four documented ways, so every \
             texel decodes to red in {{0, 8}} with green and blue 0. The image looks wrong and is \
             not: see crates/spore-texture/src/dxt5.rs."
        )?;
    }

    writeln!(
        err,
        "wrote         layer {} mip {} \u{2014} {}x{} RGBA8",
        selection.layer, selection.mip, selection.image.width, selection.image.height
    )?;
    writeln!(
        err,
        "file          {} ({written} bytes)",
        request.out_file.display()
    )?;
    writeln!(
        err,
        "file format   PNG, 8-bit RGBA, filter 0 on every scanline, deflate stored blocks. \
         Correct and readable everywhere, and large; see crates/spore-tools/src/png.rs."
    )?;
    Ok(())
}

/// The `--json` document: one object, fixed key order, every id a hex string.
#[allow(clippy::too_many_arguments)]
fn report_json(
    request: &TextureRequest,
    package_name: &str,
    memory_size: u32,
    image: &RasterImage,
    selection: &Selection<'_>,
    applied: AppliedOrder,
    written: usize,
) -> Json {
    let envelope = &image.envelope;
    let (layout, should) = swizzle_facts(envelope.fourcc);
    Json::obj()
        .push("type_id", hex_id(spore_texture::RASTER_TYPE))
        .push("record", request.key.to_tgi())
        .push("package", package_name)
        .push("index_memory_size", Json::from(memory_size))
        .push("fourcc", hex_id(envelope.fourcc))
        .push("fourcc_text", fourcc_text(envelope.fourcc))
        .push("base_width", Json::from(envelope.width))
        .push("base_height", Json::from(envelope.height))
        .push("mip_count", Json::from(envelope.mip_count))
        .push("layer_count", Json::from(image.layers.len()))
        .push("selected_layer", Json::from(selection.layer))
        .push("selected_mip", Json::from(selection.mip))
        .push("width", Json::from(selection.image.width))
        .push("height", Json::from(selection.image.height))
        .push("channels", applied.as_str())
        .push("channel_order_detail", applied.describe())
        .push("stored_lane_layout", fact_json(layout))
        .push("swizzle_should_be_applied", fact_json(should))
        .push("out_file", request.out_file.display().to_string())
        .push("png_bytes", Json::from(written))
}

/// Renders one [`Fact`] as a JSON object with the repository's envelope shape:
/// `state`, and a value **only** when the fact is available.
fn fact_json(fact: Fact<&str>) -> Json {
    match fact.state() {
        spore_core::FactState::Available => {
            let value = fact.value().copied().unwrap_or_default();
            Json::obj()
                .push("state", "available")
                .push("evidence_level", fact.level().as_str())
                .push("evidence_state", fact.evidence_state().as_str())
                .push("value", Json::from(value))
                .push(
                    "provenance",
                    Json::Arr(
                        fact.provenance()
                            .iter()
                            .map(|entry| {
                                Json::obj()
                                    .push("mode", entry.mode.as_str())
                                    .push("source_class", entry.source_class.as_str())
                                    .push("reference", entry.reference.clone())
                            })
                            .collect(),
                    ),
                )
        }
        spore_core::FactState::Unavailable => Json::obj()
            .push("state", "unavailable")
            .push("evidence_level", fact.level().as_str())
            .push("evidence_state", fact.evidence_state().as_str())
            .push("reason", Json::from(fact.reason().unwrap_or("unspecified")))
            .push("provenance", Json::Arr(Vec::new())),
    }
}

/// The `--channels` vocabulary, resolved at parse time so a command never sees
/// a string it has to re-interpret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelOrder {
    /// Decide from the fourcc: `0x15` is swizzled, everything else is stored.
    Auto,
    /// Write the decoded bytes exactly as decoded.
    Stored,
}

impl ChannelOrder {
    /// The canonical spelling, used in the usage text.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Stored => "stored",
        }
    }

    /// Parses a `--channels` value. A closed vocabulary: an unrecognised word is
    /// refused rather than mapped to a default, because the two modes produce
    /// different pixels.
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "auto" => Some(Self::Auto),
            "stored" => Some(Self::Stored),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_texture::claims::{LUMINANCE_CHANNEL_SWIZZLE, LUMINANCE_TEXEL_LAYOUT};

    #[test]
    fn auto_swizzles_only_the_family_that_is_stored_z_first() {
        assert_eq!(
            applied_order(LUMINANCE_FOURCC, ChannelOrder::Auto),
            AppliedOrder::SwizzledFromZXY
        );
        // DXT5 is never swizzled: its decode is already in stored order, and
        // re-indexing it would invent a channel mapping nobody established.
        assert_eq!(
            applied_order(DXT5_FOURCC, ChannelOrder::Auto),
            AppliedOrder::Stored
        );
        // An unknown fourcc cannot reach the writer, but the dispatch must not
        // panic on one if it ever does.
        assert_eq!(
            applied_order(0x3154_5844, ChannelOrder::Auto),
            AppliedOrder::Stored
        );
    }

    #[test]
    fn stored_means_stored_for_both_families() {
        assert_eq!(
            applied_order(LUMINANCE_FOURCC, ChannelOrder::Stored),
            AppliedOrder::Stored
        );
        assert_eq!(
            applied_order(DXT5_FOURCC, ChannelOrder::Stored),
            AppliedOrder::Stored
        );
    }

    #[test]
    fn a_swizzle_reindexes_rather_than_rotates_and_keeps_the_length() {
        // The flat-normal constant the corpus records spell out: stored
        // (255, 128, 128) must come out as (128, 128, 255) with alpha intact.
        let flat = MipImage {
            width: 2,
            height: 1,
            pixels: vec![255, 128, 128, 200, 255, 128, 128, 7],
        };
        let swizzled = to_rgba(&flat, AppliedOrder::SwizzledFromZXY).unwrap();
        assert_eq!(swizzled, vec![128, 128, 255, 200, 128, 128, 255, 7]);
        assert_eq!(
            swizzled.len(),
            flat.pixels.len(),
            "a re-index cannot resize"
        );
        // Stored mode is the identity, byte for byte.
        assert_eq!(to_rgba(&flat, AppliedOrder::Stored).unwrap(), flat.pixels);
    }

    #[test]
    fn the_four_channel_vocabulary_is_closed() {
        assert_eq!(ChannelOrder::parse("auto"), Some(ChannelOrder::Auto));
        assert_eq!(ChannelOrder::parse("stored"), Some(ChannelOrder::Stored));
        for refused in ["", "rgba", "AUTO", "swizzled"] {
            assert_eq!(ChannelOrder::parse(refused), None, "{refused}");
        }
    }

    #[test]
    fn the_swizzle_question_is_carried_as_a_non_finding_not_as_a_false() {
        let (layout, should) = swizzle_facts(LUMINANCE_FOURCC);
        assert!(layout.is_available());
        assert_eq!(
            layout.level(),
            EvidenceLevel::Verified,
            "graded by spore-texture"
        );
        assert!(
            !layout.provenance().is_empty(),
            "an available fact needs a source"
        );

        assert!(should.is_unavailable());
        assert_eq!(should.reason(), Some(LUMINANCE_CHANNEL_SWIZZLE));
        assert_eq!(should.level(), EvidenceLevel::Unknown);
        assert_eq!(
            should.value(),
            None,
            "an unavailable fact has no value to mistake for a false"
        );
        assert!(
            should.provenance().is_empty(),
            "a non-finding carries no provenance to repeat"
        );
    }

    #[test]
    fn a_fourcc_with_no_lane_layout_is_also_a_non_finding_not_a_default() {
        let (_, should) = swizzle_facts(DXT5_FOURCC);
        assert!(should.is_unavailable());
        assert_eq!(should.reason(), Some("not_applicable_to_this_fourcc"));
    }

    #[test]
    fn the_json_envelope_carries_a_value_only_when_the_fact_has_one() {
        let (layout, should) = swizzle_facts(LUMINANCE_FOURCC);
        let available = fact_json(layout);
        let Json::Obj(pairs) = &available else {
            panic!("expected an object")
        };
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "state",
                "evidence_level",
                "evidence_state",
                "value",
                "provenance"
            ]
        );

        let missing = fact_json(should);
        let Json::Obj(pairs) = &missing else {
            panic!("expected an object")
        };
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "state",
                "evidence_level",
                "evidence_state",
                "reason",
                "provenance"
            ],
            "an unavailable fact must not carry a `value` key at all"
        );
        assert!(pairs.iter().all(|(_, value)| *value != Json::Bool(false)));
    }

    #[test]
    fn an_out_of_range_layer_or_mip_names_the_range_that_exists() {
        let image = RasterImage {
            envelope: spore_texture::RasterEnvelope {
                version: 1,
                width: 4,
                height: 4,
                mip_count: 2,
                field_10: 8,
                fourcc: DXT5_FOURCC,
                field_18: 0,
                field_1c: 0,
            },
            layers: vec![vec![
                MipImage {
                    width: 4,
                    height: 4,
                    pixels: vec![0; 64],
                },
                MipImage {
                    width: 2,
                    height: 2,
                    pixels: vec![0; 16],
                },
            ]],
        };
        assert_eq!(select(&image, 0, 0).unwrap().mip, 0);
        assert_eq!(select(&image, 0, 1).unwrap().layer, 0);
        assert!(
            select(&image, 3, 0)
                .unwrap_err()
                .contains("this record has 1 layer(s)"),
            "an out-of-range layer names the count that does exist"
        );
        assert!(
            select(&image, 0, 9)
                .unwrap_err()
                .contains("layer 0 has 2 mip level(s)"),
            "an out-of-range mip names the count that does exist"
        );
    }

    #[test]
    fn the_claim_subjects_this_tool_cites_are_the_ones_that_exist() {
        // A typo in a claim subject would make the report cite nothing.
        assert_eq!(
            spore_texture::claims::grade_of(LUMINANCE_TEXEL_LAYOUT),
            EvidenceLevel::Verified
        );
        assert_eq!(
            spore_texture::claims::grade_of(LUMINANCE_CHANNEL_SWIZZLE),
            EvidenceLevel::Unknown
        );
    }

    #[test]
    fn a_png_typed_record_is_refused_with_a_pointer_to_extract() {
        let key = spore_core::ResourceKey::new(
            spore_core::record::type_id::PNG,
            0x4066_2900,
            0x067a_0901,
        );
        let error = refuse_non_raster(&key, key.type_id, 1234);
        assert_eq!(error.code(), crate::EXIT_DECODE);
        let message = error.to_string();
        assert!(message.contains("png"), "{message}");
        assert!(message.contains("osptool extract"), "{message}");
        assert!(message.contains("1234"), "the index extent is still stated");
    }

    #[test]
    fn an_unknown_type_is_refused_without_pretending_it_was_parsed() {
        let key = spore_core::ResourceKey::new(0xdead_beef, 1, 2);
        let message = refuse_non_raster(&key, key.type_id, 64).to_string();
        assert!(message.contains("0xdeadbeef"), "{message}");
        assert!(message.contains("nothing was parsed"), "{message}");
        assert!(message.contains("64 bytes"), "{message}");
    }
}
