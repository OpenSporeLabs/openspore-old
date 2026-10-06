//! The `Commands` layer: turns a [`CreatorUiView`] into an entity tree.
//!
//! # The only impure file
//!
//! Everything in here spawns and inserts; every *decision* lives in
//! [`layout`](super::layout) and every *number* in [`theme`](super::theme). A
//! reader who wants to know how tall something is will not find it here, and
//! that is the point: there is one answer per question and it lives in one
//! place. This file contains no `Node` literal at all — every style comes from a
//! `layout::` call.
//!
//! # Two hierarchies, deliberately
//!
//! The tree is built with `ChildOf` rather than by spawning each subtree in one
//! shot from its parent, because the parent relationships carry two of the
//! screen's structural claims and are asserted in `tests.rs`:
//!
//! * the bottom bar hangs off the **root**, not off the panel, so that its
//!   `Percent(100)` means the whole window;
//! * the HUD hangs off the **viewport**, not off the root, so that its absolute
//!   top-right resolves against the viewport's box rather than the window's.
//!
//! Re-parenting either one produces a `Node` byte-identical to the correct one.
//! Only the hierarchy distinguishes them, so only the hierarchy is worth
//! asserting.
//!
//! # `Commands` and the trap this file walks past
//!
//! `commands.spawn(…).id()` returns the *reserved* entity id immediately, so the
//! whole tree is built in one pass with no deferred read-back. That matters
//! because a `Query` in the same system that spawns sees nothing — this crate's
//! own trap list has that one twice. Nothing here queries, so it does not arise;
//! the comment is here so that a future "let me just check the parent exists"
//! does not walk into it.
//!
//! # What this file does not do
//!
//! It does not decide what any button *does*. Undo, redo, cancel, confirm, the
//! tabs, the arrows, the cells and the swatches are all spawned as [`Button`]s
//! with [`Interaction`](bevy::prelude::Interaction), and wiring them is
//! [`CreatorSession`](super::super::session::CreatorSession)'s job. A UI path
//! that ended in an ECS mutation would give one edit two implementations.

use std::num::NonZeroUsize;

use bevy::prelude::{Button, ChildOf, Commands, Entity, Name, Text, TextFont};

use super::layout::{self, PanelSide, PartGrid};
use super::theme::{CreatorCounts, CreatorMetrics, CreatorText};

/// What the screen currently shows.
///
/// A plain data bundle: no ECS handles, no `Res`, nothing that has to be alive.
/// It exists so the spawner is a function of one argument, which is what lets the
/// tests build any state they like — including states the reference never shows,
/// such as a flipped panel or a page beyond the last.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatorUiView {
    /// Which side the catalog panel is on. **OBSERVED** to be user-flippable.
    pub side: PanelSide,
    /// Index of the active tab, zero-based, into a
    /// [`TAB_COUNT`](CreatorCounts::TAB_COUNT)-element strip.
    ///
    /// A *position*, never a name. The corpus has no category table, so nothing
    /// may depend on what index 2 means. Read it through
    /// [`active_tab`](Self::active_tab), which bounds it.
    pub active_tab: usize,
    /// The part grid's shape.
    pub grid: PartGrid,
    /// The current page, **one-based**, as the pager displays it.
    ///
    /// One-based because the pager's `1/N` is one-based, and a zero-based index
    /// rendered through a one-based label is the classic off-by-one that ships.
    pub page: usize,
    /// How many pages exist. `NonZeroUsize` because a page always does.
    pub pages: NonZeroUsize,
    /// How many catalogue entries to lay out on this page.
    ///
    /// The *live* count, deliberately separate from
    /// [`PartGrid::cells_per_page`], which is the *observed shape*. The shape
    /// says how big a page is; this says how full it is, and the two must be
    /// allowed to disagree.
    pub cells: usize,
    /// Whether to draw the paint-view furniture: the section bars and the swatch
    /// row.
    pub paint_mode: bool,
    /// Open state of each collapsible section bar, indexed like
    /// [`SECTION_LABELS`](CreatorText::SECTION_LABELS).
    pub sections_open: [bool; CreatorCounts::SECTION_BAR_COUNT],
    /// The name field's text. Empty means unset, which is **not** the observed
    /// placeholder: the placeholder belongs to the widget and is drawn by
    /// [`spawn_name_pill`] when this is empty, so an unset name reads as unset
    /// instead of being indistinguishable from a creature literally named
    /// "Name Your Creature".
    pub name: String,
}

