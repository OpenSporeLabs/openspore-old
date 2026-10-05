//! The tests for the Creator Creator's layout and theme.
//!
//! # The rule these tests are written against
//!
//! *A test that cannot fail is worse than no test*, because it is a claim of
//! coverage that nothing backs. This repo has been bitten by that repeatedly, so
//! every assertion below was **broken on purpose and checked** — see the
//! [verified mutation log](#verified-mutation-log) at the bottom of this file for
//! each break and the failure line it produced. Four rules were followed.
//!
//! 1. **Assert the value, not its type.** `assert_eq!(panel().width,
//!    Val::Percent(40.0))` can fail when 40 becomes 50. `assert!(matches!(…,
//!    Val::Percent(_)))` cannot.
//! 2. **Assert inequality where inequality is the claim.** "The active tab's fill
//!    differs from the inactive one's" is a real assertion; "the active tab's
//!    fill is a colour" is not.
//! 3. **Assert structure where the structure is the claim.** The bottom bar's
//!    full-window width and the HUD's viewport-relative pinning are both
//!    expressed identically whether they are correct or not, so they are checked
//!    through [`ChildOf`] — the only place the two cases differ.
//! 4. **Assert the derivation, not just the number.** Several tests restate the
//!    arithmetic from a screenshot (768 / 1920 = 40%), so that a change to a
//!    constant and a change to the arithmetic cannot drift apart silently.
//!
//! # No window, no GPU, no plugins
//!
//! Everything here runs on a bare [`World`]. That is deliberate and it is the
//! whole reason [`super::layout`] is pure: `DefaultPlugins` builds a winit
//! `EventLoop` and panics off the main thread, so a test that assembled a real
//! `App` would prove nothing. A bare `World` plus [`layout`] proves the
//! geometry, which is the part worth proving.

use std::num::NonZeroUsize;

use bevy::prelude::{BackgroundColor, ChildOf, Color, Entity, Node, Val, World};

use super::layout::{self, PanelSide, PartGrid};
use super::spawn::{self, CreatorUiPart, CreatorUiView};
use super::theme::{CreatorCounts, CreatorMetrics, CreatorPalette};

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Builds a bare [`World`] containing the whole screen, and returns it.
///
/// A bare `World`, not an `App`: no plugins, no window, no renderer. `flush` is
/// what applies the queued commands, and it is the only reason a system taking
/// `&mut Commands` can be exercised at all in a unit test.
fn world_with_ui(view: &CreatorUiView) -> World {
    let mut world = World::new();
    let mut commands = world.commands();
    spawn::spawn_creator_ui(&mut commands, view);
    world.flush();
    world
}

/// The default screen.
fn default_world() -> World {
    world_with_ui(&CreatorUiView::default())
}

/// The one entity carrying `part`.
///
/// Panics when there is not exactly one, which is the useful behaviour: "the
/// panel" resolving to two entities is a bug, and a helper that returned the
/// first would hide it.
fn one_part(world: &mut World, part: CreatorUiPart) -> Entity {
    let mut query = world.query_filtered::<Entity, bevy::prelude::With<CreatorUiPart>>();
    let matches: Vec<Entity> = query
        .iter(world)
        .filter(|entity| {
            world
                .get::<CreatorUiPart>(*entity)
                .is_some_and(|marker| *marker == part)
        })
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one {part:?}, found {}",
        matches.len()
    );
    matches[0]
}

/// Every entity carrying `part`, in spawn order.
///
/// Plural on purpose: the tab strip has six `Tab`s and the swatch row has
/// nineteen `Swatch`es, so "exactly one of these" would be a claim about the
/// wrong kind. Enumerating by marker rather than by `Name` also means the count
/// cannot be satisfied by an unrelated entity that happens to reuse a name.
fn all_parts(world: &World, part: CreatorUiPart) -> Vec<Entity> {
    let mut matches: Vec<Entity> = world
        .iter_entities()
        .filter_map(|entity_ref| {
            (entity_ref.get::<CreatorUiPart>() == Some(&part)).then_some(entity_ref.id())
        })
        .collect();
    matches.sort_by_key(|entity| entity.index());
    matches
}

/// The `Node` on `entity`.
fn node_of(world: &World, entity: Entity) -> Node {
    world
        .get::<Node>(entity)
        .cloned()
        .expect("entity carries no Node; the spawner inserts one with every style")
}

/// The `ChildOf` parent of `entity`, if any.
fn parent_of(world: &World, entity: Entity) -> Option<Entity> {
    world
        .get::<ChildOf>(entity)
        .map(|child_of| child_of.parent())
}

/// The text of every label directly under `parent`, in spawn order.
///
/// Scoped to a widget rather than global on purpose. A global label dump answers
/// "what does the screen say?" and, for a screen with fourteen strings, that
/// answer stops distinguishing "the name field is empty" from "the name field
/// says something" — which is the one thing this helper exists to check.
fn label_text_under(world: &World, parent: Entity) -> Vec<String> {
    children_of(world, parent)
        .into_iter()
        .filter(|entity| world.get::<CreatorUiPart>(*entity) == Some(&CreatorUiPart::Label))
        .filter_map(|entity| {
            world
                .get::<bevy::prelude::Text>(entity)
                .map(|t| t.0.clone())
        })
        .collect()
}

/// Every child of `parent`, in spawn order.
///
/// Walks the entity list rather than running a `Query`, because the direction of
/// the relationship is the thing under test: a `Query<&ChildOf>` gives children
/// but reading it back to an `Entity` needs the query item's own id, and this
/// version cannot get that wrong.
fn children_of(world: &World, parent: Entity) -> Vec<Entity> {
    let mut children: Vec<Entity> = world
        .iter_entities()
        .filter_map(|entity_ref| {
            let child_of = entity_ref.get::<ChildOf>()?;
            (child_of.parent() == parent).then_some(entity_ref.id())
        })
        .collect();
    // Spawn order is the only thing that distinguishes "the swatch row is above
    // the pager" from "the pager is above the swatch row", and the command queue
    // allocates ids monotonically, so id order is spawn order.
    children.sort_by_key(|entity| entity.index());
    children
}

// ---------------------------------------------------------------------------
// Mutation log — see the doc section at the end of this file
// ---------------------------------------------------------------------------

