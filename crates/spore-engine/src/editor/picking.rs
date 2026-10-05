//! # Picking: a cursor position in, a real `BlockIndex` out.
//!
//! The editor's first real interaction, and the one place where a plausible
//! shortcut becomes a silent lie: the cursor is a point on the screen, the
//! creature is a pile of boxes, and anything between "nearest box" and "the block
//! under this pixel" agrees on every easy case.
//!
//! ## What this actually does
//!
//! A ray from the camera through the cursor, tested against each resolved block's
//! **world** axis-aligned box, nearest hit wins. Ties break to the lower block
//! index so a selection is deterministic.
//!
//! ## What it is not, stated plainly
//!
//! It is a **bounding-box** pick, not a triangle pick. On a box that is much
//! larger than the mesh inside it -- an outstretched limb, a shell with a hole in
//! it -- the cursor can be inside the box and outside the geometry, and this
//! reports a hit the user can see is not there.
//!
//! The exact version is available and was not taken: every block's positions are
//! already on the CPU in [`ProjectedPart::geometry`], so a per-triangle test is
//! reachable without decoding anything new. It was left out because the editor
//! needs to be *usable* before it is *exact*, and a box pick on 464 real parts is
//! usable where a triangle pick on a wireframe is not. [`PICK_METHOD`] names the
//! method so a caller reporting a mis-selection has a fact to hand back rather
//! than a mystery.
//!
//! ## Why the world box and not the local one
//!
//! A part's local geometry sits around its own origin; the creature's parts do not
//! sit at the world origin. Testing local boxes would make every block claim the
//! same corner of space, so a click would select whichever part happened to be
//! nearest the world origin regardless of where it is drawn. That is why this
//! goes through [`ProjectedBlock::world_bounds`], which applies the parent chain's
//! position, orientation **and** scale.

use bevy::math::{Ray3d, Vec3};
use spore_morphology::{Bounds, ProjectedBlock, Projection};

/// How a selection was decided, so a mis-selection is reportable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMethod {
    /// The cursor's ray entered the block's world axis-aligned box.
    ///
    /// Over-reports on non-convex parts; see the module docs.
    WorldBounds,
}

/// The method this picker uses, by name.
pub const PICK_METHOD: PickMethod = PickMethod::WorldBounds;

/// A pick ray. Plain data so the maths is testable with no camera and no window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PickRay {
    /// Where the ray starts, in world space.
    pub origin: Vec3,
    /// Which way it points.
    ///
    /// **Not** required to be normalised, and
    /// [`Pick::distance`] is a world-space distance either way: the slab test
    /// normalises internally. That is deliberate. The raw slab test returns the
    /// ray *parameter* `t` in `origin + t * direction`, which is a distance only
    /// when the direction happens to be a unit vector -- so a caller who forgot
    /// to normalise would get a distance off by exactly the factor they forgot,
    /// and comparisons against another block's distance would quietly pick the
    /// wrong one.
    pub direction: Vec3,
}

impl From<Ray3d> for PickRay {
    fn from(ray: Ray3d) -> Self {
        Self {
            origin: ray.origin,
            direction: *ray.direction,
        }
    }
}

/// What the cursor hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    /// The block, as an index into the document. Not a projected row, not an
    /// entity: the document is the source of truth and everything else has to be
    /// translated to it.
    pub block: usize,
    /// Distance along the ray to where it entered the box.
    pub distance: f32,
    /// How it was decided.
    pub method: PickMethod,
}

/// The block under the cursor, or `None` when the ray hits nothing.
pub fn pick(ray: PickRay, projection: &Projection) -> Option<Pick> {
    pick_from_bounds(ray, projection.resolved().filter_map(collect))
}

/// Every block the ray enters, nearest first.
///
/// The box pick has real overlap -- a creature's limb boxes contain each other --
/// so the ordered list is a more honest answer than the single winner, and a UI
/// that wants "the part under the cursor, or the next one in front of it" can
/// have it without re-running the maths.
pub fn pick_all(ray: PickRay, projection: &Projection) -> Vec<Pick> {
    pick_all_from_bounds(ray, projection.resolved().filter_map(collect))
}

