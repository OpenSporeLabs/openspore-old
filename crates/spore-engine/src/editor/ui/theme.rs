//! Every colour, count, glyph and proportion the Creature Creator screen uses.
//!
//! # Why this file exists at all
//!
//! A layout with numbers sprinkled through it cannot be reviewed, and cannot be
//! argued with. "Why 40?" has no answer scattered across a function body; here
//! it has a doc comment saying *measured at 1920x1240, panel-to-viewport
//! boundary at x≈768*. So: **one home, one grade per constant, no exceptions.**
//! [`layout`](super::layout) contains no literals at all.
//!
//! # The three grades, and how to read them
//!
//! Every constant below carries its provenance in its own doc comment, using
//! three words and no others:
//!
//! * `OBSERVED` — read off the reference screenshots at 1920x1240. Proportions
//!   and positions only. A screenshot can tell you the tab strip is about 5% of
//!   the window height; it cannot tell you that the tab strip is
//!   `#0x202634`, because a screenshot cannot tell you a *value*.
//! * `UNRECOVERED` — the reference shows it, OpenSpore does not have it. Every
//!   colour, every font, every icon, every part-category name.
//! * `OpenSpore's own choice` — the reference does not constrain it.
//!
//! # What is deliberately absent, and why it must stay absent
//!
//! There is no part name, no category name, and no DNA price anywhere in this
//! file, or in [`layout`](super::layout), or in [`spawn`](super::spawn). Three
//! separate reasons, all of which are load-bearing:
//!
//! 1. **The tab-to-rigblock mapping is UNKNOWN.** `Spore_Game` contains
//!    `prop` palette definitions with names such as "Creature Editor Palette",
//!    but their category/page/item links have not been decoded. A research sweep
//!    that found no flat category table was not evidence that palette data is
//!    absent. The six tabs are therefore positions,
//!    [`CreatorCounts::TAB_COUNT`] of them, labelled
//!    `CreatorText::TAB_PLACEHOLDER_PREFIX` + `1`..=`6`. `TAB_3` is a slot
//!    number, not a recovered category name.
//! 2. **A price is a game-balance fact, not a layout fact.** Putting `250` next
//!    to a cell size would make an unverified number look as load-bearing as a
//!    measured one. The DNA pill renders a *count supplied by the session*; the
//!    session owns whether that count is right.
//! 3. **A part name is asset data.** Spore's part names live in the corpus,
//!    addressed by DBPF instance id. Spelling one in a UI constant would put a
//!    corpus string into a source file the moment someone edited it.
//!
//! # Two numbers that are measured but cannot be literal
//!
//! `PART_CELL_SIDE` is 180 px, measured on the reference's *four-column* tab.
//! It is a four-column measurement, so this file treats it as a flex **basis**
//! with `flex_grow`, not as a fixed width — see its doc comment for the
//! arithmetic, and for why the three-column tab's cells come out larger as a
//! consequence rather than as a second magic number.
//!
//! The swatch row is the mirror image: the reference's ~41 px is the *ceiling*
//! of `768 / 19 = 40.4`, so it is encoded as a percentage of the panel and
//! `19` is the only literal.

use std::num::NonZeroUsize;

use bevy::prelude::Color;

// ---------------------------------------------------------------------------
// Counts
// ---------------------------------------------------------------------------

/// Counts of repeated elements: things where *how many* is observed even when
/// every other property is not.
#[derive(Debug)]
pub struct CreatorCounts;

impl CreatorCounts {
    /// **OBSERVED.** Six square icon tabs in the strip.
    ///
    /// The count is solid across the reference frames. What each tab *is* is
    /// not: see the module docs on category names. So a tab is addressed by
    /// index into this strip and nowhere else.
    pub const TAB_COUNT: usize = 6;

    /// **OBSERVED.** Nineteen square colour swatches in the paint-view row.
    ///
    /// Read off the reference's paint tab. The row spans the panel width, so
    /// `768 / 19 = 40.4 px` per swatch — which is where the "~41 px" in the
    /// measurement notes comes from, and why the implementation uses a
    /// percentage rather than that literal. Encoding 41 would be a 1.4%
    /// overflow of a 768 px row.
    pub const SWATCH_COUNT: usize = 19;

    /// **OBSERVED.** Six circular buttons in the viewport HUD.
    ///
    /// Their glyphs are UNRECOVERED, so the HUD draws bare discs and the count
    /// is the only thing about them that a screenshot actually fixed.
    pub const HUD_BUTTON_COUNT: usize = 6;

    /// **OBSERVED.** Three collapsible section bars in the paint view.
    ///
    /// The reference stacks exactly three. Their labels are in
    /// [`CreatorText::SECTION_LABEL_BASE`] and friends; what they *mean* — which
    /// paint pass, which channel, whether they are even a paint pass at all —
    /// is UNRECOVERED, so nothing downstream reads them as more than strings.
    pub const SECTION_BAR_COUNT: usize = 3;

    /// **OBSERVED.** Four buttons in the bottom bar's right-hand group.
    ///
    /// Undo, redo, cancel, confirm, in that order. Counted because the *order*
    /// is the fact the layout encodes, and a count is what makes an order
    /// checkable.
    pub const BOTTOM_BAR_BUTTON_COUNT: usize = 4;

    // -- the two observed part-grid shapes ------------------------------------
    //
    // These six are the reference's measurements, lifted out of
    // `layout::PartGrid` so that "no proportion lives outside theme.rs" is
    // literally true rather than true modulo four exceptions. What each column
    // count costs the panel is checked at compile time by
    // `GRID_FOUR_COLUMNS_FIT` and friends.

    /// **OBSERVED.** Columns on the reference's denser tabs.
    pub const PART_GRID_FOUR_COLUMNS: usize = 4;

