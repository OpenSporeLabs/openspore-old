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
//! | **What are these texels?** — a normal map? a colour image? a mask? | **Yes**, from the record's format word plus a per-record content check | [`TextureRole`] |
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
//! Measured over `SPORE/Data/Spore_Content.package` on 2026-10-05. The full
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
//! ## `DXT5` — a colour image with a separate fourth plane (`ColourAndMask`)
//!
//! **This section replaced a refuted one, and the refutation is kept.** The
//! previous model was a [`TextureRole::LayeredAlphaMask`] carrying
//! [`ChannelRole::DegenerateColour`] planes, derived from a decoder that charged
//! **8 bytes per 4x4 block**. The corpus refuted that decoder's stride — 16 bytes
//! per block, 2 954 of 2 954 records — and with it the model built on top. Read
//! as BC3, the same records carry:
//!
//! | measurement | over 1 747 DXT5 records | this crate's threshold |
//! |---|---|---|
//! | distinct values in lane 0 (red) | **≥ 32** in every record (max 235) | [`COLOUR_LANE_CARDINALITY_FLOOR`] = 8 |
//! | distinct values in lane 1 (green) | **≥ 64** in every record (max 255) | the same |
//! | distinct values in lane 2 (blue) | **≥ 32** in every record (max 235) | the same |
//! | records with a colour lane of ≤ 2 values | **0** | — |
//! | distinct values in lane 3 (alpha) | **1 … 256**; **46 records have 1** | **no threshold** |
//! | mean of the three colour-pair correlations | median **0.510**, min −0.078, max 0.712 | published, not gated |
//! | mean \|colour↔alpha\| correlation | median **0.067**, max **0.539** | published, not gated |
//!
//! Three consequences, and all three are the point:
//!
//! 1. **"DXT5 carries no colour" was a tautology of the broken decode.** R5G5B5
//!    on a 6-bit word reads green and blue as zero *by construction*, because a
//!    6-bit value cannot set bits 10..19. There was never a measurement of the
//!    data; there was a measurement of the codec. See
//!    [`crate::claims::DXT5_CARRIES_NO_COLOUR`], which now records the
//!    **refutation** under the original subject.
//! 2. **The alpha plane cannot be required to vary.** 46 of 1 747 records hold
//!    a *constant* fourth plane (every one of them all-zero). A signature that
//!    demanded a non-constant alpha would have refused 46 real records on a
//!    condition the data contradicts, so there is no such condition — and
//!    [`ResolvedTexture::mask_plane_varies`](crate::ResolvedTexture) reports
//!    the per-record answer instead of the role assuming one.
//! 3. **The role is graded no higher than the codec that produces it.**
//!    `spore_texture` grades BC3 as [`spore_texture::claims::BC3_CODEC`] —
//!    `INFERRED`, because the stride is a byte count but the codec inside a
//!    block has never been compared against the game's own output. A claim
//!    about decoded BC3 texels inherits that ceiling; see
//!    [`crate::claims::COLOUR_AND_MASK_CONTENT`].
//!
//! ## What a fourth plane is **for** is still unknown
//!
//! [`ChannelRole::MaskChannel`] says what the texels **are**: one channel,
//! independent of the colour triple, carrying its own values. It does not say
//! what the game does with it — which of the two DXT5 records in a set is
//! specular, which is gloss, which is an emissive term. Nothing measured here
//! answers that, so [`crate::claims::MASK_LAYER_PURPOSE`] is a **non-finding**,
//! and a renderer that wants a name for it has to invent one and say so.
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

/// The three lanes the colour signature looks at, in a fixed order.
///
/// Named as a constant because three separate functions would otherwise each
/// spell the triple, and three spellings of "the colour lanes" is three chances
/// to gate a different set.
pub const COLOUR_LANES: [Lane; 3] = [Lane::Red, Lane::Green, Lane::Blue];

/// The unordered lane pairs [`MipStatistics::correlation`] is tabulated over,
/// in the order its `cross` sums are laid out.
///
/// The first three are colour-to-colour and the last three colour-to-alpha, so
/// `mean_colour_correlation` and `mean_alpha_correlation_abs` are two contiguous
/// slices rather than three hand-picked pairs each.
pub const LANE_PAIRS: [(Lane, Lane); 6] = [
    (Lane::Red, Lane::Green),
    (Lane::Red, Lane::Blue),
    (Lane::Green, Lane::Blue),
    (Lane::Red, Lane::Alpha),
    (Lane::Green, Lane::Alpha),
    (Lane::Blue, Lane::Alpha),
];

