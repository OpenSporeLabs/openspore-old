//! The painted backdrop behind the creature.
//!
//! # What this is, stated first because it is easy to overclaim
//!
//! The reference Creator shows a **live 3D landscape**: sky, trees receding to a
//! horizon, grass, and a metal platform with a specular highlight. That scene is
//! **not stored**. Measured over the whole install:
//!
//! - `Spore_Graphics` contains **zero** `gmdl`, **zero** `raster` and **zero**
//!   `backgroundMap` records. Every mesh and texture in the game lives in
//!   `Spore_Content`, grouped per *model* (`CreatureModels`, `FloraModels`,
//!   `CellModels`, ...), with no scene or terrain record of any kind.
//! - There is no heightmap record class: the game's own type registry names
//!   `hm 0x0376C3DA`, and **zero** records of that type exist in any of the seven
//!   packages.
//! - The one `backgroundMap` record (`0x612B3191`, exactly one) is a 12-stop
//!   solid-colour ramp keyed to a geometric distance ladder. It is the **cell
//!   stage**'s atmospheric depth ramp, not a picture, and not the Creator's.
//! - `populate` markers carry `zOffset`, `count` and `plantType` and **no x/y**.
//!   The scene's layout is generated procedurally at runtime by
//!   `Terrain::cTerrainSphere`, whose decode path is a reverse-engineering project
//!   (`docs/analysis/future-research/08-pkg-14-terrain-world.md` is marked
//!   `BLOCKED_BY_RE`).
//!
//! So a faithful 3D backdrop is not a task, it is a research programme. What
//! exists is Spore's own **painted** scenery, shipped as raw `png` records in
//! `Spore_Graphics`. This module puts one of those on screen, and it is labelled
//! a painted matte rather than a scene because that is what it is.
//!
//! # Which record, and why this one
//!
//! [`BACKDROP`] is `0x02e9a2ca:0x9fee7381`: a 348x135 painted panorama of an alien
//! forest under a teal sky, with green foliage along the bottom edge. It was
//! chosen from the ten-odd candidates in that group by composition -- sky above,
//! horizon in the middle, ground below -- because that is the read the editor
//! needs. Its siblings are space, nebula, underwater and coral variants, which
//! are equally real and equally wrong here.
//!
//! It is 348x135 being drawn into a 768x665 region, so it is upscaled roughly
//! 2.6x and it is visibly soft. That is a property of the source art, stated
//! here rather than discovered later and mistaken for a rendering bug. The
//! highest-resolution painted alternative found -- `0x31a44893:0xc600bf3a` at
//! 1024x512 -- is an **orbital planet view** (a green globe seen from space), and
//! putting that behind a creature on the ground was tried and rejected: it reads
//! as a planet, not a forest.
//!
//! # The circle mask is cropped off, and that was measured rather than assumed
//!
//! All ten records in group `0x02E9A2CA` are the same 348x135 planet-preview strip
//! with a **circular mask** cut out of one end. Drawn uncropped, the mask is a
//! black ellipse covering the right third of the viewport -- measured: transparent
//! pixels render as the UI camera's clear colour, and the creature's largest
//! vertebra sat on top of it.
//!
//! An earlier version of this comment argued for keeping the soft alpha edge,
//! on the grounds that it reads as atmospheric depth. That was wrong, and the
//! screenshot said so: the fade is not a vignette, it is the *inside* of a hole,
//! and the surviving sliver at the far end is 9 painted rows out of 135. So
//! [`crop_to_painted`] cuts to the region that is actually painted --
//! [`PAINTED_FRACTION`] of the height, per column -- which for every one of the
//! ten strips yields 277-278 x 135.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::ui::ui_transform::UiGlobalTransform;
use bevy::ui::ComputedNode;
use spore_assets::ContentStore;
use spore_core::ResourceKey;

/// The painted backdrop: `png 0x2F7D0004 : 0x02E9A2CA : 0x9FEE7381`.
///
/// **OBSERVED.** A 348x135 RGBA8 painted panorama in `Spore_Graphics`, alpha 0-255
/// with a soft mask on the right end. Group `0x02E9A2CA` holds ten such
/// planet-preview strips; this one is the forest.
pub const BACKDROP: ResourceKey = ResourceKey::new(0x2F7D_0004, 0x02E9_A2CA, 0x9FEE_7381);

