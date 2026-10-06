//! # The Creator viewport contract: one rectangle, four consumers.
//!
//! The editor draws a 3D creature into the right-hand ~60% of the window and a
//! control panel into the left ~40%. Four things need to agree about where that
//! region is, and before this module existed they each had their own idea:
//!
//! | consumer | what it needed the rectangle for |
//! |---|---|
//! | the 3D camera | to render only into the creature's region |
//! | camera framing | to know how much room the creature actually has |
//! | the click guard | to refuse a press on the panel |
//! | the pick ray | to turn a window cursor into a direction |
//!
//! [`CreatorViewport`] is the one value all four read. It is written from the
//! **laid-out UI node**, never from a theme constant, because the node is the
//! thing Taffy actually resolved against the real window — a percentage in
//! [`crate::editor::ui::theme`] is the *intent*, and the node is the *result*.
//!
//! ## Coordinate spaces, and why this file is so loud about them
//!
//! Five spaces meet here and Bevy does not convert between all of them for you:
//!
//! ```text
//!   Window::cursor_position   LOGICAL px, origin top-left, +y down
//!   UiGlobalTransform         PHYSICAL px, node CENTRE, origin viewport top-left, +y down
//!   ComputedNode::size        PHYSICAL px
//!   Camera::viewport          PHYSICAL px, origin top-left of the RENDER TARGET, +y down
//!   NDC                       [-1, 1] on both axes, +y UP
//! ```
//!
//! The two that bite are **logical vs physical** and **top-left vs bottom-left**.
//! Both are load bearing: a wrong answer in either still renders a plausible
//! picture and only mis-picks or mis-frames, which is the worst failure mode in
//! this crate. So each conversion below names the space it converts *from* and
//! the space it converts *to*, and each one has a test that fails if it is
//! removed.
//!
//! ## Why the rectangle is not simply read off the camera
//!
//! The obvious implementation is to set `Camera::viewport` to the UI node's rect
//! and then let Bevy do the arithmetic. That does not work, and the reason is
//! worth recording because it is invisible in the source:
//!
//! `bevy_ui` lays out against `camera.physical_viewport_size()`, **not** the
//! window. So a `Camera::viewport` on the same camera that draws the UI makes
//! the UI itself resolve against the sub-rect: measured on Bevy 0.19.1 with a
//! 1280x720 window, a `Val::Percent(60)` node measures **768** wide with no
//! viewport and **461** wide once the viewport is 768 wide. Drive that viewport
//! from the node and each frame shrinks the next frame's node — a silent,
//! total layout collapse.
//!
//! That is why the rectangle lives here, in a resource, and why
//! [`crate::editor::plugin`] gives the creature its own camera: the UI camera
//! keeps `viewport: None` and therefore keeps the full-window layout, while the
//! 3D camera takes the rectangle. One rectangle, two cameras, no feedback loop.

use bevy::camera::{Camera, CameraOutputMode, ClearColorConfig, Viewport as CameraViewport};
use bevy::ecs::component::Component;
use bevy::math::{UVec2, Vec2};
use bevy::prelude::*;
use bevy::reflect::Reflect;
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::ui::ComputedNode;

/// The authoritative rectangle of the Creator's 3D region, in **physical** pixels
/// relative to the render target's top-left corner.
///
/// Reflected so BRP can read it: the whole question this module exists to answer
/// — "does the camera's viewport agree with the laid-out UI node?" — is
/// unanswerable from outside without it. It holds OpenSpore-owned plain data
/// only, never a `Viewport` or a `Transform`, because in Bevy 0.19
/// `bevy_camera` and `bevy_transform` register no types for reflection and a
/// query for one fails *silently as zero rows*.
#[derive(Resource, Reflect, Debug, Clone, Copy, PartialEq)]
#[reflect(Resource)]
pub struct CreatorViewport {
    /// Top-left corner, physical px, origin top-left of the render target.
    pub physical_position: [u32; 2],
    /// Width and height, physical px.
    pub physical_size: [u32; 2],
    /// Logical -> physical multiplier this rectangle was measured under.
    ///
    /// Kept beside the rectangle rather than looked up per consumer so the
    /// mapping is a pure function of this resource: one number, one owner, and a
    /// test can state the conversion without a live camera.
    pub scale_factor: f32,
    /// Whether a laid-out UI node was found to derive this from.
    ///
    /// `false` means **the UI had not laid out yet**, which on the first frames is
    /// normal and is not an error. It is emphatically not "the viewport is the
    /// whole window": callers must not silently substitute a default, because a
    /// default full-window rectangle would accept panel clicks and frame against
    /// a region that is not on screen. An absent measurement is
    /// [`crate::creator::Fact::unavailable`]-shaped, not a zero.
    pub available: bool,
}

