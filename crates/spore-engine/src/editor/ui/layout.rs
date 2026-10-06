//! Pure `Node` / `BackgroundColor` builders for the Creature Creator screen.
//!
//! # Purity is the whole point
//!
//! Every function in this module is `fn(…) -> Node`-shaped: it takes
//! measurements and returns styles. There is no `Commands`, no `Query`, no
//! `Res`, no `World`, and **no literal proportion** — every measurement arrives
//! from [`theme`](super::theme). That gives three properties this crate wants:
//!
//! 1. The screen's geometry is assertable in a headless `cargo test`. Per this
//!    crate's own trap list, `DefaultPlugins` builds a winit `EventLoop` and
//!    panics off the main thread, so "the UI assembles" was never going to be a
//!    headless-testable claim. Making the geometry a pure function makes the
//!    part that *is* meaningful — the numbers — testable instead.
//! 2. A wrong proportion is a failing assertion rather than a screenshot to be
//!    squinted at.
//! 3. [`spawn`](super::spawn) contains no decisions, so there is exactly one
//!    place where "how tall is the tab strip" can be answered.
//!
//! # How the tree is shaped
//!
//! ```text
//! root                     Column, 100% x 100%      <- the window
//! +-- split                Row (or RowReverse), flex_grow 1
//! |   +-- panel            width 40%
//! |   |   +-- section_bars   (paint view only)    <- top
//! |   |   +-- tab_strip       height 60
//! |   |   +-- part_grid       flex_grow 1
//! |   |   +-- swatch_row      (paint view only)
//! |   |   +-- pager           height 56            <- bottom
//! |   +-- viewport          width 60%, hairline border
//! |       +-- hud           Absolute, top/right    <- pinned to the VIEWPORT
//! +-- bottom_bar           width 100%, height 55   <- full window width
//! ```
//!
//! The two structural claims worth stating out loud, because both are
//! assertions in `tests.rs` and both are easy to break by accident:
//!
//! * **`bottom_bar` is a child of `root`, not of `panel`.** The reference's
//!   bottom bar spans the *full window width*, running underneath the panel as
//!   well as the viewport. A `width: Percent(100)` node nested inside the panel
//!   is 100% of the *panel* — 768 px instead of 1920 — and the expression is
//!   byte-identical either way. Width alone cannot tell the two apart, so the
//!   test reads the parent relationship.
//! * **`hud` is a child of `viewport`, not of `root`.** The reference pins the
//!   HUD to the viewport's top-right corner. An absolutely positioned node
//!   resolves against its *parent's* box, so as a child of `root` the same node
//!   would sit at the window's corner — 768 px to the right of where the
//!   reference puts it. Again: the parent relationship is the fact.

use std::num::NonZeroUsize;

use bevy::prelude::{
    AlignItems, BackgroundColor, BorderColor, BorderRadius, FlexDirection, FlexWrap,
    JustifyContent, Node, Overflow, OverflowAxis, PositionType, TextColor, UiRect, Val,
};

use super::theme::{CreatorCounts, CreatorMetrics, CreatorPalette, CreatorText};

// ---------------------------------------------------------------------------
// Parameters
// ---------------------------------------------------------------------------

/// Which side of the window the catalog panel sits on.
///
/// The reference lets the user flip the panel between the two sides, and that
/// choice is **OBSERVED**, so it is a parameter of this layout rather than a
/// constant in it. Everything that depends on it is expressed as a
/// [`FlexDirection`] on the split row, which is what makes the flip free: no
/// second layout, no mirrored coordinates, and the bottom bar — which spans the
/// window rather than the split — is unaffected either way.
///
/// [`DEFAULT`](Self::DEFAULT) is [`Left`](Self::Left), the orientation the
/// majority of the reference frames show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelSide {
    /// Panel on the left, viewport on the right. The observed default.
    Left,
    /// Panel on the right, viewport on the left. The reference's flipped state.
    Right,
}

impl PanelSide {
    /// The side the reference shows by default. **OBSERVED.**
    pub const DEFAULT: Self = Self::Left;

    /// The split row's `flex_direction` for this side.
    ///
    /// `RowReverse` rather than a swap of the two children: the children keep
    /// their widths and their order, so "flipped" cannot drift into "and now the
    /// panel is 60% wide" — there is exactly one place that says 40 and one that
    /// says 60, and reversing touches neither.
    pub const fn flex_direction(self) -> FlexDirection {
        match self {
            Self::Left => FlexDirection::Row,
            Self::Right => FlexDirection::RowReverse,
        }
    }
}