/// One block's index and world box, or `None` when it has no geometry.
fn collect(block: &ProjectedBlock) -> Option<(usize, Bounds)> {
    Some((block.block, block.world_bounds()?))
}

/// The decision, over boxes rather than over a [`Projection`].
///
/// Split out because **the nearest-wins rule is the whole point of a picker** and
/// it is untestable through [`pick`]: building a `Projection` needs real decoded
/// `gmsh` records, so the rule that decides *which* block the user selected had
/// no unit test at all -- mutating it from "nearest wins" to "last wins" left
/// every test green. It is now plain arithmetic over boxes, and the mutation is
/// caught.
pub fn pick_from_bounds(
    ray: PickRay,
    boxes: impl IntoIterator<Item = (usize, Bounds)>,
) -> Option<Pick> {
    pick_all_from_bounds(ray, boxes).into_iter().next()
}

/// [`pick_from_bounds`] without dropping the runners-up.
pub fn pick_all_from_bounds(
    ray: PickRay,
    boxes: impl IntoIterator<Item = (usize, Bounds)>,
) -> Vec<Pick> {
    let mut hits: Vec<Pick> = boxes
        .into_iter()
        .filter_map(|(block, bounds)| {
            let distance = intersect_bounds(ray, bounds)?;
            Some(Pick {
                block,
                distance,
                method: PICK_METHOD,
            })
        })
        .collect();
    // A total order, so a selection is reproducible. Two boxes sharing a face
    // would otherwise resolve by iteration order, and a selection that changes
    // when the document is re-saved is not a selection.
    hits.sort_by(|a, b| {
        a.distance
            .total_cmp(&b.distance)
            .then_with(|| a.block.cmp(&b.block))
    });
    hits
}

/// Where a ray enters an axis-aligned box, or `None`.
///
/// The slab method. Written out rather than reached for because it is 20 lines,
/// it has no version-sensitive API to drift, and the degenerate cases below are
/// exactly the ones a picking bug lives in.
pub fn intersect_bounds(ray: PickRay, bounds: Bounds) -> Option<f32> {
    // Normalise here, once, so the returned parameter IS a world-space distance.
    // See `PickRay::direction`. A zero-length direction has no direction at all,
    // so it hits nothing; that is the same answer a cursor outside every part
    // gives, which is the right thing for a picker to do with a degenerate input.
    let length = ray.direction.length();
    if !length.is_finite() || length <= 0.0 {
        return None;
    }
    let direction = [
        ray.direction.x / length,
        ray.direction.y / length,
        ray.direction.z / length,
    ];
    let origin = [ray.origin.x, ray.origin.y, ray.origin.z];
    // A direction component below this is treated as parallel to the slab.
    // `f32::MIN_POSITIVE` would be the "is it exactly zero" test and would divide
    // by a denormal; this is a threshold chosen to be far below any real
    // direction and far above any accidental zero.
    const PARALLEL: f32 = 1e-9;

    let mut near = 0.0f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        let (lo, hi) = (bounds.min[axis], bounds.max[axis]);
        if direction[axis].abs() <= PARALLEL {
            // Parallel to this slab: a hit needs the origin already between the
            // planes, and if it is not, the ray never crosses the box. Skipping
            // this arm instead is the classic bug -- it accepts a ray that is
            // beside the box and reports the entry as 0.
            if origin[axis] < lo || origin[axis] > hi {
                return None;
            }
            continue;
        }
        let inverse = 1.0 / direction[axis];
        let (mut first, mut second) =
            ((lo - origin[axis]) * inverse, (hi - origin[axis]) * inverse);
        if first > second {
            std::mem::swap(&mut first, &mut second);
        }
        near = near.max(first);
        far = far.min(second);
        if near > far {
            return None;
        }
    }
    // `far` is still infinite when every axis was parallel, which cannot happen
    // for a ray with a real direction, but the finite guard costs nothing and
    // makes the return type honest.
    if far.is_finite() {
        Some(near)
    } else {
        None
    }
}

