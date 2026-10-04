//! What a resolved texture's **own bytes** say it is — and, just as importantly,
//! what they do not.
//!
//! # Two different questions, kept apart on purpose
//!
//! A renderer asking "what do I bind this to?" is asking one of two questions,
//! and this repository can answer one of them from the corpus and the other not
//! at all:
//!
//! | Question | Answerable? | Where it lives |
//! |---|---|---|
//! | **What are these texels?** — a normal map? a mask? a colour map? | **Yes**, from the record's format word plus a per-record content check | [`TextureRole`] |
//! | **Which sampler stage does the game bind it to?** | **No** | [`crate::SamplerRole`], still one variant |
//!
//! Conflating them is how a renderer ends up confidently wrong. The first is a
//! statement about bytes and is checked per record; the second is a statement
//! about a shader this workspace has never read.
//!
//! # Roles are derived from content, never from list position
//!
//! Every role here comes from the record's own `fourcc` **and** a statistic
//! computed over its own base mip ([`MipStatistics`]). A record whose content
//! fails the check for its format word is reported as
//! [`TextureRole::Unidentified`] rather than keeping a role it did not earn.
//!
//! That is the whole design. A role assigned from "it is usually the first
//! texture" would survive a format change silently; this one cannot.
//!
//! # What the corpus says, and how strongly
//!
//! Measured over `SPORE/Data/Spore_Content.package` on 2026-10-04. The full
//! derivation, with every table, is in `docs/MATERIAL-ROLES.md`.
//!
//! ## `0x15` — a tangent-space normal map (`TangentSpaceNormal`)
//!
//! * **The content claim is re-published, not restated.**
//!   [`spore_texture::claims`] established it over 901 of 901 records; this
//!   crate delegates to that table through
//!   [`crate::claims::NORMAL_MAP_CONTENT`] rather than keeping a second copy.
//!   A local restatement is how a republished fact goes stale, and that defect
//!   already cost five test failures in this workspace.
//! * **The per-record check is this crate's own.** Over every texel of the base
//!   mip of all 901 records: lane 0 (which is **Z**, not R) averages
//!   232.51–255.00, exceeds the brighter lateral lane by at least 94.2, and
//!   lanes 1 and 2 sit within 10.80 of 127.5. Those are the corpus extremes the
//!   three thresholds are set against, and the margins are 8.5, 30.2 and 5.2.
//! * Seven of the 901 records hold the constant `(255, 128, 128)` — the exact
//!   encoding of the flat normal `(0, 0, 1)` — and the check accepts them,
//!   because a constant field is a degenerate normal map and not a failed one.
//!
//! ## `DXT5` — a layered single-channel mask (`LayeredAlphaMask`)
//!
//! * **It is not a colour map, and this is measured, not assumed.** Over 1374
//!   decodable DXT5 records, 720 371 712 texels per layer, the decoded red
//!   channel takes exactly **two** values and green and blue take exactly
//!   **one** each. Over a stride sample of 477 771 palette entries drawn from
//!   *every* DXT5 block in the corpus, the pinned codec yields exactly **two**
//!   distinct RGB triples: `(0,0,0)` and `(8,0,0)`.
//! * That degeneracy is **structural**, not a property of the data:
//!   `spore_texture`'s codec reads the block's colour field as two 6-bit
//!   values and expands them with a shift layout whose green and blue terms
//!   can only ever be zero. No input can produce a third colour.
//! * The only full-range channel is **alpha**: 65–256 distinct values per layer
//!   across the 1233 referenced record-instances, never a constant.
//! * It is **mostly transparent**: 52–81 % of texels have alpha exactly 0, and
//!   mean alpha is 16.5–64.6. An opaque base-colour map cannot look like that.
//! * **Its two layers are distinct surfaces**: layer 0 and layer 1 of the same
//!   record differ byte-for-byte in **1233 of 1233** referenced records. A
//!   renderer may treat them as two channels, not as one image and a copy.
//!
//! ## The purpose of a mask layer is still unknown
//!
//! [`TextureRole::LayeredAlphaMask`] says what the texels **are**. It does not
//! say what the game does with them — which of the two DXT5 records is
//! specular, which is gloss, which is an emissive mask. Nothing measured here
//! answers that, so
//! [`crate::claims::MASK_LAYER_PURPOSE`] is a **non-finding**, and a renderer
//! that wants a name for it has to invent one and say so.
//!
//! # Position is a cross-check, never the basis
//!
//! The corpus has a startling regularity (measured, see
//! `docs/MATERIAL-ROLES.md`): every gmdl texture set in `Spore_Content` is
//! either `(DXT5, DXT5, 0x15)` — 1922 sets — or `(DXT5, 0x15)` — 459 sets —
//! and the `0x15` record is **always last**. That is a real observation and this
//! crate records it, as [`crate::claims::TEXTURE_SET_SHAPES`].
//!
//! It is still not how a role is assigned here, for two measured reasons:
//!
//! 1. the *second* entry is a `DXT5` in 1922 sets and a `0x15` in 459, so the
//!    ordinal alone does not determine the content; and
//! 2. the group id's low byte happens to equal the ordinal
//!    ([`crate::claims::GROUP_LOW_BYTE_IS_SLOT`]), which is a convenient
//!    cross-check and a terrible source of truth — it would survive a reordering
//!    of the file's contents perfectly.
//!
//! [`crate::claims::SAMPLER_WORD_IS_ENTRY_ORDINAL`] records the matching fact
//! about the entry's own first header word, which is how the ordinal is known
//! to be the record's *own* statement rather than this crate's arithmetic.