/// How many columns the part grid has, and therefore how it pages.
///
/// The column count is a **parameter** rather than a constant because the
/// reference is not uniform here: its denser tabs show four columns over ten
/// rows across two pages, and its sparser tab shows three columns over four rows
/// across four pages. Both configurations are OBSERVED, so neither may be
/// inlined into the layout and both are named.
///
/// [`node`](Self::node) returning the same `Node` for both variants is not an
/// oversight — it is the finding. The two configurations differ in *how many
/// cells are placed*, not in how the row is styled: the cells are the OBSERVED
/// 180 px square in both, and `JustifyContent::SpaceBetween` absorbs the
/// difference into the gaps. So the parameter is a cell *count*, not a style,
/// and `tests.rs` asserts the equality so that if someone later
/// gives each configuration its own spacing, the change is deliberate.
///
/// The pages below are the reference's *figures*, recorded so the shapes can be
/// asserted against. They are **not** the source of truth for a live pager: how
/// many pages a catalogue actually has is the session's business and arrives in
/// [`CreatorUiView`](super::spawn::CreatorUiView). Keeping the observed figures
/// here and the live count there is what stops one question from having two
/// answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartGrid {
    /// **OBSERVED.** Four columns by ten rows, two pages.
    ///
    /// Four columns is also the widest the panel admits at the reference width:
    /// the compile-time assertion in [`theme`](super::theme) pins that a fifth column would
    /// not fit.
    FourColumns,
    /// **OBSERVED.** Three columns by four rows, four pages.
    ThreeColumns,
}

impl PartGrid {
    /// The reference's dominant configuration. **OBSERVED.**
    pub const DEFAULT: Self = Self::FourColumns;

    /// This grid's column count. **OBSERVED.**
    pub const fn columns(self) -> usize {
        match self {
            Self::FourColumns => CreatorCounts::PART_GRID_FOUR_COLUMNS,
            Self::ThreeColumns => CreatorCounts::PART_GRID_THREE_COLUMNS,
        }
    }

    /// This grid's row count per page. **OBSERVED.**
    pub const fn rows(self) -> usize {
        match self {
            Self::FourColumns => CreatorCounts::PART_GRID_FOUR_COLUMNS_ROWS,
            Self::ThreeColumns => CreatorCounts::PART_GRID_THREE_COLUMNS_ROWS,
        }
    }

    /// This grid's page count, as the reference shows it. **OBSERVED.**
    ///
    /// `NonZeroUsize` because a page count is never legitimately zero, and a
    /// pager reading `1/0` is a contradiction rather than an empty state.
    pub const fn pages(self) -> NonZeroUsize {
        let pages = match self {
            Self::FourColumns => CreatorCounts::PART_GRID_FOUR_COLUMNS_PAGES,
            Self::ThreeColumns => CreatorCounts::PART_GRID_THREE_COLUMNS_PAGES,
        };
        match NonZeroUsize::new(pages) {
            Some(n) => n,
            // Unreachable by construction: both arms above are non-zero
            // literals. Present because `Option` has to be answered.
            None => CreatorCounts::PAGE_COUNT_NONZERO,
        }
    }

    /// How many cells one page holds.
    ///
    /// This is the number the spawner needs in order to lay out a page, and it
    /// is deliberately separate from [`pages`](Self::pages): the observed shape
    /// says how big a page is, not how many the current catalogue has.
    pub const fn cells_per_page(self) -> usize {
        self.columns() * self.rows()
    }

    /// The grid node for this configuration.
    ///
    /// Returns [`part_grid`] for both variants — see the type's docs for why
    /// that is the finding and not a shortcut.
    pub fn node(self) -> Node {
        match self {
            Self::FourColumns | Self::ThreeColumns => part_grid(),
        }
    }
}

// ---------------------------------------------------------------------------
// Small shared pieces
// ---------------------------------------------------------------------------

/// A `UiRect` of one value on all four sides, in pixels.
///
/// The reason this helper exists rather than `UiRect::px(a, b, c, d)`: that
/// signature takes four arguments, so a one-sided mistake is invisible at the
/// call site. A one-argument call cannot be half-right.
const fn px4(value: f32) -> UiRect {
    UiRect::all(Val::Px(value))
}

/// A `BorderRadius` of one value on all four corners, in pixels.
///
/// `BorderRadius::px` has the four-argument shape `UiRect::px` has, for the same
/// reason; `BorderRadius::all` takes a `Val` and would need the `Val::Px` wrap at
/// every call site, which is where a wrong unit hides.
const fn px_radius(value: f32) -> BorderRadius {
    BorderRadius::all(Val::Px(value))
}

/// A fully round box, for anything the reference draws as a circle or capsule.
///
/// [`BorderRadius::MAX`] is `Val::Px(f32::MAX)`, which Bevy clamps to the box's
/// own half-extent — so this is genuinely "as round as this box allows" rather
/// than a hand-picked radius that stops being a circle the moment the size
/// changes. Used by the HUD buttons, the bottom bar's round buttons and the
/// pills, all three of which the reference draws rounded.
const fn round() -> BorderRadius {
    BorderRadius::MAX
}

// ---------------------------------------------------------------------------
// The frame: root, split, panel, viewport, bottom bar
// ---------------------------------------------------------------------------