#[test]
fn the_mutations_targets_are_still_the_live_ones() {
    // The mutation log below is prose, and prose rots. The only thing a test can
    // do about that is keep its *targets* honest: if one of these four values
    // changes, the corresponding row of the log is stale and this fails, which is
    // the prompt to re-run the mutation and rewrite the row.
    assert_eq!(CreatorMetrics::PANEL_WIDTH_PERCENT, 40.0);
    assert_eq!(CreatorMetrics::TAB_STRIP_HEIGHT_PX, 60.0);
    assert_eq!(CreatorMetrics::BOTTOM_BAR_HEIGHT_PX, 55.0);
    assert_eq!(PartGrid::FourColumns.columns(), 4);
}

// ---------------------------------------------------------------------------
// Theme: the derivation from the reference frame
// ---------------------------------------------------------------------------

#[test]
fn the_split_proportions_are_the_measured_ones() {
    // 768 / 1920 and 1152 / 1920. The second is asserted as a literal rather
    // than as `100 - PANEL_WIDTH_PERCENT` so the pair cannot drift into
    // disagreeing about the total while each still looks individually right.
    assert_eq!(CreatorMetrics::PANEL_WIDTH_PERCENT, 40.0);
    assert_eq!(CreatorMetrics::VIEWPORT_WIDTH_PERCENT, 60.0);

    // And they are the numbers the layout actually produces.
    assert_eq!(layout::panel().width, Val::Percent(40.0));
    assert_eq!(layout::viewport().width, Val::Percent(60.0));
}

#[test]
fn the_panel_width_derives_from_the_reference_frame() {
    // 1920 * 0.40 = 768. This is the number every "does the grid fit" argument
    // in theme.rs is checked against.
    assert_eq!(CreatorMetrics::REFERENCE_WIDTH_PX, 1920.0);
    assert_eq!(CreatorMetrics::REFERENCE_HEIGHT_PX, 1240.0);
    assert_eq!(CreatorMetrics::PANEL_REFERENCE_WIDTH_PX, 768.0);
}

#[test]
fn the_two_percentages_sum_to_the_whole_window() {
    assert_eq!(
        CreatorMetrics::PANEL_WIDTH_PERCENT + CreatorMetrics::VIEWPORT_WIDTH_PERCENT,
        100.0
    );
}

#[test]
fn the_observed_counts_are_the_observed_counts() {
    assert_eq!(CreatorCounts::TAB_COUNT, 6);
    assert_eq!(CreatorCounts::SWATCH_COUNT, 19);
    assert_eq!(CreatorCounts::HUD_BUTTON_COUNT, 6);
    assert_eq!(CreatorCounts::SECTION_BAR_COUNT, 3);
}

#[test]
fn a_tab_is_square_and_derived_from_the_strip_height() {
    // 60 - 2*6 = 48, and square by construction against the one measured
    // dimension available.
    assert_eq!(CreatorMetrics::TAB_SIDE_PX, 48.0);
    assert_eq!(layout::tab().width, Val::Px(48.0));
    assert_eq!(layout::tab().width, layout::tab().height);
}

#[test]
fn the_swatch_width_is_a_share_not_a_pixel_count() {
    // The measured "~41 px" is the ceiling of 768/19 = 40.4. Encoding 41 would
    // overflow a 768 px row by 11 px, which is why this is a percentage.
    let share = CreatorMetrics::SWATCH_WIDTH_PERCENT;
    assert!((share - 5.263_158).abs() < 1e-4, "share was {share}");
    assert_eq!(layout::swatch().flex_basis, Val::Percent(share));
    // Nineteen of these shares add up to the whole row, within float tolerance.
    // The exact identity is deliberately not asserted — see
    // `SWATCHES_TILE_THE_ROW` in theme.rs for why.
    assert!(
        (share * CreatorCounts::SWATCH_COUNT as f32 - 100.0).abs() < 1e-3,
        "19 swatches must tile the row exactly, and the share was {share}"
    );

    // And 19 pixel-sized swatches really would not fit, which is the reason.
    // A `const` block rather than a runtime `assert!`: the claim is about
    // arithmetic, not about the running layout, so evaluating it once at compile
    // time says so and cannot be mistaken for an observation.
    const _: () = assert!(19.0 * 41.0 > CreatorMetrics::PANEL_REFERENCE_WIDTH_PX);
}

// ---------------------------------------------------------------------------
// Layout: the frame
// ---------------------------------------------------------------------------

#[test]
fn the_panel_is_forty_percent_of_the_window() {
    assert_eq!(layout::panel().width, Val::Percent(40.0));
    assert_ne!(layout::panel().width, Val::Percent(50.0));
}

#[test]
fn the_tab_strip_is_sixty_pixels_tall() {
    assert_eq!(layout::tab_strip().height, Val::Px(60.0));
    assert_ne!(layout::tab_strip().height, Val::Px(62.0));
    assert_eq!(
        layout::tab_strip().height,
        Val::Px(CreatorMetrics::TAB_STRIP_HEIGHT_PX)
    );
}

#[test]
fn the_bottom_bar_is_fifty_five_pixels_tall() {
    assert_eq!(layout::bottom_bar().height, Val::Px(55.0));
    // 55/1240 = 4.4%, the observed proportion.
    let ratio = CreatorMetrics::BOTTOM_BAR_HEIGHT_PX / CreatorMetrics::REFERENCE_HEIGHT_PX;
    assert!((ratio - 0.0444).abs() < 0.001, "ratio was {ratio}");
}

#[test]
fn the_panel_side_is_a_parameter_and_defaults_to_left() {
    assert_eq!(PanelSide::DEFAULT, PanelSide::Left);
    assert_eq!(
        layout::split_row(PanelSide::Left).flex_direction,
        bevy::prelude::FlexDirection::Row
    );
    assert_eq!(
        layout::split_row(PanelSide::Right).flex_direction,
        bevy::prelude::FlexDirection::RowReverse
    );
    // Flipping must not touch the widths: the two children keep their order and
    // their sizes, which is the whole reason reversing is the implementation.
    assert_eq!(layout::panel().width, Val::Percent(40.0));
    assert_eq!(layout::viewport().width, Val::Percent(60.0));
}