use spore_core::evidence::EvidenceLevel;
use spore_texture::{MipImage, DXT5_FOURCC, LUMINANCE_FOURCC};

/// Byte lanes of one RGBA8 texel, in file order.
///
/// The order is the file's, not the semantic one: for the `0x15` family
/// **lane 0 is the normal's Z**, not its R. A renderer that treats the first
/// byte as red gets a normal map with its Z and X swapped, which looks almost
/// right and is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane {
    /// Byte 0 of the texel.
    Red,
    /// Byte 1.
    Green,
    /// Byte 2.
    Blue,
    /// Byte 3.
    Alpha,
}

impl Lane {
    /// Every lane, in file order.
    pub const ALL: [Lane; 4] = [Self::Red, Self::Green, Self::Blue, Self::Alpha];

    /// The lane's byte offset within a texel.
    pub const fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Green => 1,
            Self::Blue => 2,
            Self::Alpha => 3,
        }
    }

    /// The canonical spelling, for logs and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Red => "r",
            Self::Green => "g",
            Self::Blue => "b",
            Self::Alpha => "a",
        }
    }
}

/// What one lane of one layer carries, as far as the corpus establishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChannelRole {
    /// The **Z** component of a tangent-space normal. Lane 0 of a `0x15` record:
    /// the constant `(255, 128, 128)` is the encoding of the flat normal
    /// `(0, 0, 1)`.
    NormalZ,
    /// The **X** component. Lane 1 of a `0x15` record.
    NormalX,
    /// The **Y** component. Lane 2 of a `0x15` record.
    NormalY,
    /// A full-range single 8-bit channel: the alpha plane of one DXT5 layer.
    ///
    /// 65–256 distinct values per layer across every referenced record, never
    /// constant. **Its purpose is unknown** — see
    /// [`crate::claims::MASK_LAYER_PURPOSE`].
    MaskChannel,
    /// A colour plane of a DXT5 layer, which the byte-pinned codec reduces to
    /// one or two values.
    ///
    /// Not "black": *degenerate under the codec this repository pins*. The
    /// codec's colour-field expansion cannot produce a third colour for any
    /// input, so this is a property of the decoder and not of the record. See
    /// [`crate::claims::DXT5_CARRIES_NO_COLOUR`].
    DegenerateColour,
    /// The fourth lane of a `0x15` record.
    ///
    /// Statistically independent of the normal lanes (correlation 0.032) and
    /// binary in the 540 largest records, graded `INFERRED` upstream. **Its
    /// meaning is unknown**, and this variant exists so that it is visibly
    /// unknown rather than silently dropped.
    UnidentifiedLane,
}