/// The window-filling root: a column of `split` above `bottom_bar`.
///
/// `flex_direction: Column` plus both children sized as percentages of the
/// window is what makes the bottom bar span the *window* rather than the split:
/// it is a sibling of `split`, not a child of it.
pub fn root() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        height: Val::Percent(CreatorMetrics::FULL_PERCENT),
        flex_direction: FlexDirection::Column,
        ..Node::default()
    }
}

/// The horizontal split: panel and viewport side by side.
///
/// `flex_grow: 1` with no explicit height is load-bearing. The root is a column
/// whose other child is the fixed-height bottom bar, so `split` must absorb
/// whatever height is left; without `flex_grow: 1` it would size to its content
/// and the bottom bar would float up the middle of the window.
pub fn split_row(side: PanelSide) -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        flex_grow: CreatorMetrics::FLEX_GROW_FILL,
        flex_direction: side.flex_direction(),
        ..Node::default()
    }
}

/// The catalog panel. **OBSERVED:** 40% of the window width.
pub fn panel() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::PANEL_WIDTH_PERCENT),
        height: Val::Percent(CreatorMetrics::FULL_PERCENT),
        flex_direction: FlexDirection::Column,
        padding: px4(CreatorMetrics::PANEL_PADDING_PX),
        row_gap: Val::ZERO,
        ..Node::default()
    }
}

/// The panel's fill. OpenSpore's own.
pub fn panel_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::PANEL_BACKGROUND)
}

/// The 3D viewport. **OBSERVED:** 60% of the window width, i.e. the complement
/// of the panel.
///
/// `overflow: Visible` and `PositionType::Relative` are both the defaults, and
/// both are stated. `Relative` in particular is not decoration: it is the
/// containing block for the HUD's absolute positioning, and a future change to
/// `Absolute` here would move the HUD relative to the window instead of the
/// viewport without any other line changing.
pub fn viewport() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::VIEWPORT_WIDTH_PERCENT),
        height: Val::Percent(CreatorMetrics::FULL_PERCENT),
        border: UiRect::all(Val::Px(CreatorMetrics::VIEWPORT_BORDER_PX)),
        overflow: Overflow::visible(),
        position_type: PositionType::Relative,
        ..Node::default()
    }
}

/// The viewport's background.
///
/// [`bevy::color::Color::NONE`], and returned rather than left off so the choice is visible at
/// the call site: the viewport is the one region that must *not* be painted,
/// because the creature renders through it.
pub fn viewport_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::VIEWPORT_CLEAR)
}

/// The viewport's hairline border. OpenSpore's own.
///
/// A [`BorderColor`], not a [`BackgroundColor`]: the hairline belongs to the
/// viewport's *outline*, and putting it on the border is what keeps it from
/// painting over the viewport's own (deliberately transparent) background.
pub fn viewport_border_color() -> BorderColor {
    BorderColor::all(CreatorPalette::VIEWPORT_BORDER)
}

/// The bottom bar: full window width, under both the panel and the viewport.
///
/// **OBSERVED:** spans the full 1920 px of the reference, at ~55 px tall.
///
/// `width: Percent(100)` here is a percentage *of its parent*, and its parent is
/// the root, so "100%" means the whole window. The same expression inside the
/// panel would mean 100% of 768 px. That is why this node must be a child of
/// [`root`] and why the test for it reads `ChildOf`, not `width` —
/// see the module docs.
pub fn bottom_bar() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        height: Val::Px(CreatorMetrics::BOTTOM_BAR_HEIGHT_PX),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        // SpaceBetween, not three fixed columns: the reference's three groups
        // are pinned to the window's left edge, centre and right edge, and
        // SpaceBetween is what puts "centre" at an actual centre rather than at
        // the centre of whatever the left group happens to be.
        justify_content: JustifyContent::SpaceBetween,
        padding: UiRect::all(Val::Px(CreatorMetrics::BOTTOM_BAR_PADDING_PX)),
        column_gap: Val::Px(CreatorMetrics::BOTTOM_BAR_GAP_PX),
        ..Node::default()
    }
}

/// The bottom bar's fill. OpenSpore's own.
pub fn bottom_bar_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::BOTTOM_BAR_BACKGROUND)
}

// ---------------------------------------------------------------------------
// The tab strip
// ---------------------------------------------------------------------------

/// The horizontal strip of tabs across the top of the panel. **OBSERVED:** ~60 px
/// tall, full panel width, six square icons, the active one yellow.
pub fn tab_strip() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        height: Val::Px(CreatorMetrics::TAB_STRIP_HEIGHT_PX),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        // SpaceAround, not SpaceBetween: six 48 px tabs in a 768 px panel leave
        // 480 px to share, and SpaceBetween would put 96 px between each pair
        // while gluing the first and last tab to the panel's edges. The
        // reference's tabs are evenly distributed with visible margin at both
        // ends, which is what SpaceAround produces.
        justify_content: JustifyContent::SpaceAround,
        column_gap: Val::Px(CreatorMetrics::TAB_GAP_PX),
        flex_wrap: FlexWrap::NoWrap,
        ..Node::default()
    }
}

