//! The colour grammar of a `<sporemodel>` document — **canonical**.
//!
//! # Why this module exists, and why it moved here
//!
//! Two implementations of this grammar existed. `spore_assets::color` decoded
//! the colour-bearing fields directly, and `spore-creator-core` decoded the same
//! fields as part of the whole document. Both were correct on the corpus they
//! were written against, and they would have drifted: the repository's
//! cross-cutting rule is that the same record must never have two answers.
//!
//! This module is now the single owner. `spore_assets::color` re-exports
//! [`UnitRgb`] and [`ColorError`] rather than defining its own, so a consumer
//! that wants a colour out of raw bytes and a consumer that wants a whole
//! document are reading the *same* rules.
//!
//! # The grammar, and what it was measured against
//!
//! A colour is **three** components, written `r,g,b`. Measured over the
//! installed corpus — 136 `<sporemodel>` records across `Spore_Game.package` and
//! `PatchData.package`, 1 146 colour elements, 3 438 components:
//!
//! * every component is `digits` or `digits '.' digits` — **0 exceptions**;
//! * every component lies in `0.0 ..= 1.0` — **0 exceptions**, observed range
//!   exactly `[0.0, 1.0]`;
//! * 108 distinct component values corpus-wide;
//! * the canonical width is six fractional digits, with bare integers for whole
//!   values (`1,1,1`, `0,0,0`).
//!
//! So [`UnitRgb::parse`] accepts **exactly** that and refuses everything else —
//! `1e-3`, `inf`, `NaN`, a leading `+`, a trailing `.`, and surrounding
//! whitespace. A permissive float parse would be a guess about data nobody has
//! seen, and the whole point of this crate is that an unseen spelling is a
//! finding, not a silent success.
//!
//! # A fourth component does not exist here
//!
//! The creature skin shader consumes `float4` (`mColor1`, `mColor2`, `mParams`)
//! and reads `.a` from two of them, but the document writes three components and
//! never a fourth. That gap is recorded, not filled: see
//! the `COLOR_ALPHA_SOURCE` claim. Alpha is **not** defaulted to 1.0
//! here, because a default alpha would be a fourth component this corpus does
//! not contain.

use core::fmt;

/// One corpus colour: three components in `0.0 ..= 1.0`.
///
/// `PartialEq` is the **bitwise** one this crate uses everywhere: `-0.0 == 0.0`
/// is true under IEEE comparison but they are different bytes, and a colour
/// round trip that quietly changed a sign bit would be a finding this crate
/// exists to prevent.
#[derive(Debug, Clone, Copy)]
pub struct UnitRgb {
    /// Red, `0.0 ..= 1.0`.
    pub red: f32,
    /// Green, `0.0 ..= 1.0`.
    pub green: f32,
    /// Blue, `0.0 ..= 1.0`.
    pub blue: f32,
}

impl UnitRgb {
    /// Builds a triple, rejecting any component outside `0.0 ..= 1.0`.
    ///
    /// Returns `None` rather than clamping. Clamping would turn a record this
    /// build has never seen into a colour it can draw, which is the kind of
    /// quiet repair the repository forbids.
    pub const fn new(red: f32, green: f32, blue: f32) -> Option<Self> {
        if !(red >= 0.0 && red <= 1.0)
            || !(green >= 0.0 && green <= 1.0)
            || !(blue >= 0.0 && blue <= 1.0)
        {
            return None;
        }
        Some(Self { red, green, blue })
    }

    /// The three components in `red, green, blue` order.
    pub const fn components(self) -> [f32; 3] {
        [self.red, self.green, self.blue]
    }

    /// Parses `r,g,b` in the corpus's own spelling.
    ///
    /// Fails rather than repairing. Every failure names the element it came
    /// from, so a caller decoding a whole document can report *which* field is
    /// unreadable instead of "some colour was bad".
    pub fn parse(text: &str, element: &str) -> Result<Self, ColorError> {
        let mut parts = text.split(',');
        let (Some(red), Some(green), Some(blue), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(ColorError::ComponentCount {
                element: element.to_owned(),
                text: text.to_owned(),
                found: text.split(',').count(),
            });
        };
        let mut out = [0.0f32; 3];
        for (slot, raw) in out.iter_mut().zip([red, green, blue]) {
            *slot = component(raw).ok_or_else(|| ColorError::MalformedComponent {
                element: element.to_owned(),
                text: text.to_owned(),
                component: (*raw).to_owned(),
            })?;
        }
        Self::new(out[0], out[1], out[2]).ok_or(ColorError::OutOfRange {
            element: element.to_owned(),
            text: text.to_owned(),
        })
    }

    /// The components in the corpus's canonical spelling.
    ///
    /// Six fractional digits, and a bare integer for a whole value — which is
    /// what the install writes: `0.341176,0.360784,0.576470`, `1,1,1`. This is a
    /// *canonical* form, not a byte-identical one: a seven-digit component
    /// parses to the same `f32` to within one unit in the last decimal place and
    /// prints back at six. A caller that needs the original bytes must keep them;
    /// this is a spelling, not a recovery.
    pub fn to_corpus_text(self) -> String {
        fn one(v: f32) -> String {
            if v == v.trunc() {
                return format!("{}", v as u32);
            }
            format!("{v:.6}")
        }
        format!("{},{},{}", one(self.red), one(self.green), one(self.blue))
    }
}