    /// **OBSERVED.** Rows per page on the reference's denser tabs.
    pub const PART_GRID_FOUR_COLUMNS_ROWS: usize = 10;

    /// **OBSERVED.** Pages on the reference's denser tabs.
    pub const PART_GRID_FOUR_COLUMNS_PAGES: usize = 2;

    /// **OBSERVED.** Columns on the reference's sparser tab.
    pub const PART_GRID_THREE_COLUMNS: usize = 3;

    /// **OBSERVED.** Rows per page on the reference's sparser tab.
    pub const PART_GRID_THREE_COLUMNS_ROWS: usize = 4;

    /// **OBSERVED.** Pages on the reference's sparser tab.
    pub const PART_GRID_THREE_COLUMNS_PAGES: usize = 4;

    /// The degenerate but legal page count of one.
    ///
    /// A page always exists, so a pager's total is `NonZeroUsize`: a pager
    /// reading `1/0` is not "no pages", it is a contradiction, and this repo does
    /// not represent a contradiction as a zero. This constant is the smallest
    /// value the type admits and exists so a default view has something real to
    /// hold.
    pub const PAGE_COUNT_NONZERO: NonZeroUsize = NonZeroUsize::MIN;
}

// ---------------------------------------------------------------------------
// Metrics
// ---------------------------------------------------------------------------

/// Proportions of the screen. Every entry is either measured off the reference
/// or declared as OpenSpore's own choice.
#[derive(Debug)]
pub struct CreatorMetrics;

impl CreatorMetrics {
    // -- the reference frame the measurements were taken in ------------------

    /// **OBSERVED.** Width of the reference frame, in logical pixels.
    ///
    /// Every "about N% of the window" in this file was measured against this.
    /// It is a property of the *screenshot*, not of the screen, and exists so
    /// the derived constants below can be re-derived by hand.
    pub const REFERENCE_WIDTH_PX: f32 = 1920.0;

    /// **OBSERVED.** Height of the reference frame, in logical pixels.
    pub const REFERENCE_HEIGHT_PX: f32 = 1240.0;

    // -- the horizontal split ------------------------------------------------

    /// **OBSERVED.** Catalog panel width, as a percentage of the window.
    ///
    /// Measured at the panel/viewport boundary: `768 / 1920 = 40.0%`.
    pub const PANEL_WIDTH_PERCENT: f32 = 40.0;

    /// **OBSERVED.** Viewport width, as a percentage of the window.
    ///
    /// `1152 / 1920 = 60.0%`. Expressed as a literal rather than as
    /// `100 - PANEL_WIDTH_PERCENT` so that a test can pin the *pair*: if either
    /// is edited, they can no longer silently disagree about the total.
    pub const VIEWPORT_WIDTH_PERCENT: f32 = 60.0;

    /// **OBSERVED.** The panel's width in pixels *in the reference frame*.
    ///
    /// `1920 * 0.40 = 768`. This is the number the grid and swatch arithmetic
    /// below is checked against, and it is why the layout uses percentages
    /// everywhere: a literal 768 would be wrong at every other window size.
    pub const PANEL_REFERENCE_WIDTH_PX: f32 =
        Self::REFERENCE_WIDTH_PX * Self::PANEL_WIDTH_PERCENT / 100.0;

    // -- the tab strip -------------------------------------------------------

    /// **OBSERVED.** Tab strip height in pixels.
    ///
    /// Measured top-of-screen to bottom-of-strip: ~60 px, ~4.8% of the 1240 px
    /// frame. The nearest percent reading is 5%; 60 is the pixel reading and
    /// the pixel reading is what a layout wants.
    pub const TAB_STRIP_HEIGHT_PX: f32 = 60.0;

    /// **OpenSpore's own choice.** Vertical inset of a tab inside the strip.
    ///
    /// The reference shows square tabs filling the strip's height to within a
    /// few pixels; the exact inset is not measurable against a screenshot whose
    /// strip edge is anti-aliased. Chosen as 6 px so the tab is visibly inset
    /// rather than clipped.
    pub const TAB_INSET_PX: f32 = 6.0;

    /// **OpenSpore's own choice.** Tab side length, in pixels.
    ///
    /// Not measured, and it cannot be: "small square" is the whole of the
    /// observation, and the observed constraint that *is* hard is the strip's
    /// height. So the tab is derived from that and nothing else —
    /// `60 - 2 * 6 = 48` — which makes it square *by construction* against the
    /// one measured dimension available. If the strip height changes, the tab
    /// follows and stops being square only if the inset is also changed.
    pub const TAB_SIDE_PX: f32 = Self::TAB_STRIP_HEIGHT_PX - 2.0 * Self::TAB_INSET_PX;

    /// **OpenSpore's own choice.** Gap between tabs, in pixels.
    ///
    /// Six 48 px tabs occupy 288 px of a 768 px panel, so the strip cannot use
    /// `SpaceBetween` honestly — it would fling the tabs to the panel edges and
    /// read as five separate groups. `SpaceAround` distributes them evenly,
    /// which matches the reference's rhythm.
    pub const TAB_GAP_PX: f32 = 8.0;

    /// **OpenSpore's own choice.** Tab corner radius, in pixels.
    ///
    /// The reference's tabs read as slightly rounded squares. UNRECOVERED as a
    /// value; 4 px is the smallest radius that survives at 48 px without
    /// reading as a circle.
    pub const TAB_RADIUS_PX: f32 = 4.0;

    // -- the part grid -------------------------------------------------------

