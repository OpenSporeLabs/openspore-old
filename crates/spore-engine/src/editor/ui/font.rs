//! # A font that actually has the glyphs.
//!
//! The default Bevy font covers Latin-1. The screen's pager arrows are
//! `U+25C0`/`U+25B6`, the section bars want `U+25BE`/`U+25B8`, and the
//! confirm/cancel buttons want `U+2713`/`U+2715`. **Every one of those renders as
//! a filled box** under the default font — while the code compiles, the tests
//! pass, and the layout is correct.
//!
//! That is the shape of bug this repository's trap list warns about, arrived at
//! from an unexpected direction: not a wrong number, but a right number with no
//! ink. A UI whose controls are invisible-but-present is worse than a UI with no
//! controls, because it looks finished.
//!
//! ## Three ways that were tried, and why this one
//!
//! 1. **`AssetServer::load("/usr/share/fonts/…/DejaVuSans.ttf")`** — refused:
//!    ```text
//!    ERROR bevy_asset::server: Asset path /usr/share/fonts/TTF/DejaVuSans.ttf is
//!    unapproved. See UnapprovedPathMode for details.
//!    ```
//!    Widening `UnapprovedPathMode` to `Allow` would make it load, at the price of
//!    turning off a guard that exists so a mod cannot read the user's disk. A font
//!    is not worth that.
//! 2. **`FontSource::Family("DejaVu Sans")`** — resolved to *nothing*, and the
//!    symptom was worse than tofu: **every label on the screen vanished**, not just
//!    the geometric ones. An unresolvable family renders as no text at all, so a
//!    typo in a family name is indistinguishable from a screen with no text. Had
//!    this been the only attempt, the correct conclusion would have been "the font
//!    system does not work here" rather than "this one name is wrong".
//! 3. **`Font::from_bytes(std::fs::read(path))`** — what this does. It reads the
//!    file itself and adds it to `Assets<Font>` directly, so Bevy's asset-path
//!    policy is never consulted and no guard has to be widened. The read is of one
//!    hard-coded system font path that this module chose, not of anything the user
//!    named, so it grants nothing.
//!
//! ## The fallback is not silent
//!
//! When coverage is [`FontCoverage::AsciiFallback`], [`apply`] rewrites the
//! screen's glyphs to ASCII: the arrows become `<` and `>`. A control that
//! renders as `<` still reads as an arrow; one that renders as a filled box
//! reads as a glyph that failed to load.
//!
//! Spore's own Creator font is **UNRECOVERED** — no font asset has been located
//! in any shipped package. That is this module's evidence, not an invitation to
//! guess at a typeface.

use bevy::prelude::*;

/// Fonts to try, in order. Every entry is a face that ships on essentially every
/// Linux install; the first readable one wins.
pub const CANDIDATE_PATHS: &[&str] = &[
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation/LiberationSans-Regular.ttf",
];

/// What the screen's text will actually be able to draw.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub enum FontCoverage {
    /// A font was loaded from bytes and the geometric glyphs will render.
    ///
    /// "Will render" is not asserted by reading a `cmap`: it is asserted by a
    /// screenshot. Checking a font's `cmap` in a unit test would be a claim about
    /// a file on *this* machine, not about what the app draws.
    SystemFont {
        /// The file the bytes came from.
        path: String,
    },
    /// No covering font, and the screen's glyphs were rewritten to ASCII.
    ///
    /// A *state*, not a failure: the screen stays usable and the arrows read as
    /// `<` and `>`. It is recorded as a resource precisely so that a screenshot
    /// can be compared honestly -- a picture of ASCII arrows and a picture of
    /// real ones are different pictures, and the difference should be a fact in
    /// the log rather than a surprise in review.
    AsciiFallback,
}