impl Default for CreatorUiView {
    /// A view matching the reference's default frame as closely as OpenSpore can:
    /// panel on the left, first tab active, the denser grid, page 1 of a single
    /// page, no cells, no paint furniture, no name.
    fn default() -> Self {
        Self {
            side: PanelSide::DEFAULT,
            active_tab: 0,
            grid: PartGrid::DEFAULT,
            page: 1,
            pages: CreatorCounts::PAGE_COUNT_NONZERO,
            cells: 0,
            paint_mode: false,
            sections_open: [false; CreatorCounts::SECTION_BAR_COUNT],
            name: String::new(),
        }
    }
}

impl CreatorUiView {
    /// The active tab index, bounded to the strip.
    ///
    /// Clamped rather than asserted, because this is a UI: a stale index left by
    /// a catalogue that shrank underneath the screen must not panic the app. The
    /// clamp is to the *last* tab rather than the first, so a too-large index
    /// shows the user the furthest-right content rather than resetting them to
    /// the beginning — and
    /// [`active_tab_is_in_range`](Self::active_tab_is_in_range) exists so a
    /// caller can notice that it happened.
    pub fn active_tab(&self) -> usize {
        self.active_tab
            .min(CreatorCounts::TAB_COUNT.saturating_sub(1))
    }

    /// Whether [`active_tab`](Self::active_tab) was in range to begin with.
    pub fn active_tab_is_in_range(&self) -> bool {
        self.active_tab < CreatorCounts::TAB_COUNT
    }
}

/// The kind of element a spawned entity is.
///
/// Not consulted by the spawner — nothing here needs to find anything again. The
/// marker exists so a test can address the screen ("the panel", "the HUD", "the
/// fourth tab") by something durable rather than by an index, and so a later
/// system has a documented handle instead of reaching for a hard-coded entity
/// id. It is the same "one owner per entity kind" discipline this crate already
/// applies to cameras: a screen element gets one name, not two.
///
/// **One variant per entity kind, never shared.** An earlier revision reused
/// `TabStrip` for the strip *and* for each of its six tabs, which made
/// "exactly one tab strip" a false claim the moment anything counted by marker.
/// The tests caught it, which is the reason this paragraph exists.
///
/// A [`bevy::prelude::Component`] rather than a plain enum because it is carried in the spawn
/// bundle; it holds no data, so it costs nothing and cannot go stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, bevy::prelude::Component)]
pub enum CreatorUiPart {
    /// The window-filling root column.
    Root,
    /// The panel/viewport split row.
    Split,
    /// The catalog panel.
    Panel,
    /// The 3D viewport.
    Viewport,
    /// The tab strip.
    TabStrip,
    /// One tab.
    Tab,
    /// The part grid.
    PartGrid,
    /// One part cell.
    PartCell,
    /// The colour swatch row.
    SwatchRow,
    /// One colour swatch.
    Swatch,
    /// The pager.
    Pager,
    /// One of the pager's arrows.
    PagerArrow,
    /// The column holding the collapsible section bars.
    SectionBarStack,
    /// One collapsible section bar.
    SectionBar,
    /// A section bar's accent stripe.
    SectionAccent,
    /// The viewport HUD stack.
    ViewportHud,
    /// One HUD button.
    HudButton,
    /// The full-width bottom bar.
    BottomBar,
    /// The DNA counter pill.
    DnaPill,
    /// The name field pill.
    NamePill,
    /// The group holding the four bottom-bar buttons.
    ButtonGroup,
    /// One of the four bottom-bar buttons.
    RoundButton,
    /// Any text label.
    Label,
}