/// What one lane of one chain carries, as far as the corpus establishes.
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
    /// One of the three mutually-correlated colour planes of a `DXT5` record:
    /// lanes 0, 1 and 2 of the same BC1 colour half.
    ///
    /// **Which colour it names is the file's business, not this crate's.** The
    /// three lanes are given one variant rather than three because the
    /// measurement is about all three together — they are mutually correlated,
    /// which is what one RGB image is and three independent mask planes are not
    /// — and because assigning `Red`/`Green`/`Blue` *purposes* would be a claim
    /// the corpus does not support. [`Lane`] carries the position.
    ///
    /// Replaces [`ChannelRole::DegenerateColour`], which measured the refuted
    /// 8-byte codec rather than any record.
    ColourPlane,
    /// Lane 3 of a `DXT5` record: one 8-bit channel, statistically independent
    /// of the three colour lanes above.
    ///
    /// The name is about the **shape** of the data — a single channel that does
    /// not vary with the colour — and not about a job. **Its purpose is
    /// unknown**: see [`crate::claims::MASK_LAYER_PURPOSE`]. It is *also* not
    /// guaranteed to vary at all: 46 of the 1 747 DXT5 records in
    /// `Spore_Content.package` hold a constant (all-zero) fourth plane, which is
    /// why no signature demands one.
    MaskChannel,
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
            Self::ColourPlane => "colour-plane",
            Self::MaskChannel => "mask-plane",
            Self::UnidentifiedLane => "unidentified-lane",
        }
    }

    /// Whether the corpus knows what this channel is **for**.
    ///
    /// `false` is the honest answer for two of the five variants: a channel's
    /// *content* being known does not make its *purpose* known, and collapsing
    /// the two is how a renderer ends up shipping a specular map as a normal
    /// map. Note that `ColourPlane` is `false` even though "this lane carries
    /// colour" is established — what the colour is *for* is not.
    pub const fn purpose_established(self) -> bool {
        matches!(self, Self::NormalZ | Self::NormalX | Self::NormalY)
    }
}

/// One channel-plane of one chain, as a renderer needs to see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelBinding {
    /// Which of the record's chains this plane belongs to. Always `0`: a raster
    /// record in this corpus holds exactly **one** chain of mip levels — see
    /// [`crate::claims::LAYER_COUNT_IS_DERIVED`], whose recorded value is the
    /// refutation of the old two-layer reading.
    pub layer_index: usize,
    /// Which byte of the texel.
    pub lane: Lane,
    /// What the corpus says this plane carries.
    pub role: ChannelRole,
}