/// Loads the backdrop as a Bevy image, or `None` with a reason.
///
/// `None` is an ordinary outcome, not an error path: a session opened on
/// synthetic records has no packages at all, and `Spore_Graphics` may simply not
/// be among the ones that were opened. Both leave the creature against the clear
/// colour, which is what shipped until this existed.
pub fn load_backdrop(store: Option<&ContentStore>) -> Result<Image, String> {
    let store = store.ok_or_else(|| {
        "this session has no packages, so there is no backdrop to read".to_owned()
    })?;
    let bytes = store
        .read(&BACKDROP)
        .map_err(|error| format!("{} is absent: {error}", BACKDROP))?;
    let decoded = spore_texture::decode_png(&bytes).map_err(|error| error.to_string())?;
    // Cropped, because the record is a circle-masked planet-preview strip. See
    // the module docs; the mask is a hole, not a vignette.
    Ok(to_image(crop_to_painted(decoded)))
}

/// How much of a column (or row) must be painted for it to survive the crop.
///
/// 0.45 rather than "any". The mask's rim contributes a thin wedge a few rows tall
/// at the far end -- 9 rows out of 135 -- and an `alpha > 0` test keeps it, which
/// is how a cut-out hole survives a crop that was supposed to remove it. Nearly
/// half the line is the threshold that separates "painted" from "edge of a hole",
/// and it is measured across all ten strips: every one crops to 277-278 x 135,
/// which is what ten identical 348-wide strips should reduce to.
pub const PAINTED_FRACTION: f32 = 0.45;

/// Alpha above which a pixel counts as painted at all.
///
/// Not 255: the strips are anti-aliased at their edges, so a strict test would
/// shave a few pixels off every side and make the crop depend on the edge
/// filtering rather than on the artwork.
const ALPHA_FLOOR: u8 = 8;

/// Crops a decoded strip to its painted region, discarding the circle mask.
///
/// Returns the input unchanged when it is already fully painted, so a record with
/// no mask costs nothing and is not silently re-cropped to a single pixel by a
/// degenerate alpha column.
pub fn crop_to_painted(image: spore_texture::PngImage) -> spore_texture::PngImage {
    let (width, height) = (image.width as usize, image.height as usize);
    if width == 0 || height == 0 {
        return image;
    }
    let painted = |offset: usize| image.pixels[offset + 3] > ALPHA_FLOOR;

    // Columns first: a column is painted if enough of its height is painted.
    let column_painted: Vec<usize> = (0..width)
        .map(|x| {
            (0..height)
                .filter(|y| painted(y * width * 4 + x * 4))
                .count()
        })
        .collect();
    let Some((x0, x1)) = span_with_fraction(&column_painted, height, PAINTED_FRACTION) else {
        return image;
    };
    // Then rows within the surviving columns, so a horizontal mask is caught too.
    let row_painted: Vec<usize> = (0..height)
        .map(|y| (x0..=x1).filter(|x| painted(y * width * 4 + x * 4)).count())
        .collect();
    let Some((y0, y1)) = span_with_fraction(&row_painted, x1 - x0 + 1, PAINTED_FRACTION) else {
        return image;
    };

    let new_width = (x1 - x0 + 1) as u32;
    let new_height = (y1 - y0 + 1) as u32;
    let mut pixels = Vec::with_capacity(new_width as usize * new_height as usize * 4);
    for y in y0..=y1 {
        let start = (y * width + x0) * 4;
        let end = (y * width + x1 + 1) * 4;
        pixels.extend_from_slice(&image.pixels[start..end]);
    }
    spore_texture::PngImage {
        width: new_width,
        height: new_height,
        pixels,
    }
}

/// The first and last index whose count clears `fraction` of `extent`.
fn span_with_fraction(counts: &[usize], extent: usize, fraction: f32) -> Option<(usize, usize)> {
    let floor = (extent as f32 * fraction) as usize;
    let first = counts.iter().position(|count| *count > floor)?;
    let last = counts.iter().rposition(|count| *count > floor)?;
    Some((first, last))
}