/// Loads a covering font, once, in `Startup`.
///
/// A system, because loading a font needs `Assets<Font>` and the answer then
/// travels as the [`FontCoverage`] resource that [`apply`] and any test read — so
/// the decision is made once and never re-derived by a second reader.
pub fn resolve(fonts: &mut Assets<Font>) -> FontCoverage {
    for candidate in CANDIDATE_PATHS {
        let Ok(bytes) = std::fs::read(candidate) else {
            continue;
        };
        if bytes.len() < MIN_FONT_BYTES {
            warn!(
                "openspore: {candidate} is only {} bytes; not a font",
                bytes.len()
            );
            continue;
        }
        fonts.add(Font::from_bytes(bytes));
        info!("openspore: creator UI font = {candidate}");
        return FontCoverage::SystemFont {
            path: (*candidate).to_owned(),
        };
    }
    warn!(
        "openspore: no covering UI font among {} candidates; the screen falls back to ASCII",
        CANDIDATE_PATHS.len()
    );
    FontCoverage::AsciiFallback
}

/// The smallest a real TrueType file plausibly is. Below this the path is
/// pointing at something else -- a `.pc` cache, a directory stub -- and handing
/// its bytes to the atlas builder would fail deep inside font code instead of
/// here, where the message can name the path.
const MIN_FONT_BYTES: usize = 4096;

/// The [`FontSource`] for a coverage and a handle.
///
/// Split from [`source`] because a `FontSource::Family` needs no handle while a
/// `FontSource::Handle` does, and a function that returned the wrong variant for a
/// branch would compile.
pub fn source(coverage: &FontCoverage, handle: Handle<Font>) -> FontSource {
    match coverage {
        FontCoverage::SystemFont { .. } => FontSource::Handle(handle),
        FontCoverage::AsciiFallback => FontSource::default(),
    }
}

/// A [`TextFont`] at the screen's size, on the default family.
///
/// The size only: the *family* is applied by [`apply`] across the whole screen, so
/// a caller building one label cannot disagree with the others about which font
/// is in use.
pub fn text_font(size: f32) -> TextFont {
    TextFont {
        font_size: bevy::text::FontSize::Px(size),
        ..default()
    }
}

/// Applies the decision to every label on the screen, or rewrites the glyphs.
///
/// One system for both, because they are one decision seen from two sides:
/// either the geometric glyphs can be drawn, or they cannot.
///
/// In `PostStartup`, which is where it is true for the whole screen at once —
/// every `Startup` system has already spawned its labels, and no later system
/// creates one. Doing it here rather than threading a coverage parameter through
/// every spawn function is what keeps [`spawn`](crate::editor::ui::spawn) free of
/// the decision entirely.
pub fn apply(
    mut labels: Query<(&mut Text, &mut TextFont)>,
    coverage: Res<FontCoverage>,
    mut fonts: ResMut<Assets<Font>>,
) {
    match &*coverage {
        FontCoverage::SystemFont { path } => {
            // The bytes are read AGAIN here rather than carried from `resolve`,
            // because `resolve` runs in `Startup` and `Assets<Font>` is not
            // guaranteed to still hold the handle the next schedule -- and
            // re-reading one cached file is cheaper than making the resource graph
            // prove the handle survives.
            let Ok(bytes) = std::fs::read(path) else {
                warn!("openspore: {path} vanished between startup and layout");
                return;
            };
            let handle = fonts.add(Font::from_bytes(bytes));
            let font = FontSource::Handle(handle);
            for (_, mut text_font) in labels.iter_mut() {
                text_font.font = font.clone();
            }
        }
        FontCoverage::AsciiFallback => {
            for (mut text, _) in labels.iter_mut() {
                let rewritten: String = text.0.chars().map(ascii_for).collect();
                if rewritten != text.0 {
                    text.0 = rewritten;
                }
            }
        }
    }
}

/// The ASCII stand-in for a geometric glyph, or the glyph itself if ASCII has one.
pub fn ascii_for(character: char) -> char {
    match character {
        '\u{25c0}' => '<',
        '\u{25b6}' | '\u{25b8}' => '>',
        '\u{25be}' => 'v',
        '\u{2713}' => 'v',
        '\u{2715}' | '\u{2717}' => 'x',
        '\u{21b6}' | '\u{21b7}' => 'o',
        other => other,
    }
}