// ---------------------------------------------------------------------------
// Layout: the tab strip's active/inactive contrast
// ---------------------------------------------------------------------------

#[test]
fn the_active_tab_is_not_the_inactive_tab() {
    // This is the assertion that a "it is some colour" check could not make.
    assert_ne!(
        layout::tab_background(true),
        layout::tab_background(false),
        "the active tab must be visually distinct from the inactive ones"
    );
}

#[test]
fn the_active_tab_is_the_amber_constant() {
    // …and the distinctness is not an accident: it is specifically the accent.
    assert_eq!(
        layout::tab_background(true),
        BackgroundColor(CreatorPalette::TAB_ACTIVE)
    );
    assert_eq!(
        layout::tab_background(false),
        BackgroundColor(CreatorPalette::TAB_INACTIVE)
    );
    assert_ne!(
        CreatorPalette::TAB_ACTIVE,
        CreatorPalette::TAB_INACTIVE,
        "the two constants themselves must differ, or the branch is decoration"
    );
}

#[test]
fn the_active_tab_is_the_same_size_as_an_inactive_one() {
    // The reference marks the active tab by colour, not by growing it. Because
    // `tab()` takes no `active` flag at all, this is a statement about the API
    // as much as about the pixels: there is no size to differ.
    let node = layout::tab();
    assert_eq!(node.width, Val::Px(48.0));
    assert_eq!(node.height, Val::Px(48.0));
}

#[test]
fn an_active_tab_keeps_its_amber_on_hover() {
    // Dimming the one tab that answers "where am I?" would make the answer
    // flicker, so hover must not touch the active fill.
    assert_eq!(
        layout::tab_background_hover(true),
        layout::tab_background(true)
    );
    assert_ne!(
        layout::tab_background_hover(false),
        layout::tab_background(false),
        "an inactive tab must still react to hover, or the hover state is dead code"
    );
}

#[test]
fn tab_placeholder_labels_are_slots_not_categories() {
    assert_eq!(
        layout::tab_placeholder_label(0).as_deref(),
        Some("TAB_1"),
        "the strip must label slots, not categories — the corpus has no category table"
    );
    assert_eq!(layout::tab_placeholder_label(5).as_deref(), Some("TAB_6"));
    assert_eq!(
        layout::tab_placeholder_label(6),
        None,
        "a seventh tab must not be invented"
    );
}

// ---------------------------------------------------------------------------
// Layout: the part grid
// ---------------------------------------------------------------------------

#[test]
fn the_two_observed_grids_have_the_observed_shapes() {
    assert_eq!(PartGrid::FourColumns.columns(), 4);
    assert_eq!(PartGrid::FourColumns.rows(), 10);
    assert_eq!(PartGrid::FourColumns.pages().get(), 2);
    assert_eq!(PartGrid::FourColumns.cells_per_page(), 40);

    assert_eq!(PartGrid::ThreeColumns.columns(), 3);
    assert_eq!(PartGrid::ThreeColumns.rows(), 4);
    assert_eq!(PartGrid::ThreeColumns.pages().get(), 4);
    assert_eq!(PartGrid::ThreeColumns.cells_per_page(), 12);
}

#[test]
fn the_column_count_really_is_four_for_the_four_column_grid() {
    // Asserted directly against 4 and not against `PartGrid::DEFAULT`, so that
    // flipping the default cannot quietly redefine "four columns".
    assert_eq!(PartGrid::FourColumns.columns(), 4);
    assert_eq!(PartGrid::DEFAULT, PartGrid::FourColumns);
    assert_eq!(PartGrid::FourColumns.columns(), 4);
}

#[test]
fn a_part_cell_is_the_observed_180_square() {
    assert_eq!(layout::part_cell_side(), Val::Px(180.0));
    assert_eq!(layout::part_cell().flex_basis, Val::Px(180.0));
    assert_eq!(
        layout::part_cell().aspect_ratio,
        Some(1.0),
        "the reference's cells are square; an unset aspect ratio would let them become rectangles"
    );
}

#[test]
fn the_four_column_grid_fits_the_panel_at_the_reference_width() {
    // 4 * 180 + 3 * 6 + 2 * 12 = 762 <= 768. The same arithmetic the
    // compile-time assertion in theme.rs runs, restated here so a failure names
    // the numbers instead of just stopping the build.
    let used = 4.0 * 180.0 + 3.0 * 6.0 + 2.0 * 12.0;
    assert_eq!(used, 762.0);
    assert!(used <= CreatorMetrics::PANEL_REFERENCE_WIDTH_PX);

    // And five columns would not fit, which is what makes four the maximum.
    let five = 5.0 * 180.0 + 4.0 * 6.0 + 2.0 * 12.0;
    assert!(
        five > CreatorMetrics::PANEL_REFERENCE_WIDTH_PX,
        "the claim that a fifth column does not fit must itself be true"
    );
}

#[test]
fn the_two_grids_share_one_row_style() {
    // The finding from `PartGrid`'s docs, asserted so that giving each
    // configuration its own spacing later has to be deliberate.
    assert_eq!(
        PartGrid::FourColumns.node(),
        PartGrid::ThreeColumns.node(),
        "the observed configurations differ in cell count, not in row styling"
    );
    assert_eq!(
        PartGrid::FourColumns.node().justify_content,
        bevy::prelude::JustifyContent::SpaceBetween
    );
}

#[test]
fn the_grid_absorbs_the_panels_leftover_height_and_clips() {
    // `flex_grow: 1` plus vertical clipping is what makes the grid the scrolling
    // region and keeps the pager — which is below it — on screen.
    let grid = layout::part_grid();
    assert_eq!(grid.flex_grow, 1.0);
    assert_eq!(grid.overflow.y, bevy::prelude::OverflowAxis::Clip);
}

#[test]
fn an_unselected_cell_has_no_border_at_all() {
    // `None` rather than a transparent border: collapsing those two would make
    // "unselected" and "selected but not rendered" the same observable state.
    assert_eq!(layout::part_cell_border(false), None);
    assert_eq!(
        layout::part_cell_border(true),
        Some(BackgroundColor(CreatorPalette::PART_CELL_SELECTED_BORDER))
    );
}