impl ChannelRole {
    /// The canonical spelling, for logs and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NormalZ => "normal-z",
            Self::NormalX => "normal-x",
            Self::NormalY => "normal-y",
            Self::MaskChannel => "mask-channel",
            Self::DegenerateColour => "degenerate-colour",
            Self::UnidentifiedLane => "unidentified-lane",
        }
    }

    /// Whether the corpus knows what this channel is **for**.
    ///
    /// `false` is the honest answer for three of the six variants: a channel's
    /// *content* being known does not make its *purpose* known, and collapsing
    /// the two is how a renderer ends up shipping a specular map as a normal
    /// map.
    pub const fn purpose_established(self) -> bool {
        matches!(self, Self::NormalZ | Self::NormalX | Self::NormalY)
    }
}

/// One channel-plane of one layer, as a renderer needs to see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelBinding {
    /// Which of the record's layers this plane belongs to. `0` for the `0x15`
    /// family, which has exactly one.
    pub layer_index: usize,
    /// Which byte of the texel.
    pub lane: Lane,
    /// What the corpus says this plane carries.
    pub role: ChannelRole,
}

/// What a resolved texture's own texels are.
///
/// One variant per *content class the corpus demonstrates*. There is no
/// `Diffuse`: see the module documentation for the measurement that rules it
/// out for every raster record in `Spore_Content`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureRole {
    /// A tangent-space normal map, stored **Z first**.
    ///
    /// The content claim is re-published from [`spore_texture::claims`]; the
    /// per-record check that admits a record into this variant is
    /// [`normal_map_signature`].
    TangentSpaceNormal,
    /// A DXT5-family record: two layers, each carrying one full-range 8-bit
    /// channel in its alpha plane, over a colour field the pinned codec
    /// reduces to two values.
    ///
    /// **The purpose of either layer is unknown.** This variant names the
    /// content, not the job.
    LayeredAlphaMask,
    /// The record decoded, and its content matched none of the signatures
    /// above.
    ///
    /// This is the honest landing place for a record whose bytes do not look
    /// like its format word promised. It is not an error and it is not a
    /// default: a caller can see that the check ran and declined.
    Unidentified,
}

impl TextureRole {
    /// The canonical spelling, for logs and JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TangentSpaceNormal => "tangent-space-normal",
            Self::LayeredAlphaMask => "layered-alpha-mask",
            Self::Unidentified => "unidentified",
        }
    }

    /// The channel roles this texture's layers carry, in file order.
    ///
    /// Total length is `4 * layer_count`. The `Unidentified` variant yields
    /// four [`ChannelRole::UnidentifiedLane`]s per layer rather than nothing,
    /// so a caller iterating channels cannot mistake "unknown" for "absent".
    pub fn channels(self, layer_count: usize) -> Vec<ChannelBinding> {
        let per_layer: &[(Lane, ChannelRole)] = match self {
            Self::TangentSpaceNormal => &[
                (Lane::Red, ChannelRole::NormalZ),
                (Lane::Green, ChannelRole::NormalX),
                (Lane::Blue, ChannelRole::NormalY),
                (Lane::Alpha, ChannelRole::UnidentifiedLane),
            ],
            Self::LayeredAlphaMask => &[
                (Lane::Red, ChannelRole::DegenerateColour),
                (Lane::Green, ChannelRole::DegenerateColour),
                (Lane::Blue, ChannelRole::DegenerateColour),
                (Lane::Alpha, ChannelRole::MaskChannel),
            ],
            Self::Unidentified => &[
                (Lane::Red, ChannelRole::UnidentifiedLane),
                (Lane::Green, ChannelRole::UnidentifiedLane),
                (Lane::Blue, ChannelRole::UnidentifiedLane),
                (Lane::Alpha, ChannelRole::UnidentifiedLane),
            ],
        };
        let mut out = Vec::with_capacity(per_layer.len() * layer_count);
        for layer_index in 0..layer_count {
            for (lane, role) in per_layer {
                out.push(ChannelBinding {
                    layer_index,
                    lane: *lane,
                    role: *role,
                });
            }
        }
        out
    }
}

/// Exact per-lane statistics of one mip, gathered in a single pass.
///
/// Cheap enough to run on every resolved base mip (a 256-bit occupancy set per
/// lane, 128 bytes of stack) and exact rather than sampled, so a role check
/// never rests on a heuristic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MipStatistics {
    /// Texels examined. `width * height` for a well-formed image.
    pub texels: usize,
    /// Distinct values seen per lane, indexed by [`Lane::index`].
    pub distinct: [usize; 4],
    /// Smallest value seen per lane.
    pub min: [u8; 4],
    /// Largest value seen per lane.
    pub max: [u8; 4],
    /// Sum of each lane, for exact means.
    pub sum: [u64; 4],
}

