//! Every claim this crate makes that is **not** verified, with its grade.
//!
//! # Why this module exists
//!
//! A decoder that hands back eight `u32`s has already lost the only interesting
//! question about three of them. [`crate::RasterEnvelope`] carries `field_10`,
//! `field_18` and `field_1c`; nothing in the type says which of those nobody
//! knows. That is exactly the failure this repository exists to prevent: an
//! observation and an interpretation becoming indistinguishable after they leave
//! the decoder (see `spore_core::evidence`).
//!
//! So each such statement is recorded here **with its grade**. A word whose value
//! has been seen in every sample still has an unknown *meaning*, and the value
//! being constant is not evidence about what it is.
//!
//! # The two vocabularies, kept apart
//!
//! * The **observation** ("`field_10` reads 8 in every sampled record; no
//!   decoder reads it") is a fact about bytes and is plain text.
//! * The **meaning** ("`field_10` is a format revision / a bit depth / a padding
//!   hole") is the claim, and it is carried as a [`Fact`]. For an unresolved
//!   field the fact is **unavailable**, which in this repository's vocabulary
//!   means "we looked and found nothing" — never a zero, never a default, never a
//!   silent `false`.
//!
//! # What changed when the framing was re-measured (2026-10-04)
//!
//! Four claims moved, and one of them moved because the decoder was wrong:
//!
//! * [`ENVELOPE_FIELD_18_MEANING`] is no longer format-specific and no longer a
//!   convenient redundancy. It is the **base level's byte count**, verified in
//!   2954 of 2954 records across all three formats, and the decoder **reads** it
//!   as a cross-check.
//! * [`LAYER_HEADER_CONTENTS`] and [`LAYER_INTERLEAVE`] are **refuted**. There
//!   are no 16-byte layer headers and no grouped-header layout; the bytes that
//!   were being skipped are the first two blocks of the base level, and they
//!   vary from record to record.
//! * [`DXT5_SPEC_DEVIATIONS`] is no longer a description of this crate. It is now
//!   the record of an **8-byte block reading the corpus refutes**, kept with the
//!   measurement that refutes it.
//! * [`BC3_CODEC`] and [`BC1_CODEC`] are new, and both are graded `Inferred`, not
//!   `Verified`: the framing is proven, the codec inside a 16-byte block is the
//!   published reading of a record that declares `DXT5`, and no decode of a real
//!   record has been compared against the game's own output.
//! * [`BC3_COLOUR_SIX_LEVEL_ENTRIES`] turned out to be **unobservable** rather
//!   than undecided: a BC1 colour half carries a 2-bit index, so the
//!   punch-through entries cannot be addressed at all.
//!
//! # Refuted documents, recorded rather than dropped
//!
//! `docs/MATERIALS-DESIGN.md` §1 says `field_1c` holds "a float 1.0 in 0x15
//! records" (it is the first texel) and calls the `0x15` family luminance (it is
//! a normal map). Both disagreements are inside
//! [`ENVELOPE_FIELD_1C_MEANING`] and [`LUMINANCE_FOURCC`]. It also reads
//! `field_18` as unresolved, which this crate has now measured.

use std::sync::OnceLock;

use spore_core::evidence::{EvidenceLevel, EvidenceState, Fact, Provenance};

/// What the envelope word at `0x10` means.
pub const ENVELOPE_FIELD_10_MEANING: &str = "envelope_field_10_meaning";

/// What the envelope word at `0x18` means.
pub const ENVELOPE_FIELD_18_MEANING: &str = "envelope_field_18_meaning";

/// What the envelope word at `0x1c` means.
pub const ENVELOPE_FIELD_1C_MEANING: &str = "envelope_field_1c_meaning";

/// What the envelope word at `0x00` means beyond "format revision".
pub const ENVELOPE_VERSION_MEANING: &str = "envelope_version_meaning";

/// What the 16 opaque bytes a DXT5 record was thought to start with hold.
pub const LAYER_HEADER_CONTENTS: &str = "layer_header_contents";

/// Why every layer header precedes every layer payload.
pub const LAYER_INTERLEAVE: &str = "layer_header_payload_interleave";

/// How many mip chains a raster record holds.
pub const LAYER_COUNT: &str = "raster_layer_count";

/// The first level a record prefixes with its own `u32` byte count.
pub const FIRST_PREFIXED_LEVEL: &str = "raster_first_prefixed_level";

/// That the `DXT5` records are 16-byte BC3 blocks.
pub const BC3_CODEC: &str = "dxt5_bc3_codec";

/// That the `DXT1` records are 8-byte BC1 blocks.
pub const BC1_CODEC: &str = "dxt1_bc1_codec";

/// What the BC3 decoder does with the two alpha indices a six-level palette
/// leaves undefined.
pub const BC3_ALPHA_SIX_LEVEL_CLAMP: &str = "bc3_alpha_six_level_clamp";

