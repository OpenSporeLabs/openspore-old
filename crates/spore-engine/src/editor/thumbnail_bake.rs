//! Baking part thumbnails: one camera, one render, forty pictures.
//!
//! # Why an atlas rather than a camera per cell
//!
//! The grid shows 40 parts per page. A camera per cell is 40 render-graph
//! viewports *every frame* for a picture that never changes, and the crate's own
//! note on [`crate::editor::thumbnails::ThumbnailCamera`] already says so. So this
//! renders **once**: all 40 meshes are placed on a grid in front of one
//! perspective camera aimed at that grid, the whole thing is captured in a single
//! read-back, and the capture is sliced into 40 tile images.
//!
//! The camera looks straight down the grid's normal rather than orbiting it,
//! because a tilted camera makes a grid of objects overlap their neighbours. The
//! 3/4 view that makes a thumbnail readable comes from **rotating each mesh**, not
//! from moving the camera, so every cell is framed identically.
//!
//! # What the pictures will and will not look like
//!
//! They will be the real part silhouettes, flat-shaded. They will **not** be the
//! reference's textured mouths, and no work in this file can change that:
//!
//! - a `gmsh` part record declares no TEXCOORD and no NORMAL element, so the
//!   meshes carry no UVs at all (see [`crate::convert::part_mesh`]);
//! - the part layer declares no texture dependency -- measured as **zero** across
//!   all 1824 containers at `0x40606000` -- so there is nothing to bind even if
//!   there were UVs.
//!
//! `spore-rigblock` does expose a UV-*like* attribute block (`0x08000202`, 8
//! bytes per vertex, float-plausible in 1616 of 1616 records) and grades its
//! meaning UNKNOWN. Reading it as floats anyway is how a decoder invents normals,
//! so it is left alone. That is a deliberate ceiling, recorded rather than
//! quietly worked around.
//!
//! # No stand-ins
//!
//! A cell that has not baked shows **nothing**, not a grey box, not a question
//! mark, not a procedural silhouette. This crate's rule is that an absent thing is
//! shown as absent, because a stand-in and a real-but-different picture are
//! indistinguishable to the person looking at the grid.

use bevy::camera::{
    visibility::RenderLayers, Camera, ClearColorConfig, ImageRenderTarget, RenderTarget,
};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use spore_creator_core::document::BlockId;

use crate::editor::thumbnails::{source_over, Palette, THUMBNAIL_SIDE};

/// Tiles across the atlas.
///
/// 8 x 5 = 40, which is exactly one page of the four-column grid. The atlas
/// arrangement is deliberately **not** the on-screen arrangement: the UI grid is
/// 4 wide by 10 tall because that is what the panel's width allows, while the
/// atlas wants to be roughly square so one camera frames it evenly.
pub const ATLAS_COLUMNS: usize = 8;
/// Tiles down the atlas.
pub const ATLAS_ROWS: usize = 5;
/// How many thumbnails one page holds, and so how many the atlas carries.
pub const TILES_PER_PAGE: usize = ATLAS_COLUMNS * ATLAS_ROWS;
/// World spacing between tile centres.
const SPACING: f32 = 1.0;
/// How much of its tile a part fills. Below 1 so neighbouring parts do not touch.
const FILL: f32 = 0.72;
/// The layer the staging meshes and the bake camera share.
///
/// Layer 2: layer 1 is the creature viewport's, and sharing it would put 40 parts
/// in the user's creature.
const BAKE_LAYER: usize = 2;

/// Where a bake has got to.
///
/// A resource rather than inferred from the world, because "is it baking?" and
/// "did it bake?" are the two questions that were previously indistinguishable --
/// a blank grid looks the same either way.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum BakeStage {
    /// Nothing started.
    #[default]
    Idle,
    /// Meshes are placed and the camera is live; waiting for the render.
    Rendering,
    /// The capture came back and the tiles are assigned.
    Done,
    /// The bake could not start, and `BakeState::reason` says why.
    Failed,
}

/// The atlas bake's state, for one page.
#[derive(Resource, Debug, Default)]
pub struct BakeState {
    /// Where it has got to.
    pub stage: BakeStage,
    /// Why a bake did not happen, or `None` while one is fine.
    pub reason: Option<String>,
    /// The page currently shown, so a bake can be told apart from a stale one.
    pub page: usize,
    /// How many tiles actually carry an image.
    pub tiles: u32,
    /// Frames the staged scene has been given before the capture is requested.
    pub frames: u32,
}

/// Marks an entity that exists only to be photographed.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThumbnailStage;

/// Marks the bake camera, so it can be despawned without touching the creature's.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BakeCamera;