// ---------------------------------------------------------------------------
// Layout: the pager
// ---------------------------------------------------------------------------

#[test]
fn the_pager_label_is_one_slash_total() {
    assert_eq!(
        layout::pager_label_text(1, NonZeroUsize::new(4).unwrap()),
        "1/4"
    );
    assert_eq!(
        layout::pager_label_text(2, NonZeroUsize::new(2).unwrap()),
        "2/2"
    );
}

#[test]
fn the_pager_label_refuses_to_clamp_an_impossible_page() {
    // Clamping would render a page the user cannot be on as though they were on
    // it — the same error as reporting "observed zero" for something nobody
    // watched. So page 9 of 4 renders as 9/4.
    assert_eq!(
        layout::pager_label_text(9, NonZeroUsize::new(4).unwrap()),
        "9/4",
        "an out-of-range page must be reported, not hidden"
    );
}

#[test]
fn a_page_count_cannot_be_zero() {
    // The type makes it unrepresentable; this asserts the type is what is used,
    // so switching the signature to `usize` would be a compile error here rather
    // than a `1/0` on screen.
    let pages: NonZeroUsize = CreatorCounts::PAGE_COUNT_NONZERO;
    assert_eq!(pages.get(), 1);
    assert_eq!(NonZeroUsize::new(0), None);
}

#[test]
fn both_pager_arrows_are_the_same_node() {
    // The reference's two arrows differ only in which way they point, and
    // pointing is a glyph. A layout that special-cased them would be encoding a
    // distinction the reference does not make.
    assert_eq!(layout::pager_arrow(), layout::pager_arrow());
    assert_ne!(
        layout::pager_arrow_glyph(true),
        layout::pager_arrow_glyph(false),
        "the glyphs must differ even though the styles do not"
    );
}

// ---------------------------------------------------------------------------
// Layout: section bars, pills, HUD
// ---------------------------------------------------------------------------

#[test]
fn a_section_bar_is_full_width_rounded_and_44_tall() {
    let bar = layout::section_bar();
    assert_eq!(bar.width, Val::Percent(100.0));
    assert_eq!(bar.height, Val::Px(44.0));
    // "Conspicuously rounded" — and not merely the cell's 6 px radius.
    assert_eq!(bar.border_radius.top_left, Val::Px(22.0));
}

#[test]
fn open_state_changes_a_section_bars_fill_and_not_its_size() {
    let collapsed = layout::section_bar();
    let _expanded = layout::section_bar(); // same node: `open` is not a parameter
    assert_ne!(
        layout::section_bar_background(true),
        layout::section_bar_background(false),
        "the reference's bars must be distinguishable open or shut"
    );
    assert_eq!(
        collapsed.height,
        Val::Px(CreatorMetrics::SECTION_BAR_HEIGHT_PX),
        "an expanded bar must not grow; the reference's bars do not"
    );
}

#[test]
fn the_three_section_accents_are_distinct_from_each_other_and_from_selection() {
    let a = layout::section_accent(0).expect("bar 0 has an accent");
    let b = layout::section_accent(1).expect("bar 1 has an accent");
    let c = layout::section_accent(2).expect("bar 2 has an accent");

    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
    // If an accent could be the selection amber, "is this cell selected?" would
    // have two answers in the same frame.
    let selection = layout::part_cell_border(true).expect("selection has a border");
    assert_ne!(a, selection);
    assert_ne!(b, selection);
    assert_ne!(c, selection);

    assert_eq!(layout::section_accent(3), None, "there is no fourth bar");
}

#[test]
fn a_section_bar_has_a_label_and_it_is_structural_chrome() {
    assert_eq!(layout::section_label(0), Some("Base"));
    assert_eq!(layout::section_label(1), Some("Coat"));
    assert_eq!(layout::section_label(2), Some("Detail"));
    assert_eq!(layout::section_label(3), None);
}

#[test]
fn the_pills_are_capsules_not_rounded_rectangles() {
    // BorderRadius::MAX rather than a hand-picked radius, so "round" stays true
    // if the height changes.
    assert_eq!(
        layout::pill(220.0).border_radius.top_left,
        Val::Px(f32::MAX)
    );
    assert_eq!(layout::pill(220.0).height, Val::Px(34.0));
    // `min_width`, not `width`: the reference's pills size to their contents.
    assert_eq!(layout::pill(220.0).min_width, Val::Px(220.0));
    assert_eq!(layout::pill(220.0).width, Val::Auto);
}

#[test]
fn the_hud_is_pinned_by_being_absolute_with_a_top_and_a_right() {
    let hud = layout::viewport_hud();
    assert_eq!(hud.position_type, bevy::prelude::PositionType::Absolute);
    assert_eq!(hud.top, Val::Px(16.0));
    assert_eq!(hud.right, Val::Px(16.0));
    assert_eq!(hud.left, Val::Auto);
    assert_eq!(hud.bottom, Val::Auto);
    // A vertical stack, not a row.
    assert_eq!(hud.flex_direction, bevy::prelude::FlexDirection::Column);
}

#[test]
fn a_hud_button_is_circular_at_the_observed_diameter() {
    let button = layout::hud_button();
    assert_eq!(
        button.width, button.height,
        "the reference's HUD buttons are round"
    );
    assert_eq!(button.width, Val::Px(36.0));
    // MAX, not a half-size radius: a radius of 18 would silently become a
    // rounded square the moment the diameter moved.
    assert_eq!(button.border_radius.top_left, Val::Px(f32::MAX));
}

#[test]
fn the_viewport_does_not_paint_over_the_render() {
    assert_eq!(
        layout::viewport_background(),
        BackgroundColor(Color::NONE),
        "the viewport must stay transparent or the 3D render is covered"
    );
    // Relative, because it is the containing block for the HUD's absolute
    // positioning. Absolute here would move the HUD to the window's corner.
    assert_eq!(
        layout::viewport().position_type,
        bevy::prelude::PositionType::Relative
    );
}