/// The tab strip's band fill. OpenSpore's own.
pub fn tab_strip_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::PANEL_BAND_BACKGROUND)
}

/// One tab.
///
/// Note what is *not* a parameter here: whether this tab is the active one. The
/// active tab in the reference is the same square in a different colour, so a
/// highlight that also grew the tab would misrepresent it and would make the two
/// cases impossible to compare. Geometry is therefore identical for both, and
/// the active flag lives in [`tab_background`] alone —
/// which is also what lets a test assert the two fills differ without a `Node` in
/// the way.
pub fn tab() -> Node {
    Node {
        width: Val::Px(CreatorMetrics::TAB_SIDE_PX),
        height: Val::Px(CreatorMetrics::TAB_SIDE_PX),
        // Never shrink: a tab that shrank to fit would stop being square, and
        // "square" is the one shape property of a tab the reference fixes.
        flex_shrink: CreatorMetrics::FLEX_SHRINK_NONE,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border_radius: px_radius(CreatorMetrics::TAB_RADIUS_PX),
        // A tab's placeholder label must never spill onto its neighbours, and
        // the strip is only 60 px tall.
        overflow: Overflow {
            x: OverflowAxis::Clip,
            y: OverflowAxis::Clip,
        },
        ..Node::default()
    }
}

/// A tab's fill: amber when active, dark when not.
///
/// This is the one place a boolean selects a colour. A test that only checked
/// "it is some colour" could not fail, which is the trap this module is written
/// against, so `tests.rs` asserts the two are *unequal* and that the
/// active one is the amber constant.
pub fn tab_background(active: bool) -> BackgroundColor {
    if active {
        BackgroundColor(CreatorPalette::TAB_ACTIVE)
    } else {
        BackgroundColor(CreatorPalette::TAB_INACTIVE)
    }
}

/// A tab's fill while hovered. OpenSpore's own; a still screenshot has no hover
/// state, so there is nothing to measure and nothing is claimed.
///
/// An active tab keeps its amber on hover. Dimming the one tab that answers
/// "where am I?" would make the answer flicker.
pub fn tab_background_hover(active: bool) -> BackgroundColor {
    if active {
        tab_background(true)
    } else {
        BackgroundColor(CreatorPalette::TAB_INACTIVE_HOVER)
    }
}

/// The ink a tab's placeholder label is drawn in.
///
/// Active and inactive differ, because the active tab's fill is the screen's
/// brightest surface and a dim label on it would be unreadable.
pub fn tab_placeholder_ink(active: bool) -> TextColor {
    TextColor(if active {
        CreatorPalette::TAB_ACTIVE_INK
    } else {
        CreatorPalette::TAB_PLACEHOLDER_INK
    })
}

/// The label a tab carries until OpenSpore has icon assets.
///
/// `TAB_1`..=`TAB_6`, from [`CreatorText::TAB_PLACEHOLDER_PREFIX`]. This is a *slot* label: it
/// says which of the six tabs it is and nothing about what that tab does,
/// because the corpus has no category table and a screenshot cannot supply one.
/// An out-of-range index yields `None` rather than a fabricated `TAB_7`.
pub fn tab_placeholder_label(index: usize) -> Option<String> {
    if index < CreatorCounts::TAB_COUNT {
        Some(format!(
            "{}{}",
            CreatorText::TAB_PLACEHOLDER_PREFIX,
            index + 1
        ))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// The part grid
// ---------------------------------------------------------------------------

/// The scrolling body of the panel, between the tab strip and the pager.
///
/// Takes **no** column count, and that is a finding rather than an omission: see
/// [`PartGrid`]'s docs. The reference's two observed configurations differ in
/// cell count, and `JustifyContent::SpaceBetween` plus a fixed cell width
/// absorbs the difference into the gaps, so the row's style is the same for both.
///
/// `flex_grow: 1` with `overflow_y: Clip` together are what make this the
/// *scrolling* region: the grid absorbs the panel's leftover height and cells
/// that do not fit are clipped rather than pushing the pager off the panel. The
/// pager staying visible is OBSERVED — it sits at the panel's bottom edge on
/// every reference frame — so it has to be a sibling below this node, not a
/// child.
pub fn part_grid() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        flex_grow: CreatorMetrics::FLEX_GROW_FILL,
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::Wrap,
        // SpaceBetween, and not `flex_grow` on the cells. This is the load-
        // bearing choice in the whole grid: with `flex_grow` the cells would
        // expand past the OBSERVED 180 px and the two configurations would
        // differ in cell size rather than in gap — which would then be a second
        // magic number. With SpaceBetween the cells stay exactly
        // `PART_CELL_SIDE_PX` and all the slack lands in the gaps, which is what
        // makes a single cell size serve both.
        justify_content: JustifyContent::SpaceBetween,
        column_gap: Val::Px(CreatorMetrics::PART_CELL_GAP_PX),
        row_gap: Val::Px(CreatorMetrics::PART_CELL_GAP_PX),
        overflow: Overflow::clip_y(),
        ..Node::default()
    }
}