/// Spawns the whole screen and returns the root entity.
///
/// Returns the root so a caller can parent other things — a debug overlay, a
/// modal — into the same hierarchy the screen uses.
pub fn spawn_creator_ui(commands: &mut Commands, view: &CreatorUiView) -> Entity {
    let root = spawn_root(commands);
    let split = spawn_split(commands, root, view);

    spawn_panel(commands, split, view);
    let viewport = spawn_viewport(commands, split);
    spawn_viewport_hud(commands, viewport);

    // Deliberately a sibling of `split`, not a child of the panel. This one line
    // is the difference between a 1920 px bottom bar and a 768 px one, and the
    // two are indistinguishable from the `Node` alone.
    spawn_bottom_bar(commands, root, view);

    root
}

/// The window-filling root column.
///
/// Deliberately given no `BackgroundColor`: the panel and the bottom bar paint
/// themselves, and a fill here would sit behind the viewport as well — which is
/// the one region that must show the 3D render through.
pub fn spawn_root(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            layout::root(),
            CreatorUiPart::Root,
            Name::new("creator-ui-root"),
        ))
        .id()
}

/// The split row: panel and viewport.
pub fn spawn_split(commands: &mut Commands, parent: Entity, view: &CreatorUiView) -> Entity {
    commands
        .spawn((
            layout::split_row(view.side),
            ChildOf(parent),
            CreatorUiPart::Split,
            Name::new("creator-ui-split"),
        ))
        .id()
}

/// The catalog panel and everything in it.
pub fn spawn_panel(commands: &mut Commands, parent: Entity, view: &CreatorUiView) -> Entity {
    let panel = commands
        .spawn((
            layout::panel(),
            layout::panel_background(),
            ChildOf(parent),
            CreatorUiPart::Panel,
            Name::new("creator-ui-panel"),
        ))
        .id();

    // Child order *is* the panel's vertical order, so it is spelled out here
    // rather than left to a later reader to infer:
    //
    //   1. section bars  — paint view only, and the reference stacks them at the
    //      very top of the panel, above the tab strip;
    //   2. tab strip     — OBSERVED 60 px;
    //   3. part grid     — absorbs the leftover height;
    //   4. swatch row    — paint view only, above the pager;
    //   5. pager         — OBSERVED at the panel's bottom edge, so it must come
    //      last or the grid's growth would push it off.
    if view.paint_mode {
        spawn_section_bars(commands, panel, view);
    }

    spawn_tab_strip(commands, panel, view);
    spawn_part_grid(commands, panel, view);

    if view.paint_mode {
        spawn_swatch_row(commands, panel);
    }

    spawn_pager(commands, panel, view);

    panel
}

/// The tab strip and its [`TAB_COUNT`](CreatorCounts::TAB_COUNT) tabs.
pub fn spawn_tab_strip(commands: &mut Commands, parent: Entity, view: &CreatorUiView) -> Entity {
    let strip = commands
        .spawn((
            layout::tab_strip(),
            layout::tab_strip_background(),
            ChildOf(parent),
            CreatorUiPart::TabStrip,
            Name::new("creator-ui-tab-strip"),
        ))
        .id();

    let active = view.active_tab();
    for index in 0..CreatorCounts::TAB_COUNT {
        spawn_tab(commands, strip, index, index == active);
    }

    strip
}

/// One tab.
///
/// Carries [`Button`] so it is hoverable and pressable; what a tab *does* is
/// [`CreatorSession`](super::super::session::CreatorSession)'s business, so no
/// behaviour is attached here.
fn spawn_tab(commands: &mut Commands, parent: Entity, index: usize, active: bool) -> Entity {
    let entity = commands
        .spawn((
            layout::tab(),
            layout::tab_background(active),
            layout::tab_placeholder_ink(active),
            Button,
            ChildOf(parent),
            CreatorUiPart::Tab,
            Name::new("creator-ui-tab"),
        ))
        .id();

    // `index` comes from the loop above and is therefore always in range, but
    // the label is still fetched through `Option`: a caller who later raises
    // `TAB_COUNT` without raising the strip's capacity should get a nameless tab
    // rather than a panic on the first frame.
    if let Some(label) = layout::tab_placeholder_label(index) {
        spawn_label(
            commands,
            entity,
            &label,
            CreatorText::TAB_PLACEHOLDER_SIZE_PX,
        );
    }

    entity
}

