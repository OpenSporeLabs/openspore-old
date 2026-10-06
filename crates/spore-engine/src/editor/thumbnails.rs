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

/// The palette, with the evidence needed to tell "not resolved" from "not found".
///
/// # Why this is not a bare `Vec<PaletteEntry>`
///
/// [`build_palette`] resolves 482 part chains, which needs the package *files*.
/// When the install is absent it resolves none of them, and the two results are
/// both "482 rows" -- identical in a UI, opposite in meaning. One is a full palette
/// whose 18 rows are honest non-answers; the other is a screen that looked for
/// geometry and had nothing to look in. Collapsing them is how "we did not look"
/// becomes "we looked and found nothing", so the counts travel with the entries.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Palette {
    /// One row per candidate part, in store order.
    pub entries: Vec<PaletteEntry>,
    /// Packages the resolver actually opened.
    pub packages_opened: usize,
    /// Packages in the store that carry no path, so no resolver could open them.
    ///
    /// Non-zero is a **stated limitation**: those parts are enumerated but cannot be
    /// decoded, because a package built from bytes in memory has no file for
    /// `PackageSource` to re-read. Zero for every store the engine builds, since
    /// [`spore_assets::Package::open`] always records its path.
    pub packages_without_path: usize,
}

impl Palette {
    /// Whether the chains were resolved against real files at all.
    ///
    /// The one question a caller must ask before drawing anything: a `false` here
    /// means every `drawable: false` below is "we had nothing to read", not "these
    /// parts have no geometry".
    pub fn resolved(&self) -> bool {
        self.packages_opened > 0
    }

    /// Rows that reached drawable geometry.
    pub fn drawable(&self) -> usize {
        self.entries.iter().filter(|entry| entry.drawable).count()
    }
}

/// A resolver source over the same packages the store indexed.
///
/// The store enumerates identities from a memory-mapped index; [`PackageSource`]
/// re-reads the files to resolve the chains. They cannot be the same object, so the
/// paths have to be carried across — and they can only be carried across because
/// [`spore_assets::Package::path`] records where each package came from, rather than
/// this re-guessing filenames.
///
/// Packages with no path are **counted, not skipped silently**; see
/// [`Palette::packages_without_path`].
fn source_over(store: &ContentStore) -> (PackageSource, usize) {
    let mut source = PackageSource::new();
    let mut without_path = 0;
    for package in store.packages() {
        match package.path() {
            Some(path) => source = source.open(path),
            None => without_path += 1,
        }
    }
    (source, without_path)
}

/// Builds the palette over the creature rigblock group.
///
/// `group_id` is a parameter, never a constant — see
/// [`Catalog::build_from_identities`]. Infallible: a store that cannot be resolved
/// yields a palette with **zero opened packages**, which says so through
/// [`Palette::resolved`], rather than an error the caller has no better answer for.
pub fn build_palette(store: &ContentStore, group_id: u32) -> Palette {
    let ids: Vec<BlockId> = store
        .identities_of_type(spore_morphology::resolve::TYPE_RIGBLOCK)
        .into_iter()
        .filter(|key| key.group_id == group_id)
        .map(|key| BlockId::new(key.group_id, key.instance_id))
        .collect();
    let (source, packages_without_path) = source_over(store);
    let catalog = Catalog::build_from_identities(&source, ids);
    let entries = catalog
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
        .collect();
    Palette {
        packages_opened: source.package_count(),
        packages_without_path,
        entries,
    }
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
    fn an_empty_store_yields_an_unresolved_palette_not_an_empty_one() {
        // The previous version of this test built a `Vec::new()`, asserted it was
        // empty, and called it a day -- it never called `build_palette`, so it
        // passed while the real function handed `Catalog` an empty
        // `PackageSource` and marked all 482 parts non-drawable. A test of a
        // locally constructed value is not a test of the function.
        let palette = build_palette(&ContentStore::new(), 0x4062_6000);
        assert!(
            palette.entries.is_empty(),
            "an empty store enumerates nothing"
        );
        // The load-bearing assertion: "resolved against nothing" and "found
        // nothing" are different answers and only the first one is true here.
        assert!(
            !palette.resolved(),
            "no packages opened must not read as resolved"
        );
        assert_eq!(palette.drawable(), 0);
    }

    #[test]
    fn a_store_of_pathless_packages_enumerates_but_cannot_resolve() {
        // A package built from bytes has no file for `PackageSource` to re-read, so
        // its parts are enumerable and undecodable at the same time. Counting those
        // packages is what stops that from looking like a corpus of parts whose
        // geometry happens to be missing.
        let store = ContentStore::new();
        let palette = build_palette(&store, 0x4062_6000);
        assert_eq!(palette.packages_without_path, 0, "an empty store has none");
        assert_eq!(palette.packages_opened, 0);
        assert!(!palette.resolved());
    }

    #[test]
    fn a_drawable_row_and_a_resolved_palette_are_separate_facts() {
        // Guards the aggregation: `resolved()` is about packages, `drawable()` is
        // about parts. A future edit that made `resolved()` mean "some part worked"
        // would make an install that decoded 2 of 482 look resolved.
        let palette = Palette {
            entries: vec![PaletteEntry {
                id: BlockId::new(0x4062_6000, 0x1111_1111),
                drawable: true,
                triangles: Some(12),
            }],
            packages_opened: 0,
            packages_without_path: 1,
        };
        assert_eq!(palette.drawable(), 1);
        assert!(
            !palette.resolved(),
            "one good part is not a readable install"
        );
    }
}