/// One part cell.
///
/// `flex_basis: Px(PART_CELL_SIDE_PX)` rather than a percentage, so the OBSERVED
/// 180 px is visible in the node and assertable as a value. `flex_shrink` stays
/// at its default so a narrower window shrinks the cells instead of overflowing
/// the clip; the four-column fit at the reference width is a compile-time
/// assertion in [`theme`](super::theme).
///
/// `aspect_ratio: 1` pins the square the reference shows, so a cell cannot
/// become a rectangle because the flex algorithm handed it a different height.
pub fn part_cell() -> Node {
    Node {
        flex_basis: Val::Px(CreatorMetrics::PART_CELL_SIDE_PX),
        aspect_ratio: Some(CreatorMetrics::ASPECT_SQUARE),
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        // This is where the reference's "generous spacing" lives. Four 180 px
        // cells fill 93.75% of the 768 px panel, so there is no room for
        // generous *inter*-cell spacing; the reference draws a backing plate per
        // cell with the artwork inset inside it, and that inset is this padding.
        padding: px4(CreatorMetrics::PART_CELL_PADDING_PX),
        border_radius: px_radius(CreatorMetrics::PART_CELL_RADIUS_PX),
        overflow: Overflow::clip(),
        ..Node::default()
    }
}

/// One part cell's side length as a [`Val`].
///
/// [`part_cell`] is the style; this is the OBSERVED 180 px
/// stated on its own so a test can check "the cell is 180 px" as a value rather
/// than inferring it from a flex algorithm that has not run.
pub const fn part_cell_side() -> Val {
    Val::Px(CreatorMetrics::PART_CELL_SIDE_PX)
}

/// A cell's backing plate. OpenSpore's own.
pub fn part_cell_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::PART_CELL_BACKGROUND)
}

/// The painted backdrop's node: fills its parent, ignores layout flow.
///
/// # Absolute positioning, and it is not decoration
///
/// The viewport's children are laid out in a column by default, so an in-flow
/// backdrop would take a row and push the HUD down out of the region. `Absolute`
/// takes it out of flow entirely, so it covers the viewport without displacing
/// anything. This is the same reason `layout::viewport()` pins `Relative`: it is
/// the containing block this child is positioned against.
///
/// `Percent(100)` on both axes rather than a pixel size, because the backdrop has
/// to follow the viewport when the window resizes -- and a fixed size would leave
/// a strip of clear colour along one edge, which reads as a rendering bug.
pub fn backdrop() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::ZERO,
        top: Val::ZERO,
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        height: Val::Percent(CreatorMetrics::FULL_PERCENT),
        ..Node::default()
    }
}

/// A cell's backing plate while hovered. OpenSpore's own.
pub fn part_cell_background_hover() -> BackgroundColor {
    BackgroundColor(CreatorPalette::PART_CELL_HOVER)
}

/// The selected cell's border.
///
/// **OBSERVED** in the sense that the reference marks the chosen part somehow;
/// that it is specifically a *border* is OpenSpore's reading of it. `Option`,
/// because "no border" and "a transparent border" are different facts — the
/// first means unselected, the second would mean selected-but-not-rendered — and
/// collapsing them would make an unselected cell indistinguishable from a
/// selected one in exactly the test that should catch it.
pub fn part_cell_border(selected: bool) -> Option<BackgroundColor> {
    if selected {
        Some(BackgroundColor(CreatorPalette::PART_CELL_SELECTED_BORDER))
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// The pager
// ---------------------------------------------------------------------------

/// The pager row: back arrow, `1/N` label, forward arrow, all on one line.
///
/// **OBSERVED** in three parts at the panel's bottom edge: the arrows sit at the
/// panel's *left and right edge* respectively and the label is centred between
/// them. That is one row with three separated children, which is what
/// `JustifyContent::SpaceBetween` plus a centred middle child gives.
pub fn pager() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        height: Val::Px(CreatorMetrics::PAGER_HEIGHT_PX),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::SpaceBetween,
        ..Node::default()
    }
}

/// The pager's band fill. OpenSpore's own.
pub fn pager_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::PANEL_BAND_BACKGROUND)
}

/// One of the pager's arrow buttons.
///
/// Takes no `previous` flag, and again deliberately: both arrows are the same
/// square in the reference, differing only in which way they point, and pointing
/// is a glyph rather than a style. The test asserts the two arrows' styles are
/// equal, because a layout that special-cased them would be encoding a
/// distinction the reference does not make.
pub fn pager_arrow() -> Node {
    Node {
        width: Val::Px(CreatorMetrics::PAGER_ARROW_SIDE_PX),
        height: Val::Px(CreatorMetrics::PAGER_ARROW_SIDE_PX),
        flex_shrink: CreatorMetrics::FLEX_SHRINK_NONE,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border_radius: round(),
        ..Node::default()
    }
}