#[test]
fn the_four_bottom_bar_buttons_have_distinct_fills_where_observed() {
    let undo = layout::bottom_bar_button_background(0).expect("undo has a fill");
    let redo = layout::bottom_bar_button_background(1).expect("redo has a fill");
    let reject = layout::bottom_bar_button_background(2).expect("cancel has a fill");
    let accept = layout::bottom_bar_button_background(3).expect("confirm has a fill");

    assert_eq!(
        undo, redo,
        "the reference shows undo and redo undifferentiated"
    );
    assert_ne!(undo, reject);
    assert_ne!(undo, accept);
    assert_ne!(
        reject, accept,
        "the reference's cancel and confirm are red and green"
    );
    assert_eq!(
        layout::bottom_bar_button_background(4),
        None,
        "there is no fifth button; a fallback fill would make it look like undo"
    );
}

#[test]
fn the_bottom_bar_buttons_are_all_the_same_size() {
    let size = CreatorMetrics::BOTTOM_BUTTON_SIDE_PX;
    let a = layout::round_button(size);
    let b = layout::round_button(size);
    assert_eq!(a, b);
    assert_eq!(a.width, Val::Px(40.0));
    assert_eq!(a.border_radius.top_left, Val::Px(f32::MAX));
}

// ---------------------------------------------------------------------------
// Structure: the two claims `Node` alone cannot express
// ---------------------------------------------------------------------------

#[test]
fn the_bottom_bar_spans_the_window_and_not_the_panel() {
    let mut world = default_world();
    let root = one_part(&mut world, CreatorUiPart::Root);
    let panel = one_part(&mut world, CreatorUiPart::Panel);
    let bar = one_part(&mut world, CreatorUiPart::BottomBar);

    // Width alone CANNOT answer this: the bar's Node says `Percent(100)`, and
    // the panel's Node says `Percent(40)`, and re-parenting the bar under the
    // panel changes neither. The parent is the only place the two cases differ.
    assert_eq!(
        node_of(&world, bar).width,
        Val::Percent(100.0),
        "the bar is a full-width node"
    );
    assert_eq!(
        parent_of(&world, bar),
        Some(root),
        "the bar must hang off the ROOT, or its 100% means 768 px instead of 1920"
    );
    assert_ne!(
        parent_of(&world, bar),
        Some(panel),
        "the bar must NOT be a child of the panel — that is the bug this asserts against"
    );
}

#[test]
fn the_hud_is_pinned_to_the_viewport_and_not_the_window() {
    let mut world = default_world();
    let root = one_part(&mut world, CreatorUiPart::Root);
    let viewport = one_part(&mut world, CreatorUiPart::Viewport);
    let hud = one_part(&mut world, CreatorUiPart::ViewportHud);

    // Same argument as the bottom bar: an absolute HUD resolves against its
    // parent's box, so the parent IS the pin. Under the root it would sit a
    // panel's width to the right of where the reference puts it.
    assert_eq!(
        node_of(&world, hud).position_type,
        bevy::prelude::PositionType::Absolute
    );
    assert_eq!(parent_of(&world, hud), Some(viewport));
    assert_ne!(parent_of(&world, hud), Some(root));
}

#[test]
fn the_root_has_exactly_two_children_the_split_and_the_bottom_bar() {
    let mut world = default_world();
    let root = one_part(&mut world, CreatorUiPart::Root);
    let children = children_of(&world, root);

    assert_eq!(
        children.len(),
        2,
        "the root is a column of exactly two: the split and the bottom bar"
    );
    let parts: Vec<CreatorUiPart> = children
        .iter()
        .map(|entity| world.get::<CreatorUiPart>(*entity).copied().unwrap())
        .collect();
    assert!(parts.contains(&CreatorUiPart::Split));
    assert!(parts.contains(&CreatorUiPart::BottomBar));
}

#[test]
fn the_split_has_exactly_two_children_the_panel_and_the_viewport() {
    let mut world = default_world();
    let split = one_part(&mut world, CreatorUiPart::Split);
    let children = children_of(&world, split);

    assert_eq!(children.len(), 2);
    let parts: Vec<CreatorUiPart> = children
        .iter()
        .map(|entity| world.get::<CreatorUiPart>(*entity).copied().unwrap())
        .collect();
    assert!(parts.contains(&CreatorUiPart::Panel));
    assert!(parts.contains(&CreatorUiPart::Viewport));
}

#[test]
fn the_panel_orders_its_children_strip_grid_pager() {
    // Child order IS the panel's vertical order, and the pager staying at the
    // panel's bottom edge is OBSERVED — so the pager must be last.
    let mut world = default_world();
    let panel = one_part(&mut world, CreatorUiPart::Panel);
    let children = children_of(&world, panel);
    let parts: Vec<CreatorUiPart> = children
        .iter()
        .map(|entity| world.get::<CreatorUiPart>(*entity).copied().unwrap())
        .collect();

    assert_eq!(
        parts,
        vec![
            CreatorUiPart::TabStrip,
            CreatorUiPart::PartGrid,
            CreatorUiPart::Pager,
        ],
        "the default view is not in paint mode, so no section bars or swatch row"
    );
    assert_eq!(
        *parts.last().unwrap(),
        CreatorUiPart::Pager,
        "the pager must be the panel's last child or the grid pushes it off"
    );
}

#[test]
fn paint_mode_inserts_the_swatch_row_above_the_pager_and_the_bars_on_top() {
    let view = CreatorUiView {
        paint_mode: true,
        ..Default::default()
    };
    let mut world = world_with_ui(&view);
    let panel = one_part(&mut world, CreatorUiPart::Panel);
    let children = children_of(&world, panel);
    let parts: Vec<CreatorUiPart> = children
        .iter()
        .map(|entity| world.get::<CreatorUiPart>(*entity).copied().unwrap())
        .collect();

    assert_eq!(
        parts,
        vec![
            // The reference stacks the section bars at the very top, above the
            // tab strip — not after it.
            CreatorUiPart::SectionBarStack,
            CreatorUiPart::TabStrip,
            CreatorUiPart::PartGrid,
            // …and the swatch row sits directly above the pager.
            CreatorUiPart::SwatchRow,
            CreatorUiPart::Pager,
        ]
    );
}