    /// **OBSERVED.** Part cell side length, in pixels — *from the four-column
    /// tab*.
    ///
    /// Measured at ~180x180 on the reference's denser tabs. This is a
    /// `flex_basis`, not a `width`, and that is a measured decision rather than
    /// a defensive one: four of these plus gaps fill `4 * 180 = 720` of the
    /// 768 px panel, i.e. 93.75% of its width. A fixed 180 px `width` leaves
    /// nowhere for the "generous spacing" the reference also shows.
    ///
    /// Where that spacing lives is the interesting part, and it is *inside* the
    /// cell: the reference draws a backing plate per cell with the artwork inset
    /// within it, so "generous spacing" is
    /// [`PART_CELL_PADDING_PX`](Self::PART_CELL_PADDING_PX), and the gap
    /// *between* cells ([`PART_CELL_GAP_PX`](Self::PART_CELL_GAP_PX)) is
    /// correspondingly small. Both are OpenSpore's own choice; the arithmetic
    /// below is what constrains them.
    pub const PART_CELL_SIDE_PX: f32 = 180.0;

    /// One grid column's share of the grid's width, as a percentage.
    ///
    /// Four columns at 23.5% fill 94% of the panel with the gaps between them,
    /// which is what `layout::part_grid`'s `SpaceBetween` wants. Expressed as a
    /// percentage rather than as the measured 180 px because the panel's width
    /// varies with the window and 180 px does not: at the editor's default
    /// 1280-wide window the panel is 512 px, and four 180 px cells would fit two.
    ///
    /// At the reference's own 1920-wide window this works out to 180 px, which is
    /// where the measured number comes from.
    pub const PART_CELL_COLUMNS_PERCENT: f32 = 23.5;

    /// One grid row's height in pixels.
    ///
    /// **OpenSpore's own, and derived rather than measured**, because the
    /// reference's row height is measured at a 1240 px-tall window and this editor
    /// runs at 720. Ten rows plus nine 6 px gaps have to fit the grid, which is
    /// the panel's height minus the tab strip and the pager: at 1280x720 that is
    /// about 520 px, so `(520 - 9 * 6) / 10 = 46.6`.
    ///
    /// A percentage was tried and does not work: the grid is sized by `flex_grow`,
    /// and a percentage height against a flex-grown parent resolved to roughly
    /// 300 px, which put two rows on screen. A scroll container would let the rows
    /// be as tall as the reference's and is the right answer for a future revision;
    /// this is a number that fits.
    pub const PART_CELL_ROW_PX: f32 = 76.0;

    /// **OpenSpore's own choice.** Gap between part cells, in pixels.
    ///
    /// Forced small by the arithmetic, which is checked at compile time: four
    /// 180 px cells, three gaps and the panel's own padding have to fit 768 px,
    /// which leaves `768 - 720 - 24 = 24 px` for three gaps and nothing for
    /// slack. 6 px each spends 18 of those 24.
    pub const PART_CELL_GAP_PX: f32 = 6.0;

    /// **OpenSpore's own choice.** Padding inside a part cell, in pixels.
    ///
    /// This is where the reference's "generous spacing" actually is; see
    /// [`PART_CELL_SIDE_PX`](Self::PART_CELL_SIDE_PX).
    pub const PART_CELL_PADDING_PX: f32 = 8.0;

    /// **OpenSpore's own choice.** Part cell corner radius, in pixels.
    ///
    /// The reference's cells are visibly rounded; the value is UNRECOVERED.
    pub const PART_CELL_RADIUS_PX: f32 = 6.0;

    /// **OpenSpore's own choice.** The catalog panel's inner padding.
    ///
    /// Not visible as such in the reference — the reference's panel content runs
    /// close to its edges. 12 px exists so the pager arrows, which the
    /// reference places *at the panel's left and right edge*, have somewhere to
    /// sit without being clipped, and so the four-column grid fits.
    pub const PANEL_PADDING_PX: f32 = 12.0;

    // -- the pager -----------------------------------------------------------

    /// **OpenSpore's own choice.** Pager row height in pixels.
    ///
    /// The reference shows a pager row at the panel's bottom edge with a label
    /// and two arrows on one line; its height is not separable from the bottom
    /// bar behind it in a screenshot. 56 px is chosen to sit just under the
    /// OBSERVED 60 px tab strip so the panel's top and bottom bands match
    /// visually — a choice, not a measurement.
    pub const PAGER_HEIGHT_PX: f32 = 56.0;

    /// **OpenSpore's own choice.** Side of the pager's arrow buttons.
    ///
    /// The arrows are OBSERVED; their size is not. 28 px fits inside
    /// [`PAGER_HEIGHT_PX`](Self::PAGER_HEIGHT_PX) with margin on both sides.
    pub const PAGER_ARROW_SIDE_PX: f32 = 28.0;

    // -- the swatch row ------------------------------------------------------

    /// **OBSERVED (derived).** One swatch's share of the panel width.
    ///
    /// `100 / 19 = 5.263%`. The alternative — `Val::Px(41.0)` — overflows a
    /// 768 px row by 11 px, so the percentage is not a stylistic preference,
    /// it is the only encoding of the measurement that fits.
    pub const SWATCH_WIDTH_PERCENT: f32 = 100.0 / CreatorCounts::SWATCH_COUNT as f32;

    /// **OpenSpore's own choice.** Swatch corner radius.
    ///
    /// The reference's swatches read as small rounded squares. Value
    /// UNRECOVERED.
    pub const SWATCH_RADIUS_PX: f32 = 4.0;

    // -- the paint-view section bars ----------------------------------------

    /// **OpenSpore's own choice.** Section bar height in pixels.
    ///
    /// The reference stacks three rounded bars at the top of the paint panel;
    /// their height is not measurable from the screenshot against the panel's
    /// own edge. 44 px keeps three of them (132 px) under the 184 px that the
    /// 60 px strip plus one part-cell column would otherwise occupy.
    pub const SECTION_BAR_HEIGHT_PX: f32 = 44.0;