/// An arrow's fill. OpenSpore's own.
pub fn pager_arrow_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::ACTION_BUTTON)
}

/// The glyph on the pager's leading arrow. **OBSERVED as an arrow**; the exact
/// codepoint and typeface are UNRECOVERED.
pub fn pager_arrow_glyph(previous: bool) -> &'static str {
    if previous {
        CreatorText::GLYPH_PAGE_PREVIOUS
    } else {
        CreatorText::GLYPH_PAGE_NEXT
    }
}

/// The centred `page/total` label.
///
/// `pages` is [`NonZeroUsize`] for the reason given on
/// [`PAGE_COUNT_NONZERO`](CreatorCounts::PAGE_COUNT_NONZERO): `1/0` is not an
/// empty state, it is a contradiction, and this repo does not encode a
/// contradiction as a value.
///
/// A `page` beyond `pages` is **not** clamped. Clamping would render a page the
/// user cannot be on as though they were on it, which is the same error as
/// reporting "observed zero" for something nobody watched. The caller gets the
/// real numbers.
pub fn pager_label_text(page: usize, pages: NonZeroUsize) -> String {
    format!("{}/{}", page, pages.get())
}

// ---------------------------------------------------------------------------
// The colour swatch row (paint view)
// ---------------------------------------------------------------------------

/// The single row of colour swatches. **OBSERVED:** ~19 squares spanning the
/// panel width.
pub fn swatch_row() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        // No gap and no padding: the reference's swatches tile the row edge to
        // edge, which is why a swatch's width is `100 / 19` percent rather than
        // a pixel count. A gap here would stop 19 of them fitting the panel.
        column_gap: Val::ZERO,
        ..Node::default()
    }
}

/// One swatch: a square that takes its share of the row.
///
/// `flex_basis` is the OBSERVED-derived share
/// ([`SWATCH_WIDTH_PERCENT`](CreatorMetrics::SWATCH_WIDTH_PERCENT)) and
/// `flex_grow: 1` so the row tiles exactly at any panel width. `aspect_ratio: 1`
/// keeps it square when the row is taller than `100 / 19`% of the panel is wide,
/// which it will be at most window sizes.
pub fn swatch() -> Node {
    Node {
        flex_basis: Val::Percent(CreatorMetrics::SWATCH_WIDTH_PERCENT),
        flex_grow: CreatorMetrics::FLEX_GROW_FILL,
        aspect_ratio: Some(CreatorMetrics::ASPECT_SQUARE),
        border_radius: px_radius(CreatorMetrics::SWATCH_RADIUS_PX),
        overflow: Overflow::clip(),
        ..Node::default()
    }
}

/// A swatch's fill while no palette is recovered. OpenSpore's own; neutral on
/// purpose, so an unrecovered swatch reads as absent data rather than as a
/// colour somebody chose.
pub fn swatch_placeholder_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::SWATCH_PLACEHOLDER_FILL)
}

// ---------------------------------------------------------------------------
// The collapsible section bars (paint view)
// ---------------------------------------------------------------------------

/// One full-width section bar.
///
/// Square corners, not rounded. The reference's *tab cells* are visibly rounded
/// and its *section bars* are conspicuously rounded — [`CreatorMetrics::SECTION_BAR_RADIUS_PX`] is
/// where that radius lives — but nothing here rounds the strip or the panel
/// itself, because the reference shows no rounded corners there and rounding them
/// would be an invention.
///
/// `open` is deliberately **not** a parameter: the reference's bars are the same
/// height and the same full width whether or not their section is expanded, so
/// an expanded bar that grew would misrepresent it. What `open` changes is the
/// fill ([`section_bar_background`]) and the glyph ([`section_glyph`]).
pub fn section_bar() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        height: Val::Px(CreatorMetrics::SECTION_BAR_HEIGHT_PX),
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        padding: px4(CreatorMetrics::PANEL_PADDING_PX),
        border_radius: px_radius(CreatorMetrics::SECTION_BAR_RADIUS_PX),
        overflow: Overflow::clip(),
        ..Node::default()
    }
}

/// A section bar's fill — this *is* what open state changes. OpenSpore's own.
pub fn section_bar_background(open: bool) -> BackgroundColor {
    if open {
        BackgroundColor(CreatorPalette::SECTION_BAR_BACKGROUND_OPEN)
    } else {
        BackgroundColor(CreatorPalette::SECTION_BAR_BACKGROUND)
    }
}

/// A section bar's accent stripe.
///
/// **OBSERVED** only as "each bar has its own accent colour"; which hue is
/// OpenSpore's own. The stripe is a fixed-width child of the bar rather than a
/// border, because a border would round with the bar's 22 px radius while the
/// reference's stripe runs to the bar's full height.
///
/// `Option`, for the same reason as [`part_cell_border`]: an out-of-range bar
/// index has no accent, and inventing one would make "which accent is this?"
/// answerable for a bar that does not exist.
pub fn section_accent(index: usize) -> Option<BackgroundColor> {
    CreatorPalette::SECTION_ACCENTS
        .get(index)
        .copied()
        .map(BackgroundColor)
}