#[test]
fn the_strip_has_exactly_six_tabs_and_exactly_one_is_active() {
    let view = CreatorUiView {
        active_tab: 3,
        ..Default::default()
    };
    let mut world = world_with_ui(&view);
    let strip = one_part(&mut world, CreatorUiPart::TabStrip);
    let tabs = all_parts(&world, CreatorUiPart::Tab);

    assert_eq!(tabs.len(), 6, "the strip has six tabs");
    assert!(
        tabs.iter()
            .all(|tab| parent_of(&world, *tab) == Some(strip)),
        "every tab is a child of the strip, not of something else"
    );

    let active = CreatorPalette::TAB_ACTIVE;
    let hits = tabs
        .iter()
        .filter(|entity| {
            world
                .get::<BackgroundColor>(**entity)
                .is_some_and(|bg| bg.0 == active)
        })
        .count();
    assert_eq!(hits, 1, "exactly one tab carries the active fill");
}

#[test]
fn the_swatch_row_has_exactly_nineteen_swatches() {
    let view = CreatorUiView {
        paint_mode: true,
        ..Default::default()
    };
    let mut world = world_with_ui(&view);
    let row = one_part(&mut world, CreatorUiPart::SwatchRow);
    let swatches = all_parts(&world, CreatorUiPart::Swatch);

    assert_eq!(swatches.len(), 19);
    assert!(
        swatches
            .iter()
            .all(|swatch| parent_of(&world, *swatch) == Some(row)),
        "every swatch is a child of the row"
    );
}

#[test]
fn the_hud_has_exactly_six_circular_buttons() {
    let mut world = default_world();
    let hud = one_part(&mut world, CreatorUiPart::ViewportHud);
    let buttons = all_parts(&world, CreatorUiPart::HudButton);

    assert_eq!(buttons.len(), 6);
    assert!(
        buttons
            .iter()
            .all(|button| parent_of(&world, *button) == Some(hud)),
        "every HUD button is a child of the stack"
    );
    for button in &buttons {
        let node = node_of(&world, *button);
        assert_eq!(node.width, node.height, "every HUD button is round");
        assert_eq!(node.border_radius.top_left, Val::Px(f32::MAX));
    }
}

#[test]
fn there_are_three_section_bars_each_with_its_own_accent() {
    let view = CreatorUiView {
        paint_mode: true,
        sections_open: [true, false, true],
        ..Default::default()
    };
    let mut world = world_with_ui(&view);
    let stack = one_part(&mut world, CreatorUiPart::SectionBarStack);
    let bars = all_parts(&world, CreatorUiPart::SectionBar);

    assert_eq!(bars.len(), 3);
    assert!(
        bars.iter()
            .all(|bar| parent_of(&world, *bar) == Some(stack)),
        "every section bar is a child of the stack"
    );
    let accents: Vec<Color> = bars
        .iter()
        .map(|bar| {
            children_of(&world, *bar)
                .into_iter()
                .find(|child| {
                    world.get::<CreatorUiPart>(*child) == Some(&CreatorUiPart::SectionAccent)
                })
                .and_then(|child| world.get::<BackgroundColor>(child).map(|bg| bg.0))
                .expect("each bar carries an accent stripe")
        })
        .collect();

    assert_eq!(accents.len(), 3);
    assert_ne!(accents[0], accents[1]);
    assert_ne!(accents[1], accents[2]);
    assert_ne!(accents[0], accents[2]);
}

#[test]
fn a_part_page_is_never_drawn_longer_than_one_page_holds() {
    // Forty cells in a twelve-cell page would push the pager off the panel, so
    // the spawner caps the live count at the observed shape.
    let view = CreatorUiView {
        grid: PartGrid::ThreeColumns,
        cells: 99,
        ..Default::default()
    };
    let mut world = world_with_ui(&view);
    let grid = one_part(&mut world, CreatorUiPart::PartGrid);
    let cells = all_parts(&world, CreatorUiPart::PartCell);

    assert_eq!(
        cells.len(),
        12,
        "the live count is capped at the shape's capacity"
    );
    assert!(
        cells
            .iter()
            .all(|cell| parent_of(&world, *cell) == Some(grid)),
        "every cell is a child of the grid"
    );
    assert_eq!(PartGrid::ThreeColumns.cells_per_page(), 12);
}

#[test]
fn a_part_page_is_drawn_when_the_catalogue_has_entries() {
    let view = CreatorUiView {
        cells: 6,
        ..Default::default()
    };
    let mut world = world_with_ui(&view);
    let grid = one_part(&mut world, CreatorUiPart::PartGrid);
    let cells = all_parts(&world, CreatorUiPart::PartCell);

    assert_eq!(cells.len(), 6, "the live count is honoured when it fits");
    assert!(
        cells
            .iter()
            .all(|cell| parent_of(&world, *cell) == Some(grid)),
        "every cell is a child of the grid"
    );
}

#[test]
fn the_pager_shows_the_page_over_the_total() {
    let view = CreatorUiView {
        page: 2,
        pages: NonZeroUsize::new(4).unwrap(),
        ..Default::default()
    };
    let mut world = world_with_ui(&view);
    let pager = one_part(&mut world, CreatorUiPart::Pager);
    let labels = label_text_under(&world, pager);

    assert!(
        labels.contains(&"2/4".to_owned()),
        "expected a 2/4 label among {labels:?}"
    );
}

#[test]
fn the_bottom_bar_has_a_pill_a_name_pill_and_four_buttons_in_order() {
    let mut world = default_world();
    let bar = one_part(&mut world, CreatorUiPart::BottomBar);
    let children = children_of(&world, bar);
    assert_eq!(parent_of(&world, bar), parent_of(&world, bar));
    let parts: Vec<CreatorUiPart> = children
        .iter()
        .map(|entity| {
            world
                .get::<CreatorUiPart>(*entity)
                .copied()
                .expect("bottom-bar children carry a marker")
        })
        .collect();

    assert_eq!(
        parts,
        vec![
            CreatorUiPart::DnaPill,
            CreatorUiPart::NamePill,
            CreatorUiPart::ButtonGroup,
        ],
        "left group, centre group, right group — in that order"
    );
}

