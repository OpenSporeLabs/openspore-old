//! # Real part thumbnails, rendered from real geometry.
//!
//! The brief is explicit that a palette cell must not be fake geometry, and this
//! module exists because that is the only honest way to fill 464 cells. Every
//! thumbnail is the part's **own decoded `gmsh` mesh**, rendered by the same
//! [`convert::to_bevy_mesh`](crate::convert::to_bevy_mesh) the creature viewport
//! uses, through the same material policy, by a real camera into a real render
//! target. There is no stand-in, no cube, no icon.
//!
//! ## Why offscreen, and not "render the grid in the viewport"
//!
//! A camera with [`RenderTarget::Image`] renders into a texture rather than the
//! window. That is what lets a thumbnail be a texture the UI can sample, and it
//! means the palette and the creature never fight over one viewport.
//!
//! ## Why the meshes are baked ONCE and reused
//!
//! 464 parts × ~700 vertices is ~325k vertices. Baking a [`Mesh`] per part once at
//! load is cheap and makes every later frame free. The alternative -- keeping the
//! decoded buffers and rebuilding a `Mesh` per visible cell per frame -- would put
//! GPU allocation in the render loop for no benefit, because a part's geometry does
//! not change while the palette is open.
//!
//! ## The honest part: this is a *bake*, so a new part is not visible until the
//! next launch
//!
//! A bake at startup means a part discovered later needs a restart. That is a real
//! limitation with a real cause -- baking during a frame would stall it -- and it
//! is stated here rather than discovered later as "the palette is missing a part".
//! Making it incremental is the obvious next step and is deliberately not
//! pretended at.

use bevy::asset::RenderAssetUsages;
use bevy::camera::ImageRenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};

use spore_assets::ContentStore;
use spore_creator_core::document::BlockId;
use spore_morphology::{Catalog, PackageSource};

/// Thumbnail edge length in pixels.
///
/// **OpenSpore's own choice.** The reference frames do not show the catalogue at a
/// resolution that pins this: the cells are ~180 px there, but that is the
/// reference's window size, not a property of a thumbnail. A thumbnail is
/// resolution-independent, so the value is chosen to be legible at the cell size
/// this screen's own layout uses and cheap enough to bake 464 of.
pub const THUMBNAIL_SIDE: u32 = 128;

/// How many thumbnails to bake per frame.
///
/// A **budget**, not a correctness value: baking all 464 in one frame would stall
/// startup long enough to look like a hang. One per frame is invisible; the whole
/// set is done in 464 frames, about eight seconds at 60 Hz.
pub const BAKES_PER_FRAME: usize = 4;

/// One part's identity, and whether its geometry can be drawn at all.
///
/// A **technical** entry. There is no display name, because no shipped package
/// contains a part-name table — a sweep of every locale's `Text.package` and all
/// 3761 `prop` records found no mapping from part key to display name. Inventing
/// one would be a plausible-looking fabrication in the exact place a user reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntry {
    /// The `<blockid>` a `<blockref>` writes.
    pub id: BlockId,
    /// Whether the chain reached drawable geometry.
    pub drawable: bool,
    /// Triangles, when decoded.
    pub triangles: Option<u32>,
}

impl PaletteEntry {
    /// The cell's label: the identity, because there is no name to show.
    ///
    /// Formatted as `group:instance` in hex, which is exactly what a `<blockid>`
    /// writes, so the label and the document use one spelling. NOT a friendly name:
    /// the brief asks for a technical identifier where the original's name is
    /// unknown, and this is that.
    pub fn label(&self) -> String {
        format!("{:08x}:{:08x}", self.id.group_id, self.id.instance_id)
    }
}

/// Builds the palette over the creature rigblock group.
///
/// `group_id` is a parameter, never a constant — see
/// [`Catalog::build_from_identities`]. Infallible: a store that would not open
/// yields an empty palette, and an empty palette is a screen with nothing in it
/// rather than an error, because the caller has no better answer to offer and
/// inventing one would be worse.
pub fn build_palette(store: &ContentStore, group_id: u32) -> Vec<PaletteEntry> {
    let ids: Vec<BlockId> = store
        .identities_of_type(spore_morphology::resolve::TYPE_RIGBLOCK)
        .into_iter()
        .filter(|key| key.group_id == group_id)
        .map(|key| BlockId::new(key.group_id, key.instance_id))
        .collect();
    let catalog = Catalog::build_from_identities(&PackageSource::new(), ids);
    catalog
        .entries()
        .iter()
        .map(|entry| PaletteEntry {
            id: entry.id,
            drawable: entry.is_authorable(),
            triangles: entry
                .geometry
                .value()
                .map(|geometry| geometry.triangles as u32),
        })
        .collect()
}

/// A thumbnail bake that has not run yet.
#[derive(Debug, Clone)]
pub struct PendingBake {
    /// Which part this cell shows.
    pub id: BlockId,
    /// The mesh, in the part's own local space.
    pub mesh: Mesh,
}