/// The accent stripe's width.
pub const fn section_accent_width() -> Val {
    Val::Px(CreatorMetrics::SECTION_ACCENT_WIDTH_PX)
}

/// The label of section bar `index`, or `None` if there is no such bar.
///
/// Delegates to [`CreatorText::section_label`]; the indirection exists so the
/// "no such bar" case is `None` here too rather than becoming a panic at the
/// spawn site.
pub fn section_label(index: usize) -> Option<&'static str> {
    CreatorText::section_label(index)
}

/// The disclosure glyph for a section bar. OpenSpore's own; the reference's
/// chevron style is UNRECOVERED.
pub fn section_glyph(open: bool) -> &'static str {
    if open {
        CreatorText::GLYPH_SECTION_EXPANDED
    } else {
        CreatorText::GLYPH_SECTION_COLLAPSED
    }
}

/// The column that stacks the section bars above the tab strip.
///
/// Exists so `spawn` never writes a `Node` literal: the stack is a *layout*
/// decision (bars go on top, in a column, with no gap between them) and the
/// caller should not be able to make it differently. `row_gap` is zero because
/// the reference's bars touch, with their rounded corners reading as a
/// continuous stack.
pub fn section_bar_stack() -> Node {
    Node {
        width: Val::Percent(CreatorMetrics::FULL_PERCENT),
        flex_direction: FlexDirection::Column,
        row_gap: Val::ZERO,
        ..Node::default()
    }
}

/// The row that groups the four bottom-bar buttons at the right edge.
///
/// `JustifyContent::FlexEnd` so the group itself sits hard right inside the bar's
/// padding; the bar's own `SpaceBetween` then places it in the third third.
pub fn button_group() -> Node {
    Node {
        flex_direction: FlexDirection::Row,
        align_items: AlignItems::Center,
        column_gap: Val::Px(CreatorMetrics::BOTTOM_BAR_GAP_PX),
        ..Node::default()
    }
}

/// The accent stripe inside a section bar: a fixed-width, full-height sliver.
///
/// Full height as a percentage rather than a pixel, because the bar's height is
/// set once and the stripe should follow it rather than restate it.
pub fn section_accent_strip() -> Node {
    Node {
        width: section_accent_width(),
        height: Val::Percent(CreatorMetrics::FULL_PERCENT),
        flex_shrink: CreatorMetrics::FLEX_SHRINK_NONE,
        ..Node::default()
    }
}

// ---------------------------------------------------------------------------
// The bottom bar's controls
// ---------------------------------------------------------------------------

/// A rounded pill — the DNA counter and the name field are both this shape.
///
/// `min_width` rather than `width`: the reference's pills size to their contents
/// and the name field's contents are a string that changes. A fixed width would
/// either clip a long name or leave a short one floating in a big box.
pub fn pill(min_width: f32) -> Node {
    Node {
        height: Val::Px(CreatorMetrics::PILL_HEIGHT_PX),
        min_width: Val::Px(min_width),
        flex_shrink: CreatorMetrics::FLEX_SHRINK_NONE,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        padding: UiRect {
            left: Val::Px(CreatorMetrics::PILL_PADDING_PX),
            right: Val::Px(CreatorMetrics::PILL_PADDING_PX),
            top: Val::ZERO,
            bottom: Val::ZERO,
        },
        border_radius: round(),
        ..Node::default()
    }
}

/// A pill's fill. OpenSpore's own.
pub fn pill_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::PILL_BACKGROUND)
}

/// The name field's fill. OpenSpore's own; a step lighter than a plain pill,
/// because the reference makes the centre pill the visually dominant one.
pub fn name_pill_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::NAME_PILL_BACKGROUND)
}

/// One of the bottom bar's round buttons: undo, redo, cancel, confirm.
///
/// `size` is a parameter rather than a constant so the four buttons can be
/// asserted to be *the same* size by being handed the same number. Comparing four
/// `Node`s for equality is a stronger statement than four calls to one function
/// quietly agreeing, and it survives someone changing the constant.
pub fn round_button(size: f32) -> Node {
    Node {
        width: Val::Px(size),
        height: Val::Px(size),
        flex_shrink: CreatorMetrics::FLEX_SHRINK_NONE,
        align_items: AlignItems::Center,
        justify_content: JustifyContent::Center,
        border_radius: round(),
        ..Node::default()
    }
}

/// The neutral fill of the undo and redo buttons. OpenSpore's own; the reference
/// shows them undifferentiated from each other, so
/// [`redo_button_background`] returns this same
/// colour rather than a second neutral.
pub fn undo_button_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::ACTION_BUTTON)
}

/// The redo button's fill. Identical to undo's, by observation rather than by
/// omission.
pub fn redo_button_background() -> BackgroundColor {
    undo_button_background()
}