/// The atlas texture: `ATLAS_COLUMNS * TILE` wide, `ATLAS_ROWS * TILE` tall.
///
/// `COPY_SRC` is not set. This build reads pixels back through
/// [`Screenshot`], which allocates its own intermediate texture with the usage it
/// needs, so a `COPY_SRC` here would be paying for a capability nothing uses --
/// and, more importantly, `TextureUsages::RENDER_ATTACHMENT` alone is what the
/// camera needs, so getting that wrong is the failure that matters.
pub fn atlas_image() -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: (ATLAS_COLUMNS as u32) * THUMBNAIL_SIDE,
            height: (ATLAS_ROWS as u32) * THUMBNAIL_SIDE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        // Transparent, so a part that fails to bake leaves an empty cell rather
        // than a black one. `thumbnail_image` clears to opaque black for the same
        // reason it must not: it is a per-cell target, this is a whole page.
        &[0, 0, 0, 0],
        TextureFormat::Rgba8UnormSrgb,
        bevy::asset::RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    image.sampler = ImageSampler::linear();
    image
}

/// Where tile `index` sits in the atlas, in pixels.
pub fn tile_rect(index: usize) -> (u32, u32, u32, u32) {
    let column = (index % ATLAS_COLUMNS) as u32;
    let row = (index / ATLAS_COLUMNS) as u32;
    (
        column * THUMBNAIL_SIDE,
        row * THUMBNAIL_SIDE,
        THUMBNAIL_SIDE,
        THUMBNAIL_SIDE,
    )
}