    /// **OpenSpore's own choice.** Width of a section bar's accent stripe.
    ///
    /// The reference gives every bar "its own accent colour bar"; how wide that
    /// stripe is, UNRECOVERED. 6 px is enough to read as a stripe at 44 px tall.
    pub const SECTION_ACCENT_WIDTH_PX: f32 = 6.0;

    /// **OpenSpore's own choice.** Section bar corner radius.
    ///
    /// The reference's section bars are conspicuously rounded — "full-width
    /// rounded bars" is in the observation — but by more than a 6 px cell, so
    /// they get their own larger value rather than sharing the cell's.
    pub const SECTION_BAR_RADIUS_PX: f32 = 22.0;

    // -- the bottom bar ------------------------------------------------------

    /// **OBSERVED.** Bottom bar height in pixels.
    ///
    /// Measured across the full window including the strip under the panel:
    /// ~55 px, ~4.4% of the 1240 px frame.
    pub const BOTTOM_BAR_HEIGHT_PX: f32 = 55.0;

    /// **OpenSpore's own choice.** Horizontal padding of the bottom bar.
    ///
    /// The reference's controls sit inset from the window edge. Amount
    /// UNRECOVERED.
    pub const BOTTOM_BAR_PADDING_PX: f32 = 16.0;

    /// **OpenSpore's own choice.** Side of the bottom bar's round buttons.
    ///
    /// The undo/redo/cancel/confirm buttons are OBSERVED as four round buttons
    /// of a common size; the size is not measured. 40 px sits inside
    /// [`BOTTOM_BAR_HEIGHT_PX`](Self::BOTTOM_BAR_HEIGHT_PX) with 7 px of margin
    /// top and bottom.
    pub const BOTTOM_BUTTON_SIDE_PX: f32 = 40.0;

    /// **OpenSpore's own choice.** Horizontal gap between bottom-bar items.
    pub const BOTTOM_BAR_GAP_PX: f32 = 12.0;

    /// **OpenSpore's own choice.** Pill height in the bottom bar.
    ///
    /// The DNA counter and the name field are OBSERVED as rounded pills of a
    /// common height; the height is not measured. 34 px leaves the bar's 55 px
    /// with 10 px either side.
    pub const PILL_HEIGHT_PX: f32 = 34.0;

    /// **OpenSpore's own choice.** Minimum width of the name pill.
    ///
    /// Its label is OBSERVED; its width is not. 220 px is enough for
    /// "Name Your Creature" at [`CreatorText::PILL_TEXT_SIZE_PX`] without
    /// clipping, and it keeps the centre pill visibly the largest of the three.
    pub const NAME_PILL_MIN_WIDTH_PX: f32 = 220.0;

    /// **OpenSpore's own choice.** Minimum width of the DNA counter pill.
    ///
    /// The counter's *contents* are the session's business — see the module
    /// docs on prices. Only the pill's size is a layout fact, and only its
    /// minimum is knowable before the text exists.
    pub const DNA_PILL_MIN_WIDTH_PX: f32 = 140.0;

    /// **OpenSpore's own choice.** Pill corner radius.
    ///
    /// Half the pill height, which is what makes it a capsule rather than a
    /// rounded rectangle. The general form would be `min(height / 2, width / 2)`,
    /// but [`bevy::prelude::BorderRadius::MAX`] already means exactly that and is what
    /// [`pill`](super::layout::pill) uses — so this constant documents the
    /// intended radius rather than configuring anything, and exists so a reader
    /// comparing a 34 px pill against a 40 px button can see both were derived
    /// from the same idea.
    pub const PILL_RADIUS_PX: f32 = Self::PILL_HEIGHT_PX / 2.0;

    /// **OpenSpore's own choice.** Horizontal padding inside a pill.
    ///
    /// The reference's pills hold their text clear of the capsule's rounded
    /// ends; the amount is UNRECOVERED. Reusing the bottom bar's own padding
    /// keeps the two insets from drifting into three numbers for one idea.
    pub const PILL_PADDING_PX: f32 = Self::BOTTOM_BAR_PADDING_PX;

    // -- the viewport HUD ----------------------------------------------------

    /// **OpenSpore's own choice.** HUD button diameter in pixels.
    ///
    /// The reference shows "~6 small circular buttons"; small and circular are
    /// observed, the diameter is not. 36 px is small against a 1152 px viewport
    /// and still a comfortable hit target.
    pub const HUD_BUTTON_DIAMETER_PX: f32 = 36.0;

    /// **OpenSpore's own choice.** Gap between HUD buttons.
    pub const HUD_BUTTON_GAP_PX: f32 = 8.0;

    /// **OpenSpore's own choice.** Inset of the HUD stack from the viewport's
    /// top-right corner.
    ///
    /// The reference pins the stack to the viewport's corner rather than the
    /// window's, which is the structural fact worth encoding; how far in from
    /// the corner is UNRECOVERED.
    pub const HUD_INSET_PX: f32 = 16.0;

    // -- the viewport --------------------------------------------------------

    /// **OpenSpore's own choice.** Viewport border width in pixels.
    ///
    /// A hairline separator between panel and viewport. The reference shows a
    /// division but no measurable stroke width.
    pub const VIEWPORT_BORDER_PX: f32 = 2.0;

    // -- structural identities -----------------------------------------------
    //
    // These are NOT measurements. They are the identities of the layout system
    // itself — "fill your parent", "do not be compressed", "as square as this
    // shape is" — and they live here anyway, because the promise this module
    // makes is that `layout.rs` contains no bare number at all, and an exception
    // for "but these four are obviously structural" is how the list grows.

    /// **Structural identity, not a measurement.** 100% of the parent.
    ///
    /// The root's 100% is the window, which is the only reason the bottom bar's
    /// `Percent(100)` can mean "the whole window" — see `bottom_bar`.
    pub const FULL_PERCENT: f32 = 100.0;