/// The part grid and its cells.
pub fn spawn_part_grid(commands: &mut Commands, parent: Entity, view: &CreatorUiView) -> Entity {
    let grid = commands
        .spawn((
            view.grid.node(),
            layout::panel_background(),
            ChildOf(parent),
            CreatorUiPart::PartGrid,
            Name::new("creator-ui-part-grid"),
        ))
        .id();

    // Capped at `cells_per_page`, because a catalogue longer than one page
    // belongs on page two and rendering forty cells into a twelve-cell page would
    // push the pager off the panel — a clipping problem dressed as a layout one.
    let count = view.cells.min(view.grid.cells_per_page());
    for _ in 0..count {
        commands.spawn((
            layout::part_cell(),
            layout::part_cell_background(),
            Button,
            ChildOf(grid),
            CreatorUiPart::PartCell,
            Name::new("creator-ui-part-cell"),
        ));
    }

    grid
}

/// The pager: back arrow, `page/total` label, forward arrow.
pub fn spawn_pager(commands: &mut Commands, parent: Entity, view: &CreatorUiView) -> Entity {
    let pager = commands
        .spawn((
            layout::pager(),
            layout::pager_background(),
            ChildOf(parent),
            CreatorUiPart::Pager,
            Name::new("creator-ui-pager"),
        ))
        .id();

    spawn_pager_arrow(commands, pager, true);

    spawn_label(
        commands,
        pager,
        &layout::pager_label_text(view.page, view.pages),
        CreatorText::PAGER_TEXT_SIZE_PX,
    );

    spawn_pager_arrow(commands, pager, false);

    pager
}

/// One of the pager's arrows.
///
/// `previous` selects the *glyph* and nothing else; both arrows share one style
/// function, and the tests assert the two nodes are equal so the distinction
/// cannot quietly become a distinction in geometry.
fn spawn_pager_arrow(commands: &mut Commands, parent: Entity, previous: bool) -> Entity {
    let arrow = commands
        .spawn((
            layout::pager_arrow(),
            layout::pager_arrow_background(),
            Button,
            ChildOf(parent),
            CreatorUiPart::PagerArrow,
            Name::new("creator-ui-pager-arrow"),
        ))
        .id();

    spawn_label(
        commands,
        arrow,
        layout::pager_arrow_glyph(previous),
        CreatorText::BUTTON_GLYPH_SIZE_PX,
    );

    arrow
}

/// The colour swatch row (paint view).
pub fn spawn_swatch_row(commands: &mut Commands, parent: Entity) -> Entity {
    let row = commands
        .spawn((
            layout::swatch_row(),
            ChildOf(parent),
            CreatorUiPart::SwatchRow,
            Name::new("creator-ui-swatch-row"),
        ))
        .id();

    for _ in 0..CreatorCounts::SWATCH_COUNT {
        commands.spawn((
            layout::swatch(),
            layout::swatch_placeholder_background(),
            Button,
            ChildOf(row),
            CreatorUiPart::Swatch,
            Name::new("creator-ui-swatch"),
        ));
    }

    row
}

/// The collapsible section bars (paint view).
pub fn spawn_section_bars(commands: &mut Commands, parent: Entity, view: &CreatorUiView) -> Entity {
    let stack = commands
        .spawn((
            layout::section_bar_stack(),
            layout::panel_background(),
            ChildOf(parent),
            CreatorUiPart::SectionBarStack,
            Name::new("creator-ui-section-bars"),
        ))
        .id();

    for (index, &open) in view.sections_open.iter().enumerate() {
        spawn_section_bar(commands, stack, index, open);
    }

    stack
}