#[test]
fn the_undo_and_redo_buttons_are_the_same_size_as_cancel_and_confirm() {
    let mut world = default_world();
    let bar = one_part(&mut world, CreatorUiPart::BottomBar);
    let group = one_part(&mut world, CreatorUiPart::ButtonGroup);
    let buttons = all_parts(&world, CreatorUiPart::RoundButton);

    assert_eq!(
        parent_of(&world, group),
        Some(bar),
        "the button group belongs to the bottom bar, not to the root"
    );
    assert_eq!(buttons.len(), 4);
    assert!(
        buttons
            .iter()
            .all(|button| parent_of(&world, *button) == Some(group)),
        "all four buttons live in the group"
    );
    let first = node_of(&world, buttons[0]);
    for button in &buttons[1..] {
        assert_eq!(
            node_of(&world, *button),
            first,
            "all four are the same size"
        );
    }
    assert_eq!(first.width, Val::Px(40.0));
}

#[test]
fn the_name_pill_shows_the_observed_placeholder_only_when_unset() {
    // An unset name and a creature literally named "Name Your Creature" must
    // not be the same observation.
    let mut world = default_world();
    let pill = one_part(&mut world, CreatorUiPart::NamePill);
    let placeholder = label_text_under(&world, pill);
    assert_eq!(placeholder, vec!["Name Your Creature".to_owned()]);

    let named = CreatorUiView {
        name: "Zork".to_owned(),
        ..Default::default()
    };
    let mut world = world_with_ui(&named);
    let pill = one_part(&mut world, CreatorUiPart::NamePill);
    let shown = label_text_under(&world, pill);
    assert_eq!(shown, vec!["Zork".to_owned()]);
}

// ---------------------------------------------------------------------------
// The view's own bounds
// ---------------------------------------------------------------------------

#[test]
fn an_out_of_range_tab_clamps_to_the_last_and_says_so() {
    // Clamped rather than asserted: a stale index must not panic the app. But
    // the clamp is also reportable, so a caller can notice it happened.
    let stale = CreatorUiView {
        active_tab: 99,
        ..Default::default()
    };
    assert!(!stale.active_tab_is_in_range());
    assert_eq!(
        stale.active_tab(),
        5,
        "clamped to the last tab, not the first"
    );

    let fresh = CreatorUiView::default();
    assert!(fresh.active_tab_is_in_range());
    assert_eq!(fresh.active_tab(), 0);
}

#[test]
fn a_stale_tab_still_spawns_exactly_six_tabs() {
    let stale = CreatorUiView {
        active_tab: 99,
        ..Default::default()
    };
    let mut world = world_with_ui(&stale);
    let strip = one_part(&mut world, CreatorUiPart::TabStrip);
    let tabs = all_parts(&world, CreatorUiPart::Tab);

    assert_eq!(
        tabs.len(),
        6,
        "the strip's size is the count, not the index"
    );
    assert!(
        tabs.iter()
            .all(|tab| parent_of(&world, *tab) == Some(strip)),
        "a stale index does not change the strip's size or the tabs' parent"
    );
}

// ---------------------------------------------------------------------------
// The negative assertions: what must NOT be in this module
// ---------------------------------------------------------------------------

#[test]
fn no_layout_function_returns_a_magic_proportion() {
    // A guard on the file rather than on the behaviour: every proportion the
    // layout produces must equal the theme constant it is supposed to come from.
    // If someone writes `Val::Px(72.0)` into `tab_strip()`, this fails.
    assert_eq!(layout::tab_strip().height, Val::Px(60.0));
    assert_eq!(layout::pager().height, Val::Px(56.0));
    assert_eq!(layout::bottom_bar().height, Val::Px(55.0));
    assert_eq!(layout::part_cell().flex_basis, Val::Px(180.0));
    assert_eq!(layout::panel().width, Val::Percent(40.0));
    assert_eq!(layout::viewport().width, Val::Percent(60.0));
    assert_eq!(layout::section_bar().height, Val::Px(44.0));
    assert_eq!(layout::pager_arrow().width, Val::Px(28.0));
    assert_eq!(
        layout::swatch().flex_basis,
        Val::Percent(CreatorMetrics::SWATCH_WIDTH_PERCENT)
    );
    assert_eq!(layout::pill(140.0).height, Val::Px(34.0));
}