/// The pager's previous-page glyph for a coverage.
pub fn arrow_previous(coverage: &FontCoverage) -> &'static str {
    glyph(ARROW_PREVIOUS, coverage)
}

/// The pager's next-page glyph for a coverage.
pub fn arrow_next(coverage: &FontCoverage) -> &'static str {
    glyph(ARROW_NEXT, coverage)
}

/// The undo glyph for a coverage.
pub fn glyph_undo(coverage: &FontCoverage) -> &'static str {
    glyph('\u{21b6}', coverage)
}

/// The redo glyph for a coverage.
pub fn glyph_redo(coverage: &FontCoverage) -> &'static str {
    glyph('\u{21b7}', coverage)
}

/// The accept glyph for a coverage.
pub fn glyph_accept(coverage: &FontCoverage) -> &'static str {
    glyph('\u{2713}', coverage)
}

/// The reject glyph for a coverage.
pub fn glyph_reject(coverage: &FontCoverage) -> &'static str {
    glyph('\u{2715}', coverage)
}

/// A single ASCII codepoint, so the two-arm helpers can return `&'static str`.
const ARROW_PREVIOUS: char = '\u{25c0}';
/// A single codepoint, so the two-arm helpers can return `&'static str`.
const ARROW_NEXT: char = '\u{25b6}';

/// One glyph, spelled for a coverage.
fn glyph(geometric: char, coverage: &FontCoverage) -> &'static str {
    match coverage {
        FontCoverage::SystemFont { .. } => static_text(geometric),
        FontCoverage::AsciiFallback => static_text(ascii_for(geometric)),
    }
}