/// One section bar: accent stripe, label, disclosure glyph.
///
/// The stripe is a child rather than a border, for the reason given on
/// [`layout::section_accent`]: a border would round with the bar's 22 px radius
/// while the reference's stripe runs the bar's full height.
fn spawn_section_bar(commands: &mut Commands, parent: Entity, index: usize, open: bool) -> Entity {
    let bar = commands
        .spawn((
            layout::section_bar(),
            layout::section_bar_background(open),
            Button,
            ChildOf(parent),
            CreatorUiPart::SectionBar,
            Name::new("creator-ui-section-bar"),
        ))
        .id();

    if let Some(accent) = layout::section_accent(index) {
        commands.spawn((
            layout::section_accent_strip(),
            accent,
            ChildOf(bar),
            CreatorUiPart::SectionAccent,
            Name::new("creator-ui-section-accent"),
        ));
    }

    if let Some(label) = layout::section_label(index) {
        spawn_label(commands, bar, label, CreatorText::SECTION_LABEL_SIZE_PX);
    }

    spawn_label(
        commands,
        bar,
        layout::section_glyph(open),
        CreatorText::SECTION_GLYPH_SIZE_PX,
    );

    bar
}

/// The 3D viewport.
///
/// Its background is [`layout::viewport_background`] — transparent, because the
/// creature renders through it — and it carries the border rather than the split
/// row, so the hairline belongs to the thing it outlines.
/// The viewport's 3D window, and the one node the input guards measure against.
///
/// # The `Viewport` marker is load bearing, not documentation
///
/// [`crate::editor::plugin::Viewport`] is a separate component from the
/// [`CreatorUiPart::Viewport`] variant on the next line, and the input guards query
/// for **the marker**: `With<CreatorUiPart>, With<Viewport>`. For a while the marker
/// was declared and used as a filter but never inserted, so the query matched
/// nothing and the guards silently degraded to "no node, so no opinion, so accept".
/// A click on the panel and a scroll on the catalog both reached the creature, and
/// the counters stayed at zero — which read as "the guard passed" when the guard had
/// never run. Keep the two in step: the variant says what the node **is**, the marker
/// says what can **find** it.
pub fn spawn_viewport(commands: &mut Commands, parent: Entity) -> Entity {
    commands
        .spawn((
            layout::viewport(),
            layout::viewport_background(),
            layout::viewport_border_color(),
            ChildOf(parent),
            CreatorUiPart::Viewport,
            crate::editor::plugin::Viewport,
            Name::new("creator-ui-viewport"),
        ))
        .id()
}

/// The HUD stack, pinned to the **viewport**'s top-right.
///
/// The parent argument is the viewport and not the root, and that is the entire
/// content of this function's signature as far as the layout is concerned: the
/// `Node` is identical either way and only the containing block differs. See
/// [`layout::viewport_hud`].
///
/// The stack carries a fill so its buttons have something to read against; it is
/// OpenSpore's own and semi-transparent for the same reason the buttons' are.
pub fn spawn_viewport_hud(commands: &mut Commands, viewport: Entity) -> Entity {
    let hud = commands
        .spawn((
            layout::viewport_hud(),
            layout::hud_button_background(),
            ChildOf(viewport),
            CreatorUiPart::ViewportHud,
            Name::new("creator-ui-hud"),
        ))
        .id();

    for _ in 0..CreatorCounts::HUD_BUTTON_COUNT {
        commands.spawn((
            layout::hud_button(),
            layout::hud_button_background(),
            Button,
            ChildOf(hud),
            CreatorUiPart::HudButton,
            Name::new("creator-ui-hud-button"),
        ));
    }

    hud
}