/// The corpus test for [`build_palette`], in its own module so it does not
/// drag `ContentStore` into the hermetic build's test binary.
///
/// `#[ignore]`d like every other corpus test here: it needs `SPORE/`, it resolves
/// 482 chains, and the hermetic suite must pass with no game installed.
#[cfg(test)]
mod corpus {
    use super::{build_palette, Palette};
    use spore_assets::ContentStore;

    /// The creature-part group, from the corpus census.
    const CREATURE_PARTS: u32 = 0x4062_6000;

    /// The workspace root, resolved from the crate directory.
    ///
    /// Not a relative path and not the cwd: cargo runs a test binary with the
    /// *package* directory as its cwd, so `SPORE/Data/...` resolves to nothing here
    /// even though it works from a shell at the workspace root. A path that works
    /// under `cargo test` and fails under `cargo test --release` is the worst kind,
    /// because it looks like a corpus finding when it is a path bug.
    fn install_root() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(std::path::Path::parent)
            .expect("the crate directory has a workspace root above it")
            .to_path_buf()
    }

    fn store() -> Option<ContentStore> {
        let root = install_root();
        if !root.join("SPORE/Data/Spore_Game.package").is_file() {
            return None;
        }
        spore_assets::open_packages([
            root.join("SPORE/Data/Spore_Game.package"),
            root.join("SPORE/Data/Spore_Graphics.package"),
        ])
        .ok()
    }

    #[test]
    #[ignore = "needs SPORE/ and resolves 482 chains"]
    fn the_palette_really_resolves_four_hundred_and_some_parts() {
        let Some(store) = store() else {
            eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
            return;
        };
        let palette: Palette = build_palette(&store, CREATURE_PARTS);

        // The regression this guards: an empty `PackageSource` still yields 482
        // enumerated rows, so the row count alone proves nothing. `resolved()` and
        // `drawable()` are what separate "read the files" from "invented nothing".
        assert!(
            palette.resolved(),
            "no package opened: the chains were never resolved"
        );
        assert_eq!(
            palette.packages_without_path, 0,
            "engine-built stores always record a path"
        );
        assert_eq!(
            palette.entries.len(),
            482,
            "the corpus census is 482 creature rigblocks"
        );
        assert_eq!(
            palette.drawable(),
            464,
            "464 of 482 reach drawable geometry"
        );
        // A drawable row with no triangle count would draw an empty cell while still
        // counting as authorable, which is the "looks populated, is not" failure one
        // level down.
        let counted = palette
            .entries
            .iter()
            .filter(|entry| entry.drawable)
            .filter(|entry| entry.triangles.is_some())
            .count();
        assert_eq!(counted, 464, "every drawable row carries a triangle count");
    }
}