/// Cuts one tile out of a captured atlas image.
///
/// The capture arrives **tightly packed** -- `Screenshot` un-pads the 256-byte row
/// alignment itself (`screenshot.rs` in Bevy 0.19), unlike the lower-level
/// `Readback` path which hands back padded rows and makes the caller undo it. The
/// row stride below is therefore `width * 4` and not the aligned one; using the
/// aligned stride shears every tile but the first, which is the kind of bug that
/// looks like "the art is wrong" rather than "the stride is wrong".
pub fn slice_tile(atlas: &Image, index: usize) -> Option<Image> {
    let width = atlas.width() as usize;
    let (x, y, tile, _) = tile_rect(index);
    let pixels = atlas.data.as_ref()?;
    if (x as usize + tile as usize) > width
        || (y as usize + tile as usize) > atlas.height() as usize
    {
        return None;
    }
    let _stride = width * 4;
    let mut out = Vec::with_capacity((tile * tile * 4) as usize);
    for row in 0..tile as usize {
        let start = ((y as usize + row) * width + x as usize) * 4;
        out.extend_from_slice(&pixels[start..start + tile as usize * 4]);
    }
    Some(Image::new(
        Extent3d {
            width: tile,
            height: tile,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        out,
        atlas.texture_descriptor.format,
        bevy::asset::RenderAssetUsages::default(),
    ))
}

/// Places one part on its tile: centred, uniformly scaled, and turned a little.
///
/// # Why rotation is per-mesh and not per-camera
///
/// A camera angled off the grid's normal projects each tile's neighbour into it.
/// Per-mesh rotation gives every cell the same 3/4 read with zero overlap, and it
/// is why this can be one pass at all.
fn tile_transform(index: usize, mesh: &Mesh) -> Transform {
    let column = index % ATLAS_COLUMNS;
    let row = index / ATLAS_COLUMNS;
    let centre_x = (column as f32 - (ATLAS_COLUMNS as f32 - 1.0) * 0.5) * SPACING;
    let centre_y = ((ATLAS_ROWS as f32 - 1.0) * 0.5 - row as f32) * SPACING;

    // Bounds from the vertex positions rather than `Mesh::compute_aabb`, which
    // needs a render-world asset and this runs on the main world before upload.
    let Some((size, centre)) = mesh_bounds(mesh) else {
        return Transform::from_translation(Vec3::new(centre_x, centre_y, 0.0));
    };
    // Scale by the LONGEST axis, so the part fits whichever way it is oriented.
    // Scaling by the bounding-sphere radius would shrink a flat part to nothing.
    let longest = size.x.max(size.y).max(size.z).max(1e-6);
    let scale = SPACING * FILL / longest;

    // Rotate about the part's own centre FIRST, then move that centre onto the
    // tile. This order is what makes the rotation read as "the part is turned"
    // rather than "the part swung around the tile's origin", which is the
    // difference between a thumbnail and a scatter of off-centre fragments.
    let rotation = Quat::from_rotation_y(0.7) * Quat::from_rotation_x(0.25);
    let rotated_centre = rotation * (centre * scale);
    Transform {
        translation: Vec3::new(centre_x, centre_y, 0.0) - rotated_centre,
        rotation,
        scale: Vec3::splat(scale),
    }
}

/// A mesh's axis-aligned size and centre, from its vertex positions.
///
/// `None` for a mesh with no vertices, and for one whose positions are not
/// finite -- a NaN would otherwise propagate into a `Transform` and silently
/// delete the entity from the render, which is how a part "fails to bake" with no
/// error anywhere.
fn mesh_bounds(mesh: &Mesh) -> Option<(Vec3, Vec3)> {
    let positions = mesh.attribute(bevy::mesh::Mesh::ATTRIBUTE_POSITION)?;
    let positions = positions.as_float3()?;
    let mut lo = Vec3::splat(f32::INFINITY);
    let mut hi = Vec3::splat(f32::NEG_INFINITY);
    for p in positions.iter() {
        let point = Vec3::new(p[0], p[1], p[2]);
        if !point.x.is_finite() || !point.y.is_finite() || !point.z.is_finite() {
            return None;
        }
        lo = lo.min(point);
        hi = hi.max(point);
    }
    (hi.x >= lo.x).then(|| (hi - lo, (lo + hi) * 0.5))
}

/// How far back the bake camera must sit for the grid to land on the tile grid.
///
/// # This is the whole alignment contract
///
/// The slices assume **one part per [`THUMBNAIL_SIDE`] pixels**. That only holds
/// if one `SPACING` of world distance is exactly `THUMBNAIL_SIDE` pixels, which
/// fixes the distance uniquely:
///
/// ```text
///   visible height = ATLAS_ROWS * SPACING        (the grid exactly fills the image)
///   visible height = 2 * distance * tan(fov / 2) (what a perspective camera gives)
///   => distance    = (ATLAS_ROWS * SPACING / 2) / tan(fov / 2)
/// ```
///
/// The width then works out too, and only because the atlas aspect ratio equals
/// the grid's: `1024/640 == 8/5`. That is not a coincidence to preserve carelessly
/// -- it is the second half of the same equation, and `tile_alignment_holds` below
/// asserts it.
///
/// # The earlier version had a margin in it, and that was the bug
///
/// It added `+ 0.5` and scaled the vertical term by the aspect ratio, giving a
/// distance of 7.24 instead of 6.04 -- a 20% error that put the parts 106.7 px
/// apart on a 128 px grid. The atlas *looked* correct: 40 tidy silhouettes in 8
/// columns. Every one of the forty cells then displayed a slice cut from between
/// two parts, which renders as a thin horizontal smear, so the grid showed forty
/// identical smears over correctly-baked art.
///
/// **Any margin at all breaks this**, including a safety one, because the error is
/// a scale error and it is invisible in the atlas and obvious in the cells.
fn camera_distance() -> f32 {
    (ATLAS_ROWS as f32 * SPACING * 0.5) / (BAKE_FOV_DEGREES.to_radians() * 0.5).tan()
}

/// The bake camera's vertical field of view, in degrees.
///
/// 45, the same default the creature camera uses, so the two are comparable and
/// neither is a surprise. Spelled as a constant because
/// [`camera_distance`] divides by its tangent: a literal `45.0` inside that
/// function and a different one in a comment is how the two drift apart.
const BAKE_FOV_DEGREES: f32 = 45.0;

/// Spawns the staging scene for one page and returns how many parts placed.
///
/// Returns the count rather than a `Result` because a part that will not place is
/// an **expected** outcome (the catalogue has entries with no drawable geometry)
/// and must leave an empty cell, not abort the other 39.
#[allow(clippy::too_many_arguments)]
pub fn stage_page(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    _materials: &mut Assets<StandardMaterial>,
    palette: &Palette,
    store: Option<&spore_assets::ContentStore>,
    page: usize,
    material: Handle<StandardMaterial>,
) -> u32 {
    // `None` means a session with no packages, which is the corpus suite's normal
    // state. Reported as zero tiles rather than a panic, because "the grid is
    // empty" is the honest answer there and a crash is not.
    let Some(store) = store else {
        return 0;
    };
    let (source, _) = source_over(store);
    let entries: Vec<BlockId> = palette
        .entries
        .iter()
        .filter(|entry| entry.drawable)
        .map(|entry| entry.id)
        .collect();
    if entries.is_empty() {
        return 0;
    }
    let start = page * TILES_PER_PAGE;
    let mut placed = 0u32;
    for slot in 0..TILES_PER_PAGE {
        let Some(id) = entries.get(start + slot).copied() else {
            break;
        };
        let Some(geometry) = spore_morphology::geometry_of(&source, id) else {
            continue;
        };
        let Some(mesh) = crate::convert::part_mesh(&geometry) else {
            continue;
        };
        let handle = meshes.add(mesh.clone());
        let transform = tile_transform(slot, &mesh);
        commands.spawn((
            Mesh3d(handle),
            MeshMaterial3d(material.clone()),
            transform,
            ThumbnailStage,
            RenderLayers::layer(BAKE_LAYER),
        ));
        placed += 1;
    }
    placed
}

/// Spawns the bake camera and its light, framed on the atlas grid.
pub fn stage_camera(commands: &mut Commands, atlas: Handle<Image>) {
    let distance = camera_distance();
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 2,
            // Transparent, so a part with no geometry in its cell shows the page
            // background rather than black. Same reasoning as the atlas fill.
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
        RenderTarget::Image(ImageRenderTarget {
            handle: atlas,
            scale_factor: 1.0,
        }),
        Transform::from_xyz(0.0, 0.0, distance).looking_at(Vec3::ZERO, Vec3::Y),
        BakeCamera,
        RenderLayers::layer(BAKE_LAYER),
    ));
    // Two lights, because a single key at the scene's `9_000` still photographs as
    // near-black silhouettes: the capture is sRGB-encoded and a mid-tone material
    // under one directional light lands in the bottom of the curve. A fill from the
    // opposite side opens the shadow side so a part's shape reads. The creature
    // viewport has the same two-light rig, which is why they match.
    commands.spawn((
        DirectionalLight {
            illuminance: 26_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(1.5, 2.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        ThumbnailStage,
        RenderLayers::layer(BAKE_LAYER),
    ));
    commands.spawn((
        DirectionalLight {
            illuminance: 11_000.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-2.0, -1.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
        ThumbnailStage,
        RenderLayers::layer(BAKE_LAYER),
    ));
}

/// Removes the staging scene once the capture is in hand.
///
/// Despawning by marker rather than by remembering entity ids: the camera and the
/// forty meshes are created by two systems that need not agree on a list, and a
/// leaked bake camera would render the grid into the user's viewport forever.
pub fn clear_stage(mut commands: Commands, staged: Query<Entity, With<ThumbnailStage>>) {
    for entity in staged.iter() {
        commands.entity(entity).despawn();
    }
}
// ---------------------------------------------------------------------------
// The systems that drive one bake, in order.
// ---------------------------------------------------------------------------

/// The creature rigblock group the grid enumerates.
pub const PART_GROUP: u32 = 0x4062_6000;

/// The palette, built once.
#[derive(Resource, Debug, Default)]
pub struct PaletteState {
    /// The resolved catalogue, or `None` when the session has no packages.
    pub palette: Option<Palette>,
}

impl PaletteState {
    /// Records a resolved catalogue.
    pub fn set(&mut self, palette: Palette) {
        self.palette = Some(palette);
    }
}

/// The atlas texture, held so a later system can ask for its capture.
#[derive(Resource, Debug, Clone)]
pub struct AtlasHandle(pub Handle<Image>);

/// Which cell of the grid an entity is.
///
/// Assigned at spawn rather than inferred from iteration order: two queries for
/// "the nth part cell" would otherwise be two answers, and `ChildOf` order is an
/// implementation detail of `Children` that a filter change can reorder.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartCellIndex(pub usize);

/// Builds the palette, once.
///
/// # Once, and it has to be
///
/// Resolving 482 rigblock identities is a read of each `prop`, its paired
/// container and its `gmsh`, and the measured cost is roughly 340k vertices of
/// decode. Doing that inside `refresh_view_model` -- which runs **every frame** --
/// would make the editor unusable, so this is gated on `is_none`.
pub fn prepare_palette(
    state: Res<crate::editor::plugin::CreatorEditorState>,
    mut palette: ResMut<PaletteState>,
) {
    if palette.palette.is_some() {
        return;
    }
    let Some(store) = state.session.store() else {
        return;
    };
    palette.palette = Some(crate::editor::thumbnails::build_palette(store, PART_GROUP));
}

/// Starts one bake, once per page.
///
/// A `Startup` system rather than an `Update` one: it must not be chained against
/// the creature's spawn systems, and a bake that re-ran on every edit would redo
/// forty decodes for a picture that has not changed.
pub fn start_bake(
    mut commands: Commands,
    state: Res<crate::editor::plugin::CreatorEditorState>,
    palette: Res<PaletteState>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut bake: ResMut<BakeState>,
    atlas: Res<AtlasHandle>,
) {
    if bake.stage != BakeStage::Idle {
        return;
    }
    let Some(catalogue) = palette.palette.as_ref() else {
        // No packages: this is the corpus suite's normal state, so it is a stated
        // reason rather than a silent nothing.
        bake.stage = BakeStage::Failed;
        bake.reason = Some("no packages, so no parts to photograph".to_owned());
        return;
    };
    if !catalogue.resolved() {
        bake.stage = BakeStage::Failed;
        bake.reason = Some(format!(
            "the palette opened {} of {} packages, so every part would be blank",
            catalogue.packages_opened,
            catalogue.packages_opened + catalogue.packages_without_path
        ));
        return;
    }
    let page = 0usize;
    let material = materials.add(StandardMaterial {
        // Pale cream, OpenSpore's own choice.
        //
        // It started as the document's own skin base, on the reasoning that a
        // thumbnail should look like the creature it replaces. That was wrong on
        // both counts: the bake photographs the part in isolation, with no
        // lighting context to carry it, and a mid-tone brown on the panel's dark
        // blue measured at RGB (44, 22, 16) -- a shape you can barely see. The
        // reference's own part thumbnails are pale, and legibility on a dark panel
        // is the requirement here, not colour fidelity to a skin nobody chose yet.
        base_color: Color::srgb(0.88, 0.82, 0.72),
        perceptual_roughness: 0.8,
        metallic: 0.0,
        ..default()
    });
    let placed = stage_page(
        &mut commands,
        &mut meshes,
        &mut materials,
        catalogue,
        state.session.store(),
        page,
        material,
    );
    stage_camera(&mut commands, atlas.0.clone());
    bake.page = page;
    bake.tiles = placed;
    bake.stage = if placed == 0 {
        BakeStage::Failed
    } else {
        BakeStage::Rendering
    };
    if placed == 0 {
        bake.reason = Some("no part on this page produced drawable geometry".to_owned());
    }
}

/// Asks for the capture, once the staged meshes have had frames to upload.
///
/// The wait is not optional. `Commands::spawn` is deferred and the render world
/// extracts assets a frame later, so capturing on the same frame the bake starts
/// photographs an empty grid -- and an empty grid slices into forty valid,
/// fully-transparent tiles, which is the most convincing way to produce a blank
/// result.
pub fn request_capture(
    mut commands: Commands,
    mut bake: ResMut<BakeState>,
    atlas: Res<AtlasHandle>,
) {
    if bake.stage != BakeStage::Rendering {
        return;
    }
    if bake.frames < SETTLE_FRAMES {
        bake.frames += 1;
        return;
    }
    commands
        .spawn(bevy::render::view::screenshot::Screenshot::image(
            atlas.0.clone(),
        ))
        .observe(on_captured);
}

/// Frames the staged scene is given before the capture.
const SETTLE_FRAMES: u32 = 6;

/// Slices the capture into the cells.
///
/// An observer rather than a poll, because `Screenshot` despawns its own entity
/// once captured and the image is delivered as an owned value that only exists
/// here.
fn on_captured(
    capture: On<bevy::render::view::screenshot::ScreenshotCaptured>,
    // No `Commands`: the staging scene is torn down by `clear_staging`, which can
    // take the wider parameter set this observer cannot.
    mut cells: Query<&mut ImageNode, With<PartCellIndex>>,
    mut images: ResMut<Assets<Image>>,
    mut bake: ResMut<BakeState>,
) {
    // QA escape hatch: write the raw capture so a wrong atlas can be looked at
    // rather than inferred from forty identical smears. Off unless the variable is
    // set, because writing a file from a render callback is not something to do
    // silently.
    if std::env::var_os("OPENSPORE_DUMP_ATLAS").is_some() {
        let path = std::env::var("OPENSPORE_DUMP_ATLAS").unwrap_or_default();
        let path = std::path::PathBuf::from(path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        // Raw RGBA plus a one-line header, because this crate has no PNG encoder
        // and taking a dependency on one for a debug hook is not a trade worth
        // making. `bevy_image::Image::save_to_disk` does not exist in 0.19.
        let header = format!("{} {} 4\n", capture.image.width(), capture.image.height());
        match capture.image.data.as_ref() {
            Some(pixels) => {
                let written =
                    std::fs::write(&path, [header.as_bytes(), pixels.as_slice()].concat());
                match written {
                    Ok(()) => info!("atlas dumped to {path:?}"),
                    Err(error) => warn!("atlas dump to {path:?} failed: {error}"),
                }
            }
            None => warn!("atlas dump to {path:?}: the capture carried no pixels"),
        }
    }

    let mut assigned = 0u32;
    for mut cell in cells.iter_mut() {
        let Some(tile) = slice_tile(&capture.image, assigned as usize) else {
            break;
        };
        cell.image = images.add(tile);
        assigned += 1;
    }
    bake.tiles = assigned;
    bake.stage = if assigned == 0 {
        BakeStage::Failed
    } else {
        BakeStage::Done
    };
    if assigned == 0 {
        bake.reason = Some("the capture produced no readable tiles".to_owned());
    }
}

/// Removes the staging scene once the bake is finished with it.
///
/// Separate from [`on_captured`] because the observer gets a narrower set of
/// systems than an `Update` one, and "despawn forty meshes and a camera" is not
/// worth fighting an observer signature for.
///
/// Runs on `Done` **and** `Failed`: a bake that placed meshes and then could not
/// photograph them has still left forty entities in the world, and they would
/// otherwise render into the creature's viewport forever. That leak is silent --
/// the user's creature would simply gain a grid of extra parts.
pub fn clear_staging(
    mut commands: Commands,
    bake: Res<BakeState>,
    staged: Query<Entity, (With<ThumbnailStage>, Without<KeepAlive>)>,
    cameras: Query<Entity, With<BakeCamera>>,
) {
    if bake.stage != BakeStage::Done && bake.stage != BakeStage::Failed {
        return;
    }
    for entity in staged.iter().chain(cameras.iter()) {
        commands.entity(entity).despawn();
    }
}

/// Publishes the bake's outcome where BRP can read it.
///
/// A blank grid is indistinguishable from "the catalogue is empty", "the atlas
/// captured nothing", and "the tiles were sliced out of the wrong place", and all
/// three look identical in a screenshot. These four numbers separate them.
pub fn publish_bake_state(
    bake: Res<BakeState>,
    mut runtime: ResMut<crate::creator::CreatorRuntimeState>,
) {
    runtime.bake_stage = format!("{:?}", bake.stage);
    runtime.bake_tiles = bake.tiles;
    runtime.bake_reason = bake.reason.clone();
    runtime.bake_pages = runtime.bake_pages.max(1);
}

/// Marks an entity that must survive [`clear_staging`].
///
/// Unused today, and deliberately so: it exists so that "the staging sweep
/// despawns everything with the marker" cannot silently grow to include the
/// creature's own content, which lives on a different layer but would be a
/// natural mistake to make later.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeepAlive;

/// The `BlockId` a cell shows, or `None` when that cell has no part.
///
/// The cell-to-part mapping is the **drawable entries in catalogue order**, which
/// is exactly the order [`stage_page`] places them into the atlas -- so cell `n`
/// and tile `n` are the same part by construction, not by two lists agreeing.
pub fn cell_part(catalogue: &Palette, cell: usize) -> Option<BlockId> {
    catalogue
        .entries
        .iter()
        .filter(|entry| entry.drawable)
        .map(|entry| entry.id)
        .nth(cell)
}

/// Turns a click on a grid cell into a document edit.
///
/// # Replace when something is selected, add when nothing is
///
/// One click, two meanings, decided by whether a block is selected. That is the
/// reference's behaviour as far as it is observable here, and it is the only rule
/// that needs no mode switch the UI does not have.
///
/// The edit goes through [`crate::editor::session::CreatorSession`], so it records
/// a history row like every other edit and therefore undoes with the same button.
/// Writing the document directly would make parts the only edit the user cannot
/// take back.
pub fn apply_cell_choice(
    state: &mut crate::editor::plugin::CreatorEditorState,
    part: BlockId,
) -> Result<String, String> {
    match state.session.selection() {
        Some(block) => state
            .session
            .apply_replace(block, part)
            .map(|()| format!("replaced block {block} with {part:?}")),
        None => state
            .session
            .apply_add(part, None)
            .map(|index| format!("added {part:?} as block {index}")),
    }
    .map_err(|error| error.to_string())
}

/// The grid cells whose pointer state changed this frame.
///
/// A named alias because the filter is long enough that a caller could typo it
/// into something that still compiles -- and `Changed<Interaction>` dropped turns
/// this from "one press is one edit" into "sixty edits a second", which is the
/// failure mode the name is here to make obvious.
type PressedCells<'w, 's> =
    Query<'w, 's, (Entity, &'static Interaction), (Changed<Interaction>, With<PartCellIndex>)>;

/// Reads a press on a grid cell and applies it.
///
/// `Changed<Interaction>` rather than a poll, so one press is one edit: a system
/// that runs every frame would replace the selected block sixty times a second
/// and fill the undo history with identical rows.
pub fn read_part_cell_click(
    cells: PressedCells,
    indices: Query<&PartCellIndex>,
    palette: Res<PaletteState>,
    mut state: ResMut<crate::editor::plugin::CreatorEditorState>,
) {
    let Some(catalogue) = palette.palette.as_ref() else {
        return;
    };
    for (entity, interaction) in cells.iter() {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let Ok(index) = indices.get(entity) else {
            continue;
        };
        let Some(part) = cell_part(catalogue, index.0) else {
            continue;
        };
        match apply_cell_choice(&mut state, part) {
            Ok(what) => info!("part cell {index:?}: {what}"),
            Err(error) => warn!("part cell {index:?} refused: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The one test that matters for the cells: a slice must land on a part.
    ///
    /// Asserts the geometry rather than "the atlas has pixels", because the atlas
    /// looked perfectly good while every cell was wrong -- 40 tidy silhouettes on
    /// a grid that did not match the tiles.
    #[test]
    fn tile_alignment_holds() {
        // One world unit must be exactly THUMBNAIL_SIDE pixels.
        let distance = camera_distance();
        let visible_height = 2.0 * distance * (BAKE_FOV_DEGREES.to_radians() * 0.5).tan();
        let atlas_height = (ATLAS_ROWS as f32) * THUMBNAIL_SIDE as f32;
        let pixels_per_unit = atlas_height / visible_height;
        assert!(
            (pixels_per_unit - THUMBNAIL_SIDE as f32).abs() < 0.5,
            "one world unit must be {THUMBNAIL_SIDE} px so slices land on parts, \
             got {pixels_per_unit:.2} px/unit (distance {distance:.4})"
        );
    }

    /// The width must line up as well, and only because the atlas aspect matches.
    #[test]
    fn the_atlas_aspect_matches_the_grid_aspect() {
        let atlas = (ATLAS_COLUMNS as f32 * THUMBNAIL_SIDE as f32)
            / (ATLAS_ROWS as f32 * THUMBNAIL_SIDE as f32);
        let grid = ATLAS_COLUMNS as f32 / ATLAS_ROWS as f32;
        assert!(
            (atlas - grid).abs() < 1e-6,
            "the camera frames by aspect: an atlas that is not the grid's shape \
             makes the columns drift out of their tiles even when the rows do not. \
             atlas {atlas}, grid {grid}"
        );
    }

    /// Every tile must be inside the atlas, or the last row slices into nothing.
    #[test]
    fn every_tile_is_inside_the_atlas() {
        for index in 0..TILES_PER_PAGE {
            let (x, y, tile, _) = tile_rect(index);
            assert!(
                x + tile <= ATLAS_COLUMNS as u32 * THUMBNAIL_SIDE,
                "tile {index} runs past the atlas width"
            );
            assert!(
                y + tile <= ATLAS_ROWS as u32 * THUMBNAIL_SIDE,
                "tile {index} runs past the atlas height"
            );
        }
    }

    /// Slicing must copy the tile's own pixels, not a neighbour's row.
    ///
    /// A stride error shears every tile but the first and still produces
    /// plausible-looking images, which is why the arithmetic is pinned to a known
    /// pattern rather than to "the output is not empty".
    #[test]
    fn slicing_copies_exactly_one_tile() {
        let width = ATLAS_COLUMNS as u32 * THUMBNAIL_SIDE;
        let height = ATLAS_ROWS as u32 * THUMBNAIL_SIDE;
        // A distinctive value per pixel, so a wrong offset cannot hide.
        let pixels: Vec<u8> = (0..(width * height * 4)).map(|i| (i % 251) as u8).collect();
        let width_px = width as usize;
        let atlas = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels.clone(),
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        // Tile 0 is the top-left THUMBNAIL_SIDE square.
        let tile = slice_tile(&atlas, 0).expect("tile 0 is inside the atlas");
        assert_eq!(
            (tile.width(), tile.height()),
            (THUMBNAIL_SIDE, THUMBNAIL_SIDE)
        );
        let side = THUMBNAIL_SIDE as usize;
        for y in 0..side {
            for x in 0..side {
                let got = tile.data.as_ref().expect("slice carries pixels");
                let got = got[(y * side + x) * 4];
                let want = pixels[y * width_px * 4 + x * 4];
                assert_eq!(got, want, "pixel ({x}, {y}) came from the wrong place");
            }
        }
    }

    /// Tile 5 is the first of row 1, so it is the stride test.
    #[test]
    fn slicing_crosses_rows_correctly() {
        let width = ATLAS_COLUMNS as u32 * THUMBNAIL_SIDE;
        let height = ATLAS_ROWS as u32 * THUMBNAIL_SIDE;
        let pixels: Vec<u8> = (0..(width * height * 4)).map(|i| (i % 251) as u8).collect();
        let atlas = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels.clone(),
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        let tile = slice_tile(&atlas, ATLAS_COLUMNS).expect("first tile of row 1");
        let (x, y, _, _) = tile_rect(ATLAS_COLUMNS);
        assert_eq!(y, THUMBNAIL_SIDE, "tile 8 starts the second row");
        let got = tile.data.as_ref().expect("slice carries pixels");
        assert_eq!(
            got[0],
            pixels[y as usize * width as usize * 4 + x as usize * 4],
            "the first pixel of row 1 must come from row 1, not row 0"
        );
    }

    /// A tile outside the atlas is refused rather than panicking.
    #[test]
    fn slicing_past_the_end_is_refused() {
        let width = ATLAS_COLUMNS as u32 * THUMBNAIL_SIDE;
        let height = ATLAS_ROWS as u32 * THUMBNAIL_SIDE;
        let atlas = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            vec![0u8; (width * height * 4) as usize],
            TextureFormat::Rgba8UnormSrgb,
            bevy::asset::RenderAssetUsages::default(),
        );
        assert!(
            slice_tile(&atlas, TILES_PER_PAGE).is_none(),
            "a tile past the last one has nothing to copy"
        );
    }

    /// A part must land inside its own tile.
    #[test]
    fn a_part_is_centred_in_its_tile() {
        // A unit-ish mesh, deliberately offset from the origin, which is how the
        // real parts are: modelled around the origin but not exactly on it.
        let mesh = offset_mesh(0.2);
        let transform = tile_transform(0, &mesh);
        let mapped = transform.transform_point(centre_of(&mesh));
        // Tile 0's centre is the top-left of the grid, at (-3.5, +2.5).
        let want = Vec3::new(
            -(ATLAS_COLUMNS as f32 - 1.0) * 0.5 * SPACING,
            (ATLAS_ROWS as f32 - 1.0) * 0.5 * SPACING,
            0.0,
        );
        assert!(
            mapped.distance(want) < 1e-3,
            "the part's own centre must land on the tile's centre; got {mapped:?} wanted {want:?}"
        );
    }

    /// The centre of the LAST tile, which is where an off-by-one in the row maths
    /// would show up.
    #[test]
    fn the_last_tile_is_centred_too() {
        let mesh = offset_mesh(0.2);
        let transform = tile_transform(TILES_PER_PAGE - 1, &mesh);
        let mapped = transform.transform_point(centre_of(&mesh));
        let column = (TILES_PER_PAGE - 1) % ATLAS_COLUMNS;
        let row = (TILES_PER_PAGE - 1) / ATLAS_COLUMNS;
        let want = Vec3::new(
            (column as f32 - (ATLAS_COLUMNS as f32 - 1.0) * 0.5) * SPACING,
            ((ATLAS_ROWS as f32 - 1.0) * 0.5 - row as f32) * SPACING,
            0.0,
        );
        assert!(
            mapped.distance(want) < 1e-3,
            "the last tile is at column {column} row {row}; got {mapped:?} wanted {want:?}"
        );
    }

    /// A mesh with no positions has no centre, and must not become a NaN transform.
    #[test]
    fn a_mesh_with_no_positions_gets_a_plain_translation() {
        let mesh = Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        let transform = tile_transform(0, &mesh);
        assert!(transform.translation.is_finite());
        assert_eq!(transform.scale, Vec3::ONE);
    }

    /// A one-triangle mesh deliberately **offset from the origin**.
    ///
    /// Offset on purpose: parts are modelled around the origin but not exactly on
    /// it (measured median centre `[0.000, -0.094, 0.000]`), so a test whose mesh
    /// is centred on the origin cannot tell "the part is centred in its tile" from
    /// "the transform happens to be the identity".
    fn offset_mesh(span: f32) -> Mesh {
        let mut mesh = Mesh::new(
            bevy::mesh::PrimitiveTopology::TriangleList,
            bevy::asset::RenderAssetUsages::default(),
        );
        mesh.insert_attribute(
            bevy::mesh::Mesh::ATTRIBUTE_POSITION,
            // Asymmetric on purpose, and non-zero on every axis: a symmetric mesh
            // with a zero on one axis cannot distinguish a wrong Y from a right one.
            vec![
                [0.0, 0.0, 0.0],
                [span, span * 0.5, span * 0.3],
                [0.0, span, span * 0.6],
            ],
        );
        mesh
    }

    /// The mesh's own centre, read the same way [`tile_transform`] reads it, so
    /// the probe point cannot drift away from what the function measured.
    fn centre_of(mesh: &Mesh) -> Vec3 {
        mesh_bounds(mesh).expect("the test mesh has positions").1
    }
}