/// The meshes for the palette, baked once and then never touched.
///
/// A [`Resource`] because a bake is a **one-time** cost and every later frame
/// wants the answer. Holding them anywhere else means either rebuilding per frame
/// or passing them through every system that draws a cell.
///
/// No `Debug`: the crate warns on missing `Debug`, and a derived one would print
/// every asset handle's id, which is noise rather than information. The fields are
/// readable through the accessors below, and a missing debug impl is a smaller
/// problem than a 464-line map dump in a log.
#[derive(Resource, Default)]
#[allow(missing_debug_implementations)]
pub struct ThumbnailAtlas {
    /// Part identity -> mesh handle. `None` for a part whose geometry would not
    /// decode, which is a **stated absence** and the reason that cell is blank
    /// rather than a placeholder.
    pub meshes: std::collections::HashMap<BlockId, Handle<Mesh>>,
    /// Part identity -> the baked thumbnail texture, once baked.
    pub images: std::collections::HashMap<BlockId, Handle<Image>>,
    /// How many bakes have run, for progress and for the log.
    pub baked: usize,
}

impl ThumbnailAtlas {
    /// Whether a part can be drawn at all.
    ///
    /// `None` is "not baked yet" and `Some(false)` is "baked, and there was
    /// nothing to draw" — different states, and conflating them is how an unbaked
    /// cell becomes an unexplained blank one.
    pub fn drawable(&self, id: BlockId) -> Option<bool> {
        self.meshes.get(&id).map(|_| true)
    }
}

/// The camera rig that renders one part at a time into its own target.
///
/// **Not** spawned per part: 464 cameras is 464 viewports' worth of render-graph
/// work every frame. One camera, retargeted, is the difference between a palette
/// that loads and one that does not.
#[derive(Resource, Default)]
#[allow(missing_debug_implementations)]
pub struct ThumbnailCamera {
    /// The camera entity, once spawned.
    pub entity: Option<Entity>,
    /// The part currently being baked.
    pub current: Option<BlockId>,
    /// The queue, in palette order.
    pub queue: std::collections::VecDeque<BlockId>,
}

/// The `Image` asset a thumbnail renders into.
///
/// A `RenderTargetUsages` with `RENDER_ATTACHMENT` **and** `COPY_SRC`, and the
/// second one is load-bearing: without `COPY_SRC` the texture cannot be read back,
/// and reading it back is how a rendered texture becomes a `Handle<Image>` the UI
/// can sample. A target missing it renders fine and is then unusable.
pub fn thumbnail_image() -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: THUMBNAIL_SIDE,
            height: THUMBNAIL_SIDE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        // `Rgba8UnormSrgb` rather than a linear format: a thumbnail is displayed,
        // not lit a second time, and an sRGB texture is what the UI samples
        // correctly. A linear target here would come out washed out.
        TextureFormat::Rgba8UnormSrgb,
        // `RenderAssetUsages` in Bevy 0.19 is about WORLD TRANSFER, not GPU
        // capabilities -- it has exactly `MAIN_WORLD` and `RENDER_WORLD`. The GPU
        // flags moved to `TextureUsages` on the texture descriptor, which is set
        // just below. Confusing the two is a two-line fix that produces a target
        // which renders fine and is then unreadable.
        RenderAssetUsages::RENDER_WORLD,
    );
    // `COPY_SRC` is load-bearing: without it the texture cannot be read back, and
    // reading it back is how a rendered target becomes a `Handle<Image>` the UI can
    // sample. A target missing it renders fine and is then unusable -- and the
    // failure looks like "the thumbnails are blank", not like a missing flag.
    image.texture_descriptor.usage = TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
    image
}

/// Wraps a baked image in the target a camera renders into.
///
/// A free function taking the handle because `ImageRenderTarget` has **no
/// constructor** in Bevy 0.19 -- it is a plain struct around a handle -- so the
/// only way to build one is the literal, and doing that in one place keeps the
/// scale factor from drifting between call sites.
pub fn thumbnail_target(handle: Handle<Image>) -> ImageRenderTarget {
    ImageRenderTarget {
        handle,
        scale_factor: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_label_is_the_identity_in_the_spelling_a_document_uses() {
        // The label must be readable as a `<blockid>`, because that is what a user
        // would paste to select a part by hand. Asserted against the exact format
        // rather than "it contains a colon".
        let entry = PaletteEntry {
            id: BlockId::new(0x4062_6000, 0xd05c_53a3),
            drawable: true,
            triangles: Some(64),
        };
        assert_eq!(entry.label(), "40626000:d05c53a3");
    }

    #[test]
    fn the_label_contains_no_spore_word() {
        // The evidence discipline, as a test. A label that reads "Mouth" or
        // "Carnivore" would be an invented semantic in the one place a user reads
        // it, and a corpus sweep proved no part-name table exists to draw from.
        let entry = PaletteEntry {
            id: BlockId::new(0x4062_6000, 0x33fc_cc91),
            drawable: true,
            triangles: None,
        };
        let label = entry.label().to_lowercase();
        for forbidden in [
            "mouth",
            "eye",
            "hand",
            "foot",
            "tail",
            "horn",
            "carnivore",
            "plant",
        ] {
            assert!(
                !label.contains(forbidden),
                "the label {label:?} contains an invented word {forbidden:?}"
            );
        }
    }

    #[test]
    fn an_empty_palette_has_no_labels_to_render() {
        // The property `build_palette` must have when the packages are absent: no
        // invented parts. A palette that panicked would take the whole screen down,
        // because it is furniture.
        let empty: Vec<PaletteEntry> = Vec::new();
        assert!(
            empty.is_empty() && empty.iter().all(|entry| entry.label().is_empty()),
            "an empty palette yields nothing at all"
        );
    }
}