/// The full-width bottom bar, hung off the **root**.
///
/// Three groups, in the reference's order: the DNA counter on the left, the name
/// field in the centre, undo / redo / cancel / confirm on the right.
pub fn spawn_bottom_bar(commands: &mut Commands, root: Entity, view: &CreatorUiView) -> Entity {
    let bar = commands
        .spawn((
            layout::bottom_bar(),
            layout::bottom_bar_background(),
            ChildOf(root),
            CreatorUiPart::BottomBar,
            Name::new("creator-ui-bottom-bar"),
        ))
        .id();

    spawn_dna_pill(commands, bar);
    spawn_name_pill(commands, bar, view);
    spawn_round_buttons(commands, bar);

    bar
}

/// The DNA counter pill.
///
/// Renders an **empty** body: a count is a game-balance fact
/// [`CreatorSession`](super::super::session::CreatorSession) owns, and no count
/// is baked into a layout module. The pill's minimum width and its fill are the
/// only parts of this widget that are layout facts, and both are asserted.
pub fn spawn_dna_pill(commands: &mut Commands, parent: Entity) -> Entity {
    commands
        .spawn((
            layout::pill(CreatorMetrics::DNA_PILL_MIN_WIDTH_PX),
            layout::pill_background(),
            ChildOf(parent),
            CreatorUiPart::DnaPill,
            Name::new("creator-ui-dna-pill"),
        ))
        .id()
}

/// The name field pill, showing the OBSERVED placeholder when unset.
pub fn spawn_name_pill(commands: &mut Commands, parent: Entity, view: &CreatorUiView) -> Entity {
    let pill = commands
        .spawn((
            layout::pill(CreatorMetrics::NAME_PILL_MIN_WIDTH_PX),
            layout::name_pill_background(),
            ChildOf(parent),
            CreatorUiPart::NamePill,
            Name::new("creator-ui-name-pill"),
        ))
        .id();

    // An empty name draws the placeholder. This is one of only two Spore strings
    // OpenSpore reproduces, and it is structural chrome: it names the widget and
    // contains no part, category or price. See `CreatorText::NAME_PILL_LABEL`.
    let text = if view.name.is_empty() {
        CreatorText::NAME_PILL_LABEL
    } else {
        view.name.as_str()
    };
    spawn_label(commands, pill, text, CreatorText::PILL_TEXT_SIZE_PX);

    pill
}

/// The four bottom-bar buttons, in the reference's order.
fn spawn_round_buttons(commands: &mut Commands, parent: Entity) -> Entity {
    let group = commands
        .spawn((
            layout::button_group(),
            ChildOf(parent),
            CreatorUiPart::ButtonGroup,
            Name::new("creator-ui-button-group"),
        ))
        .id();

    let size = CreatorMetrics::BOTTOM_BUTTON_SIDE_PX;
    for index in 0..layout::BOTTOM_BAR_GLYPHS.len() {
        let button = commands
            .spawn((
                layout::round_button(size),
                Button,
                ChildOf(group),
                CreatorUiPart::RoundButton,
                Name::new("creator-ui-round-button"),
            ))
            .id();

        // Inserted separately from the bundle so an out-of-range index leaves the
        // button un-filled rather than falling back to a neighbouring colour. It
        // cannot be out of range today — the loop bound is the same array — but
        // the fallback would be a silent lie if it ever were.
        if let Some(fill) = layout::bottom_bar_button_background(index) {
            commands.entity(button).insert(fill);
        }

        spawn_label(
            commands,
            button,
            layout::BOTTOM_BAR_GLYPHS[index],
            CreatorText::BUTTON_GLYPH_SIZE_PX,
        );
    }

    group
}

/// A text label as a child of `parent`.
///
/// Takes `&str` and copies it, because the source is either a `String` the
/// caller owns or a `&'static str` from [`CreatorText`], and two call shapes are
/// not worth unifying behind a `Cow`.
fn spawn_label(commands: &mut Commands, parent: Entity, text: &str, size: f32) -> Entity {
    commands
        .spawn((
            Text(text.to_owned()),
            TextFont::from_font_size(size),
            ChildOf(parent),
            CreatorUiPart::Label,
            Name::new("creator-ui-label"),
        ))
        .id()
}