impl Default for CreatorViewport {
    fn default() -> Self {
        Self {
            physical_position: [0, 0],
            physical_size: [0, 0],
            scale_factor: 1.0,
            available: false,
        }
    }
}

impl CreatorViewport {
    /// Derives the rectangle from a laid-out UI node.
    ///
    /// # Why the centre has to be undone by hand
    ///
    /// [`UiGlobalTransform::translation`] is the node's **centre** in physical
    /// pixels, while the rectangle wants the **top-left** corner. The
    /// subtraction is the whole of the conversion, and getting it wrong puts the
    /// camera viewport half a node up and to the left — which still renders, and
    /// still looks approximately right, and picks everything one node off.
    ///
    /// Bevy's own UI picking backend takes the top-left directly rather than
    /// subtracting from a centre, so this asymmetry is easy to get wrong by
    /// copying the wrong one of its two precedents.
    pub fn from_node(
        node: &ComputedNode,
        transform: &UiGlobalTransform,
        scale_factor: f32,
    ) -> Self {
        let half = node.size * 0.5;
        let min = transform.translation - half;
        Self {
            physical_position: [clamp_physical(min.x), clamp_physical(min.y)],
            physical_size: [clamp_physical(node.size.x), clamp_physical(node.size.y)],
            scale_factor,
            available: true,
        }
    }

    /// The rectangle in **logical** pixels, the space [`Camera`] wants a pointer
    /// in and the space [`bevy::camera::Camera::viewport_to_ndc`] divides by.
    ///
    /// Bevy computes exactly this inside `logical_viewport_rect` (physical
    /// corners, each divided by the scale factor). Doing it here means the gate
    /// and the arithmetic that follows it are derived from one number.
    pub fn logical_rect(&self) -> Option<Rect> {
        if !self.available || !self.usable() {
            return None;
        }
        let scale = self.scale_factor;
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let size = Vec2::new(self.physical_size[0] as f32, self.physical_size[1] as f32) / scale;
        let min = Vec2::new(
            self.physical_position[0] as f32,
            self.physical_position[1] as f32,
        ) / scale;
        Some(Rect::from_corners(min, min + size))
    }

    /// Whether the rectangle can be mapped at all: non-zero on both axes.
    ///
    /// A zero-sized rectangle has no inverse, so every mapping below has to
    /// refuse it rather than divide by zero and hand back `NaN`.
    fn usable(&self) -> bool {
        self.physical_size[0] > 0 && self.physical_size[1] > 0
    }