impl MipStatistics {
    /// Gathers the statistics of `image`.
    ///
    /// A `pixels` slice whose length is not `width * height * 4` is measured as
    /// far as it goes rather than refused: this runs on already-decoded images
    /// and its job is to describe, not to validate. [`Self::texels`] then
    /// reports how many texels were actually seen, so a short buffer cannot
    /// masquerade as a fully-measured one.
    pub fn of(image: &MipImage) -> Self {
        let mut seen = [[0u64; 4]; 4];
        let mut stats = Self {
            texels: 0,
            distinct: [0; 4],
            min: [u8::MAX; 4],
            max: [0; 4],
            sum: [0; 4],
        };
        for texel in image.pixels.chunks_exact(4) {
            stats.texels += 1;
            for lane in 0..4 {
                let value = texel[lane];
                stats.min[lane] = stats.min[lane].min(value);
                stats.max[lane] = stats.max[lane].max(value);
                stats.sum[lane] += u64::from(value);
                let bit = 1u64 << (value % 64);
                if seen[lane][(value / 64) as usize] & bit == 0 {
                    seen[lane][(value / 64) as usize] |= bit;
                    stats.distinct[lane] += 1;
                }
            }
        }
        stats
    }

    /// The mean of one lane, or `None` when no texel was seen.
    ///
    /// `None` rather than `0.0`: "no texels" and "a mean of zero" are different
    /// findings, and a role check that divides by an empty sample would
    /// otherwise pass on nothing.
    pub fn mean(&self, lane: Lane) -> Option<f64> {
        if self.texels == 0 {
            return None;
        }
        Some(self.sum[lane.index()] as f64 / self.texels as f64)
    }

    /// Whether this lane is constant across every texel seen.
    pub fn is_constant(&self, lane: Lane) -> bool {
        self.texels > 0 && self.distinct[lane.index()] == 1
    }
}

/// Lowest mean lane 0 may have and still read as a normal map's Z.
///
/// Per-record means over the 901 `0x15` records of `Spore_Content.package` run
/// 232.51 to 255.00, so the bound leaves a margin of 8.5. That is thin, which
/// is why it is only half the test: [`NORMAL_Z_LEAD`] is the scale-free
/// condition, and this one is the absolute sanity bound beside it.
pub const NORMAL_Z_MEAN_FLOOR: f64 = 224.0;

/// How far lane 0's mean must exceed the *brighter* lateral lane's.
///
/// **This is the condition that actually establishes the Z-first order**, and it
/// is scale-free on purpose: a bound expressed as a difference between lanes
/// cannot be defeated by a brighter image, only by reordering the lanes. Over
/// the corpus the smallest observed lead is 94.2 (lane 0 at 232.51 against lane
/// 1 at 138.30), against a required 64.
pub const NORMAL_Z_LEAD: f64 = 64.0;

/// How far a lateral normal lane's mean may sit from 127.5 and still read as
/// centred.
///
/// The corpus's worst deviation is 10.80, so the margin is 5.2. Wider than one
/// might choose on a sample this size would weaken the check without evidence;
/// narrower would refuse real records.
pub const NORMAL_LANE_TOLERANCE: f64 = 16.0;

/// The neutral value the two lateral normal lanes are centred on.
pub const NORMAL_LANE_CENTRE: f64 = 127.5;