/// A block's world box, re-exported so the picking tests can build one without
/// reaching into `spore-morphology` internals.
pub fn block_world_bounds(block: &ProjectedBlock) -> Option<Bounds> {
    block.world_bounds()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A unit box centred on the origin.
    fn unit() -> Bounds {
        Bounds {
            min: [-0.5, -0.5, -0.5],
            max: [0.5, 0.5, 0.5],
        }
    }

    /// A ray down -Z from far away, aimed at the origin.
    fn at_origin() -> PickRay {
        PickRay {
            origin: Vec3::new(0.0, 0.0, 5.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        }
    }

    #[test]
    fn a_zero_length_direction_hits_nothing_rather_than_everything() {
        let ray = PickRay {
            origin: Vec3::ZERO,
            direction: Vec3::ZERO,
        };
        assert_eq!(
            intersect_bounds(ray, unit()),
            None,
            "a ray with no direction hits nothing; NaN comparisons would otherwise \
             make it hit the first box it was tested against"
        );
    }

    #[test]
    fn a_ray_straight_at_a_box_reports_where_it_entered() {
        let hit = intersect_bounds(at_origin(), unit()).expect("the ray goes through the box");
        assert!(
            (hit - 4.5).abs() < 1e-5,
            "enters at z = +0.5, i.e. 4.5 along the ray, got {hit}"
        );
    }

    // ---- the decision: which block wins ----------------------------------

    /// A box at `z`, spanning unit size, so a ray down -Z meets them in order.
    fn at_z(z: f32) -> Bounds {
        Bounds {
            min: [-0.5, -0.5, z - 0.5],
            max: [0.5, 0.5, z + 0.5],
        }
    }

    #[test]
    fn the_nearest_box_wins_not_the_first_one_listed() {
        // The rule that decides which block the user selected. Before this test
        // existed, mutating "nearest wins" into "last wins" left every test green,
        // because `pick` could only be reached through a `Projection` -- which
        // needs real decoded `gmsh` records -- so the decision had no test at all.
        let ray = at_origin();
        let hit = pick_from_bounds(ray, [(7, at_z(-2.0)), (3, at_z(1.0)), (5, at_z(-1.0))])
            .expect("all three are in the ray's path");
        assert_eq!(
            hit.block, 3,
            "the camera is at z = +5 looking down -Z, so the box at z = +1.0 (block 3) \
             is nearest -- and it is the SECOND listed, so 'first listed wins' is refuted"
        );
    }

    #[test]
    fn input_order_does_not_change_the_answer() {
        let ray = at_origin();
        let forward = pick_from_bounds(ray, [(7, at_z(-2.0)), (3, at_z(1.0)), (5, at_z(-1.0))]);
        let reversed = pick_from_bounds(ray, [(5, at_z(-1.0)), (3, at_z(1.0)), (7, at_z(-2.0))]);
        assert_eq!(
            forward, reversed,
            "a selection that depends on the order blocks happen to be listed in is not a selection"
        );
    }

    #[test]
    fn an_exact_tie_breaks_to_the_lower_block_index() {
        // Two boxes sharing a face. Without the tie-break this is whatever the
        // iteration order happened to be.
        let ray = at_origin();
        let hit = pick_from_bounds(ray, [(9, at_z(0.0)), (4, at_z(0.0))]).expect("both are hit");
        assert_eq!(hit.block, 4, "the lower index wins an exact tie");
    }

    #[test]
    fn pick_all_returns_every_hit_nearest_first() {
        // Overlapping boxes are the normal case, not an edge case: a creature's
        // limb boxes contain each other. A picker that only answers "one" cannot
        // support "the part in front".
        let ray = at_origin();
        let all = pick_all_from_bounds(
            ray,
            [
                (7, at_z(-2.0)),
                (3, at_z(1.0)),
                (5, at_z(-1.0)),
                (9, at_z(50.0)),
            ],
        );
        assert_eq!(
            all.iter().map(|h| h.block).collect::<Vec<_>>(),
            vec![3, 5, 7],
            "nearest first, and the box behind the ray's start is absent"
        );
    }

    #[test]
    fn a_ray_that_hits_nothing_selects_nothing_rather_than_block_zero() {
        let ray = PickRay {
            origin: Vec3::new(50.0, 50.0, 50.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(
            pick_from_bounds(ray, [(0, unit()), (1, unit())]),
            None,
            "no hit must be no selection, not the first block"
        );
    }

    #[test]
    fn an_empty_world_selects_nothing() {
        assert_eq!(pick_from_bounds(at_origin(), []), None);
    }

    #[test]
    fn a_ray_that_misses_reports_nothing() {
        let ray = PickRay {
            origin: Vec3::new(5.0, 0.0, 5.0),
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        assert_eq!(
            intersect_bounds(ray, unit()),
            None,
            "beside the box, not in front of it"
        );
    }

    #[test]
    fn a_ray_pointing_away_reports_nothing() {
        let ray = PickRay {
            origin: Vec3::new(0.0, 0.0, 5.0),
            direction: Vec3::new(0.0, 0.0, 1.0),
        };
        assert_eq!(
            intersect_bounds(ray, unit()),
            None,
            "the box is behind the ray; `near` stays 0 and `far` goes negative"
        );
    }

    #[test]
    fn a_ray_parallel_to_a_slab_and_beside_it_reports_nothing() {
        // The degenerate arm. A ray along X at y = 2 never crosses the y slab, and
        // a picker that skips the parallel test instead of failing it reports the
        // entry distance as 0 -- which is inside the box, so it is a false hit.
        let ray = PickRay {
            origin: Vec3::new(-5.0, 2.0, 0.0),
            direction: Vec3::new(1.0, 0.0, 0.0),
        };
        assert_eq!(
            intersect_bounds(ray, unit()),
            None,
            "parallel and outside the slab"
        );
    }

    #[test]
    fn a_ray_parallel_to_a_slab_and_inside_it_still_hits() {
        // The other half of the same arm: parallel is not the same as a miss.
        let ray = PickRay {
            origin: Vec3::new(-5.0, 0.25, 0.0),
            direction: Vec3::new(1.0, 0.0, 0.0),
        };
        let hit = intersect_bounds(ray, unit()).expect("parallel but inside two of the slabs");
        assert!((hit - 4.5).abs() < 1e-5, "enters at x = -0.5, got {hit}");
    }

    #[test]
    fn a_ray_inside_the_box_enters_at_zero_not_behind_it() {
        let ray = PickRay {
            origin: Vec3::ZERO,
            direction: Vec3::new(0.0, 0.0, -1.0),
        };
        let hit = intersect_bounds(ray, unit()).expect("the origin is inside");
        assert_eq!(
            hit, 0.0,
            "distance 0, not the negative distance to the far face"
        );
    }

    #[test]
    fn an_unnormalised_direction_gives_the_same_hit() {
        // Documented promise: the distance is scaled to the direction, so a
        // caller who forgets to normalise gets the same answer, not a different
        // one. `Ray3d::direction` IS normalised in Bevy, but this picker should
        // not depend on that.
        let mut fast = at_origin();
        fast.direction *= 7.5;
        let slow = at_origin();
        let a = intersect_bounds(fast, unit()).expect("hit");
        let b = intersect_bounds(slow, unit()).expect("hit");
        assert!(
            (a - b).abs() < 1e-4,
            "a 7.5x longer direction must not change the reported distance: {a} vs {b}"
        );
        assert!(
            (a - 4.5).abs() < 1e-4,
            "and both must be the true distance, got {a}"
        );
    }

    #[test]
    fn a_flat_box_still_picks_and_does_not_collapse() {
        // A zero-thickness box is degenerate, and Spore's flat parts exist. The
        // slab test handles it as long as the ray is not parallel to the flat
        // axis.
        let flat = Bounds {
            min: [-0.5, 0.0, -0.5],
            max: [0.5, 0.0, 0.5],
        };
        let ray = PickRay {
            origin: Vec3::new(0.0, 1.0, 0.0),
            direction: Vec3::new(0.0, -1.0, 0.0),
        };
        assert!(
            intersect_bounds(ray, flat).is_some(),
            "a flat part is still selectable"
        );
    }
}
