//! # The Creature Creator's visual layout and theme.
//!
//! This module reproduces the *shape* of Spore's Creature Creator screen. It
//! deliberately does **not** reproduce Spore's *content*: no icon, no font, no
//! colour value and no category name in here is Spore's, because none of them
//! have been recovered. See "Evidence" below for what is known and what is not.
//!
//! | module | what it owns | why it is separate |
//! |---|---|---|
//! | [`theme`] | every colour, count and proportion, as a named constant with its provenance in the doc comment | the single place a number may live, so "a magic number in the layout" is checkable by reading one file |
//! | [`layout`] | pure `Node`/`BackgroundColor` builders | no `Commands`, no `Query`, no `Res`: the whole screen's geometry is assertable in a headless test with no window |
//! | [`spawn`] | the `Commands` layer that turns a [`CreatorUiView`] into an entity tree | the only impure part; it owns no decision of its own and every style it applies comes from [`layout`] |
//!
//! ## The split that makes this testable
//!
//! [`layout`] is a set of `fn part_cell(side: f32) -> Node`-shaped functions.
//! `spawn` calls them; the tests call them directly. Nothing in [`layout`]
//! touches the ECS, so `cargo test -p spore-engine` exercises the real
//! geometry without `DefaultPlugins` — which, per the crate's own trap list,
//! panics off the main thread and therefore can never be part of a headless
//! claim in the first place.
//!
//! ## Evidence
//!
//! Three distinct grades appear below, and each constant says which one it is:
//!
//! * **OBSERVED** — measured from the reference screenshots at 1920x1240. These
//!   are *proportions and positions*, not values: "the panel is 40% of the
//!   window width" survives, "the panel is `0x1a1c22`" does not exist.
//! * **UNRECOVERED** — a thing the reference plainly shows but whose exact
//!   value OpenSpore does not have: every colour, every font, every icon, and
//!   every part-category name. Palette `prop` definitions exist, but their
//!   category-to-rigblock links are not decoded, so the six tabs are addressed by position —
//!   [`CreatorCounts::TAB_COUNT`] of them, rendered as the placeholder labels
//!   `TAB_1`..=`TAB_6` — and never by a name.
//! * **OpenSpore's own choice** — a dimension the reference does not pin down
//!   at all, chosen here to be internally consistent and said out loud.
//!
//! The one hard rule this module holds itself to: **a number appears exactly
//! once, in [`theme`], with its provenance in the doc comment.** If you find a
//! bare `180.0` in [`layout`], that is a defect, not a style.

#![forbid(unsafe_code)]

pub mod font;
pub mod layout;
pub mod spawn;
pub mod theme;

#[cfg(test)]
mod tests;

pub use layout::{PanelSide, PartGrid};
pub use spawn::CreatorUiView;
pub use theme::{CreatorCounts, CreatorMetrics, CreatorPalette, CreatorText};