    /// Maps a **window-space logical cursor** to NDC for this rectangle, or
    /// `None` when the cursor is not inside it.
    ///
    /// # The chain, and who owns which step
    ///
    /// ```text
    ///   cursor (logical, top-left origin, +y down)   <- Window::cursor_position
    ///     -> reject if outside the LOGICAL rectangle  <- this function
    ///     -> relative = (cursor - min) / size         <- physical -> logical
    ///     -> ndc = relative * 2 - 1                  <- [0,1] -> [-1,1]
    ///     -> ndc.y = -ndc.y                           <- top-left -> bottom-left
    /// ```
    ///
    /// The last two lines are Bevy's own `viewport_to_ndc` verbatim
    /// (`bevy_camera-0.19.1/src/camera.rs`), including the comment *"Flip the Y
    /// co-ordinate from the top to the bottom to enter NDC"*. It is reproduced
    /// rather than called because Bevy can only compute it from a live `Camera`,
    /// and a mapping that needs a window and a GPU is a mapping with no test.
    ///
    /// # Boundary behaviour, stated because it was previously undefined
    ///
    /// The rectangle is **closed**: a cursor exactly on any edge or corner maps,
    /// and the maximum edge maps to exactly `+1` (or `-1` after the Y flip).
    /// One logical pixel past any edge is outside. This matches
    /// [`glam::Rect::contains`], which upstream uses for the same gate
    /// (`bevy_picking-0.19.1/src/pointer.rs`), so a cursor that Bevy's own
    /// picking would accept is not rejected here and vice versa.
    ///
    /// Nothing is ever clamped. A cursor outside the rectangle has no honest NDC
    /// and clamping it to the edge would fabricate a click on whatever geometry
    /// happens to sit against that edge — the exact "pick a part somewhere
    /// off-screen" failure the old code refused to risk.
    pub fn cursor_to_ndc(&self, cursor: Vec2) -> Option<Vec2> {
        let rect = self.logical_rect()?;
        if !cursor.is_finite() || !rect.contains(cursor) {
            return None;
        }
        let relative = (cursor - rect.min) / rect.size();
        let mut ndc = relative * 2.0 - Vec2::ONE;
        ndc.y = -ndc.y;
        Some(ndc)
    }

    /// The [`bevy::camera::Viewport`] this rectangle means, ready to hand to the
    /// 3D camera.
    ///
    /// A thin conversion rather than a struct literal at the call site, because
    /// `physical_position` being a `Vec2` in one place and a `UVec2` in another is
    /// exactly the kind of pair that disagrees by a half-pixel.
    pub fn camera_viewport(&self) -> Option<CameraViewport> {
        if !self.available || !self.usable() {
            return None;
        }
        Some(CameraViewport {
            physical_position: UVec2::new(self.physical_position[0], self.physical_position[1]),
            physical_size: UVec2::new(self.physical_size[0], self.physical_size[1]),
            ..default()
        })
    }
}

/// The 3D camera the Creator renders the creature through.
///
/// Deliberately **not** [`MainCamera`](crate::scene::MainCamera). Every
/// `MainCamera` query in this crate is `.single()`- or count-based — five tests
/// and four framing systems — so a second marked camera would make every one of
/// them ambiguous. This marker is also what tells the framing systems which
/// transform to frame, because the creature is rendered by this camera and not by
/// the UI camera.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CreatorViewportCamera;