/// Whether `stats` is the signature of a tangent-space normal map stored **Z
/// first**.
///
/// Three conditions, and a record failing any of them is
/// [`TextureRole::Unidentified`]:
///
/// 1. lane 0 (Z) averages at least [`NORMAL_Z_MEAN_FLOOR`];
/// 2. lane 0's mean exceeds **the brighter lateral lane's** by at least
///    [`NORMAL_Z_LEAD`];
/// 3. lanes 1 (X) and 2 (Y) each average within [`NORMAL_LANE_TOLERANCE`] of
///    127.5.
///
/// Condition 2 is what pins the order rather than merely the brightness: a
/// record whose lanes were rotated to R,G,B would have its first lane averaging
/// near 127 and its second near 250, so the lead would be negative. Condition 3
/// is what makes this a *normal map* rather than a picture with a bright first
/// channel, and condition 1 is the absolute bound that keeps a near-black image
/// with one hot texel out.
///
/// Conditions 1 and 2 are **not** independent tests of the same thing and
/// neither is redundant: 1 bounds the value, 2 bounds the relationship. Both
/// margins over the corpus are recorded beside their constants.
pub fn normal_map_signature(stats: &MipStatistics) -> bool {
    let Some(z) = stats.mean(Lane::Red) else {
        return false;
    };
    let (Some(x), Some(y)) = (stats.mean(Lane::Green), stats.mean(Lane::Blue)) else {
        return false;
    };
    z >= NORMAL_Z_MEAN_FLOOR
        && z - x.max(y) >= NORMAL_Z_LEAD
        && (x - NORMAL_LANE_CENTRE).abs() <= NORMAL_LANE_TOLERANCE
        && (y - NORMAL_LANE_CENTRE).abs() <= NORMAL_LANE_TOLERANCE
}

/// The signature of a DXT5-family record: a degenerate colour field over a
/// non-constant alpha plane.
///
/// 1. red takes at most **two** distinct values;
/// 2. green and blue take at most **one** each;
/// 3. alpha takes at least **two**.
///
/// Condition 3 is what keeps this from degenerating into "a black image", and
/// the bar is deliberately low: it says the alpha plane is **not constant**, not
/// that it is photographic. A binary mask satisfies it, and a binary mask is
/// still a mask. The corpus is far above the bar — every referenced record's
/// alpha plane has 65–256 distinct values — but a threshold that high would be
/// asserting a statistic rather than a property, and would refuse small
/// synthetic records that genuinely carry a channel.
pub fn layered_mask_signature(stats: &MipStatistics) -> bool {
    stats.distinct[Lane::Red.index()] <= 2
        && stats.distinct[Lane::Green.index()] <= 1
        && stats.distinct[Lane::Blue.index()] <= 1
        && stats.distinct[Lane::Alpha.index()] >= 2
}

/// The grade a role is recorded at when its content signature was checked and
/// held, and the grade when it was checked and did not.
pub fn role_evidence(role: TextureRole) -> EvidenceLevel {
    match role {
        // Checked against this record's own bytes, which is an observation of
        // the record rather than of the corpus-wide family finding.
        TextureRole::TangentSpaceNormal | TextureRole::LayeredAlphaMask => EvidenceLevel::Observed,
        // The check ran and declined. Still not "unknown about everything" —
        // it is a specific negative with a known cause.
        TextureRole::Unidentified => EvidenceLevel::Observed,
    }
}

/// Classifies one base mip against the signatures above.
///
/// `fourcc` only selects **which** signature is tried; it never decides the
/// outcome on its own. That is the difference between a role and a format
/// lookup, and it is why a record whose content disagrees with its format word
/// lands in [`TextureRole::Unidentified`] instead of keeping a role.
pub fn classify(fourcc: u32, stats: &MipStatistics) -> TextureRole {
    match fourcc {
        LUMINANCE_FOURCC if normal_map_signature(stats) => TextureRole::TangentSpaceNormal,
        DXT5_FOURCC if layered_mask_signature(stats) => TextureRole::LayeredAlphaMask,
        _ => TextureRole::Unidentified,
    }
}

/// The byte count of the block codec's smallest input, used only by tests to
/// describe the surface [`MipStatistics::of`] walks.
#[cfg(test)]
const _: () = assert!(Lane::ALL.len() == 4);