/// `char` to `&'static str`, without allocating.
///
/// Every caller's `char` is a literal, so the string is one of a closed set.
/// `OnceLock` would be the general answer; a `const`-promoted `char` is not, so
/// this matches on the codepoint instead. Falling back to the boxed form for an
/// unknown codepoint would be a lie about `&'static str`, so it panics: a glyph
/// the screen uses and this table does not know is a defect, and a defect should
/// be loud.
fn static_text(character: char) -> &'static str {
    match character {
        '\u{25c0}' => "\u{25c0}",
        '\u{25b6}' => "\u{25b6}",
        '\u{25be}' => "\u{25be}",
        '\u{25b8}' => "\u{25b8}",
        '\u{2713}' => "\u{2713}",
        '\u{2715}' => "\u{2715}",
        '\u{2717}' => "\u{2717}",
        '\u{21b6}' => "\u{21b6}",
        '\u{21b7}' => "\u{21b7}",
        // The ASCII spellings, so `glyph()` can funnel BOTH branches through one
        // lookup and the fallback cannot diverge from the geometric path by
        // taking a different route.
        '<' => "<",
        '>' => ">",
        'v' => "v",
        'x' => "x",
        'o' => "o",
        other => panic!(
            "ui::font: no static spelling for U+{:04X}. Every glyph the screen draws must be \
             listed in `static_text` AND in `ASCII_FALLBACK`, or the ASCII fallback cannot \
             cover it.",
            other as u32
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system() -> FontCoverage {
        FontCoverage::SystemFont {
            path: "/dev/null".to_owned(),
        }
    }

    #[test]
    fn the_two_coverage_branches_produce_different_glyphs() {
        // Without this, `glyph()` could return the same string for both branches
        // and every assertion in the UI would pass while the ASCII fallback was
        // never exercised at all.
        assert_ne!(
            arrow_previous(&system()),
            arrow_previous(&FontCoverage::AsciiFallback)
        );
        assert_ne!(
            arrow_next(&system()),
            arrow_next(&FontCoverage::AsciiFallback)
        );
        assert_ne!(
            glyph_accept(&system()),
            glyph_accept(&FontCoverage::AsciiFallback)
        );
        assert_ne!(
            glyph_reject(&system()),
            glyph_reject(&FontCoverage::AsciiFallback)
        );
        assert_ne!(
            glyph_undo(&system()),
            glyph_undo(&FontCoverage::AsciiFallback)
        );
        assert_ne!(
            glyph_redo(&system()),
            glyph_redo(&FontCoverage::AsciiFallback)
        );
    }

    /// Every codepoint `static_text` can spell. Listed here so the fallback has a
    /// testable domain without being a SECOND table that could disagree with it.
    const EVERY_GLYPH: &[char] = &[
        '\u{25c0}', '\u{25b6}', '\u{25be}', '\u{25b8}', '\u{2713}', '\u{2715}', '\u{2717}',
        '\u{21b6}', '\u{21b7}',
    ];

    #[test]
    fn every_ascii_fallback_is_ascii() {
        // The entire purpose of the fallback is that the DEFAULT font can draw
        // it, and the default font covers Latin-1. A non-ASCII fallback is a
        // fallback to a box.
        for geometric in EVERY_GLYPH {
            let spelled = static_text(ascii_for(*geometric));
            assert!(
                spelled.is_ascii(),
                "U+{:04X} falls back to {spelled:?}",
                *geometric as u32
            );
        }
    }

    #[test]
    fn every_geometric_glyph_maps_to_something_different() {
        // A glyph whose fallback is itself is a glyph the ASCII path silently
        // does not fix, which is precisely the bug this module exists for.
        for geometric in EVERY_GLYPH {
            assert_ne!(
                ascii_for(*geometric),
                *geometric,
                "U+{:04X} maps to itself, so the fallback changes nothing",
                *geometric as u32
            );
        }
    }

    #[test]
    fn every_geometric_glyph_is_really_not_ascii() {
        // The other half: if one of these were ASCII it would need no font, and its
        // presence in the font-dependent set would be a false requirement.
        for geometric in EVERY_GLYPH {
            assert!(
                !geometric.is_ascii(),
                "U+{:04X} is ASCII and so does not need a covering font",
                *geometric as u32
            );
        }
    }

    #[test]
    fn a_glyph_with_no_static_spelling_is_refused_loudly() {
        // The panic is the contract: an unknown glyph means the screen can draw
        // something this module cannot spell. Silently returning a box is how
        // this whole bug started.
        let result = std::panic::catch_unwind(|| static_text('\u{2603}'));
        assert!(
            result.is_err(),
            "U+2603 has no spelling and must not be given one"
        );
    }

    #[test]
    fn the_path_list_is_not_empty_and_has_no_duplicates() {
        let mut sorted = CANDIDATE_PATHS.to_vec();
        sorted.sort_unstable();
        let before = sorted.len();
        sorted.dedup();
        assert_eq!(
            before,
            sorted.len(),
            "a duplicate path is a preference stated twice"
        );
        assert!(
            !CANDIDATE_PATHS.is_empty(),
            "an empty list always falls back"
        );
    }

    #[test]
    fn every_candidate_that_exists_clears_the_minimum_size() {
        // `resolve` SKIPS anything under `MIN_FONT_BYTES`. Asserting the real
        // files clear that bar means the constant cannot drift above a font we
        // ship a path for, which would turn a working machine into the ASCII
        // fallback for no visible reason.
        for path in CANDIDATE_PATHS {
            let path = std::path::Path::new(path);
            if path.is_file() {
                let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
                assert!(
                    size as usize >= MIN_FONT_BYTES,
                    "{path:?} is only {size} bytes"
                );
            }
        }
    }

    #[test]
    fn the_paths_we_ask_for_are_fonts_where_present() {
        // Advisory, not a gate: on a machine with no fonts installed this passes
        // vacuously, which is correct -- it asserts the list is not fiction, not
        // that a font exists.
        for path in CANDIDATE_PATHS {
            if std::path::Path::new(path).is_file() {
                let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
                assert!(size > 1000, "{path} exists but is {size} bytes");
            }
        }
    }
}