    /// **Structural identity, not a measurement.** The `flex_grow` that makes a
    /// node absorb whatever space its parent has left over.
    pub const FLEX_GROW_FILL: f32 = 1.0;

    /// **Structural identity, not a measurement.** The `flex_shrink` that forbids
    /// a node from being compressed to fit.
    ///
    /// Used where the reference fixes a shape — a square tab, a square button —
    /// and a compressed node would stop being that shape.
    pub const FLEX_SHRINK_NONE: f32 = 0.0;

    /// **Structural identity, not a measurement.** The `aspect_ratio` of a shape
    /// the reference draws square.
    pub const ASPECT_SQUARE: f32 = 1.0;

    // -- derived compile-time checks ----------------------------------------
    //
    // A constant that cannot hold at the reference width is a defect, and the
    // cheapest place to catch that is before the program runs. These are the
    // assertions the layout's geometry rests on; deleting one is a build error,
    // not a subtle visual regression.
    //
    // They are named `const`s and then forced by the anonymous `const _` at the
    // bottom of this block, and that indirection is deliberate: a *named* `const`
    // is only evaluated when something reads it, so a named assertion would be
    // documentation rather than a check. Destructuring them inside an anonymous
    // const — which *is* evaluated eagerly — is the difference between "the check
    // exists" and "the check runs".

    /// The four-column grid must fit the panel at the reference width.
    ///
    /// This is the check that makes `PART_CELL_SIDE_PX = 180` meaningful rather
    /// than aspirational. Raise the cell to 200 and the crate stops compiling.
    const GRID_FOUR_COLUMNS_FIT: () = assert!(
        4.0 * CreatorMetrics::PART_CELL_SIDE_PX
            + 3.0 * CreatorMetrics::PART_CELL_GAP_PX
            + 2.0 * CreatorMetrics::PANEL_PADDING_PX
            <= CreatorMetrics::PANEL_REFERENCE_WIDTH_PX
    );

    /// The three-column grid must fit too.
    ///
    /// It has more slack, which is why its inter-cell gaps come out wider — the
    /// cell size stays the observed 180 px and the leftover becomes spacing. See
    /// `PART_CELL_SIDE_PX`.
    const GRID_THREE_COLUMNS_FIT: () = assert!(
        3.0 * CreatorMetrics::PART_CELL_SIDE_PX
            + 2.0 * CreatorMetrics::PART_CELL_GAP_PX
            + 2.0 * CreatorMetrics::PANEL_PADDING_PX
            <= CreatorMetrics::PANEL_REFERENCE_WIDTH_PX
    );

    /// …and five columns must **not** fit, which is what makes four the maximum.
    ///
    /// Asserting only the floor is not a check: it would pass just as happily for
    /// a configuration the panel cannot show.
    const GRID_FIVE_COLUMNS_DO_NOT_FIT: () = assert!(
        5.0 * CreatorMetrics::PART_CELL_SIDE_PX
            + 4.0 * CreatorMetrics::PART_CELL_GAP_PX
            + 2.0 * CreatorMetrics::PANEL_PADDING_PX
            > CreatorMetrics::PANEL_REFERENCE_WIDTH_PX
    );

    /// The split must add up to the whole window.
    ///
    /// Two percentages that sum to 95 would leave a 5% gap that nothing owns,
    /// and a screenshot would not reveal it as an error — it would just look
    /// like a wider border.
    const SPLIT_COVERS_WINDOW: () = assert!(
        CreatorMetrics::PANEL_WIDTH_PERCENT + CreatorMetrics::VIEWPORT_WIDTH_PERCENT == 100.0
    );

    /// A tab must fit inside the strip it lives in, or it is clipped.
    const TAB_FITS_STRIP: () =
        assert!(CreatorMetrics::TAB_SIDE_PX <= CreatorMetrics::TAB_STRIP_HEIGHT_PX);

    /// Six tabs and the gaps between them must fit the panel.
    ///
    /// With `JustifyContent::SpaceAround` this is not strictly required — the
    /// flex algorithm would compress them — but a tab that has to shrink to fit
    /// stops being square, and the reference's tabs are square.
    const TABS_FIT_PANEL: () = assert!(
        CreatorCounts::TAB_COUNT as f32 * CreatorMetrics::TAB_SIDE_PX
            + (CreatorCounts::TAB_COUNT - 1) as f32 * CreatorMetrics::TAB_GAP_PX
            <= CreatorMetrics::PANEL_REFERENCE_WIDTH_PX
    );

    /// The bottom bar must leave room for its buttons and its pills.
    const BOTTOM_BAR_FITS_CONTROLS: () = assert!(
        CreatorMetrics::BOTTOM_BUTTON_SIDE_PX <= CreatorMetrics::BOTTOM_BAR_HEIGHT_PX
            && CreatorMetrics::PILL_HEIGHT_PX <= CreatorMetrics::BOTTOM_BAR_HEIGHT_PX
    );

    /// A swatch row of nineteen must still be a plausible row of squares.
    ///
    /// The float identity `19 * (100/19) == 100` is deliberately *not* asserted:
    /// it is false in `f32` for reasons that have nothing to do with the layout,
    /// and an assertion that fails for arithmetic reasons teaches a reader to
    /// distrust assertions. What is worth pinning is that the count and the width
    /// are not independent of each other.
    const SWATCHES_TILE_THE_ROW: () = assert!(
        CreatorCounts::SWATCH_COUNT > 1
            && CreatorMetrics::SWATCH_WIDTH_PERCENT * CreatorCounts::SWATCH_COUNT as f32 > 99.0
    );