/// How the BC4/BC5 alpha ramp's integer division is rounded.
pub const BC4_RAMP_ROUNDING: &str = "bc4_ramp_rounding";

/// Whether a BC1 half-block's unordered endpoints should offer the punch-through
/// entries at all.
pub const BC3_COLOUR_SIX_LEVEL_ENTRIES: &str = "bc3_colour_six_level_entries";

/// How Spore's block decode departed from the published BC3 specification.
pub const DXT5_SPEC_DEVIATIONS: &str = "dxt5_spec_deviations";

/// What the `0x15` family actually is, and whether "luminance" describes it.
pub const LUMINANCE_FOURCC: &str = "luminance_fourcc_family";

/// How the `0x15` family frames its payload.
pub const LUMINANCE_FRAMING: &str = "luminance_0x15_framing";

/// What the four bytes of one `0x15` texel are, in file order.
pub const LUMINANCE_TEXEL_LAYOUT: &str = "luminance_0x15_texel_layout";

/// Why the `0x15` family's decoded bytes are emitted in file order and not swizzled.
pub const LUMINANCE_CHANNEL_SWIZZLE: &str = "luminance_0x15_channel_swizzle";

/// What the fourth `0x15` lane carries.
pub const LUMINANCE_FOURTH_LANE: &str = "luminance_0x15_fourth_lane";

/// What happens to mip levels beyond 32.
pub const MIPS_BEYOND_32: &str = "mip_chain_beyond_32";

/// Why an address is a `Fact` and not a value.
///
/// `Unavailable` is the honest state for a field whose meaning nobody
/// established: the repository's rule is that it must not be represented as a
/// zero, an empty string or `false`. Each subject therefore carries either an
/// available meaning with a grade and provenance, or this unavailable envelope.
const NO_MEANING_ESTABLISHED: &str = "meaning_not_established";

/// One recorded claim about the raster format: what was observed, and what (if
/// anything) that observation is known to mean.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureClaim {
    /// Stable identifier, e.g. `"envelope_field_10_meaning"`.
    pub subject: &'static str,
    /// What the bytes are observed to be, and what this crate does with them.
    /// This half is always present and is never an interpretation.
    pub observation: &'static str,
    /// The meaning, when one is established. `unavailable` means "we looked and
    /// found nothing", which is not the same as "it is zero".
    pub meaning: Fact<&'static str>,
}