impl PartialEq for UnitRgb {
    /// Bitwise equality — see the type docs.
    fn eq(&self, other: &Self) -> bool {
        self.red.to_bits() == other.red.to_bits()
            && self.green.to_bits() == other.green.to_bits()
            && self.blue.to_bits() == other.blue.to_bits()
    }
}

impl fmt::Display for UnitRgb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_corpus_text())
    }
}

/// One corpus colour component: `digits`, or `digits '.' digits`.
///
/// Nothing else. Notably **not** an exponent form, even though the same
/// documents write `7.26504e-9` for other fields — the colour fields are simply
/// narrower than the rest of the grammar, and a decoder that widened them for
/// symmetry would be inventing tolerance the corpus does not show.
fn component(raw: &str) -> Option<f32> {
    let (whole, frac) = match raw.split_once('.') {
        Some((whole, frac)) => (whole, Some(frac)),
        None => (raw, None),
    };
    let digits_only = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits_only(whole) || frac.is_some_and(|f| !digits_only(f)) {
        return None;
    }
    raw.parse::<f32>().ok().filter(|v| v.is_finite())
}

/// Why a colour could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColorError {
    /// The text did not hold exactly three comma-separated components.
    ComponentCount {
        /// The element the text came from.
        element: String,
        /// The text as written.
        text: String,
        /// How many components were found.
        found: usize,
    },
    /// One component did not match `digits [ '.' digits ]`.
    MalformedComponent {
        /// The element the text came from.
        element: String,
        /// The whole triple as written.
        text: String,
        /// The component that failed.
        component: String,
    },
    /// A component was finite and well-formed but outside `0.0 ..= 1.0`.
    OutOfRange {
        /// The element the text came from.
        element: String,
        /// The text as written.
        text: String,
    },
}

impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ComponentCount {
                element,
                text,
                found,
            } => write!(
                f,
                "<{element}>: expected 3 comma-separated components, got {found} in {text:?}"
            ),
            Self::MalformedComponent {
                element,
                text,
                component,
            } => write!(
                f,
                "<{element}>: {component:?} in {text:?} is not `digits` or `digits.digits`"
            ),
            Self::OutOfRange { element, text } => write!(
                f,
                "<{element}>: a component of {text:?} is outside 0.0 ..= 1.0"
            ),
        }
    }
}

impl std::error::Error for ColorError {}

impl From<ColorError> for crate::error::CreatorError {
    fn from(error: ColorError) -> Self {
        Self::NotANumber {
            element: match &error {
                ColorError::ComponentCount { element, .. }
                | ColorError::MalformedComponent { element, .. }
                | ColorError::OutOfRange { element, .. } => element.clone(),
            },
            block: 0,
            value: match &error {
                ColorError::ComponentCount { text, .. }
                | ColorError::MalformedComponent { text, .. }
                | ColorError::OutOfRange { text, .. } => text.clone(),
            },
            reason: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corpus_spellings_all_parse() {
        for text in [
            "0.341176,0.360784,0.576470",
            "1,1,1",
            "0,0,0",
            "0.450979,0.450979,0.450979",
            "0.749020,0.749020,0.749020",
        ] {
            assert!(
                UnitRgb::parse(text, "color1").is_ok(),
                "{text} should parse"
            );
        }
    }

    #[test]
    fn the_spellings_the_corpus_does_not_contain_are_refused() {
        // Exponent form appears elsewhere in these documents, never in a colour.
        for text in [
            "1e-3,0,0",
            "0.5,0.5,0.5 ",
            " 0.5,0.5,0.5",
            "+0.5,0.5,0.5",
            "0.5,0.5",
            "0.5,0.5,0.5,0.5",
            "0.5.,0.5,0.5",
            ".5,0.5,0.5",
            "inf,0,0",
            "NaN,0,0",
            "1.5,0,0",
            "-0.5,0,0",
        ] {
            assert!(
                UnitRgb::parse(text, "color1").is_err(),
                "{text:?} must be refused, not repaired"
            );
        }
    }

    #[test]
    fn a_round_trip_prints_the_canonical_spelling() {
        let colour = UnitRgb::parse("0.341176,0.360784,0.576470", "skincolor1").unwrap();
        assert_eq!(colour.to_corpus_text(), "0.341176,0.360784,0.576470");
        let whole = UnitRgb::parse("1,1,1", "skincolor2").unwrap();
        assert_eq!(whole.to_corpus_text(), "1,1,1");
    }

    #[test]
    fn seven_digit_components_fold_onto_six() {
        let colour = UnitRgb::parse("0.4509791,0.5,0.5", "color1").unwrap();
        assert_eq!(colour.to_corpus_text(), "0.450979,0.500000,0.500000");
    }

    #[test]
    fn out_of_range_is_refused_never_clamped() {
        assert!(UnitRgb::new(1.5, 0.0, 0.0).is_none());
        assert!(UnitRgb::new(-0.1, 0.0, 0.0).is_none());
        assert!(UnitRgb::new(f32::NAN, 0.0, 0.0).is_none());
        assert!(UnitRgb::new(1.0, 1.0, 1.0).is_some());
    }

    #[test]
    fn equality_is_bitwise_so_a_sign_bit_is_not_a_rounding() {
        let positive = UnitRgb::new(0.0, 0.0, 0.0).unwrap();
        let negative = UnitRgb::new(-0.0, -0.0, -0.0).unwrap();
        assert_eq!(positive.red, negative.red, "IEEE says these compare equal");
        assert_ne!(positive, negative, "but they are different bytes");
    }
}