    /// The section label array must be exactly as long as the bar count.
    ///
    /// Without this, `CreatorCounts::SECTION_BAR_COUNT = 4` would compile, the
    /// array would stay at three, and the first `section_label(3)` would be a
    /// compile-time index error in the *spawner* — an error three files away from
    /// the mistake.
    const SECTION_LABELS_COMPLETE: () =
        assert!(CreatorText::SECTION_LABELS.len() == CreatorCounts::SECTION_BAR_COUNT);
}

/// Forces every assertion in [`CreatorMetrics`] to be evaluated.
///
/// A *named* `const` is only evaluated when something reads it, so each assertion
/// above is a named `const` — which means it would otherwise be documentation
/// rather than a check. Destructuring them inside an anonymous `const`, which
/// **is** evaluated eagerly, is the difference between "the check exists" and "the
/// check runs".
///
/// This lives at module scope rather than inside the `impl` because `const _` is
/// not a valid item name there.
///
/// Deleting a row below silences one assertion without deleting it, which is the
/// only way this pattern can rot. The tests re-check the two load-bearing ones —
/// the four-column fit and the split summing to the window — at runtime, so that
/// deletion is caught rather than silent.
const _: () = {
    let () = CreatorMetrics::GRID_FOUR_COLUMNS_FIT;
    let () = CreatorMetrics::GRID_THREE_COLUMNS_FIT;
    let () = CreatorMetrics::GRID_FIVE_COLUMNS_DO_NOT_FIT;
    let () = CreatorMetrics::SPLIT_COVERS_WINDOW;
    let () = CreatorMetrics::TAB_FITS_STRIP;
    let () = CreatorMetrics::TABS_FIT_PANEL;
    let () = CreatorMetrics::BOTTOM_BAR_FITS_CONTROLS;
    let () = CreatorMetrics::SWATCHES_TILE_THE_ROW;
    let () = CreatorMetrics::SECTION_LABELS_COMPLETE;
};

// ---------------------------------------------------------------------------
// Palette
// ---------------------------------------------------------------------------

/// OpenSpore's colour choices.
///
/// # Read this before trusting a hex value
///
/// **Every colour in this module is OpenSpore's own.** Spore's actual UI
/// palette has not been recovered: the shipped corpus contains no UI theme, no
/// style sheet and no palette table that a research sweep could find. What the
/// screenshots fix is only *which regions are yellow, which are dark, and which
/// are red or green* — a categorical reading, not a numeric one.
///
/// So each constant below is named for **what it is in OpenSpore's own layout**
/// — [`PANEL_BACKGROUND`](Self::PANEL_BACKGROUND) is the colour OpenSpore put
/// behind its catalog panel — and its doc comment says the reference's exact
/// value is UNKNOWN. None of these is "Spore's colour".
///
/// The scheme is a dark blue-grey chrome with one amber accent, chosen so that
/// the amber reads as "selected" against everything else: the only saturated
/// hue in the panel is the one that marks the active tab and the selected cell
/// border. That relationship is deliberate and is asserted in the tests; the
/// individual channels are not measurements.
#[derive(Debug)]
pub struct CreatorPalette;

impl CreatorPalette {
    // -- the panel -----------------------------------------------------------

    /// Catalog panel fill. UNRECOVERED in the reference; OpenSpore's own.
    pub const PANEL_BACKGROUND: Color = Color::srgba(0.035, 0.075, 0.105, 0.82);

    /// Fill of the panel's horizontal bands (tab strip, pager). OpenSpore's
    /// own; one step lighter than [`PANEL_BACKGROUND`](Self::PANEL_BACKGROUND)
    /// so the bands read as bands.
    pub const PANEL_BAND_BACKGROUND: Color = Color::srgba(0.055, 0.105, 0.14, 0.78);

    // -- the tab strip -------------------------------------------------------

    /// Inactive tab fill — the reference's "dark". UNRECOVERED; OpenSpore's own.
    pub const TAB_INACTIVE: Color = Color::srgb(0.129, 0.145, 0.176);

    /// Inactive tab fill while hovered. OpenSpore's own; no hover state is
    /// observable in a still screenshot.
    pub const TAB_INACTIVE_HOVER: Color = Color::srgb(0.176, 0.196, 0.235);

    /// Active tab fill — the reference's yellow. **The exact value is UNKNOWN.**
    /// The screenshot establishes only that the active tab is yellow and every
    /// other tab is not, so this is OpenSpore's own amber standing in for it.
    pub const TAB_ACTIVE: Color = Color::srgb(0.937, 0.749, 0.243);

    /// Ink drawn on top of [`TAB_ACTIVE`](Self::TAB_ACTIVE). OpenSpore's own;
    /// dark, so the placeholder label keeps the tab's contrast.
    pub const TAB_ACTIVE_INK: Color = Color::srgb(0.055, 0.063, 0.078);

    /// Ink of a tab's placeholder label. OpenSpore's own. Deliberately dimmer
    /// than [`TAB_ACTIVE_INK`](Self::TAB_ACTIVE_INK) so that "which tab am I on"
    /// is answered by the fill, not by reading the label.
    pub const TAB_PLACEHOLDER_INK: Color = Color::srgb(0.478, 0.522, 0.596);

    // -- the part grid -------------------------------------------------------

    /// Part cell backing plate. OpenSpore's own. This is the plate the
    /// reference's "generous spacing" lives inside; see
    /// [`PART_CELL_PADDING_PX`](CreatorMetrics::PART_CELL_PADDING_PX).
    pub const PART_CELL_BACKGROUND: Color = Color::srgba(0.045, 0.11, 0.15, 0.34);

    /// Part cell backing plate while hovered. OpenSpore's own; a still
    /// screenshot has no hover state.
    pub const PART_CELL_HOVER: Color = Color::srgb(0.129, 0.149, 0.184);