/// The cancel button's fill — the reference's red. **Exact value UNKNOWN.**
pub fn reject_button_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::REJECT_BUTTON)
}

/// The confirm button's fill — the reference's green. **Exact value UNKNOWN.**
pub fn accept_button_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::ACCEPT_BUTTON)
}

/// The DNA counter pill's minimum width.
///
/// Its *contents* are the session's business — a price is a game-balance fact,
/// not a layout fact, so no count is baked in here. Only the pill's minimum size
/// is a layout fact, and only its minimum is knowable before the text exists.
pub const fn dna_pill_min_width() -> Val {
    Val::Px(CreatorMetrics::DNA_PILL_MIN_WIDTH_PX)
}

/// The name field pill's minimum width.
pub const fn name_pill_min_width() -> Val {
    Val::Px(CreatorMetrics::NAME_PILL_MIN_WIDTH_PX)
}

/// The four bottom-bar glyphs, in the reference's left-to-right order.
///
/// Undo, redo, cancel, confirm. **OBSERVED** as four glyphs with those shapes;
/// the exact codepoints and the typeface are UNRECOVERED, so these are OpenSpore's
/// nearest Unicode stand-ins. Indexed access rather than four named constants
/// because the order is the fact and the names are just labels on it.
pub const BOTTOM_BAR_GLYPHS: [&str; CreatorCounts::BOTTOM_BAR_BUTTON_COUNT] = [
    CreatorText::GLYPH_UNDO,
    CreatorText::GLYPH_REDO,
    CreatorText::GLYPH_REJECT,
    CreatorText::GLYPH_ACCEPT,
];

/// The fill for bottom-bar button `index`, in reference order.
///
/// `Option`, because an out-of-range index has no fill and returning the neutral
/// one would make a fifth button indistinguishable from undo.
pub fn bottom_bar_button_background(index: usize) -> Option<BackgroundColor> {
    match index {
        0 => Some(undo_button_background()),
        1 => Some(redo_button_background()),
        2 => Some(reject_button_background()),
        3 => Some(accept_button_background()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The viewport HUD
// ---------------------------------------------------------------------------

/// The HUD stack: absolute, pinned to the viewport's top-right corner.
///
/// `position_type: Absolute` with `top`/`right` set is what "pinned to the
/// top-right" means in a flex layout, and it is why this node must be spawned
/// under the **viewport** rather than under the root: an absolutely positioned
/// node resolves against its *parent's* box, so under the root it would sit at
/// the window's corner — a panel's width to the right of where the reference puts
/// it, and the identical `Node` would look correct in isolation.
pub fn viewport_hud() -> Node {
    Node {
        position_type: PositionType::Absolute,
        top: Val::Px(CreatorMetrics::HUD_INSET_PX),
        right: Val::Px(CreatorMetrics::HUD_INSET_PX),
        flex_direction: FlexDirection::Column,
        align_items: AlignItems::FlexEnd,
        row_gap: Val::Px(CreatorMetrics::HUD_BUTTON_GAP_PX),
        ..Node::default()
    }
}

/// One HUD button: a circle.
///
/// [`round_button`] with the HUD's own diameter, so it inherits
/// [`BorderRadius::MAX`] and "circular" stays true if the diameter constant
/// changes — which is the whole reason the reference's "circular" is honoured
/// with `MAX` instead of a half-size radius that would silently become a rounded
/// square the moment the size moved.
pub fn hud_button() -> Node {
    round_button(CreatorMetrics::HUD_BUTTON_DIAMETER_PX)
}

/// A HUD button's fill. Semi-transparent: the HUD sits over a rendered creature
/// and an opaque disc would occlude the thing being edited. OpenSpore's own.
pub fn hud_button_background() -> BackgroundColor {
    BackgroundColor(CreatorPalette::HUD_BUTTON_BACKGROUND)
}

/// A HUD button's fill while hovered. OpenSpore's own.
pub fn hud_button_background_hover() -> BackgroundColor {
    BackgroundColor(CreatorPalette::HUD_BUTTON_HOVER)
}

/// The HUD's button count. **OBSERVED.**
pub const HUD_BUTTON_COUNT: usize = CreatorCounts::HUD_BUTTON_COUNT;

// ---------------------------------------------------------------------------
// Compile-time checks
// ---------------------------------------------------------------------------

/// The glyph array must cover exactly the four observed bottom-bar buttons.
///
/// Without this, `BOTTOM_BAR_BUTTON_COUNT` could be raised to five, the loop in
/// `spawn_round_buttons` would then ask for a fifth glyph, and
/// `bottom_bar_button_background(4)` — which correctly returns `None` — would be
/// reached with an index the *array* believes is valid. The array is the thing
/// that decides, so the array is what gets checked.
const _: () = assert!(BOTTOM_BAR_GLYPHS.len() == CreatorCounts::BOTTOM_BAR_BUTTON_COUNT);