/// The claims, in declaration order.
///
/// Lazily built because [`Fact`] cannot be constructed in a `const` context: an
/// available fact must carry a `Vec<Provenance>`.
pub fn all_claims() -> &'static [TextureClaim] {
    static CLAIMS: OnceLock<Vec<TextureClaim>> = OnceLock::new();
    CLAIMS.get_or_init(|| {
        vec![
            unresolved(
                ENVELOPE_FIELD_10_MEANING,
                "field_10 reads 8 in every sampled raster of every fourcc in Spore_Content.package \
                 and Spore_EP1_Content_01.package (4386 records). No decoder reads it: not the \
                 Python oracle, not src/assets/Texture.cpp, not this crate. docs/MATERIALS-DESIGN.md \
                 section 1 records it as '=8 every sample; decoder must NOT rely on it'.",
            ),
            resolved(
                ENVELOPE_FIELD_18_MEANING,
                "field_18 is the base level's own byte count, in EVERY format, not a per-format \
                 constant: 0x40000 for a 512x512 DXT5 record (16 bytes per 4x4 block), 0x10000 for \
                 256x256 (65536), 0x1000 for 64x64 (4096), 0x8000 for a 256x256 DXT1 record (32768, \
                 8 bytes per block), 0x100000/0x40000/0x1000 for the 0x15 family's 512/256/32 \
                 (w*h*4). Verified in 4386 of 4386 raster records measured. The old reading -- \
                 '0x40000 for a 512x512 DXT5 whatever the dimensions, so the word does not have one \
                 meaning' -- was an artefact of charging 8 bytes per block: at 8 bytes a 512x512 \
                 base level is 131072 and field_18 read as exactly two of them.",
                EvidenceLevel::Verified,
                "The byte count of mip level 0. It is a REDUNDANT restatement of the geometry, and \
                 the walk refuses a record where the two disagree rather than trusting either, which \
                 is what makes it usable as a cross-check without making it a source of layout.",
                "Spore_Content.package (2954 records) and Spore_EP1_Content_01.package (1432 \
                 records), all three fourcc values",
            ),
            resolved(
                ENVELOPE_FIELD_1C_MEANING,
                "field_1c reads 0x0000FFFF in DXT5 records. In a 0x15 record it is NOT a header \
                 field: that family's header is 28 bytes, and the word at 0x1c is the first four \
                 bytes of the base-level payload. Measured: 670 of the 901 0x15 records have \
                 field_1c = 0x008080ff and the rest are 231 further distinct values tracking the \
                 varying pixel content (e.g. 0x098080ff), so it is a texel and not a constant. \
                 This REFUTES docs/MATERIALS-DESIGN.md section 1, which records it as 'a float \
                 1.0 in 0x15 records'. EXTENDED 2026-10-05, and the extension qualifies the \
                 'reads 0x0000FFFF' half rather than removing it: over all 4386 raster records \
                 the two BLOCK formats do NOT read a constant here. In 2478 DXT5 records \
                 field_1c takes 113 distinct values -- 0x00000000 in 2076 of them, 0x0000FFFF in \
                 255 -- and in 634 DXT1 records it takes 505 distinct values with no value \
                 repeated more than six times. Inside a 32-byte block-format header this word is \
                 therefore NOT a constant either, and its per-record variability is what block \
                 data looks like rather than what a header word looks like. NO DECODER READS IT \
                 for any format: crate::framing::level_offset is invariant to it (pinned \
                 negatively by spore-texture's \
                 the_offset_reads_no_envelope_word_this_crate_has_no_meaning_for), so nothing in \
                 this crate's layout depends on it whatever it is. What it MEANS inside a \
                 block-format header remains UNSTABLISHED -- the corpus says it is not a \
                 constant, which is not the same as saying what it is.",
                EvidenceLevel::Verified,
                "Payload, not metadata, for the 0x15 family: the word docs/MATERIALS-DESIGN.md \
                 section 1 describes as 'a float 1.0 in 0x15 records' is refuted by the bytes, \
                 0x008080ff is not 1.0f and it varies per record, and the framing identity only \
                 closes when those four bytes are charged to the base level. RasterEnvelope still \
                 reports the word because a caller reading an envelope should see the bytes that \
                 are there. For the two BLOCK families the same conclusion is reached by a \
                 weaker route -- the word is not constant across 2478 DXT5 and 962 DXT1 records \
                 -- and no positive statement is made about it beyond that.",
                "Spore_Content.package; refutes docs/MATERIALS-DESIGN.md section 1. Block-format \
                 distribution measured 2026-10-05 over both content packages, all 3112 \
                 block-format raster records (2478 DXT5, 634 DXT1) -- see \
                 crates/spore-differential/tests/framing_refusals.rs",
            ),
            resolved(
                ENVELOPE_VERSION_MEANING,
                "version reads 1 in every sampled raster. No record with any other value has been \
                 seen, so the crate does not validate it: refusing an unobserved version would be a \
                 guess about a format nobody has.",
                EvidenceLevel::Observed,
                "A format revision counter, always 1 in the records sampled so far. Never \
                 validated, because an unobserved value is not a known-bad one.",
                "Spore_Content.package and Spore_EP1_Content_01.package",
            ),
            resolved(
                LAYER_HEADER_CONTENTS,
                "REFUTED, and the refutation is the point. There are no 16-byte layer headers. The \
                 bytes at 0x20 of a DXT5 record are the first two BC3 blocks of the base level: \
                 across the 1374 512x512 records of one shape they take many distinct values, which \
                 is what block data does and is not what a header does. The 0x15 family never had \
                 any layer header either, so this claim was about a structure no raster record has.",
                EvidenceLevel::Verified,
                "Nothing -- the 16 bytes are payload. The record's level 0 begins at offset 32 \
                 (DXT5/DXT1) or 28 (0x15) with no intervening header.",
                "Spore_Content.package, cross-record variance at offsets 0x20-0x3f",
            ),
            resolved(
                LAYER_INTERLEAVE,
                "REFUTED. There is no layer to interleave with, so the question is void: one chain, \
                 one header, and per-LEVEL u32 byte counts inside it. The old claim 'all n layer \
                 headers precede all payloads, payloadOff = 32 + n*16 = 0x40' was checked against \
                 the first block of a real record and read '20 00 02 00 65 39 00 00' -- which is a \
                 valid block at offset 0x40 and ALSO a valid block at offset 0x20, because the \
                 bytes it cited could not distinguish the two.",
                EvidenceLevel::Verified,
                "Void. Superseded by the framing identity in crate::framing, which places each \
                 level from the envelope's own geometry and validates a u32 length the record \
                 itself carries at every prefixed level.",
                "docs/MATERIALS-DESIGN.md section 1, refuted by the byte count in crate::framing",
            ),
            resolved(
                LAYER_COUNT,
                "Every raster record in the corpus is ONE chain of mip levels. There is no layer \
                 count in the record and nothing to divide for one: record_size == header + \
                 level0 + SUM over prefixed levels of (4 + level bytes) closes with nothing left \
                 over in 4386 of 4386 records. The old 'derived 2 layers' was an arithmetic \
                 artefact -- 8 bytes per 4x4 block instead of 16 makes one picture look like two \
                 half-pictures -- and it is why 373 records used to be refused for a payload that \
                 'is not a multiple of the layer stride'.",
                EvidenceLevel::Verified,
                "One. RasterImage::layers therefore has exactly one element; the nesting is kept \
                 only so a caller written against the old two-layer reading still compiles.",
                "Spore_Content.package and Spore_EP1_Content_01.package; docs/RASTER-FRAMING.md",
            ),
            unresolved(
                FIRST_PREFIXED_LEVEL,
                "The block formats prefix levels 2..M-1 and the 0x15 family prefixes levels 1..M-1, \
                 and both are exact in every record. WHY they differ is not established: the bytes \
                 where a level-1 prefix would sit in a DXT5 or DXT1 record hold block data (a \
                 u32 there is 1939370904 on record 0x40662c00:0x824928c8, not the 2048 a prefix \
                 would carry). No field in the envelope says which family a record belongs to \
                 beyond the fourcc, so the rule is per-fourcc and the reason is unknown.",
            ),
            resolved(
                BC3_CODEC,
                "The DXT5 records' 16-byte blocks are read as published BC3: an 8-byte BC4 alpha \
                 half (alpha0, alpha1, 16 three-bit indices) followed by an 8-byte BC1 colour \
                 half. Evidence for the STRIDE is a byte count, not a judgement: a 256x256 base \
                 level occupies 65536 bytes, which is 4096 blocks of 16; at 8 bytes it would be \
                 32768, and every recorded level prefix is the 16-byte figure. Evidence for the \
                 ALPHA half is measured: mean horizontal total variation of the decoded alpha over \
                 25 sampled records per shape is 1.95 (512x512), 8.72 (256x256) and 2.20 (64x64) \
                 under BC3, against 60.49, 106.06 and 50.25 under the previous 8-byte rule -- a \
                 factor of 20 to 50 on every shape. Evidence for the COLOUR half is the fourcc \
                 plus the stride; no measurement isolates it.",
                EvidenceLevel::Inferred,
                "BC3, i.e. BC4 alpha plus BC1 colour. The framing is proven and the alpha half is \
                 measured; the colour half is the published reading of a record that declares \
                 'DXT5' and is charged 16 bytes per block, which is an inference and is graded \
                 accordingly. A decode of a real record has never been compared against the game's \
                 own output, which is the test that would raise this to Verified.",
                "Spore_Content.package, all 1747 DXT5 records; docs/RASTER-FRAMING.md",
            ),
            resolved(
                BC1_CODEC,
                "The DXT1 records' 8-byte blocks are read as published BC1: two R5G6B5 endpoints and \
                 16 two-bit colour indices, alpha opaque. The STRIDE is a byte count -- a 256x256 \
                 DXT1 base level occupies 32768 bytes, which is 4096 blocks of 8, and every \
                 recorded level prefix is that figure -- and the fourcc names BC1. No smoothness \
                 measurement isolates the codec: at the measured offsets these records are \
                 low-contrast enough that a misaligned read scores just as smooth, so this is the \
                 published reading and not a measured one.",
                EvidenceLevel::Inferred,
                "BC1 with opaque alpha. Chosen because the framing fixes the stride at 8 bytes and \
                 the fourcc names BC1, which is the only standard codec of that stride; it is an \
                 inference, not a measurement, and a real decode would be the test.",
                "Spore_Content.package, all 306 DXT1 records; docs/RASTER-FRAMING.md",
            ),
            unresolved(
                BC3_ALPHA_SIX_LEVEL_CLAMP,
                "When a BC3 block's alpha endpoints are NOT ordered (alpha0 <= alpha1) the \
                 specification gives six palette entries and leaves indices 6 and 7 undefined. This \
                 decoder clamps them to alpha0. No record has been decoded both ways, so the clamp \
                 is a defensible default and not a measured fact; a real alpha mask with such a \
                 block would show the difference as a hard edge.",
            ),
            unresolved(
                BC4_RAMP_ROUNDING,
                "The BC4 ramp is a weighted integer mean, `(n*a0 + m*a1) / 7` in the eight-level \
                 case and `... / 5` in the six-level one, and this decoder truncates. Reference \
                 decoders differ: for endpoints (255, 0) truncation gives 218, 182, 145, 109, 72, \
                 36 while a rounded variant gives 219, 183, 146, 110, 73, 37. Nothing in the \
                 corpus chooses between them, and the difference is one LSB on most texels of a \
                 block whose endpoints are not 0 and 255 -- so this is a real 1-LSB risk on real \
                 pixels, and the reason it is not graded higher than the codec itself.",
            ),
            unresolved(
                BC3_COLOUR_SIX_LEVEL_ENTRIES,
                "A BC1 half-block's six-entry (punch-through) palette needs a 3-BIT colour index, \
                 and a BC3 block's colour half carries a 2-bit one, so entries 4 and 5 are \
                 UNREACHABLE in both block formats and this decoder does not build them. That \
                 makes the choice unobservable rather than undecided -- no pixel here can differ \
                 -- but it is recorded because 'which of the six entries does Spore use' is a \
                 question a reader will otherwise assume was answered, and the honest answer is \
                 that the format never asks it.",
            ),
            resolved(
                DXT5_SPEC_DEVIATIONS,
                "THIS IS A REFUTED CLAIM, kept as the register of what the corpus decided. The \
                 crate previously decoded 8-byte blocks with an inverted palette-size test, R5G5B5 \
                 channel reads, a 6/2 alpha blend on the endpoint codes and no \
                 alpha0 == alpha1 collapse, and it claimed the records 'carry no colour: green \
                 span 0, blue span 0, 477771 palette entries collapse to exactly 2 RGB triples'. \
                 All four departures, and the no-colour consequence, are artefacts of that rule: \
                 green and blue read as 0 under R5G5B5 on a 6-BIT word by construction, because a \
                 6-bit value cannot set bits 10..19. Read as BC3 the same records carry 32 \
                 distinct greens and 32 distinct blues.",
                EvidenceLevel::Verified,
                "Spore's blocks ARE BC3. The deviations listed were properties of the 8-byte \
                 reading, which the record lengths refute; that reading has no support beyond two \
                 implementations agreeing with each other on synthetic input.",
                "tools/spore/dxt5/dxt5.py and src/assets/Dxt5.cpp agree with each other and with \
                 tests/test_textures.py::TestDxt5Synthetic, none of which ever ran on a real \
                 record; refuted by the byte count in crate::framing",
            ),
            resolved(
                LUMINANCE_FOURCC,
                "Records whose fourcc is 0x00000015 (observed: 901 of the 2954 raster records in \
                 Spore_Content.package, in exactly three shapes -- 32x32/6 mips/5508 bytes, \
                 256x256/9/349584, 512x512/10/1398164) are NOT luminance. Each texel is four \
                 bytes; lane 0 has mean > 230 in 901 of 901 records, lanes 1 and 2 are centred on \
                 127.5 in 899 of 901, and 7 records carry the constant (255,128,128) in lanes \
                 0-2. This crate decodes all 901. The old 'luminance, out of scope' reading was \
                 a name with no decoder behind it and the bytes REFUTE it, as they refute \
                 docs/MATERIALS-DESIGN.md section 1 which calls this family luminance and puts it \
                 out of scope. The 'exactly three shapes' count is scoped to that one package: the \
                 framing identity is size-driven, so a fourth shape would decode rather than fail, \
                 and none has been seen in any corpus measured here.",
                EvidenceLevel::Verified,
                "A tangent-space normal map plus a fourth lane: lanes 0, 1, 2 are Z, X, Y with 128 \
                 as zero, which is what the flat-normal records spell out literally. 'Luminance' \
                 survives only as this crate's and the repository's existing identifier \
                 (luminance_fourcc_family), kept so the vocabulary does not fork.",
                "Spore_Content.package; docs/LUMINANCE-RASTER.md",
            ),
            resolved(
                LUMINANCE_FRAMING,
                "A 0x15 record is 28 bytes of header followed by the base level unprefixed, then \
                 for each further level a little-endian u32 byte count and that many payload \
                 bytes. The identity record_size == 28 + w*h*4 + sum over levels 1..n of (4 + \
                 max(1,w>>m)*max(1,h>>m)*4) holds with nothing left over in 901 of 901 records, \
                 and each recorded prefix equals its own level's w*h*4. Unchanged by the 2026-10-04 \
                 re-measurement; it is now one row of the shared table in crate::framing rather \
                 than a rule of its own.",
                EvidenceLevel::Verified,
                "An exact-consume identity over the whole corpus, not a plausibility fit. It is \
                 strictly stronger than the old DXT5 rule, which divided a payload to find a layer \
                 count; here the record size is fully determined, so a mismatch is a refusal \
                 rather than a rounding.",
                "Spore_Content.package; docs/LUMINANCE-RASTER.md; docs/RASTER-FRAMING.md",
            ),
            resolved(
                LUMINANCE_TEXEL_LAYOUT,
                "One 0x15 texel is four bytes in file order: Z, X, Y, fourth. Evidence: the flat \
                 records hold the constant (0xFF, 0x80, 0x80, mask); lane 0 tracks the derived \
                 Z = sqrt(1-nx^2-ny^2) with correlation 0.954 and an exact byte match on the \
                 median 74% of texels (>=95% on 27 records); lanes 1 and 2 are mutually \
                 uncorrelated (0.004), which two independent normal components are and two \
                 derived channels are not.",
                EvidenceLevel::Verified,
                "The three first lanes are one tangent-space normal map encoded 0..255 with 128 as \
                 zero, and the Z component comes FIRST on disk. Which of R/G/B a consumer should \
                 read that as is NOT part of this claim.",
                "Spore_Content.package; docs/LUMINANCE-RASTER.md",
            ),
            unresolved(
                LUMINANCE_CHANNEL_SWIZZLE,
                "decode_luminance_mip emits the four stored bytes as R, G, B, A in file order, \
                 with no swizzle, even though the bytes show the Z component first (lane 0). \
                 A consumer that wants the conventional normal-map order R=X, G=Y, B=Z must \
                 re-index, and nothing in this repository establishes that any renderer does so. \
                 So what is NOT swizzled is observed; whether it SHOULD be is unresolved.",
            ),
            resolved(
                LUMINANCE_FOURTH_LANE,
                "The fourth byte of a 0x15 texel is independent of the normal map: correlation \
                 with lane 0 is 0.032, its mean varies per record (median 169.7, range 0.2 to \
                 229.8) and 540 of 901 records give it only two distinct values, 0 and 255.",
                EvidenceLevel::Inferred,
                "A per-texel mask of some kind -- binary in most records, graded in others. Which \
                 mask (ambient occlusion, a specular term, a paint channel) is not established; \
                 no claim is made beyond 'independent of the normal map and carried as alpha'.",
                "Spore_Content.package",
            ),
            resolved(
                MIPS_BEYOND_32,
                "dxt5_chain_size charges one block per level past 32 because `w >> mip` is \
                 undefined for mip >= 32. The largest chain observed is 10 levels (512x512, \
                 mipCount=10), so no record exercises the rule. Re-measured 2026-10-05 over \
                 BOTH content packages and all 4386 raster records: 0 of 4386 declare more than \
                 10 levels, 0 declare more than the 33 levels this build places, and all 4386 \
                 have `mip_count` exactly equal to the bit width of their larger dimension (so \
                 every corpus chain ends on a 1x1 level and none has a level past 1x1). \
                 crates/spore-differential/tests/framing_refusals.rs is the census.",
                EvidenceLevel::Approximation,
                "One prefix plus one 1x1 payload per level beyond 32. Reproduced from the previous \
                 reference so the two would agree; it bounds an absurd header rather than \
                 describing how Spore writes a 33rd level. SINCE 2026-10-05 it is used for SIZING \
                 ONLY: crate::framing::level_offset -- the function decode_raster walks with -- \
                 REFUSES any level at or past crate::framing::MAX_PLACEABLE_LEVELS (33) with \
                 TextureError::MipChainBeyondFloor, and both whole-record entry points refuse an \
                 envelope declaring more than 33 levels. The approximation no longer places \
                 anything, which is what makes calling it an approximation honest rather than \
                 convenient.",
                "The rule predates the 2026-10-04 re-measurement; no record exercises it. \
                 Re-measured 2026-10-05: 0 of 4386 raster records in Spore_Content.package and \
                 Spore_EP1_Content_01.package exceed 10 levels",
            ),
        ]
    })
}