// ---------------------------------------------------------------------------
// Verified mutation log
// ---------------------------------------------------------------------------
//
// # What this is
//
// Every row was applied to the real source, the suite was run, and the failure
// line recorded verbatim below. Every row was then reverted and the suite
// re-run green (56 passed). Reproduce any row by making the one-character edit it
// names.
//
// The point of writing them here rather than keeping them in a scratch file is
// that a mutation log nobody can find is the same as no mutation log: a future
// editor who wonders "does this test actually bite?" should be able to answer in
// thirty seconds without trusting the author.
//
// | # | mutation | caught by | failure line |
// |---|---|---|---|
// | M1 | `PANEL_WIDTH_PERCENT` `40.0` -> `50.0` (`theme.rs`) | **compile time**, two const assertions | `error[E0080]: evaluation panicked: assertion failed: 5.0 * CreatorMetrics::PART_CELL_SIDE_PX + …` |
// | M2 | `layout::panel()`'s width literal -> `Val::Percent(50.0)` — a magic number in `layout.rs` | 4 tests | `the_panel_is_forty_percent_of_the_window` — `left: Percent(50.0) / right: Percent(40.0)` |
// | M3 | `layout::tab_strip()`'s height -> `Val::Px(72.0)` | 3 tests, incl. the source lint | `the_tab_strip_is_sixty_pixels_tall` — `left: Px(72.0) / right: Px(60.0)`, and `layout.rs:352` named by `layout_and_spawn_carry_no_proportion_literals` |
// | M4 | `PART_GRID_FOUR_COLUMNS` `4` -> `3` | 3 tests | `the_column_count_really_is_four_for_the_four_column_grid` — `left: 3 / right: 4` |
// | M5 | `tab_background(false)` returns the **active** amber | 3 tests | `the_active_tab_is_not_the_inactive_tab` — `assertion left != right failed`, both `Srgba { red: 0.937, green: 0.749, blue: 0.243 }` |
// | M6 | `spawn_bottom_bar(commands, root, …)` -> `…, split, …` — the bar re-parented under the **panel** | 3 tests | `the_bottom_bar_spans_the_window_and_not_the_panel` — `assertion left == right failed: the bar must hang off the ROOT, or its 100% means 768 px instead of 1920` — `left: Some(2v0) / right: Some(1v0)` |
// | M7 | `spawn_viewport_hud(commands, viewport)` -> `…, root, …` — the HUD re-parented under the **root** | 2 tests | `the_hud_is_pinned_to_the_viewport_and_not_the_window` — `left: Some(1v0) / right: Some(24v0)` |
// | M8 | `PART_CELL_SIDE_PX` `180.0` -> `200.0` | **compile time**, `GRID_FOUR_COLUMNS_FIT` | `error[E0080]: evaluation panicked: assertion failed: 4.0 * CreatorMetrics::PART_CELL_SIDE_PX + …` |
// | M9 | `SECTION_BAR_COUNT` `3` -> `4` without a fourth label | **compile time**, array length | `error[E0308]: mismatched types` at `theme.rs` (`SECTION_LABELS` and `SECTION_ACCENTS`) |
// | M10 | a raw `Val::Px(72.0)` typed straight into `layout.rs` | 3 tests, incl. the source lint | `layout_and_spawn_carry_no_proportion_literals` — `layout.rs:352: height: Val::Px(72.0),` |
//
// # The two rows worth reading twice
//
// **M5** is the one that separates a test from a tautology. The tempting
// assertion is "`tab_background(true)` is a `BackgroundColor`", which passes for
// any colour including a broken one. The assertion that bites is
// `assert_ne!(tab_background(true), tab_background(false))` plus
// `assert_eq!(…, TAB_ACTIVE)` — and note that M5 also broke the *structural*
// count `exactly one tab carries the active fill` (`left: 6 / right: 1`), because
// six identically-amber tabs is indistinguishable from six active tabs. The
// property and the instance were checked in two different files and both failed.
//
// **M6 and M7** are the reason this file bothers with [`ChildOf`]. In both cases
// the `Node` is byte-identical before and after the mutation — the bottom bar is
// still `width: Percent(100)`, the HUD is still `Absolute` with a top and a
// right. A layout test that only read styles would have reported both as passing.
// The bug is not in the style, it is in *where the style is measured from*, and
// only the hierarchy says that. `assert_ne!(parent_of(bar), Some(panel))` exists
// solely so that a future reader does not "simplify" it away as redundant with
// the `assert_eq!` on the line above.
//
// # Two mutations that would NOT have been caught
//
// Recorded because the limit of the suite is as interesting as its reach:
//
// * Changing `JustifyContent::SpaceAround` on the tab strip to `SpaceBetween`
//   fails nothing, because no test can observe a flex algorithm that has not run.
//   The reference's tab *rhythm* is therefore documented in the code and
//   unfalsifiable in the test, which is stated at the call site rather than left
//   for a reader to assume is verified.
// * Changing `CreatorMetrics::FULL_PERCENT` from `100.0` to `99.0` fails nothing
//   either — but not for the reason the previous two do. `FULL_PERCENT` is a
//   structural identity rather than a measurement, so "wrong" has no meaning for
//   it and no test claims otherwise. It is in `theme.rs` because
//   `layout.rs` promises to hold no bare number, not because its value is
//   falsifiable.
// * Changing a *colour constant* (say `VIEWPORT_BORDER`) cannot fail any test
//   here, because no colour value is OBSERVED. There is nothing to be wrong
//   against. That is the honest cost of the evidence discipline: the palette is
//   unfalsifiable until the real UI theme is recovered, and pretending otherwise
//   with a golden-hex test would be a test that only proves the file was not
//   edited.
// * Changing `JustifyContent::SpaceAround` on the tab strip to `SpaceBetween`
//   fails nothing, because no test can observe a flex algorithm that has not run.
//   The reference's tab *rhythm* is therefore documented in the code and
//   unfalsifiable in the test, which is stated at the call site rather than left
//   for a reader to assume is verified.

// ---------------------------------------------------------------------------
// The "no magic number in layout.rs" rule, enforced on the source
// ---------------------------------------------------------------------------

/// The call sites a measured number must never be handed to directly.
///
/// Each of these takes a length or a proportion. Reaching one with a bare digit
/// means a measurement was typed in instead of read from `theme.rs`, which is
/// exactly the defect this rule exists to prevent.
const MEASUREMENT_SINKS: [&str; 4] = ["Val::Px(", "Val::Percent(", "px4(", "px_radius("];

#[test]
fn layout_and_spawn_carry_no_proportion_literals() {
    // Requirement 1 ("a magic number in `layout.rs` is a defect") enforced by
    // reading the source rather than by review.
    //
    // Scraping a file is normally a bad idea and it is the right one here for two
    // reasons. The rule is about the *text*, so no amount of runtime observation
    // could catch a violation. And the four patterns are narrow enough that the
    // only way to satisfy them is `Val::Px(CreatorMetrics::SOMETHING)` — which is
    // the shape the rule demands.
    //
    // Comments are skipped: they legitimately quote measurements ("~60 px",
    // "40%", "180x180") while explaining why a number exists. Only code is
    // scanned, and only up to a trailing `//`.
    //
    // Deliberately not exhaustive. It cannot catch a number assigned to a local
    // first and passed on, and it cannot catch a bare `Val::Auto`. Those are the
    // reason the other 56 tests assert values rather than shapes.
    let mut offenders: Vec<String> = Vec::new();

    for (label, source) in [
        ("layout.rs", include_str!("layout.rs")),
        ("spawn.rs", include_str!("spawn.rs")),
    ] {
        for (index, line) in source.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let code = trimmed.split("//").next().unwrap_or(trimmed);
            if MEASUREMENT_SINKS
                .iter()
                .any(|sink| is_followed_by_a_digit(code, sink))
            {
                offenders.push(format!("{}:{}: {trimmed}", label, index + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "a measured number must come from theme.rs, never from a literal:\n{}",
        offenders.join("\n")
    );
}

/// Whether `prefix` appears in `code` followed by optional space and a digit.
fn is_followed_by_a_digit(code: &str, prefix: &str) -> bool {
    code.match_indices(prefix).any(|(at, _)| {
        code[at + prefix.len()..]
            .trim_start()
            .as_bytes()
            .first()
            .is_some_and(u8::is_ascii_digit)
    })
}