/// What a resolved texture's own texels are.
///
/// One variant per *content class the corpus demonstrates*. There is no
/// `Diffuse`: see the module documentation for the measurement that keeps it out
/// of this build for a reason that is now about **purpose** rather than about
/// colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextureRole {
    /// A tangent-space normal map, stored **Z first**.
    ///
    /// The content claim is re-published from [`spore_texture::claims`]; the
    /// per-record check that admits a record into this variant is
    /// [`normal_map_signature`].
    TangentSpaceNormal,
    /// A `DXT5`-family record: a three-channel colour image in lanes 0–2 over a
    /// fourth plane that does not vary with them.
    ///
    /// **The purpose of neither the colour nor the fourth plane is established.**
    /// This variant names the content classes that are present, not the job
    /// either does.
    ///
    /// Replaces `LayeredAlphaMask`, which was derived from the refuted 8-byte
    /// block stride. The old name is not kept: there are no layers to count, and
    /// the old "the colour planes are degenerate" premise is refuted.
    ColourAndMask,
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
            Self::ColourAndMask => "colour-and-mask",
            Self::Unidentified => "unidentified",
        }
    }

    /// The channel roles this texture's chains carry, in file order.
    ///
    /// Total length is `4 * layer_count`, and `layer_count` is **1** for every
    /// decoded raster record in this corpus — so 4 entries, not 8. The
    /// `Unidentified` variant yields four [`ChannelRole::UnidentifiedLane`]s per
    /// layer rather than nothing, so a caller iterating channels cannot mistake
    /// "unknown" for "absent".
    pub fn channels(self, layer_count: usize) -> Vec<ChannelBinding> {
        let per_layer: &[(Lane, ChannelRole)] = match self {
            Self::TangentSpaceNormal => &[
                (Lane::Red, ChannelRole::NormalZ),
                (Lane::Green, ChannelRole::NormalX),
                (Lane::Blue, ChannelRole::NormalY),
                (Lane::Alpha, ChannelRole::UnidentifiedLane),
            ],
            Self::ColourAndMask => &[
                (Lane::Red, ChannelRole::ColourPlane),
                (Lane::Green, ChannelRole::ColourPlane),
                (Lane::Blue, ChannelRole::ColourPlane),
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
///
/// The cross-lane sums ([`Self::squares`], [`Self::cross`]) are **integers**,
/// so the struct stays `Copy + Eq` and a correlation is computed from them on
/// demand rather than stored as an `f64`. Two reasons that matters: a stored
/// correlation cannot be recomputed by a caller who disagrees with the formula,
/// and it cannot be `Eq`.
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
    /// Sum of each lane's **squares**, for exact variances.
    pub squares: [u64; 4],
    /// Sum of each lane *pair's* products, in [`LANE_PAIRS`] order, for exact
    /// covariances.
    pub cross: [u64; 6],
}

impl MipStatistics {
    /// The statistics of an image no texel was read from.
    ///
    /// An explicit constructor rather than a `Default`, because "no texels" must
    /// be a value a caller can see rather than a set of zeroes that happens to
    /// satisfy a content check.
    pub const fn empty() -> Self {
        Self {
            texels: 0,
            distinct: [0; 4],
            min: [u8::MAX; 4],
            max: [0; 4],
            sum: [0; 4],
            squares: [0; 4],
            cross: [0; 6],
        }
    }

    /// Gathers the statistics of `image`.
    ///
    /// A `pixels` slice whose length is not `width * height * 4` is measured as
    /// far as it goes rather than refused: this runs on already-decoded images
    /// and its job is to describe, not to validate. [`Self::texels`] then
    /// reports how many texels were actually seen, so a short buffer cannot
    /// masquerade as a fully-measured one.
    pub fn of(image: &MipImage) -> Self {
        let mut seen = [[0u64; 4]; 4];
        let mut stats = Self::empty();
        for texel in image.pixels.chunks_exact(4) {
            stats.texels += 1;
            for (slot, &value) in texel.iter().enumerate() {
                let wide = u64::from(value);
                stats.min[slot] = stats.min[slot].min(value);
                stats.max[slot] = stats.max[slot].max(value);
                stats.sum[slot] = stats.sum[slot].saturating_add(wide);
                stats.squares[slot] = stats.squares[slot].saturating_add(wide * wide);
                let bit = 1u64 << (value % 64);
                if seen[slot][usize::from(value / 64)] & bit == 0 {
                    seen[slot][usize::from(value / 64)] |= bit;
                    stats.distinct[slot] += 1;
                }
            }
            for (index, (a, b)) in LANE_PAIRS.iter().enumerate() {
                stats.cross[index] = stats.cross[index]
                    .saturating_add(u64::from(texel[a.index()]) * u64::from(texel[b.index()]));
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

    /// The Pearson correlation of two lanes, or `None` when it is undefined.
    ///
    /// `None` for **three** distinct reasons, and none of them is a zero:
    ///
    /// * no texel was seen — there is no sample;
    /// * fewer than two texels were seen — a correlation over one point is
    ///   `0/0`; and
    /// * either lane is constant — the variance is zero, so no *linear*
    ///   relationship can be computed, and reporting `0.0` would say "these
    ///   lanes are unrelated" where the truth is "these lanes do not vary, so
    ///   the question does not apply".
    ///
    /// A caller that wants a number can substitute its own default; a caller
    /// that wants honesty has to handle the `None`.
    pub fn correlation(&self, a: Lane, b: Lane) -> Option<f64> {
        if self.texels < 2 {
            return None;
        }
        let (ia, ib) = (a.index(), b.index());
        if self.distinct[ia] <= 1 || self.distinct[ib] <= 1 {
            return None;
        }
        let n = self.texels as f64;
        let mean_a = self.sum[ia] as f64 / n;
        let mean_b = self.sum[ib] as f64 / n;
        let variance_a = self.squares[ia] as f64 / n - mean_a * mean_a;
        let variance_b = self.squares[ib] as f64 / n - mean_b * mean_b;
        // Clamped rather than trusted: the sums are exact integers, but the
        // floating-point subtraction of two nearly equal numbers can go
        // microscopically negative for a real non-constant lane.
        let spread_a = variance_a.max(0.0).sqrt();
        let spread_b = variance_b.max(0.0).sqrt();
        if spread_a <= 0.0 || spread_b <= 0.0 {
            return None;
        }
        let index = LANE_PAIRS
            .iter()
            .position(|(x, y)| (*x == a && *y == b) || (*x == b && *y == a))?;
        let covariance = self.cross[index] as f64 / n - mean_a * mean_b;
        Some((covariance / (spread_a * spread_b)).clamp(-1.0, 1.0))
    }

    /// The mean of the three colour-to-colour correlations, or `None` if any of
    /// the three is undefined.
    ///
    /// The published statistic behind "these three lanes are one colour image
    /// and not three independent channels". Over the 1 747 `DXT5` records of
    /// `Spore_Content.package` the median is **0.510** and the minimum is
    /// **−0.078**; it is **not** used as a gate, because 467 records fall below
    /// 0.30 and a threshold that size would be a statistic rather than a
    /// property.
    pub fn mean_colour_correlation(&self) -> Option<f64> {
        let (r, g, b) = (
            self.correlation(Lane::Red, Lane::Green)?,
            self.correlation(Lane::Red, Lane::Blue)?,
            self.correlation(Lane::Green, Lane::Blue)?,
        );
        Some((r + g + b) / 3.0)
    }

    /// The mean of the three **absolute** colour-to-alpha correlations, or `None`
    /// if any of the three is undefined.
    ///
    /// The published statistic behind "the fourth plane does not track the
    /// colour". Over the same 1 747 records the median is **0.067** and the
    /// maximum is **0.539**, against a median mutual correlation of 0.510 among
    /// the colour lanes themselves.
    pub fn mean_alpha_correlation_abs(&self) -> Option<f64> {
        let (r, g, b) = (
            self.correlation(Lane::Red, Lane::Alpha)?.abs(),
            self.correlation(Lane::Green, Lane::Alpha)?.abs(),
            self.correlation(Lane::Blue, Lane::Alpha)?.abs(),
        );
        Some((r + g + b) / 3.0)
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
/// the corpus the smallest observed lead is 94.2 (lane 0 at 232.51 against lane 1
/// at 138.30), against a required 64.
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

/// Fewest distinct values a colour lane may take and still be a colour lane.
///
/// **The corpus minimum is 32** — over all 1 747 `DXT5` records of
/// `Spore_Content.package`, no record's red or blue lane takes fewer than 32
/// distinct values and none takes fewer than 64 in green. So this bound has a
/// margin of 4x.
///
/// It is deliberately set an order of magnitude below the corpus rather than at
/// it, because the corpus minimum is a property of *these* records at 64² and up
/// and the bound has to be a property of the *format*. A single 4x4 record has
/// sixteen texels and can hold at most sixteen distinct values per lane, so a
/// bound at 32 would make a real small record unclassifiable for the sake of a
/// statistic. 8 is the point below which a lane is a mask plane rather than a
/// colour plane.
pub const COLOUR_LANE_CARDINALITY_FLOOR: usize = 8;

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

/// The signature of a `DXT5`-family record read as BC3: three colour planes over
/// a fourth plane.
///
/// One condition, and it is deliberately only one:
///
/// 1. each of lanes 0, 1 and 2 takes at least
///    [`COLOUR_LANE_CARDINALITY_FLOOR`] distinct values.
///
/// **The fourth lane is deliberately not constrained**, and the reason is a
/// measurement rather than an omission: 46 of the 1 747 `DXT5` records in
/// `Spore_Content.package` hold a *constant* fourth plane, and every one of
/// those 46 is all-zero. The previous signature of this family required a
/// non-constant alpha plane, so it would have refused those 46 real records on a
/// condition the corpus contradicts — and it would have done so while claiming
/// corpus support. Per-record variation of the fourth plane is published on
/// [`crate::ResolvedTexture::mask_plane_varies`] instead of being assumed here.
///
/// What the condition does establish is that lanes 0–2 are **not** a mask field
/// and not a degenerate constant: the refuted predecessor required red ≤ 2
/// values and green ≤ 1, which 0 of 1 747 records satisfy under BC3.
pub fn colour_and_mask_signature(stats: &MipStatistics) -> bool {
    COLOUR_LANES
        .iter()
        .all(|lane| stats.distinct[lane.index()] >= COLOUR_LANE_CARDINALITY_FLOOR)
}

/// The grade a role is recorded at when its content signature was checked and
/// held, and the grade when it was checked and did not.
pub fn role_evidence(role: TextureRole) -> EvidenceLevel {
    match role {
        // Checked against this record's own bytes, which is an observation of
        // the record rather than of the corpus-wide family finding. The grade is
        // `Observed` and not higher for a reason stated on
        // `COLOUR_AND_MASK_CONTENT`: the *content* claim about decoded BC3
        // texels cannot exceed the codec's own grade, which is `Inferred`.
        TextureRole::TangentSpaceNormal | TextureRole::ColourAndMask => EvidenceLevel::Observed,
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
        DXT5_FOURCC if colour_and_mask_signature(stats) => TextureRole::ColourAndMask,
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
        TextureRole::ColourAndMask,
        TextureRole::Unidentified,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claims;

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

    /// Whether `stats`'s `lane` clears the colour-cardinality floor.
    fn stats_has_enough(stats: &MipStatistics, lane: Lane) -> bool {
        stats.distinct[lane.index()] >= COLOUR_LANE_CARDINALITY_FLOOR
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
        // The two lane lists the cross-lane statistics walk are derived from,
        // and their lengths are what the `cross` array is indexed by.
        assert_eq!(COLOUR_LANES.len(), 3);
        assert_eq!(LANE_PAIRS.len(), 6);
        assert_eq!(
            COLOUR_LANES.len() * (COLOUR_LANES.len() + 1) / 2,
            LANE_PAIRS.len()
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
            ChannelRole::ColourPlane,
            ChannelRole::MaskChannel,
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
        // And the cross-lane sums are exact integers: squares 0^2+8^2, 5^2+5^2,
        // 255^2+255^2, 1^2+2^2; and the pair sums R*G, R*B, G*B, B*A over the two
        // texels (0, 5, 255, 1) and (8, 5, 255, 2).
        assert_eq!(stats.squares, [64, 50, 130_050, 5]);
        assert_eq!(stats.cross[0], 8 * 5, "r*g");
        assert_eq!(stats.cross[1], 8 * 255, "r*b");
        assert_eq!(stats.cross[2], 2 * 5 * 255, "g*b");
        assert_eq!(stats.cross[5], 255 * (1 + 2), "b*a");
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
        assert_eq!(stats, MipStatistics::empty());
        assert_eq!(stats.texels, 0);
        assert_eq!(stats.mean(Lane::Red), None);
        assert!(!stats.is_constant(Lane::Red));
        assert!(!normal_map_signature(&stats));
        assert!(
            !colour_and_mask_signature(&stats),
            "an empty image must not match the colour signature either"
        );
        assert_eq!(stats.correlation(Lane::Red, Lane::Green), None);
        assert_eq!(stats.mean_colour_correlation(), None);
        assert_eq!(stats.mean_alpha_correlation_abs(), None);
    }

    #[test]
    fn an_undefined_correlation_is_none_and_never_zero() {
        // Three ways the correlation does not apply, and none of them may become
        // a 0.0 that reads as "these lanes are unrelated".
        let one_texel = MipStatistics::of(&image(1, 1, &[[1, 2, 3, 4]]));
        assert_eq!(
            one_texel.correlation(Lane::Red, Lane::Green),
            None,
            "one texel is a correlation of 0/0"
        );
        let constant = MipStatistics::of(&image(4, 1, &[[7, 1, 2, 3], [7, 2, 4, 6]]));
        assert!(
            constant.is_constant(Lane::Red),
            "lane 0 never varies in this fixture"
        );
        assert_eq!(
            constant.correlation(Lane::Red, Lane::Green),
            None,
            "a constant lane has zero variance, so no linear relationship exists"
        );
        assert_eq!(
            constant.mean_colour_correlation(),
            None,
            "one undefined pair makes the mean undefined rather than partial"
        );
    }

    #[test]
    fn a_correlated_triple_is_measured_and_not_assumed() {
        // Red = green = blue, varied: the shape of one RGB image. The three
        // pairwise correlations must all come back at 1.
        let texels: Vec<[u8; 4]> = (0..16u8)
            .map(|v| [v * 16, v * 16, v * 16, 255 - v * 16])
            .collect();
        let stats = MipStatistics::of(&image(4, 4, &texels));
        assert_eq!(stats.distinct[Lane::Red.index()], 16);
        assert_eq!(stats.distinct[Lane::Green.index()], 16);
        assert_eq!(stats.distinct[Lane::Blue.index()], 16);
        for pair in LANE_PAIRS.iter().take(3) {
            let value = stats.correlation(pair.0, pair.1).expect("defined");
            assert!(
                value > 0.999,
                "{:?} should be 1 for identical lanes, got {value}",
                pair
            );
        }
        let colour = stats.mean_colour_correlation().expect("defined");
        assert!(colour > 0.999, "{colour}");
        // And the fourth plane, which here runs the other way, is reported as a
        // strong negative rather than as "independent".
        let alpha = stats.mean_alpha_correlation_abs().expect("defined");
        assert!(
            alpha > 0.999,
            "a reversed fourth plane is strongly correlated: {alpha}"
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
        assert_eq!(stats.mean(Lane::Red), Some(255.0));
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
    fn a_colour_map_is_misread_as_neither_role_when_its_format_word_says_mask() {
        // A full-range RGB image presented under a `0x15` fourcc: the normal-map
        // signature must decline it. Presented under `DXT5` the same bytes DO
        // satisfy the colour signature, which is the next test's subject.
        //
        // Sixteen texels rather than four, because the colour signature gates on
        // a distinct-value *count* and four texels cannot reach eight distinct
        // values in any lane — a small fixture would fail the floor for a reason
        // that has nothing to do with the code under test.
        let stats = MipStatistics::of(&image(
            4,
            4,
            &(0..16u8)
                .map(|v| [10 + v, 200 - v * 3, 30 + v * 5, v * 16])
                .collect::<Vec<_>>(),
        ));
        for lane in COLOUR_LANES {
            assert_eq!(
                stats.distinct[lane.index()],
                16,
                "every colour lane takes all sixteen values"
            );
        }
        assert!(!normal_map_signature(&stats));
        assert!(
            colour_and_mask_signature(&stats),
            "full-range colour lanes are exactly what this signature admits"
        );
        assert_eq!(
            classify(LUMINANCE_FOURCC, &stats),
            TextureRole::Unidentified
        );
    }

    #[test]
    fn a_constant_colour_field_is_not_a_colour_and_mask() {
        // The refuted predecessor's case, kept as the negative: a record whose
        // colour lanes take one or two values is a flat or mask-like field, and
        // the corpus contains **none** of them (0 of 1 747 records have a colour
        // lane of ≤ 2 values). 0 of 1 747 must stay 0, so this has to refuse.
        let flat_colour = MipStatistics::of(&image(2, 2, &[[0, 0, 0, 0]; 4]));
        assert_eq!(flat_colour.distinct[Lane::Red.index()], 1);
        assert!(!colour_and_mask_signature(&flat_colour));
        let two_values = MipStatistics::of(&image(2, 2, &[[0, 0, 0, 0], [8, 0, 0, 90]]));
        assert_eq!(two_values.distinct[Lane::Red.index()], 2);
        assert!(
            !colour_and_mask_signature(&two_values),
            "the OLD signature accepted this; the corpus says it does not occur"
        );
    }

    #[test]
    fn a_constant_fourth_plane_does_not_refuse_the_record() {
        // The measured reason the signature has no alpha condition: 46 of the
        // 1 747 DXT5 records hold a constant, all-zero fourth plane. A signature
        // that required a varying alpha would refuse them.
        let mut texels: Vec<[u8; 4]> = Vec::new();
        for v in 0..8u8 {
            for w in 0..8u8 {
                texels.push([v * 30, w * 30, (v + w) * 15, 0]);
            }
        }
        let stats = MipStatistics::of(&image(8, 8, &texels));
        assert_eq!(stats.distinct[Lane::Alpha.index()], 1);
        assert!(stats.is_constant(Lane::Alpha));
        assert!(
            colour_and_mask_signature(&stats),
            "a constant fourth plane must not cost a real record its role"
        );
        assert_eq!(classify(DXT5_FOURCC, &stats), TextureRole::ColourAndMask);
    }

    #[test]
    fn a_below_floor_colour_lane_refuses_the_record() {
        // Seven distinct values in a colour lane: below
        // `COLOUR_LANE_CARDINALITY_FLOOR`, so the record is `Unidentified` rather
        // than confidently named. Refusing is the correct answer for a record
        // whose lanes this crate cannot characterise.
        //
        // All three colour lanes are held at eight or seven together, because the
        // floor is a floor on **each** lane: a fixture that varied only red would
        // fail for a different reason than the one under test.
        assert_eq!(COLOUR_LANE_CARDINALITY_FLOOR, 8);
        let at_floor = |count: usize| -> Vec<[u8; 4]> {
            (0..count)
                .map(|v| [v as u8 * 7, v as u8 * 11, v as u8 * 5, 128])
                .collect()
        };
        let stats = MipStatistics::of(&image(8, 1, &at_floor(8)));
        for lane in COLOUR_LANES {
            assert_eq!(stats.distinct[lane.index()], 8, "{lane:?}");
        }
        assert!(colour_and_mask_signature(&stats));
        let stats = MipStatistics::of(&image(7, 1, &at_floor(7)));
        for lane in COLOUR_LANES {
            assert_eq!(stats.distinct[lane.index()], 7, "{lane:?}");
        }
        assert!(!colour_and_mask_signature(&stats));
        assert_eq!(classify(DXT5_FOURCC, &stats), TextureRole::Unidentified);
    }

    #[test]
    fn classification_uses_the_format_word_to_pick_the_signature_and_content_to_decide() {
        let normal = MipStatistics::of(&flat_normal_image());
        let colour = MipStatistics::of(&image(
            8,
            8,
            &(0..64u8)
                .map(|v| {
                    [
                        v * 3,
                        (v as u32 * 5 % 251) as u8,
                        (v as u32 * 7 % 253) as u8,
                        v % 200,
                    ]
                })
                .collect::<Vec<_>>(),
        ));
        for lane in COLOUR_LANES {
            assert!(
                stats_has_enough(&colour, lane),
                "the fixture must clear the floor in every colour lane"
            );
        }

        // Right format, right content.
        assert_eq!(
            classify(LUMINANCE_FOURCC, &normal),
            TextureRole::TangentSpaceNormal
        );
        assert_eq!(classify(DXT5_FOURCC, &colour), TextureRole::ColourAndMask);

        // Right format, wrong content: Unidentified, not a free pass.
        assert_eq!(
            classify(LUMINANCE_FOURCC, &colour),
            TextureRole::Unidentified
        );
        assert_eq!(classify(0x3154_5844, &normal), TextureRole::Unidentified);
        assert_eq!(classify(0xDEAD_BEEF, &colour), TextureRole::Unidentified);

        // The two signatures are NOT disjoint, and this test pins that. The same
        // statistics can satisfy either signature depending only on which fourcc
        // is presented: a field with a bright, slightly-varying first lane and two
        // lateral lanes centred on 127.5 satisfies the normal-map signature, and
        // because all three lanes vary it satisfies the colour signature too.
        // That is exactly why `classify` needs the format word to choose -- which
        // is exactly why `classify` is not a content lookup, and why the format
        // word is not a free pass either.
        let both = MipStatistics::of(&image(
            8,
            8,
            &(0..64u8)
                .map(|v| [248 + (v % 8), 120 + (v % 8), 130 - (v % 8), v])
                .collect::<Vec<_>>(),
        ));
        assert!(colour_and_mask_signature(&both));
        assert!(
            normal_map_signature(&both),
            "the overlap is real; if this stops holding, classify needs a rethink"
        );
        assert_eq!(
            classify(DXT5_FOURCC, &both),
            TextureRole::ColourAndMask,
            "the format word decides which signature is tried"
        );
        assert_eq!(
            classify(LUMINANCE_FOURCC, &both),
            TextureRole::TangentSpaceNormal,
            "and the other word reaches the other role from the SAME bytes"
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
        // Not a style preference, and the reason has CHANGED. A `Diffuse` variant
        // would claim the record's three colour planes are the model's base
        // colour. The corpus now says the colour planes are real -- that is what
        // `ColourAndMask` states -- and it says nothing about what they are FOR:
        // no shader has been read, no group name is known, and
        // `ChannelRole::ColourPlane::purpose_established()` is `false`. Adding
        // the variant back must break here.
        assert_eq!(TextureRole::ALL_FOR_TESTS.len(), 3);
        assert!(!TextureRole::ALL_FOR_TESTS
            .iter()
            .any(|role| role.as_str().contains("diffuse")));
        assert!(
            !TextureRole::ColourAndMask
                .channels(1)
                .iter()
                .any(|channel| channel.role.purpose_established()),
            "a DXT5 record has no lane whose purpose the corpus established"
        );
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
    fn the_colour_role_names_three_colour_planes_and_one_mask_plane() {
        // One chain, so FOUR channels -- not eight. "Eight" was two records x two
        // layers; there are no layers.
        let channels = TextureRole::ColourAndMask.channels(1);
        assert_eq!(channels.len(), 4);
        for (lane, expected) in COLOUR_LANES.iter().enumerate() {
            assert_eq!(channels[lane].lane, *expected);
            assert_eq!(
                channels[lane].role,
                ChannelRole::ColourPlane,
                "lanes 0-2 are the colour half, in file order"
            );
            assert!(!channels[lane].role.purpose_established());
        }
        assert_eq!(channels[3].lane, Lane::Alpha);
        assert_eq!(channels[3].role, ChannelRole::MaskChannel);
        assert!(!channels[3].role.purpose_established());
        assert!(channels.iter().all(|channel| channel.layer_index == 0));
    }

    #[test]
    fn every_grade_is_observed_and_never_unknown() {
        // `role_evidence` answers "did this record's OWN check run, and did it
        // decline?", which is `Observed` for every variant including the
        // `Unidentified` one: a check that ran and said no is not an absence.
        //
        // It is deliberately NOT the content claim's grade. The two are different
        // questions with different ceilings: `role_evidence` is bounded by what
        // happened to this record, and `claims::COLOUR_AND_MASK_CONTENT` is
        // bounded by the BC3 codec upstream. Asserting they agree is what the
        // pre-refutation version of this test did, and it is exactly the conflation
        // that let a codec reading be graded `Verified`.
        for role in TextureRole::ALL_FOR_TESTS {
            assert_eq!(role_evidence(role), EvidenceLevel::Observed, "{role:?}");
            assert_ne!(role_evidence(role), EvidenceLevel::Unknown);
        }
        assert_eq!(
            claims::grade_of(crate::claims::TEXTURE_CONTENT_ROLE),
            crate::claims::bc3_claim_ceiling(),
            "the content claim sits at the BC3 ceiling, which is below Observed"
        );
        assert!(
            crate::claims::bc3_claim_ceiling() < EvidenceLevel::Observed,
            "if the codec were ever raised above Observed this test's premise changes"
        );
    }

    #[test]
    fn a_role_about_decoded_bc3_texels_never_exceeds_the_codecs_own_grade() {
        // The rule that stops this crate from re-asserting a refuted codec
        // reading: a statement about what a decoded BC3 texel CONTAINS cannot be
        // graded above the codec that decoded it.
        let ceiling = crate::claims::bc3_claim_ceiling();
        assert_eq!(
            ceiling,
            spore_texture::claims::grade_of(spore_texture::claims::BC3_CODEC),
            "the ceiling is whatever the crate that measured the codec says"
        );
        assert_eq!(
            crate::claims::grade_of(crate::claims::COLOUR_AND_MASK_CONTENT),
            ceiling
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
            ChannelRole::ColourPlane,
            ChannelRole::MaskChannel,
            ChannelRole::UnidentifiedLane,
        ];
        let mut spellings: Vec<&str> = all.iter().map(|role| role.as_str()).collect();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), all.len());
    }
}