/// The camera bundle the 3D viewport needs, minus the transform.
///
/// # Every field here is load bearing, and three of them are the opposite of the
/// obvious choice
///
/// - `viewport` — the whole point. Set from [`CreatorViewport`] every frame so a
///   window resize moves it.
/// - `order: 1` — **must differ from the UI camera's `0`.** Bevy raises
///   *"Camera order ambiguities detected"* when two active cameras share an order
///   on one target, and this crate has already paid for that warning once, wearing
///   a framing bug's clothes.
/// - `output_mode.clear_color: ClearColorConfig::None` — **not** the default.
///   Bevy's clear is an attachment-level `LoadOp` applied by
///   `begin_render_pass`, and `set_scissor_rect` runs *after* it: a sub-rect
///   camera with the default clear **erases the entire window**, including the UI
///   camera's output. This one field is what stops the two cameras destroying each
///   other.
/// - `clear_color: ClearColorConfig::Custom(Color::NONE)` — transparent, and
///   **not** `None`. See the inline note: `None` skips the intermediate clear and
///   this camera then replaces its own viewport rect with an uncleared buffer,
///   taking the UI and the painted backdrop with it.
/// - `blend_state: None` — left `None` on purpose. Bevy *auto*-picks replace for
///   the first camera on a target and alpha-blend for later ones
///   (`bevy_core_pipeline-0.19.1/src/upscaling/mod.rs`), which is exactly the
///   layering behaviour needed here. Pinning a blend state opts out of it.
pub fn viewport_camera_bundle(viewport: Option<CameraViewport>) -> impl Bundle {
    (
        Camera3d::default(),
        Camera {
            order: 1,
            viewport,
            // TRANSPARENT, not `None`. These are two different instructions and
            // getting them the wrong way round costs the whole picture:
            //
            //   `None` on the intermediate clear means "do not clear", so this
            //   camera draws into whatever the previous camera left in the shared
            //   intermediate -- and then its output pass *replaces* its viewport
            //   rect with that intermediate. The UI and the painted backdrop that
            //   the UI camera had already put there are erased, and the symptom
            //   is a black region containing only the creature.
            //
            //   `Custom(Color::NONE)` clears the intermediate to fully transparent,
            //   so this camera's contribution is "creature over nothing", and the
            //   auto alpha-blend (see `blend_state` below) lays it over the UI
            //   camera's output. That is what puts the creature in front of the
            //   backdrop rather than replacing it.
            clear_color: ClearColorConfig::Custom(Color::NONE),
            output_mode: CameraOutputMode::Write {
                blend_state: None,
                // `None` here, and this one IS about not clearing: it is the
                // final-target clear, and Bevy applies it attachment-wide rather
                // than scissored to `viewport`. A default clear here would erase
                // the entire window, UI included.
                clear_color: ClearColorConfig::None,
            },
            ..default()
        },
        CreatorViewportCamera,
        // Layer 1, so the creature is drawn once by this camera and not also by
        // the full-window UI camera. `RenderLayers` has no effect on UI: neither
        // `bevy_ui` nor `bevy_ui_render` mentions it, so restricting this camera
        // cannot hide the interface.
        bevy::camera::visibility::RenderLayers::layer(1),
    )
}