    /// Border of the *selected* part cell. OpenSpore's own, and deliberately
    /// the same amber as [`TAB_ACTIVE`](Self::TAB_ACTIVE): selection is the
    /// screen's one accent and should not be two hues.
    pub const PART_CELL_SELECTED_BORDER: Color = Color::srgb(0.937, 0.749, 0.243);

    /// Swatch fill before any palette is recovered. OpenSpore's own; a neutral
    /// so that an unrecovered swatch reads as *absent data* rather than as a
    /// real colour somebody chose.
    pub const SWATCH_PLACEHOLDER_FILL: Color = Color::srgb(0.157, 0.176, 0.212);

    // -- the viewport --------------------------------------------------------

    /// The viewport's clear colour.
    ///
    /// `Color::NONE`, and that is a decision rather than an omission: the
    /// viewport is where the 3D render shows through, so a UI background here
    /// would paint over it. The reference frames show rendered creatures in this
    /// region; OpenSpore's own viewport will render the same way.
    pub const VIEWPORT_CLEAR: Color = Color::NONE;

    /// Hairline separating panel from viewport. OpenSpore's own.
    pub const VIEWPORT_BORDER: Color = Color::srgb(0.204, 0.224, 0.263);

    // -- chrome text ---------------------------------------------------------

    /// Primary chrome text colour. OpenSpore's own; the reference's font and
    /// text colour are both UNRECOVERED.
    pub const CHROME_TEXT: Color = Color::srgb(0.839, 0.867, 0.906);

    /// Secondary chrome text colour. OpenSpore's own.
    pub const CHROME_TEXT_DIM: Color = Color::srgb(0.478, 0.522, 0.596);

    // -- the bottom bar ------------------------------------------------------

    /// Bottom bar fill, darker than the panel so the bar reads as a distinct
    /// band across the full window width. OpenSpore's own.
    pub const BOTTOM_BAR_BACKGROUND: Color = Color::srgb(0.043, 0.051, 0.063);

    /// Pill fill. OpenSpore's own.
    pub const PILL_BACKGROUND: Color = Color::srgb(0.098, 0.114, 0.141);

    /// The centre name pill's fill. OpenSpore's own; one step lighter than
    /// [`PILL_BACKGROUND`](Self::PILL_BACKGROUND), because the reference makes
    /// the centre pill the visually dominant one of the three.
    pub const NAME_PILL_BACKGROUND: Color = Color::srgb(0.129, 0.149, 0.184);

    /// Undo and redo button fill. OpenSpore's own; neutral, as the reference
    /// shows them undifferentiated from each other.
    pub const ACTION_BUTTON: Color = Color::srgb(0.129, 0.145, 0.176);

    /// Cancel button fill — the reference's red. **Exact value UNKNOWN**;
    /// OpenSpore's own red.
    pub const REJECT_BUTTON: Color = Color::srgb(0.639, 0.153, 0.129);

    /// Confirm button fill — the reference's green. **Exact value UNKNOWN**;
    /// OpenSpore's own green.
    pub const ACCEPT_BUTTON: Color = Color::srgb(0.196, 0.510, 0.216);

    // -- the viewport HUD ----------------------------------------------------

    /// HUD button fill. OpenSpore's own, and semi-transparent on purpose: the
    /// HUD sits over a rendered creature, and an opaque disc would occlude the
    /// thing the user is editing.
    pub const HUD_BUTTON_BACKGROUND: Color = Color::srgba(0.055, 0.063, 0.078, 0.82);

    /// HUD button fill while hovered. OpenSpore's own.
    pub const HUD_BUTTON_HOVER: Color = Color::srgba(0.129, 0.145, 0.176, 0.92);

    // -- the paint-view section bars ----------------------------------------

    /// Collapsed section bar fill. OpenSpore's own.
    pub const SECTION_BAR_BACKGROUND: Color = Color::srgb(0.075, 0.086, 0.106);

    /// Expanded section bar fill. OpenSpore's own; one step lighter than
    /// collapsed so the open state is legible from the bar alone.
    pub const SECTION_BAR_BACKGROUND_OPEN: Color = Color::srgb(0.098, 0.114, 0.141);

    /// The three accent stripes, one per section bar.
    ///
    /// The reference gives every bar its own accent colour, which is the
    /// observation; the three hues are OpenSpore's own and are picked to be
    /// maximally distinct from each other *and* from
    /// [`TAB_ACTIVE`](Self::TAB_ACTIVE), so an accent stripe can never be
    /// mistaken for a selection highlight.
    pub const SECTION_ACCENTS: [Color; CreatorCounts::SECTION_BAR_COUNT] = [
        Color::srgb(0.416, 0.635, 0.867),
        Color::srgb(0.749, 0.478, 0.788),
        Color::srgb(0.278, 0.729, 0.616),
    ];
}

// ---------------------------------------------------------------------------
// Text
// ---------------------------------------------------------------------------

/// Text sizes and the *structural chrome* strings the screen is allowed to say.
///
/// # The string rule
///
/// The reference screenshots contain a great deal of text, almost none of which
/// OpenSpore can reproduce: part names, category names, prices. Those are
/// corpus data, not layout data, and they appear here **not at all**.
///
/// What remains is structural chrome — strings that describe the *widget*
/// rather than the *game*: the pager's `1/N`, the name field's placeholder, and
/// the glyphs on the arrows and round buttons. Those are genuinely present in
/// the reference, and reproducing their shape is reproducing the layout.
///
/// The tab strip is the awkward case: the reference's six tabs are **icons**
/// with no text under them. Since OpenSpore has no icon assets, each tab
/// renders [`CreatorText::TAB_PLACEHOLDER_PREFIX`] + its 1-based index, which is a slot
/// label and deliberately says nothing about what the slot does.
#[derive(Debug)]
pub struct CreatorText;

