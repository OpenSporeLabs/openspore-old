//! The domain model: ordinary Rust structs, no Bevy.
//!
//! # Why this layer exists
//!
//! The engine used to hold one decoded `gmdl` record in one resource and spawn
//! one entity per mesh. That is a vertical slice, not a scene: there was
//! nowhere to put a second asset, a transform, or a model made of several
//! records.
//!
//! This module is the answer, and it is deliberately **not** ECS. A scene is a
//! value: it can be built, inspected, compared and asserted on with no window,
//! no GPU and no game install. Only [`crate::loader`] turns one into entities,
//! and only [`crate::texture`] turns its surfaces into GPU images.
//!
//! # The split, in one table
//!
//! | layer | module | Bevy? | holds |
//! |---|---|---|---|
//! | world | [`world`] | no | packages, and the store over them |
//! | model | [`model`] | no | one record decoded into parts |
//! | creature | [`creature`] | no | one or more model instances in a frame |
//! | scene | [`scene`] | no | objects, placements, layout |
//! | materials | [`materials`] | no | which surface feeds which material slot |
//! | runtime | [`crate::loader`] | yes | `Assets<Mesh>` handles, entities |
//!
//! # The one rule that keeps the boundary honest
//!
//! Nothing below this module imports `bevy`. That is what makes
//! `cargo test -p spore-engine` able to assert the whole asset path — including
//! the material roles and the normal-map swizzle — with no window open.

pub mod creature;
pub mod materials;
pub mod model;
pub mod scene;
pub mod world;

use spore_core::ResourceKey;

pub use scene::{Layout, Scene, SceneObject};

/// An axis-aligned box in world units, or `None` when nothing was measured.
///
/// `Option` rather than a zero box: "there were no meshes" and "the meshes
/// occupy no space" are different findings, and a camera framed on a zero box
/// sits inside the model.
pub type Bounds = Option<([f32; 3], [f32; 3])>;

/// Grows `bounds` by `other`, keeping `None` as an absence.
pub fn grow(bounds: Bounds, other: Bounds) -> Bounds {
    match (bounds, other) {
        (None, other) => other,
        (bounds, None) => bounds,
        (Some((lo, hi)), Some((lo2, hi2))) => Some((
            [lo[0].min(lo2[0]), lo[1].min(lo2[1]), lo[2].min(lo2[2])],
            [hi[0].max(hi2[0]), hi[1].max(hi2[1]), hi[2].max(hi2[2])],
        )),
    }
}

/// The bounds of one `spore_gmdl::Mesh`.
pub fn mesh_bounds(mesh: &spore_gmdl::Mesh) -> ([f32; 3], [f32; 3]) {
    (mesh.bounds_min, mesh.bounds_max)
}

/// The union of every mesh's bounds, or `None` when there are none.
pub fn union_bounds<'a>(meshes: impl IntoIterator<Item = &'a spore_gmdl::Mesh>) -> Bounds {
    let mut bounds: Bounds = None;
    for mesh in meshes {
        bounds = grow(bounds, Some(mesh_bounds(mesh)));
    }
    bounds
}

/// Triangles a mesh contributes, after the fan expansion in [`crate::convert`].
///
/// Reads the **expanded** index count, because that is what the GPU is handed;
/// quoting the record's raw count would under-report every `primType` 6 mesh.
pub fn mesh_triangles(buffers: &crate::convert::MeshBuffers) -> usize {
    buffers.indices.len() / 3
}

/// The identity of a model instance, used as a stable key inside a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceId(pub ResourceKey);