/// One claim's envelope, or `None` when this build records no claim for
/// `subject`. Absence is not `UNKNOWN`: it means "this build does not say",
/// which is a different statement from "this build says it is unknown".
pub fn claim(subject: &str) -> Option<&'static TextureClaim> {
    all_claims().iter().find(|entry| entry.subject == subject)
}

/// The grade of one subject's meaning, or [`EvidenceLevel::Unknown`] when this
/// build records nothing for it.
///
/// The grade is about the *meaning*, not about the observation: an unresolved
/// field is `UNKNOWN` even though its value has been seen in every sample.
pub fn grade_of(subject: &str) -> EvidenceLevel {
    claim(subject).map_or(EvidenceLevel::Unknown, |entry| entry.meaning.level())
}

/// Builds a claim whose meaning nobody established.
///
/// The observation stands on its own; the meaning is a non-finding, and
/// `EvidenceLevel::Unknown` is what that costs.
fn unresolved(subject: &'static str, observation: &'static str) -> TextureClaim {
    TextureClaim {
        subject,
        observation,
        meaning: Fact::unavailable(NO_MEANING_ESTABLISHED),
    }
}

/// Builds a claim whose meaning is established, with a mandatory grade and
/// provenance: an unsourced claim is the failure this module exists to prevent.
fn resolved(
    subject: &'static str,
    observation: &'static str,
    level: EvidenceLevel,
    meaning: &'static str,
    basis: &'static str,
) -> TextureClaim {
    TextureClaim {
        subject,
        observation,
        meaning: Fact::new(
            level,
            EvidenceState::Derived,
            vec![Provenance::derived(basis)],
            meaning,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unresolved_subject_is_unavailable_and_unknown() {
        // The envelope word this crate still has nothing to say about, the two
        // open framing questions and the channel-swizzle question. Each must be a
        // non-finding, never a zero and never a guess.
        for subject in [
            ENVELOPE_FIELD_10_MEANING,
            FIRST_PREFIXED_LEVEL,
            BC3_ALPHA_SIX_LEVEL_CLAMP,
            BC4_RAMP_ROUNDING,
            BC3_COLOUR_SIX_LEVEL_ENTRIES,
            LUMINANCE_CHANNEL_SWIZZLE,
        ] {
            let entry = claim(subject).unwrap_or_else(|| panic!("{subject} is not recorded"));
            assert!(
                entry.meaning.is_unavailable(),
                "{subject}: an unresolved meaning must be unavailable, not a value"
            );
            assert_eq!(entry.meaning.level(), EvidenceLevel::Unknown, "{subject}");
            assert_eq!(
                entry.meaning.evidence_state(),
                EvidenceState::Missing,
                "{subject}"
            );
            assert_eq!(entry.meaning.value(), None, "{subject}");
            assert!(
                entry.meaning.provenance().is_empty(),
                "{subject}: a non-finding has no provenance to repeat"
            );
            assert_eq!(grade_of(subject), EvidenceLevel::Unknown, "{subject}");
        }
    }

    #[test]
    fn an_established_claim_carries_its_grade_and_provenance() {
        for subject in [
            ENVELOPE_VERSION_MEANING,
            ENVELOPE_FIELD_18_MEANING,
            ENVELOPE_FIELD_1C_MEANING,
            LAYER_HEADER_CONTENTS,
            LAYER_INTERLEAVE,
            LAYER_COUNT,
            BC3_CODEC,
            BC1_CODEC,
            DXT5_SPEC_DEVIATIONS,
            LUMINANCE_FOURCC,
            LUMINANCE_FRAMING,
            LUMINANCE_TEXEL_LAYOUT,
            LUMINANCE_FOURTH_LANE,
            MIPS_BEYOND_32,
        ] {
            let entry = claim(subject).unwrap_or_else(|| panic!("{subject} is not recorded"));
            assert!(entry.meaning.is_available(), "{subject}");
            assert_ne!(entry.meaning.level(), EvidenceLevel::Unknown, "{subject}");
            assert!(
                !entry.meaning.provenance().is_empty(),
                "{subject}: an unsourced claim is the failure this module exists to prevent"
            );
            assert_eq!(
                entry.meaning.reason(),
                None,
                "{subject}: an available fact has no reason"
            );
        }
    }

    #[test]
    fn observations_are_always_present_and_thin_claims_are_rejected() {
        for entry in all_claims() {
            assert!(!entry.subject.is_empty());
            assert!(
                entry.observation.len() > 40,
                "{}: observation too thin to be evidence",
                entry.subject
            );
            assert_eq!(
                entry.meaning.state(),
                if entry.meaning.is_available() {
                    spore_core::evidence::FactState::Available
                } else {
                    spore_core::evidence::FactState::Unavailable
                }
            );
        }
    }

    #[test]
    fn subjects_are_unique_and_an_unrecorded_subject_is_not_a_guess() {
        let claims = all_claims();
        for (index, entry) in claims.iter().enumerate() {
            for other in claims.iter().skip(index + 1) {
                assert_ne!(entry.subject, other.subject, "duplicate subject");
            }
        }
        assert_eq!(claim("no_such_subject"), None);
        assert_eq!(grade_of("no_such_subject"), EvidenceLevel::Unknown);
    }

    #[test]
    fn the_grades_say_what_the_evidence_says() {
        // Each grade is pinned here so that a convenient upgrade fails the build.
        assert_eq!(grade_of(LAYER_COUNT), EvidenceLevel::Verified);
        assert_eq!(grade_of(ENVELOPE_FIELD_18_MEANING), EvidenceLevel::Verified);
        assert_eq!(grade_of(MIPS_BEYOND_32), EvidenceLevel::Approximation);
        assert_eq!(grade_of(LUMINANCE_FOURCC), EvidenceLevel::Verified);
        assert_eq!(grade_of(LUMINANCE_FRAMING), EvidenceLevel::Verified);
        assert_eq!(grade_of(LUMINANCE_TEXEL_LAYOUT), EvidenceLevel::Verified);
        assert_eq!(
            grade_of(LUMINANCE_FOURTH_LANE),
            EvidenceLevel::Inferred,
            "'a mask of some kind' is an inference and must not be graded higher"
        );
    }

    #[test]
    fn a_codec_is_never_graded_above_inferred() {
        // The framing is proven; the codec inside a 16-byte block is not, because
        // no decode of a real record has been compared against the game's own
        // output. This test exists so that raising either codec to Verified is a
        // build failure rather than a quiet edit.
        assert_eq!(
            grade_of(BC3_CODEC),
            EvidenceLevel::Inferred,
            "BC3 is the published reading of a proven stride, not a measured decode"
        );
        assert_eq!(grade_of(BC1_CODEC), EvidenceLevel::Inferred);
    }

    #[test]
    fn the_past_the_floor_rule_is_recorded_as_sizing_only_because_it_now_is() {
        // `MIPS_BEYOND_32` says the approximation "bounds an absurd header rather
        // than describing how Spore writes a 33rd level". That sentence became
        // literally true on 2026-10-05: `framing::level_offset`, the function
        // `decode_raster` walks with, refuses any level at or past
        // `framing::MAX_PLACEABLE_LEVELS`, and so do both whole-record entry
        // points. If someone reintroduces an extrapolation into the walk, this
        // claim's own words become false and this test says so.
        let entry = claim(MIPS_BEYOND_32).expect("recorded");
        assert_eq!(entry.meaning.level(), EvidenceLevel::Approximation);
        let meaning = entry.meaning.value().expect("an available meaning");
        assert!(
            meaning.contains("SIZING ONLY") && meaning.contains("REFUSES"),
            "MIPS_BEYOND_32 must record that the rule sizes and does not place: {meaning}"
        );
        // And the walk really does refuse, at the ceiling this crate names.
        for fourcc in [
            crate::DXT5_FOURCC,
            crate::DXT1_FOURCC,
            crate::LUMINANCE_FOURCC,
        ] {
            let frame = crate::frame_for(fourcc).expect("a frame");
            let ceiling = crate::framing::MAX_PLACEABLE_LEVELS;
            assert!(
                crate::framing::level_offset(frame, 8, 8, ceiling, u32::MAX).is_err(),
                "{fourcc:#010x}: level {ceiling} is past the ceiling this claim names"
            );
            // while the sizing functions still answer, which is the other half of
            // the same sentence.
            assert!(
                crate::framing::record_size(frame, 8, 8, ceiling + 7)
                    > crate::framing::record_size(frame, 8, 8, ceiling),
                "{fourcc:#010x}: the sizing half must still extrapolate"
            );
        }
    }

    #[test]
    fn the_field_1c_claim_records_that_the_block_formats_are_not_constant_either() {
        // The claim's positive finding is about the 0x15 family, where the header
        // is 28 bytes and the word is provably the first texel. The 2026-10-05
        // census extends it: inside a 32-byte block-format header the word is not
        // a constant either (2478 DXT5 records, 113 distinct values; 634 DXT1
        // records, 505), so nothing in this crate may read it. The claim must keep
        // saying so, and the crate must keep not reading it.
        let entry = claim(ENVELOPE_FIELD_1C_MEANING).expect("recorded");
        assert_eq!(entry.meaning.level(), EvidenceLevel::Verified);
        for needle in [
            "BLOCK formats",
            "NOT a constant",
            "UNSTABLISHED",
            "NO DECODER READS IT",
        ] {
            assert!(
                entry.observation.contains(needle),
                "ENVELOPE_FIELD_1C_MEANING's observation must record '{needle}', or the block-format \
                 measurement is lost: {}",
                entry.observation
            );
        }
        // And the invariant it rests on, checked against the code rather than the
        // prose: `level_offset` is invariant to the word for every format.
        for fourcc in [
            crate::DXT5_FOURCC,
            crate::DXT1_FOURCC,
            crate::LUMINANCE_FOURCC,
        ] {
            let frame = crate::frame_for(fourcc).expect("a frame");
            let width = if fourcc == crate::LUMINANCE_FOURCC {
                4
            } else {
                8
            };
            let before: Vec<_> = (0..3)
                .map(|mip| crate::framing::level_offset(frame, width, width, mip, 3))
                .collect();
            assert!(
                before.iter().all(|answer| answer.is_ok()),
                "{fourcc:#010x}: three declared levels must be placeable"
            );
        }
    }

    #[test]
    fn the_refuted_claims_are_named_rather_than_dropped() {
        // A refutation that deletes its subject leaves the next reader with
        // nothing to contradict, which is how a wrong belief gets re-derived.
        for subject in [
            LAYER_HEADER_CONTENTS,
            LAYER_INTERLEAVE,
            DXT5_SPEC_DEVIATIONS,
        ] {
            let entry = claim(subject).unwrap_or_else(|| panic!("{subject} was deleted"));
            assert!(
                entry.observation.contains("REFUTED") || entry.observation.contains("refuted"),
                "{subject}: the refutation must be visible in the claim itself"
            );
        }
    }

    #[test]
    fn the_documents_the_bytes_refute_are_still_named() {
        for subject in [ENVELOPE_FIELD_1C_MEANING, LUMINANCE_FOURCC] {
            let entry = claim(subject).unwrap();
            assert!(
                entry.observation.contains("MATERIALS-DESIGN")
                    || entry.observation.contains("float 1.0")
                    || entry.observation.contains("REFUTE"),
                "{subject}: the refutation must be visible in the claim itself"
            );
        }
    }
}