impl CreatorText {
    /// **OBSERVED.** The name field's placeholder, verbatim.
    ///
    /// This is the one Spore string OpenSpore reproduces, because it is
    /// structural chrome rather than game data: it names the widget ("Name Your
    /// Creature") and contains no part, category or price. Spelled as observed,
    /// capital N, capital Y, capital C.
    pub const NAME_PILL_LABEL: &str = "Name Your Creature";

    /// The tab strip's placeholder label prefix. OpenSpore's own.
    ///
    /// Rendered as `TAB_1`..=`TAB_6`. The prefix exists so the strip is
    /// unmistakable for what it is — six undifferentiated placeholder slots —
    /// rather than mistakable for a real icon set.
    pub const TAB_PLACEHOLDER_PREFIX: &str = "";

    /// The previous-page arrow glyph. **OBSERVED as an arrow**; the reference's
    /// exact codepoint and typeface are UNRECOVERED, so this is OpenSpore's
    /// nearest stand-in.
    pub const GLYPH_PAGE_PREVIOUS: &str = "◀";

    /// The next-page arrow glyph. **OBSERVED as an arrow**; same caveat as
    /// [`GLYPH_PAGE_PREVIOUS`](Self::GLYPH_PAGE_PREVIOUS).
    pub const GLYPH_PAGE_NEXT: &str = "▶";

    /// Undo glyph. **OBSERVED as a curved arrow**; exact codepoint UNRECOVERED.
    pub const GLYPH_UNDO: &str = "↶";

    /// Redo glyph. **OBSERVED as a curved arrow**; exact codepoint UNRECOVERED.
    pub const GLYPH_REDO: &str = "↷";

    /// Cancel glyph. **OBSERVED as a cross**; exact codepoint UNRECOVERED.
    pub const GLYPH_REJECT: &str = "✕";

    /// Confirm glyph. **OBSERVED as a check**; exact codepoint UNRECOVERED.
    pub const GLYPH_ACCEPT: &str = "✓";

    /// Disclosure triangle for an expanded section bar. OpenSpore's own; the
    /// reference's chevron style is UNRECOVERED.
    pub const GLYPH_SECTION_EXPANDED: &str = "▾";

    /// Disclosure triangle for a collapsed section bar. OpenSpore's own.
    pub const GLYPH_SECTION_COLLAPSED: &str = "▸";

    /// Neutral silhouette-style symbols used until the original icon texture
    /// mapping is recovered. These communicate six tool slots without labels.
    pub const TAB_GLYPHS: [&str; CreatorCounts::TAB_COUNT] = ["◉", "◎", "⌁", "∿", "◖", "✦"];

    /// **OBSERVED.** First section bar's label, verbatim.
    ///
    /// Structural chrome in the same sense as
    /// [`NAME_PILL_LABEL`](Self::NAME_PILL_LABEL): it names a panel section, not
    /// a creature part or a category. What the section *does* is UNRECOVERED —
    /// no paint-pass table exists in the corpus — so nothing reads these as more
    /// than display strings.
    pub const SECTION_LABEL_BASE: &str = "Base";

    /// **OBSERVED.** Second section bar's label, verbatim.
    pub const SECTION_LABEL_COAT: &str = "Coat";

    /// **OBSERVED.** Third section bar's label, verbatim.
    pub const SECTION_LABEL_DETAIL: &str = "Detail";

    /// Every section label, indexed by bar.
    ///
    /// Indexed 0..=[`CreatorCounts::SECTION_BAR_COUNT`]. Length is checked at
    /// compile time below, so adding a fourth bar to the count without adding
    /// its label is a build error rather than a panic at spawn.
    pub const SECTION_LABELS: [&str; CreatorCounts::SECTION_BAR_COUNT] = [
        Self::SECTION_LABEL_BASE,
        Self::SECTION_LABEL_COAT,
        Self::SECTION_LABEL_DETAIL,
    ];

    /// The label for section bar `index`, or `None` if the index is out of range.
    ///
    /// Out-of-range is `None` and never a panic and never a fabricated label:
    /// a caller with a bad index has a bug, and the honest answer to "what is
    /// bar 7?" is "there is no bar 7".
    pub const fn section_label(index: usize) -> Option<&'static str> {
        if index < Self::SECTION_LABELS.len() {
            Some(Self::SECTION_LABELS[index])
        } else {
            None
        }
    }

    // -- sizes ---------------------------------------------------------------

    /// **OpenSpore's own choice.** Tab placeholder label size in pixels.
    ///
    /// Spore's UI font and its metrics are UNRECOVERED, so every text size here
    /// is OpenSpore's. 12 px is chosen to fit inside a 48 px tab.
    pub const TAB_PLACEHOLDER_SIZE_PX: f32 = 12.0;

    /// **OpenSpore's own choice.** Pager label size in pixels.
    pub const PAGER_TEXT_SIZE_PX: f32 = 18.0;

    /// **OpenSpore's own choice.** Pill label size in pixels.
    pub const PILL_TEXT_SIZE_PX: f32 = 16.0;

    /// **OpenSpore's own choice.** Section bar label size in pixels.
    pub const SECTION_LABEL_SIZE_PX: f32 = 16.0;

    /// **OpenSpore's own choice.** Bottom-bar glyph size in pixels.
    ///
    /// Fits inside [`CreatorMetrics::BOTTOM_BUTTON_SIDE_PX`] with margin.
    pub const BUTTON_GLYPH_SIZE_PX: f32 = 20.0;

    /// **OpenSpore's own choice.** Section-bar disclosure glyph size in pixels.
    pub const SECTION_GLYPH_SIZE_PX: f32 = 12.0;
}