/// Wraps decoded pixels as a Bevy [`Image`] ready to draw.
///
/// sRGB, because the record is an sRGB PNG and the alternative is a washed-out
/// backdrop that looks like a colour-space bug rather than a picture.
pub fn to_image(decoded: spore_texture::PngImage) -> Image {
    Image::new(
        Extent3d {
            width: decoded.width,
            height: decoded.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        decoded.pixels,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

/// Puts the backdrop on the 3D region, once, if it can be read.
///
/// Runs in `PostStartup` because the node it fills is spawned by
/// `spawn_creator_screen` in `Startup` and `Commands::spawn` is deferred -- the
/// same trap as the camera framing, and the same reason this is not in `Startup`.
///
/// The image is drawn by the **UI** camera, not the creature camera, and that is
/// what puts it *behind* the creature rather than in front of it: the UI camera
/// has `order: 0` and the creature's viewport camera `order: 1`, and the latter
/// does not clear (`ClearColorConfig::None`, see `editor::viewport`). So the
/// creature composites over the backdrop with nothing in between. If the creature
/// camera ever clears again, this image disappears behind it -- which is a
/// property of the layering, not of the backdrop, and is why the clear is
/// asserted in `editor::viewport` rather than left implicit.
pub fn install_backdrop(
    mut commands: Commands,
    state: Res<crate::editor::plugin::CreatorEditorState>,
    mut nodes: PendingBackdrops,
    mut images: ResMut<Assets<Image>>,
) {
    let Ok((entity, mut node)) = nodes.single_mut() else {
        // NOT silent. The first version returned here without a word, and the
        // symptom -- a black viewport and no warning anywhere -- is exactly what
        // a missing asset looks like, so the two could not be told apart. An exit
        // that produces no observable state has to say so.
        let found = nodes.iter().count();
        warn!(
            "creator backdrop not shown: expected exactly one backdrop node with an \
             ImageNode, found {found}; the node was never spawned or was spawned twice"
        );
        return;
    };
    // Marked installed whether or not it worked.
    //
    // The first version gated on `node.image != Handle::<Image>::default()`, which
    // is wrong in a way worth recording: `ImageNode::default()` sets
    // `TRANSPARENT_IMAGE_HANDLE`, a *valid* handle to Bevy's built-in 1x1
    // transparent texture, not the default handle. So the guard read "already
    // installed" on the very first frame, returned before ever reaching the load,
    // and the backdrop silently never appeared -- with no warning, because the
    // warn! lived below the early return.
    //
    // A marker component cannot be wrong that way. It says what happened rather
    // than inferring it from a value whose default is not the obvious one.
    match load_backdrop(state.session.store()) {
        Ok(image) => {
            info!(
                "creator backdrop loaded: {}x{} from {}",
                image.width(),
                image.height(),
                BACKDROP
            );
            node.image = images.add(image);
        }
        Err(reason) => {
            // Once, and visible. A silently missing backdrop is exactly the
            // "the art is not there" report this module exists to prevent, and a
            // black rectangle is indistinguishable from a render failure.
            warn!("creator backdrop not shown: {reason}");
        }
    }
    commands.entity(entity).insert(BackdropInstalled);
}

/// Republishes the backdrop node's laid-out geometry every frame.
///
/// The counterpart to [`install_backdrop`]: that one answers "did the bytes load",
/// this one answers "is there anything on screen to draw them into". Both are
/// needed, because a `Handle<Image>` holding real pixels and a `ComputedNode` of
/// `0 x 0` are indistinguishable in a screenshot -- the region is simply black --
/// and the two failures have completely different fixes.
pub fn publish_backdrop_geometry(
    nodes: Query<(&ComputedNode, &UiGlobalTransform), With<BackdropInstalled>>,
    mut runtime: ResMut<crate::creator::CreatorRuntimeState>,
) {
    runtime.backdrop_loaded = runtime.backdrop_loaded || !nodes.is_empty();
    let Ok((node, transform)) = nodes.single() else {
        runtime.backdrop_size = None;
        runtime.backdrop_position = None;
        return;
    };
    runtime.backdrop_size = Some([node.size.x, node.size.y]);
    let half = node.size * 0.5;
    let min = transform.translation - half;
    runtime.backdrop_position = Some([min.x, min.y]);
}

/// The one backdrop node whose image slot is still unresolved.
///
/// `Without<BackdropInstalled>` is what makes this idempotent, and it is why
/// [`install_backdrop`] can run in `PostStartup` and simply stop: there is no
/// counter and no handle comparison to get wrong.
type PendingBackdrops<'w, 's> =
    Query<'w, 's, (Entity, &'static mut ImageNode), (With<Backdrop>, Without<BackdropInstalled>)>;

/// Marks **the** backdrop node.
///
/// Its own component rather than a `CreatorUiPart` filter, and that is a fix
/// rather than a style choice. The query used to filter on
/// `With<CreatorUiPart>`, which was unique when the backdrop was the only node
/// carrying an `ImageNode`. Adding one to each of the forty part cells made it
/// match **41** entities, `single_mut()` refused, and the backdrop silently went
/// away again -- the log said so outright: "expected exactly one backdrop node
/// with an ImageNode, found 41".
///
/// A filter broad enough to collect every future `ImageNode` is not a filter.
/// This one names the node it means.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Backdrop;

/// Marks a backdrop node whose image slot has been resolved, one way or the other.
///
/// See [`install_backdrop`] for why this exists instead of a handle comparison.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BackdropInstalled;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backdrop_is_a_raw_png_record_and_not_a_raster() {
        // If this ever becomes a raster id the decoder in `load_backdrop` is the
        // wrong one, and the failure would be "not a valid image" at runtime
        // rather than here.
        assert_eq!(BACKDROP.type_id, 0x2F7D_0004, "png, not raster 0x2f4e681c");
    }

    /// A synthetic RGBA image: `painter` returns the alpha for each pixel.
    fn synthetic(
        width: u32,
        height: u32,
        painter: impl Fn(u32, u32) -> u8,
    ) -> spore_texture::PngImage {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                pixels.extend_from_slice(&[0x40, 0x80, 0x20, painter(x, y)]);
            }
        }
        spore_texture::PngImage {
            width,
            height,
            pixels,
        }
    }

    #[test]
    fn a_fully_painted_image_is_returned_untouched() {
        // A record with no mask must not be re-cropped. If the threshold logic
        // were wrong in the tight direction this is what would catch it.
        let image = synthetic(64, 32, |_, _| 255);
        let cropped = crop_to_painted(image.clone());
        assert_eq!(
            (cropped.width, cropped.height),
            (64, 32),
            "an unmasked image must survive the crop whole"
        );
        assert_eq!(cropped.pixels, image.pixels);
    }

    #[test]
    fn a_circular_mask_is_cut_off_rather_than_kept() {
        // The real defect: a 200-wide image whose right 60 columns are a hole,
        // with a thin 2-row rim surviving at the very end -- the exact shape the
        // planet-preview strips have.
        let width = 200u32;
        let height = 100u32;
        let mut image = synthetic(width, height, |x, _| if x < 140 { 255 } else { 0 });
        // Paint the rim only on the first and last two rows, so it is a sliver --
        // which is the whole point: an `alpha > 0` test would keep those columns.
        for y in 0..height {
            for x in 140..width {
                let i = ((y * width + x) * 4 + 3) as usize;
                image.pixels[i] = if y < 2 || y >= height - 2 { 255 } else { 0 };
            }
        }
        let cropped = crop_to_painted(image);
        assert_eq!(
            cropped.width, 140,
            "the hole and its rim must both be cut; the painted span is 0..=139"
        );
        assert_eq!(
            cropped.height, height,
            "no horizontal mask, so height stands"
        );
    }

    #[test]
    fn a_degenerate_alpha_column_cannot_shrink_an_image_to_nothing() {
        // Every pixel transparent except one. A `> 0` test would keep a 1x1 crop
        // and throw away the picture; the fraction test has no span at all and
        // returns the original.
        let image = synthetic(32, 32, |_, _| 0);
        let cropped = crop_to_painted(image.clone());
        assert_eq!(
            (cropped.width, cropped.height),
            (32, 32),
            "an image with no painted region is returned, not reduced to a speck"
        );
        assert_eq!(cropped.pixels, image.pixels);
    }

    #[test]
    fn cropping_preserves_the_pixels_it_keeps_in_order() {
        // Row-major order is assumed by the copy; a stride mistake here would
        // shear the picture, which no dimension assertion would catch.
        let image = synthetic(8, 4, |_, _| 255);
        let cropped = crop_to_painted(image.clone());
        assert_eq!(cropped.pixels, image.pixels, "an unmasked copy is verbatim");
    }

    #[test]
    fn a_session_without_packages_says_so_rather_than_failing_to_read() {
        // The `None` branch is the ordinary case for the corpus suite, so it has
        // to be a *reason* and not a panic or a silent default.
        let error = load_backdrop(None).expect_err("no packages is an error");
        assert!(
            error.contains("no packages"),
            "the reason must name the cause, got {error:?}"
        );
    }
}