#[cfg(test)]
impl TextureRole {
    /// Every variant, for the exhaustive tests above. Test-only so that adding
    /// a variant is a deliberate act that these tests notice.
    const ALL_FOR_TESTS: [TextureRole; 3] = [
        TextureRole::TangentSpaceNormal,
        TextureRole::LayeredAlphaMask,
        TextureRole::Unidentified,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(width: u32, height: u32, texels: &[[u8; 4]]) -> MipImage {
        let mut pixels = Vec::with_capacity(texels.len() * 4);
        for texel in texels {
            pixels.extend_from_slice(texel);
        }
        MipImage {
            width,
            height,
            pixels,
        }
    }

    /// A 2x2 field holding the flat normal, plus one tilted texel.
    fn flat_normal_image() -> MipImage {
        image(
            2,
            2,
            &[
                [255, 128, 128, 0],
                [255, 128, 128, 255],
                [255, 128, 128, 17],
                [255, 128, 128, 3],
            ],
        )
    }

    #[test]
    fn lanes_are_file_order_and_nothing_else() {
        assert_eq!(Lane::Red.index(), 0);
        assert_eq!(Lane::Alpha.index(), 3);
        assert_eq!(
            Lane::ALL.map(Lane::as_str),
            ["r", "g", "b", "a"],
            "the order is the file's, which is why 0x15's first lane is Z"
        );
    }

    #[test]
    fn the_three_normal_lanes_are_distinguishable_and_only_those_three_have_a_purpose() {
        // The naming is the whole point: a renderer must not be able to read
        // "NormalX" and reach for lane 0.
        assert_eq!(
            ChannelRole::NormalZ.channels_note(),
            "normal-z = file lane 0 of a 0x15 record"
        );
        for role in [
            ChannelRole::NormalZ,
            ChannelRole::NormalX,
            ChannelRole::NormalY,
        ] {
            assert!(role.purpose_established(), "{}", role.as_str());
        }
        for role in [
            ChannelRole::MaskChannel,
            ChannelRole::DegenerateColour,
            ChannelRole::UnidentifiedLane,
        ] {
            assert!(!role.purpose_established(), "{}", role.as_str());
        }
    }

    impl ChannelRole {
        /// Test-only: the file lane a normal role occupies.
        fn channels_note(self) -> &'static str {
            match self {
                Self::NormalZ => "normal-z = file lane 0 of a 0x15 record",
                Self::NormalX => "normal-x = file lane 1 of a 0x15 record",
                Self::NormalY => "normal-y = file lane 2 of a 0x15 record",
                _ => "not a normal role",
            }
        }
    }

    #[test]
    fn statistics_are_exact_and_cover_all_four_lanes() {
        let stats = MipStatistics::of(&image(2, 1, &[[0, 5, 255, 1], [8, 5, 255, 2]]));
        assert_eq!(stats.texels, 2);
        assert_eq!(stats.distinct, [2, 1, 1, 2]);
        assert_eq!(stats.min, [0, 5, 255, 1]);
        assert_eq!(stats.max, [8, 5, 255, 2]);
        assert_eq!(stats.sum, [8, 10, 510, 3]);
        assert_eq!(stats.mean(Lane::Red), Some(4.0));
        assert!(stats.is_constant(Lane::Green));
        assert!(!stats.is_constant(Lane::Red));
    }

    #[test]
    fn a_full_range_lane_is_counted_exactly() {
        // Every one of the 256 byte values in one lane. The occupancy set is
        // 4 x u64, so a value at the top of the range must land in the last word
        // rather than aliasing: `1 << (value % 64)` with `value / 64` indexing is
        // the pair that makes this exact.
        let texels: Vec<[u8; 4]> = (0..=255u8).map(|v| [0, 0, 0, v]).collect();
        let stats = MipStatistics::of(&image(16, 16, &texels));
        assert_eq!(stats.texels, 256);
        assert_eq!(stats.distinct[Lane::Alpha.index()], 256);
        assert_eq!(stats.distinct[Lane::Red.index()], 1);
        assert_eq!(stats.min[Lane::Alpha.index()], 0);
        assert_eq!(stats.max[Lane::Alpha.index()], 255);
        assert_eq!(stats.sum[Lane::Alpha.index()], 32_640, "0..=255 summed");
    }

    #[test]
    fn an_image_with_no_texels_has_means_rather_than_zeroes() {
        // The failure this rules out: `mean() == 0.0` would satisfy
        // "green and blue are constant" and let an empty decode pass a content
        // check it never ran.
        let stats = MipStatistics::of(&MipImage {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        });
        assert_eq!(stats.texels, 0);
        assert_eq!(stats.mean(Lane::Red), None);
        assert!(!stats.is_constant(Lane::Red));
        assert!(!normal_map_signature(&stats));
        assert!(
            !layered_mask_signature(&stats),
            "an empty image must not match the mask signature either"
        );
    }