/// Physical px, rounded, never negative.
///
/// A UI node can in principle report a negative origin (an over-scrolled or
/// absolutely-positioned node). Clamping to zero is right here because a camera
/// viewport is a `UVec2` and Bevy itself clamps viewports to the render target
/// (`Viewport::clamp_to_size`) — so a negative camera viewport and a clamped one
/// are the same value, and clamping here keeps the published rectangle equal to
/// the rectangle the camera actually used instead of publishing a number Bevy
/// silently overrode.
fn clamp_physical(value: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else {
        value.round() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rectangle matching the measured Creator layout: 60% of a 1280-wide
    /// window, minus the 55px bottom bar.
    fn measured() -> CreatorViewport {
        CreatorViewport {
            physical_position: [512, 0],
            physical_size: [768, 665],
            scale_factor: 1.0,
            available: true,
        }
    }

    fn ndc(viewport: &CreatorViewport, x: f32, y: f32) -> Option<Vec2> {
        viewport.cursor_to_ndc(Vec2::new(x, y))
    }

    // ---- case 1: full-window viewport reduces to the pre-existing behaviour --

    #[test]
    fn a_full_window_viewport_maps_the_whole_window() {
        let viewport = CreatorViewport {
            physical_position: [0, 0],
            physical_size: [1280, 720],
            scale_factor: 1.0,
            available: true,
        };
        // Before this module the picker passed the raw logical cursor straight to
        // `viewport_to_world` and Bevy divided by the full window. Same answer.
        assert_eq!(ndc(&viewport, 640.0, 360.0), Some(Vec2::ZERO));
        assert_eq!(ndc(&viewport, 0.0, 0.0), Some(Vec2::new(-1.0, 1.0)));
        // Bottom-RIGHT, so NDC y is -1: the viewport's y grows downward and NDC's
        // grows upward. This is the single assertion most likely to be written
        // backwards, which is why it is here rather than left implicit.
        assert_eq!(ndc(&viewport, 1280.0, 720.0), Some(Vec2::new(1.0, -1.0)));
        assert_eq!(ndc(&viewport, 1280.1, 360.0), None);
    }

    // ---- case 2: offset viewport --

    #[test]
    fn an_offset_viewport_ignores_the_offset_for_its_own_centre() {
        let viewport = measured();
        // The centre of the RIGHT-hand region, not of the window.
        assert_eq!(ndc(&viewport, 896.0, 332.5), Some(Vec2::ZERO));
        // ...whereas the same window point is far LEFT of centre in viewport space:
        // it is (640 - 512) / 768 = 0.1667 of the way across, so NDC x is
        // 0.1667 * 2 - 1 = -0.667. A mapping that forgot the viewport's x offset
        // would call this point dead centre, which is the mutation that matters.
        //
        // Compared with a tolerance rather than `assert_eq!`: the value comes from
        // a division, so the last bit is not reproducible, and `-0.0 == 0.0` holds
        // while printing differently -- neither is worth a brittle assertion.
        let got = ndc(&viewport, 640.0, 332.5).expect("inside the viewport");
        assert!(
            (got.x - -2.0 / 3.0).abs() < 1e-5 && got.y.abs() < 1e-5,
            "the window's centre is left of the viewport's centre, got {got}"
        );
    }

    // ---- case 3: smaller right-side viewport, the actual project case --

    #[test]
    fn the_measured_viewport_rejects_the_panel_beside_it() {
        let viewport = measured();
        // The panel is the 40% to the left. Every one of these is inside the
        // window and outside the creature's region.
        for x in [0.0, 1.0, 300.0, 511.0, 511.9] {
            assert_eq!(
                ndc(&viewport, x, 300.0),
                None,
                "x={x} is in the panel and must not produce a ray"
            );
        }
    }

    // ---- case 4: cursor at viewport centre --

    #[test]
    fn the_centre_of_the_viewport_is_exactly_the_ndc_origin() {
        let viewport = measured();
        let centre = ndc(&viewport, 896.0, 332.5).expect("the centre is inside");
        assert!(
            centre.abs().max_element() < 1e-6,
            "the centre must be NDC (0, 0), got {centre}"
        );
        // Explicit geometry, not just "it returned something".
        assert!((centre.x - 0.0).abs() < 1e-6 && (centre.y - 0.0).abs() < 1e-6);
    }

    // ---- case 5: every edge and corner --

    #[test]
    fn every_edge_and_corner_maps_to_the_expected_ndc() {
        let viewport = measured();
        // (min, max) in logical px -> NDC. Top-left is (-1, +1) because NDC's Y
        // is up and the viewport's is down.
        let cases = [
            ((512.0, 0.0), Vec2::new(-1.0, 1.0)),
            ((1280.0, 0.0), Vec2::new(1.0, 1.0)),
            ((512.0, 665.0), Vec2::new(-1.0, -1.0)),
            ((1280.0, 665.0), Vec2::new(1.0, -1.0)),
        ];
        for ((x, y), want) in cases {
            let got = ndc(&viewport, x, y).unwrap_or_else(|| panic!("({x}, {y}) is a corner"));
            assert!(
                got.distance(want) < 1e-4,
                "({x}, {y}) mapped to {got}, expected {want}"
            );
        }
    }

    #[test]
    fn an_edge_is_closed_and_one_pixel_past_it_is_not() {
        let viewport = measured();
        // Left edge: exactly on it is inside, one pixel before is outside.
        assert!(
            ndc(&viewport, 512.0, 300.0).is_some(),
            "left edge is closed"
        );
        assert!(ndc(&viewport, 511.0, 300.0).is_none(), "one px left is out");
        // Right edge.
        assert!(
            ndc(&viewport, 1280.0, 300.0).is_some(),
            "right edge is closed"
        );
        assert!(
            ndc(&viewport, 1281.0, 300.0).is_none(),
            "one px right is out -- and this is the load-bearing one: a camera \
             viewport is a UVec2, so a cursor past it must not be clamped in"
        );
        // Top and bottom, which is where a flipped Y origin would show up.
        assert!(ndc(&viewport, 900.0, 0.0).is_some(), "top edge is closed");
        assert!(ndc(&viewport, 900.0, -1.0).is_none(), "one px above is out");
        assert!(
            ndc(&viewport, 900.0, 665.0).is_some(),
            "bottom edge is closed"
        );
        assert!(
            ndc(&viewport, 900.0, 666.0).is_none(),
            "one px below is out"
        );
    }

    // ---- case 6: outside left/top ----

    #[test]
    fn a_cursor_outside_to_the_left_or_above_is_rejected() {
        let viewport = measured();
        assert_eq!(ndc(&viewport, -10.0, 300.0), None, "left of the window");
        assert_eq!(ndc(&viewport, 300.0, -10.0), None, "above the window");
    }

    // ---- case 7: outside right/bottom ----

    #[test]
    fn a_cursor_outside_to_the_right_or_below_is_rejected() {
        let viewport = measured();
        assert_eq!(ndc(&viewport, 1400.0, 300.0), None, "right of the window");
        assert_eq!(ndc(&viewport, 900.0, 800.0), None, "below the window");
    }

    // ---- case 8: zero-sized and unavailable rectangles ----

    #[test]
    fn a_zero_sized_or_unavailable_viewport_maps_nothing_rather_than_dividing_by_zero() {
        let zero_width = CreatorViewport {
            physical_position: [512, 0],
            physical_size: [0, 665],
            scale_factor: 1.0,
            available: true,
        };
        let zero_height = CreatorViewport {
            physical_size: [768, 0],
            ..zero_width
        };
        let absent = CreatorViewport::default();
        for viewport in [zero_width, zero_height, absent] {
            assert_eq!(
                ndc(&viewport, 600.0, 300.0),
                None,
                "a degenerate rectangle has no inverse and must refuse"
            );
            // `bevy::camera::Viewport` has no `PartialEq`, so this is an `is_none`
            // rather than an equality. Worth knowing: it is also why the runtime
            // disagreement check in the picker compares the two `[u32; 2]` pairs
            // by hand instead of comparing viewports.
            assert!(
                viewport.camera_viewport().is_none(),
                "and no camera viewport"
            );
            assert_eq!(viewport.logical_rect(), None, "and no logical rect");
        }
        // A NaN cursor is refused too: a non-finite point inside a finite rect
        // would otherwise sail through `contains` on some platforms.
        assert_eq!(ndc(&measured(), f32::NAN, 300.0), None);
    }

    #[test]
    fn a_non_positive_scale_factor_is_refused_rather_than_producing_infinities() {
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let viewport = CreatorViewport {
                scale_factor: scale,
                ..measured()
            };
            assert_eq!(
                ndc(&viewport, 900.0, 300.0),
                None,
                "scale {scale} cannot convert physical to logical"
            );
        }
    }

    // ---- case 9: logical/physical conversion ----

    #[test]
    fn a_hidpi_target_divides_the_rectangle_not_just_the_cursor() {
        // Scale 2: the node measures 1536x1330 physical, which is 768x665 logical.
        // A logical cursor must be tested against the LOGICAL rectangle, or every
        // cursor past 768 is rejected -- the exact HiDPI failure recorded on
        // `pointer_in_node_space`.
        let viewport = CreatorViewport {
            physical_position: [1024, 0],
            physical_size: [1536, 1330],
            scale_factor: 2.0,
            available: true,
        };
        let rect = viewport.logical_rect().expect("a usable rectangle");
        assert!(
            rect.min.distance(Vec2::new(512.0, 0.0)) < 1e-4,
            "logical min must be the physical min halved, got {:?}",
            rect.min
        );
        assert!(
            rect.size().distance(Vec2::new(768.0, 665.0)) < 1e-4,
            "logical size must be the physical size halved, got {:?}",
            rect.size()
        );
        // The same window point that is the centre at scale 1 is still the centre.
        assert_eq!(ndc(&viewport, 896.0, 332.5), Some(Vec2::ZERO));
        // The panel boundary is now at 512 *logical*, not 1024.
        assert_eq!(ndc(&viewport, 511.0, 300.0), None, "left of the panel edge");
        assert!(ndc(&viewport, 512.0, 300.0).is_some(), "on the panel edge");
    }

    // ---- case 10: the regression that the old refusal is gone ----

    #[test]
    fn a_valid_in_viewport_click_is_no_longer_the_refused_case() {
        // The old code compared `viewport.physical_position != ZERO ||
        // viewport.physical_size != physical_target_size()` and refused the whole
        // click, so *every* click was a miss once the camera had a real viewport.
        // The camera's rectangle is now the node's rectangle and the mapping is
        // total over it: a point inside maps, and only points outside do not.
        let viewport = measured();
        for i in 0..=64u32 {
            let t = i as f32 / 64.0;
            let x = 512.0 + t * 768.0;
            let y = t * 665.0;
            assert!(
                ndc(&viewport, x, y).is_some(),
                "({x}, {y}) is inside the viewport and must map"
            );
        }
    }

    // ---- the conversion that used to be wrong: centre vs top-left ----

    #[test]
    fn the_rectangle_is_the_nodes_top_left_not_its_centre() {
        let node = ComputedNode {
            size: Vec2::new(768.0, 665.0),
            ..default()
        };
        // Taffy centres the viewport node in the window, so its centre is
        // (896, 332.5). Using that as `physical_position` would put the camera
        // viewport a half-node right and down of where the node is.
        let transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
            896.0, 332.5,
        )));
        let viewport = CreatorViewport::from_node(&node, &transform, 1.0);
        assert_eq!(
            viewport.physical_position,
            [512, 0],
            "the rectangle's origin is centre minus half the size"
        );
        assert_eq!(viewport.physical_size, [768, 665]);
        assert!(viewport.available);
        // And the consequence: the node's own centre is the viewport's centre.
        assert_eq!(
            ndc(&viewport, 896.0, 332.5),
            Some(Vec2::ZERO),
            "a node mapped to its own centre must give NDC (0, 0)"
        );
    }

    #[test]
    fn a_negative_node_origin_is_clamped_rather_than_wrapped() {
        let node = ComputedNode {
            size: Vec2::new(400.0, 300.0),
            ..default()
        };
        let transform = UiGlobalTransform::from(bevy::math::Affine2::from_translation(Vec2::new(
            -100.0, -50.0,
        )));
        let viewport = CreatorViewport::from_node(&node, &transform, 1.0);
        assert_eq!(
            viewport.physical_position,
            [0, 0],
            "a negative origin must clamp to zero, never wrap to 2^32"
        );
    }

    #[test]
    fn the_camera_viewport_carries_the_physical_rectangle_verbatim() {
        let viewport = measured();
        let camera_viewport = viewport.camera_viewport().expect("a usable rectangle");
        assert_eq!(camera_viewport.physical_position, UVec2::new(512, 0));
        assert_eq!(camera_viewport.physical_size, UVec2::new(768, 665));
        // `depth` is Bevy's, untouched: this function owns geometry, not clipping.
        assert_eq!(camera_viewport.depth, (0.0..1.0));
    }

    #[test]
    fn the_mapping_agrees_with_the_rectangle_bevy_computes_for_itself() {
        // `Camera::viewport_to_ndc` is `(position - logical_rect.min) /
        // logical_rect.size() * 2 - 1` with Y flipped. This pins our arithmetic to
        // that formula stated independently, so a future edit that changes one and
        // not the other fails here rather than in a screenshot.
        let viewport = measured();
        let cursor = Vec2::new(700.0, 200.0);
        let rect = viewport.logical_rect().unwrap();
        let mut want = ((cursor - rect.min) / rect.size()) * 2.0 - Vec2::ONE;
        want.y = -want.y;
        assert_eq!(ndc(&viewport, cursor.x, cursor.y), Some(want));
    }
}