    #[test]
    fn a_short_pixel_buffer_is_measured_as_far_as_it_goes() {
        let mut short = flat_normal_image();
        short.pixels.truncate(9); // two texels and a stray byte
        let stats = MipStatistics::of(&short);
        assert_eq!(stats.texels, 2, "the stray byte is not a texel");
    }

    #[test]
    fn the_flat_normal_is_a_normal_map_and_its_channel_order_is_z_first() {
        let stats = MipStatistics::of(&flat_normal_image());
        assert!(normal_map_signature(&stats));
        // And the same bytes under an R,G,B reading would fail: lane 0 averages
        // 127.5 here, far below the floor.
        assert!(stats.mean(Lane::Red) == Some(255.0));
        assert_eq!(stats.mean(Lane::Green), Some(128.0));
    }

    #[test]
    fn an_rgb_ordered_normal_map_is_rejected() {
        // The same field with the lanes rotated: red now carries X, not Z. A
        // check that only looked for "a bright channel" would let this through.
        let stats = MipStatistics::of(&image(2, 1, &[[128, 255, 128, 0], [128, 250, 128, 0]]));
        assert!(
            !normal_map_signature(&stats),
            "lane 0 averaging 128 is not Z"
        );
    }

    #[test]
    fn a_colour_map_is_not_misread_as_a_mask() {
        // Full-range RGB with a varying alpha: the signature must decline it.
        let stats = MipStatistics::of(&image(
            2,
            2,
            &[
                [10, 200, 30, 0],
                [11, 201, 31, 85],
                [12, 202, 32, 170],
                [13, 203, 33, 255],
            ],
        ));
        assert!(!layered_mask_signature(&stats));
    }

    #[test]
    fn a_black_opaque_image_is_not_a_mask() {
        // Distinct counts guard against it: RGB constant is fine, but an alpha
        // plane with a single value carries no channel at all.
        let stats = MipStatistics::of(&image(2, 2, &[[0, 0, 0, 255]; 4]));
        assert!(!layered_mask_signature(&stats));
        assert_eq!(stats.distinct[Lane::Alpha.index()], 1);
    }

    #[test]
    fn a_binary_alpha_plane_is_a_mask_and_an_empty_one_is_not() {
        // The threshold's two edges, so "non-constant" cannot drift into
        // "high-resolution" by accident.
        let binary = MipStatistics::of(&image(2, 1, &[[0, 0, 0, 0], [8, 0, 0, 255]]));
        assert_eq!(binary.distinct[Lane::Alpha.index()], 2);
        assert!(layered_mask_signature(&binary), "a binary mask is a mask");
        let constant = MipStatistics::of(&image(2, 1, &[[0, 0, 0, 0], [8, 0, 0, 0]]));
        assert_eq!(constant.distinct[Lane::Alpha.index()], 1);
        assert!(!layered_mask_signature(&constant));
    }

    #[test]
    fn classification_uses_the_format_word_to_pick_the_signature_and_content_to_decide() {
        let normal = MipStatistics::of(&flat_normal_image());
        let mask = MipStatistics::of(&image(
            2,
            2,
            &[[0, 0, 0, 0], [8, 0, 0, 90], [0, 0, 0, 200], [8, 0, 0, 255]],
        ));

        // Right format, right content.
        assert_eq!(
            classify(LUMINANCE_FOURCC, &normal),
            TextureRole::TangentSpaceNormal
        );
        assert_eq!(classify(DXT5_FOURCC, &mask), TextureRole::LayeredAlphaMask);

        // Right format, wrong content: Unidentified, not a free pass.
        assert_eq!(classify(LUMINANCE_FOURCC, &mask), TextureRole::Unidentified);
        assert_eq!(classify(0x3154_5844, &normal), TextureRole::Unidentified);
        assert_eq!(classify(0xDEAD_BEEF, &mask), TextureRole::Unidentified);

        // The two signatures are NOT disjoint, and this test pins that. A flat
        // normal field has constant red and green and blue, which satisfies the
        // mask's colour conditions, and its fourth lane takes four values here,
        // which satisfies the mask's alpha condition. So `classify` needs the
        // format word to choose -- which is exactly why `classify` is not a
        // content lookup and the format word is not a free pass either.
        assert!(
            layered_mask_signature(&normal),
            "the overlap is real; if this stops holding, classify needs a rethink"
        );
        assert_eq!(
            classify(DXT5_FOURCC, &normal),
            TextureRole::LayeredAlphaMask,
            "the format word decides which signature is tried"
        );
        // And a record satisfying NEITHER signature is Unidentified whichever
        // format word it carries -- the only route to that variant from a
        // decodable record.
        let neither = MipStatistics::of(&image(
            2,
            2,
            &[
                [10, 200, 30, 1],
                [11, 201, 31, 2],
                [12, 202, 32, 3],
                [13, 203, 33, 4],
            ],
        ));
        assert_eq!(
            classify(LUMINANCE_FOURCC, &neither),
            TextureRole::Unidentified
        );
        assert_eq!(classify(DXT5_FOURCC, &neither), TextureRole::Unidentified);
    }

    #[test]
    fn there_is_no_diffuse_role_to_reach_for() {
        // Not a style preference. A `Diffuse` variant would be a claim that
        // some raster record in this corpus is a base-colour map, and the
        // measurement rules that out for all 2954 of them. Adding the variant
        // back must break here.
        assert_eq!(TextureRole::ALL_FOR_TESTS.len(), 3);
        assert!(!TextureRole::ALL_FOR_TESTS
            .iter()
            .any(|role| role.as_str().contains("diffuse")));
    }

    #[test]
    fn unidentified_yields_four_unknown_channels_per_layer_not_none() {
        let channels = TextureRole::Unidentified.channels(2);
        assert_eq!(channels.len(), 8);
        assert!(channels
            .iter()
            .all(|channel| channel.role == ChannelRole::UnidentifiedLane));
        assert_eq!(
            channels
                .iter()
                .filter(|channel| channel.layer_index == 1)
                .count(),
            4
        );
    }

    #[test]
    fn the_normal_role_maps_lane_zero_to_z() {
        let channels = TextureRole::TangentSpaceNormal.channels(1);
        assert_eq!(channels.len(), 4);
        assert_eq!(channels[0].lane, Lane::Red);
        assert_eq!(
            channels[0].role,
            ChannelRole::NormalZ,
            "file order puts Z first; this is the assertion that says so"
        );
        assert_eq!(channels[1].role, ChannelRole::NormalX);
        assert_eq!(channels[2].role, ChannelRole::NormalY);
        assert_eq!(channels[3].role, ChannelRole::UnidentifiedLane);
    }

    #[test]
    fn the_mask_role_names_the_alpha_plane_and_degrades_the_colour_planes() {
        let channels = TextureRole::LayeredAlphaMask.channels(2);
        assert_eq!(channels.len(), 8);
        for layer in 0..2 {
            let at = |lane: usize| channels[layer * 4 + lane];
            assert_eq!(at(0).role, ChannelRole::DegenerateColour);
            assert_eq!(at(1).role, ChannelRole::DegenerateColour);
            assert_eq!(at(2).role, ChannelRole::DegenerateColour);
            assert_eq!(at(3).role, ChannelRole::MaskChannel);
            assert_eq!(at(3).layer_index, layer);
        }
    }

    #[test]
    fn every_grade_is_observed_and_never_unknown() {
        for role in TextureRole::ALL_FOR_TESTS {
            assert_eq!(role_evidence(role), EvidenceLevel::Observed, "{role:?}");
            assert_ne!(role_evidence(role), EvidenceLevel::Unknown);
        }
        assert_eq!(
            role_evidence(TextureRole::Unidentified),
            crate::claims::grade_of(crate::claims::TEXTURE_CONTENT_ROLE)
        );
    }

    #[test]
    fn every_role_and_channel_spelling_is_distinct() {
        let mut names: Vec<&str> = TextureRole::ALL_FOR_TESTS
            .iter()
            .map(|role| role.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), TextureRole::ALL_FOR_TESTS.len());

        let all = [
            ChannelRole::NormalZ,
            ChannelRole::NormalX,
            ChannelRole::NormalY,
            ChannelRole::MaskChannel,
            ChannelRole::DegenerateColour,
            ChannelRole::UnidentifiedLane,
        ];
        let mut spellings: Vec<&str> = all.iter().map(|role| role.as_str()).collect();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), all.len());
    }
}
