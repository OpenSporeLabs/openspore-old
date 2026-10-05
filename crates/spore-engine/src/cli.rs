//! Command line parsing.
//!
//! Hand-written rather than pulled from a crate, for two reasons: the flag set
//! is small and stable, and the engine's dependency graph is meant to stay
//! auditable (see the root `Cargo.toml`). Every parse is a pure function from
//! `&[String]` to [`LaunchRequest`], so the whole CLI is unit-testable without
//! spawning a process.
//!
//! # Many objects, not one preset
//!
//! `--record` and `--preset` are **repeatable**. Each one adds an object to the
//! scene, and `--creature` starts a new object that `--part` can add to. That is
//! the difference between a preset demo and an engine: a scene is a list, and
//! the list is what the command line builds.
//!
//! # Exit behaviour
//!
//! A malformed command line is a *user error*, so it prints usage to stderr and
//! exits non-zero. A well-formed command line that cannot find its asset is a
//! *data* error, so it exits with [`EXIT_ASSET`] and prints a diagnosis that
//! names the packages searched. Keeping the two apart matters: one is a typo,
//! the other is a missing game install or a wrong record identity.

use std::path::PathBuf;

use spore_core::ResourceKey;
use spore_creator_core::document::BlockId;

use crate::domain::creature::Creature;
use crate::domain::model::{ModelInstance, Placement};
use crate::domain::scene::{Scene, SceneObject, DEFAULT_GAP};
use crate::domain::Layout;

/// Exit code for a malformed command line (matches the `argparse` convention
/// the repository's Python tools already use).
pub const EXIT_USAGE: i32 = 2;

/// Exit code for a well-formed request whose asset could not be loaded.
pub const EXIT_ASSET: i32 = 3;

/// What the engine should do on start-up.
#[derive(Debug, Clone, PartialEq)]
pub enum LaunchRequest {
    /// Open a window with no Spore package, showing a placeholder.
    ///
    /// This is the `cargo run` default: the engine must be provably alive
    /// before any game data is involved, so "no data" is a first-class mode
    /// rather than an error.
    Placeholder(WindowOptions),

    /// Load Spore records and show them.
    Scene(SceneRequest),

    /// Load one `<sporemodel>` creator document and stage its projection.
    ///
    /// A **separate** mode rather than a flag on [`SceneRequest`], because the two
    /// stage different things: a scene is a list of *records* placed by hand, and
    /// a creator document is a *composition* of parts whose transforms come from
    /// the document. One of them can fail partially and must say so per block; the
    /// other fails whole. Mixing them would give a caller two completeness
    /// vocabularies and no way to tell which applied.
    Creator(CreatorRequest),

    /// Print the usage text and exit successfully.
    Help,
}

/// A request to stage one creator document.
#[derive(Debug, Clone, PartialEq)]
pub struct CreatorRequest {
    /// Packages in resolution order. Earlier wins.
    ///
    /// A part spans two of them -- the rigblock bounds live in
    /// `Spore_Game.package`, the `prop` pointer and the `0x40606000` pair in
    /// `Spore_Graphics.package` -- so a source holding only one resolves nothing
    /// and looks like a resolver bug. Name both.
    pub packages: Vec<PathBuf>,
    /// The document to stage.
    pub document: CreatorSource,
    /// Where to write the document as `spore-creator-doc/1`.
    ///
    /// Orthogonal to `--info`: a headless run that saves is the whole point of the
    /// flag, so the file is written before the early exit.
    pub save_to: Option<PathBuf>,
    /// The block to mark as selected, or `None`.
    pub select: Option<usize>,
    /// Replace one block's part before anything is staged: `(block, part)`.
    ///
    /// This is the authoring operation that turns the loader into an editor. It is
    /// applied to the document **before** resolution, so the replacement runs the
    /// whole `blockref -> rigblock -> prop -> part` chain exactly as an authored
    /// part would, and an instance that does not resolve shows up as an unresolved
    /// block with its stop stage rather than as a silently missing entity.
    pub replace: Option<(usize, BlockId)>,
    /// A world-space uniform scale applied to the whole creature.
    pub scale: f32,
    /// Window sizing and title.
    pub window: WindowOptions,
    /// Print what was loaded and exit without opening a window.
    pub info_only: bool,
    /// Open the **editor** rather than the viewer: the Creature Creator screen,
    /// with the catalog panel, the viewport, the tab strip, the bottom bar, and
    /// click-to-select against the real document.
    ///
    /// A flag on the same request rather than a separate subcommand because every
    /// other creator flag -- `--creator-doc`, `--creator-replace`, `--select-block`,
    /// `--creator-save` -- means the same thing in both modes, and a second
    /// subcommand would mean parsing and validating all of them twice.
    pub editor: bool,
}

/// Which creator document to stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatorSource {
    /// A `<sporemodel>` record in one of `--package`.
    ///
    /// The authoring family is derived from the record's own type word, not from a
    /// flag: a document's grammar is family-specific, so a wrong guess parses into
    /// the wrong shape rather than failing.
    Record(ResourceKey),
    /// A `spore-creator-doc/1` file, as written by `--creator-save`.
    SavedFile(PathBuf),
}

impl CreatorSource {
    /// A one-word label for a report line.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Record(_) => "record",
            Self::SavedFile(_) => "saved-file",
        }
    }

    /// The engine-level source this request means.
    ///
    /// `DocumentSource::for_record` refuses a type id that is not one of the six
    /// authoring families, so an unknown `--creator` value fails as a *data* error
    /// with the type word in the message rather than as a usage error.
    pub fn to_document_source(&self) -> Result<crate::creator::DocumentSource, String> {
        match self {
            Self::Record(key) => {
                crate::creator::DocumentSource::for_record(*key).map_err(|error| error.to_string())
            }
            Self::SavedFile(path) => Ok(crate::creator::DocumentSource::SavedFile(path.clone())),
        }
    }
}

/// Window sizing and title.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowOptions {
    /// Physical width in pixels.
    ///
    /// Physical, not logical: Bevy scales a logical resolution by the display's
    /// scale factor, and this engine has no UI density to design for yet.
    pub width: u32,
    /// Physical height in pixels.
    pub height: u32,
    /// Window title.
    pub title: String,
}

impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            width: 1280,
            height: 720,
            title: "OpenSpore".to_owned(),
        }
    }
}

/// One thing the command line asked to be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObjectRequest {
    /// A single model record.
    Model {
        /// The identity.
        key: ResourceKey,
        /// How the identity was chosen, for the on-screen banner and the log.
        origin: RecordOrigin,
    },
    /// A creature: one or more model records presented together.
    Creature {
        /// The identity of the first part.
        key: ResourceKey,
        /// Extra part identities, in the order given.
        parts: Vec<ResourceKey>,
        /// How the first identity was chosen.
        origin: RecordOrigin,
    },
}

impl ObjectRequest {
    /// The record identities this object needs decoded.
    pub fn keys(&self) -> Vec<ResourceKey> {
        match self {
            Self::Model { key, .. } => vec![*key],
            Self::Creature { key, parts, .. } => {
                let mut keys = vec![*key];
                keys.extend(parts.iter().copied());
                keys
            }
        }
    }

    /// How the identity was chosen.
    pub fn origin(&self) -> RecordOrigin {
        match self {
            Self::Model { origin, .. } | Self::Creature { origin, .. } => *origin,
        }
    }

    /// `"model"` or `"creature"`, for a log.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Model { .. } => "model",
            Self::Creature { .. } => "creature",
        }
    }
}

/// A request to show real Spore content.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneRequest {
    /// Packages in resolution order. Earlier wins.
    pub packages: Vec<PathBuf>,
    /// What to show. **Repeatable**: one entry per object on stage.
    pub objects: Vec<ObjectRequest>,
    /// World-space uniform scale applied to every loaded mesh.
    pub scale: f32,
    /// Window sizing and title.
    pub window: WindowOptions,
    /// Print what was loaded and exit without opening a window.
    pub info_only: bool,
    /// Resolve the model's texture references and bind each one by its content
    /// role.
    pub textured: bool,
    /// The record type assumed for a texture reference.
    ///
    /// A gmdl texture-set entry stores `{instance, group}` with **no type
    /// word**, so the type has to come from somewhere. It is a visible,
    /// overridable parameter rather than a constant buried in the resolver,
    /// because a wrong assumption produces a confidently wrong texture instead
    /// of an error.
    pub assumed_texture_type: u32,
    /// How objects are spread.
    pub layout: Layout,
    /// Space between objects, in world units.
    pub spacing: f32,
    /// Whether to swap the normal map's green channel.
    ///
    /// **UNKNOWN** which convention Spore authored in; the default is the only
    /// one the corpus supports evidence for.
    pub flip_normal_y: bool,
    /// The `sporemodel` record to read colour from, instead of resolving one per
    /// asset family.
    ///
    /// A colour override rather than a link: the record's numbers are read
    /// verbatim, and the *association* to the model stays
    /// [`spore_core::EvidenceLevel::Inferred`] either way, because no byte in
    /// the corpus names a creature-part model from a `sporemodel` record. See
    /// `crate::domain::paint`.
    pub sporemodel: Option<ResourceKey>,
    /// Extra packages searched only for `sporemodel` records.
    ///
    /// The colour records live in `Spore_Game.package` and
    /// `PatchData.package`; `Spore_Content.package` holds none. These are
    /// appended after `--package`, at the **lowest** resolution priority, so
    /// naming them can never shadow a record the caller asked for explicitly.
    pub sporemodel_packages: Vec<PathBuf>,
}

impl SceneRequest {
    /// Every package to open, in resolution order: `--package` first, then the
    /// `--sporemodel-package` companions.
    pub fn all_packages(&self) -> Vec<PathBuf> {
        let mut all = self.packages.clone();
        all.extend(self.sporemodel_packages.iter().cloned());
        all
    }

    /// Every record the scene needs, in declaration order.
    pub fn records(&self) -> Vec<ResourceKey> {
        self.objects
            .iter()
            .flat_map(|object| object.keys())
            .collect()
    }

    /// The domain scene this request describes, with no extents applied.
    pub fn to_scene(&self) -> Scene {
        let mut scene = Scene::new();
        for object in &self.objects {
            match object {
                ObjectRequest::Model { key, .. } => {
                    scene.push(SceneObject::Model(ModelInstance::new(
                        *key,
                        Placement {
                            scale: self.scale,
                            ..Placement::default()
                        },
                    )));
                }
                ObjectRequest::Creature { key, parts, .. } => {
                    let mut creature = Creature::single(ModelInstance::new(
                        *key,
                        Placement {
                            scale: self.scale,
                            ..Placement::default()
                        },
                    ));
                    for part in parts {
                        creature = creature.with(ModelInstance::new(
                            *part,
                            Placement {
                                scale: self.scale,
                                ..Placement::default()
                            },
                        ));
                    }
                    scene.push(SceneObject::Creature(creature));
                }
            }
        }
        scene
    }
}

/// How a record identity was chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordOrigin {
    /// Given explicitly on the command line.
    Explicit,
    /// Chosen by a named, documented preset.
    Preset(&'static str),
}

/// The identity of the smallest geometry-bearing `gmdl` in the base content
/// package, and the asset the C++ vertical slice used.
///
/// 1156 bytes decompressed, 32 vertices, 20 triangles. Chosen because it is
/// the smallest record that still yields geometry; see `docs/ASSET-PATH.md`.
///
/// Measured 2026-10-04: one mesh, three texture references resolving to
/// `(layered-alpha-mask, layered-alpha-mask, tangent-space-normal)`, all
/// ordinals agreeing, so this is the asset the normal-map path is checked on.
pub const PRESET_DOCUMENTED_ASSET: (u32, u32, u32) = (0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0);

/// The largest creature model in the base content package: 2 meshes, 47 016
/// triangles, 1 351 408 bytes decompressed.
///
/// Group `0x40627100` is `CreatureModelsHQ` in
/// `tools/spore/types/groupnames.json`. Note that `CreatureModels`
/// (`0x40626200`) holds **no** gmdl records at all -- it carries traits,
/// summaries, `prop` and `png` previews -- so a creature model has to be
/// looked for in the HQ group. Measured, not assumed.
///
/// This record's material-info tail names shader-data id `0x218`, whose size is
/// unknown, so it decodes COMPLETELY through the mesh table and stops there.
/// Its geometry is fully validated; the report line says `decode=material-info`.
/// It is the milestone asset: the first real creature on screen.
///
/// Measured 2026-10-04: it carries **two** texture references, which the even
/// per-mesh cut splits one per mesh. `0x40622900` is a 512² `DXT5` mask (two
/// alpha planes) and `0x40622901` is a 256² `tangent-space-normal`.
pub const PRESET_CREATURE: (u32, u32, u32) = (0x00E6_BCE5, 0x4062_7100, 0x067C_79D2);

/// The named presets this build knows.
pub const PRESETS: &[(&str, (u32, u32, u32))] = &[
    ("documented-asset", PRESET_DOCUMENTED_ASSET),
    ("creature", PRESET_CREATURE),
];

/// Why a command line was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// A flag that needs a value was given none.
    MissingValue {
        /// The flag as the user typed it.
        flag: String,
    },
    /// A value could not be parsed.
    BadValue {
        /// The flag as the user typed it.
        flag: String,
        /// What the user typed.
        value: String,
        /// What it should have been.
        expected: &'static str,
    },
    /// An unknown flag.
    UnknownFlag(String),
    /// A preset name that is not in [`PRESETS`].
    UnknownPreset(String),
    /// `--preset` or `--record` was required but absent.
    NoRecordRequested,
    /// `--part` was given with no `--creature` to attach it to.
    OrphanPart,
    /// `--sporemodel` was given more than once.
    RepeatedSporemodel,
    /// `--creator-replace` given twice.
    ///
    /// A separate variant rather than a generic duplicate because the two
    /// occurrences name DIFFERENT blocks, so silently keeping the last one would
    /// edit a block the caller did not ask to edit. The error carries the block the
    /// first flag named, which is the one that would have been dropped.
    RepeatedCreatorReplace {
        /// The block index the first `--creator-replace` named.
        first: usize,
    },
    /// A creator flag was used with a mode that cannot honour it.
    CreatorModeConflict {
        /// The creator flag that was given.
        flag: &'static str,
        /// The flag or mode it conflicts with, named so the fix is in the message.
        other: &'static str,
    },
    /// A creator-only flag was given without `--creator` or `--creator-doc`.
    OrphanCreatorFlag {
        /// The flag as the user typed it.
        flag: String,
        /// What it needs.
        needs: &'static str,
    },
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingValue { flag } => write!(f, "`{flag}` needs a value"),
            Self::BadValue { flag, value, expected } => {
                write!(f, "`{flag}` got `{value}`, expected {expected}")
            }
            Self::UnknownFlag(flag) => write!(f, "unknown flag `{flag}`"),
            Self::UnknownPreset(name) => write!(f, "unknown preset `{name}`"),
            Self::NoRecordRequested => write!(
                f,
                "nothing to load: pass `--record T:G:I`, `--preset {}`, or `--placeholder` for the no-data mode",
                PRESETS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join("|")
            ),
            Self::OrphanPart => write!(
                f,
                "`--part` needs a `--creature` before it: a part is attached to the creature it follows"
            ),
            Self::RepeatedCreatorReplace { first } => write!(
                f,
                "`--creator-replace` may be given once: it edits one block, and a second \
                 value would silently win by flag order. The first named block {first}, \
                 which would have been left unedited"
            ),
            Self::RepeatedSporemodel => write!(
                f,
                "`--sporemodel` may be given once: it names the colour source for the whole scene, \
                 and a second value would silently win by flag order"
            ),
            Self::CreatorModeConflict { flag, other } => write!(
                f,
                "`{flag}` cannot be combined with `{other}`: a creator document is staged on \
                 its own, with its own completeness report, and merging the two would leave a \
                 caller unable to tell which one it got"
            ),
            Self::OrphanCreatorFlag { flag, needs } => {
                write!(f, "`{flag}` needs {needs}")
            }
        }
    }
}

impl std::error::Error for CliError {}

/// The usage text, printed on error and by `--help`.
pub fn usage() -> String {
    let presets = PRESETS
        .iter()
        .map(|(name, _)| *name)
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "openspore - a clean-room Rust + Bevy reimplementation of Spore\n\
         \n\
         USAGE:\n\
         \x20   openspore [OPTIONS]\n\
         \n\
         With --placeholder, opens a window showing a built-in mesh, so the\n\
         engine can be proven alive before any game data is involved.\n\
         \n\
         --record and --preset are REPEATABLE: each one adds an object to the\n\
         scene, and objects are laid out side by side from their own bounds.\n\
         \n\
         OPTIONS:\n\
         \x20   --package <PATH>     A Spore .package file. Repeatable. Earlier\n\
         \x20                       packages win when an identity appears in more\n\
         \x20                       than one, so list base content before patches.\n\
         \x20   --record <T:G:I>     A record to show, e.g.\n\
         \x20                       0x00e6bce5:0x40637e03:0x067a07f0. Repeatable.\n\
         \x20   --preset <NAME>      A named record. Known: {presets}. Repeatable.\n\
         \x20   --creature <T:G:I>   Start a creature object at this record.\n\
         \x20   --part <T:G:I>       Add a model to the creature that follows it.\n\
         \x20   --layout <row|grid>  How to spread objects (default row)\n\
         \x20   --spacing <F>        Space between objects (default {gap})\n\
         \x20   --scale <F>          Uniform scale applied to the mesh (default 1.0)\n\
         \x20   --width <PX>         Window width (default 1280)\n\
         \x20   --height <PX>        Window height (default 720)\n\
         \x20   --title <TEXT>       Window title\n\
         \x20   --info               Load, report, and exit without a window\n\
         \x20   --texture            Resolve the model's texture references and bind\n\
         \x20                       each one to the material slot its CONTENT role\n\
         \x20                       implies. No raster record in this corpus decodes\n\
         \x20                       as base colour, so none is bound as diffuse.\n\
         \x20   --assumed-texture-type <ID>\n\
         \x20                       The record type assumed for a texture reference,\n\
         \x20                       which carries no type word (default 0x2f4e681c)\n\
         \x20   --flip-normal-y      Swap the normal map's green channel. UNKNOWN\n\
         \x20                       which convention Spore authored in, so off by\n\
         \x20                       default: as-stored is the only reading the corpus\n\
         \x20                       supports evidence for.\n\
         \x20   --sporemodel <T:G:I>  Read this model's COLOUR from the named\n\
         \x20                       `sporemodel` record, instead of resolving one\n\
         \x20                       per asset family. The record's numbers are read\n\
         \x20                       verbatim; the link to the model is still\n\
         \x20                       INFERRED, because no record in the corpus names\n\
         \x20                       a model from a `sporemodel`.\n\
         \x20   --sporemodel-package <PATH>\n\
         \x20                       An extra package searched for `sporemodel`\n\
         \x20                       records, e.g. SPORE/Data/Spore_Game.package.\n\
         \x20                       Spore_Content holds none, so a creature renders\n\
         \x20                       its neutral tint without this. Repeatable, and\n\
         \x20                       appended after --package at the lowest priority.\n\
         \x20   --placeholder        Show the built-in mesh with no game data at all\n\
         \x20   -h, --help           Print this text\n\
         \n\
         CREATOR MODE -- one <sporemodel> document, staged as a composition:\n\
         \x20   --creator <T:G:I>    The creator document record to stage, e.g.\n\
         \x20                       0x2b978c46:0x40626200:0x05e3df13. The authoring family\n\
         \x20                       (crt/cll/vcl/ufo/bld/flr) is read from the record's\n\
         \x20                       own type id; a type id this build does not know is\n\
         \x20                       refused rather than guessed. Cannot be combined\n\
         \x20                       with --record/--preset/--creature/--placeholder,\n\
         \x20                       which stage records instead.\n\
         \x20   --creator-doc <PATH> Stage a previously saved creator document\n\
         \x20                       (spore-creator-doc/1) instead of a record. The\n\
         \x20                       format carries no type word, so the family is\n\
         \x20                       reported as `unknown` rather than inferred.\n\
         \x20   --creator-save <PATH>\n\
         \x20                       Write the document to PATH as\n\
         \x20                       spore-creator-doc/1. Written even with --info.\n\
         \x20   --creator-replace <BLOCK>:<PART>\n\
         \x20                       Swap block BLOCK's part for PART before anything is\n\
         \x20                       resolved, e.g. 3:0x40626000,0x00013530. The block\n\
         \x20                       keeps its index, parent, transform and children;\n\
         \x20                       only the part changes. The new part walks the same\n\
         \x20                       chain an authored one would, so a part that does not\n\
         \x20                       resolve is reported as an unresolved block rather than\n\
         \x20                       silently missing. May be given once.\n\
         \x20   --select-block <N>   Mark block N as selected; it is tinted, and the\n\
         \x20                       selection is readable in the runtime state.\n\
         \x20   --editor              Open the Creature Creator screen: catalog panel,\n\
         \x20                       viewport, tab strip, bottom bar, and click to select\n\
         \x20                       a real body part. Every other creator flag still\n\
         \x20                       applies.\n\
         \n\
         A creator part spans TWO packages: the rigblock bounds live in\n\
         Spore_Game.package and the prop pointer plus the 0x40606000 pair in\n\
         Spore_Graphics.package, so name both or nothing resolves.\n\
         \n\
         Every block is reported. One that did not resolve spawns nothing and\n\
         is named on its own OPENSPORE-CREATOR-BLOCK line with the stage it\n\
         stopped at and why, so a partial creature is never a whole one.\n\
         \n\
         EXIT CODES:\n\
         \x20   0  ok            2  bad command line ({EXIT_USAGE})   3  asset unavailable ({EXIT_ASSET})\n",
        gap = DEFAULT_GAP,
    )
}

/// Parses arguments, **excluding** the program name.
pub fn parse<I, S>(args: I) -> Result<LaunchRequest, CliError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let args: Vec<String> = args.into_iter().map(|a| a.as_ref().to_owned()).collect();
    parse_vec(&args)
}

fn parse_vec(args: &[String]) -> Result<LaunchRequest, CliError> {
    let mut packages: Vec<PathBuf> = Vec::new();
    let mut sporemodel_packages: Vec<PathBuf> = Vec::new();
    let mut objects: Vec<ObjectRequest> = Vec::new();
    let mut scale = 1.0f32;
    let mut window = WindowOptions::default();
    let mut info_only = false;
    let mut editor = false;
    let mut placeholder = false;
    let mut textured = false;
    let mut sporemodel: Option<ResourceKey> = None;
    let mut assumed_texture_type = spore_core::record::type_id::RASTER;
    let mut layout = Layout::Row;
    let mut spacing = DEFAULT_GAP;
    let mut flip_normal_y = false;
    let mut creator: Option<CreatorSource> = None;
    let mut creator_save: Option<PathBuf> = None;
    let mut select_block: Option<usize> = None;
    let mut replace: Option<(usize, BlockId)> = None;

    let mut index = 0usize;
    while index < args.len() {
        let arg = args[index].as_str();
        let mut value_for = |name: &str| -> Result<String, CliError> {
            index += 1;
            args.get(index)
                .cloned()
                .ok_or_else(|| CliError::MissingValue {
                    flag: name.to_owned(),
                })
        };
        match arg {
            "-h" | "--help" => return Ok(LaunchRequest::Help),
            "--placeholder" => {
                // Explicitly asks for the no-data mode. Reached below via the
                // `objects.is_empty()` arm, so it must not fall through.
                placeholder = true;
            }
            "--package" => packages.push(PathBuf::from(value_for("--package")?)),
            "--sporemodel-package" => {
                sporemodel_packages.push(PathBuf::from(value_for("--sporemodel-package")?));
            }
            "--sporemodel" => {
                // One value for the whole scene. Repeating it would make the
                // winner depend on flag order, which is a silent policy nobody
                // would expect from a flag that looks per-object.
                let raw = value_for("--sporemodel")?;
                if sporemodel.is_some() {
                    return Err(CliError::RepeatedSporemodel);
                }
                sporemodel = Some(parse_key("--sporemodel", &raw)?);
            }
            "--record" => {
                let raw = value_for("--record")?;
                objects.push(ObjectRequest::Model {
                    key: parse_key("--record", &raw)?,
                    origin: RecordOrigin::Explicit,
                });
            }
            "--preset" => {
                let raw = value_for("--preset")?;
                let (name, triple) = PRESETS
                    .iter()
                    .find(|(name, _)| *name == raw)
                    .ok_or_else(|| CliError::UnknownPreset(raw.clone()))?;
                objects.push(ObjectRequest::Model {
                    key: ResourceKey::new(triple.0, triple.1, triple.2),
                    origin: RecordOrigin::Preset(name),
                });
            }
            "--creature" => {
                let raw = value_for("--creature")?;
                objects.push(ObjectRequest::Creature {
                    key: parse_key("--creature", &raw)?,
                    parts: Vec::new(),
                    origin: RecordOrigin::Explicit,
                });
            }
            "--part" => {
                let raw = value_for("--part")?;
                let key = parse_key("--part", &raw)?;
                match objects.last_mut() {
                    Some(ObjectRequest::Creature { parts, .. }) => parts.push(key),
                    _ => return Err(CliError::OrphanPart),
                }
            }
            "--scale" => {
                let raw = value_for("--scale")?;
                let parsed: f32 = raw.parse().map_err(|_| CliError::BadValue {
                    flag: "--scale".into(),
                    value: raw.clone(),
                    expected: "a number",
                })?;
                if !(parsed.is_finite() && parsed > 0.0) {
                    return Err(CliError::BadValue {
                        flag: "--scale".into(),
                        value: raw,
                        expected: "a finite number greater than zero",
                    });
                }
                scale = parsed;
            }
            "--layout" => {
                let raw = value_for("--layout")?;
                layout = Layout::parse(&raw).ok_or_else(|| CliError::BadValue {
                    flag: "--layout".into(),
                    value: raw.clone(),
                    expected: "`row` or `grid`",
                })?;
            }
            "--spacing" => {
                let raw = value_for("--spacing")?;
                let parsed: f32 = raw.parse().map_err(|_| CliError::BadValue {
                    flag: "--spacing".into(),
                    value: raw.clone(),
                    expected: "a number",
                })?;
                if !(parsed.is_finite() && parsed >= 0.0) {
                    return Err(CliError::BadValue {
                        flag: "--spacing".into(),
                        value: raw,
                        expected: "a finite number not below zero",
                    });
                }
                spacing = parsed;
            }
            "--width" => {
                let raw = value_for("--width")?;
                window.width = raw.parse().map_err(|_| CliError::BadValue {
                    flag: "--width".into(),
                    value: raw,
                    expected: "a pixel count",
                })?;
            }
            "--height" => {
                let raw = value_for("--height")?;
                window.height = raw.parse().map_err(|_| CliError::BadValue {
                    flag: "--height".into(),
                    value: raw,
                    expected: "a pixel count",
                })?;
            }
            "--title" => {
                window.title = value_for("--title")?;
            }
            "--info" => info_only = true,
            "--editor" => editor = true,
            "--texture" => textured = true,
            "--flip-normal-y" => flip_normal_y = true,
            "--creator" => {
                // One value for the whole run, like `--sporemodel`: two creator
                // documents on one stage would need a completeness report per
                // document, and there is exactly one projection.
                let raw = value_for("--creator")?;
                if creator.is_some() {
                    return Err(CliError::CreatorModeConflict {
                        flag: "--creator",
                        other: "--creator",
                    });
                }
                creator = Some(CreatorSource::Record(parse_key("--creator", &raw)?));
            }
            "--creator-doc" => {
                let raw = value_for("--creator-doc")?;
                if creator.is_some() {
                    return Err(CliError::CreatorModeConflict {
                        flag: "--creator-doc",
                        other: "--creator",
                    });
                }
                creator = Some(CreatorSource::SavedFile(PathBuf::from(raw)));
            }
            "--creator-save" => {
                creator_save = Some(PathBuf::from(value_for("--creator-save")?));
            }
            "--creator-replace" => {
                // `<BLOCK>:<PART>`, e.g. `3:0x0001_3530`. Both halves are parsed by
                // the crate that owns them: `usize` for an index and `BlockId` for a
                // part identity. The alternative -- splitting on ':' here and reading
                // the two words as u32 -- is a second id parser in the CLI, and this
                // repo has already been bitten by exactly that twice.
                let raw = value_for("--creator-replace")?;
                let (block, part) = raw.split_once(':').ok_or_else(|| CliError::BadValue {
                    flag: "--creator-replace".into(),
                    value: raw.clone(),
                    expected: "<BLOCK>:<PART>, e.g. 3:0x00013530",
                })?;
                let index: usize = block.trim().parse().map_err(|_| CliError::BadValue {
                    flag: "--creator-replace".into(),
                    value: block.to_owned(),
                    expected: "a block index",
                })?;
                let identity =
                    BlockId::parse(part.trim(), index).map_err(|_| CliError::BadValue {
                        flag: "--creator-replace".into(),
                        value: part.to_owned(),
                        expected: "a part identity, e.g. 0x40626000,0x00013530",
                    })?;
                if let Some((existing, _)) = replace {
                    return Err(CliError::RepeatedCreatorReplace { first: existing });
                }
                replace = Some((index, identity));
            }
            "--select-block" => {
                let raw = value_for("--select-block")?;
                let parsed: usize = raw.parse().map_err(|_| CliError::BadValue {
                    flag: "--select-block".into(),
                    value: raw.clone(),
                    expected: "a block index",
                })?;
                select_block = Some(parsed);
            }
            "--assumed-texture-type" => {
                let raw = value_for("--assumed-texture-type")?;
                // `spore_core::parse_id`, not `u32::from_str`: the latter rejects
                // a `0x` prefix, and every other tool here accepts one.
                assumed_texture_type =
                    spore_core::parse_id(&raw).map_err(|_| CliError::BadValue {
                        flag: "--assumed-texture-type".into(),
                        value: raw,
                        expected: "a type id, decimal or 0x-prefixed hex",
                    })?;
            }
            other => return Err(CliError::UnknownFlag(other.to_owned())),
        }
        index += 1;
    }

    if !objects.is_empty() {
        if let Some(creator_flag) = creator_flag_of(&creator) {
            return Err(CliError::CreatorModeConflict {
                flag: creator_flag,
                other: "--record",
            });
        }
        if window.title == WindowOptions::default().title {
            window.title = format!("OpenSpore - {} object(s)", objects.len());
        }
        return Ok(LaunchRequest::Scene(SceneRequest {
            packages,
            sporemodel_packages,
            objects,
            scale,
            window,
            info_only,
            textured,
            sporemodel,
            assumed_texture_type,
            layout,
            spacing,
            flip_normal_y,
        }));
    }
    // Packages with no record is the ambiguous case: the caller named game data
    // but not what to show. Guessing a "nice" record here would hide a typo
    // behind a plausible picture, so it is a usage error.
    if placeholder {
        if let Some(creator_flag) = creator_flag_of(&creator) {
            return Err(CliError::CreatorModeConflict {
                flag: creator_flag,
                other: "--placeholder",
            });
        }
        return Ok(LaunchRequest::Placeholder(window));
    }
    if let Some(document) = creator {
        if window.title == WindowOptions::default().title {
            window.title = "OpenSpore - creator".to_owned();
        }
        return Ok(LaunchRequest::Creator(CreatorRequest {
            packages,
            document,
            save_to: creator_save,
            select: select_block,
            replace,
            scale,
            window,
            info_only,
            editor,
        }));
    }
    // A creator-only flag with no creator document is a typo, not a default: the
    // block it names has no meaning without the document it belongs to.
    // `--creator-replace` belongs here too, and was missing: without this arm it was
    // parsed, carried in the request, and then never read, because no creator
    // document existed to apply it to. A flag that names a block in a document that
    // was never loaded is a typo, and silently doing nothing is the worst answer.
    for (given, flag) in [
        (creator_save.is_some(), "--creator-save"),
        (replace.is_some(), "--creator-replace"),
    ] {
        if given {
            return Err(CliError::OrphanCreatorFlag {
                flag: flag.into(),
                needs: "--creator <T:G:I> or --creator-doc <PATH>",
            });
        }
    }
    if select_block.is_some() {
        return Err(CliError::OrphanCreatorFlag {
            flag: "--select-block".into(),
            needs: "--creator <T:G:I> or --creator-doc <PATH>",
        });
    }
    Err(CliError::NoRecordRequested)
}

/// The flag a creator source came from, for a conflict message that names it.
fn creator_flag_of(creator: &Option<CreatorSource>) -> Option<&'static str> {
    match creator {
        Some(CreatorSource::Record(_)) => Some("--creator"),
        Some(CreatorSource::SavedFile(_)) => Some("--creator-doc"),
        None => None,
    }
}

fn parse_key(flag: &str, raw: &str) -> Result<ResourceKey, CliError> {
    raw.parse::<ResourceKey>().map_err(|_| CliError::BadValue {
        flag: flag.into(),
        value: raw.to_owned(),
        expected: "a T:G:I triple",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(args: &[&str]) -> SceneRequest {
        let LaunchRequest::Scene(request) = parse(args).unwrap() else {
            panic!("expected a scene request from {args:?}")
        };
        request
    }

    #[test]
    fn no_arguments_is_a_usage_error_not_a_silent_default() {
        // Bare `openspore` must NOT silently invent a record. The placeholder
        // mode is reachable only by asking for it with `--placeholder`, so a
        // bare invocation and a typo are distinguishable.
        assert_eq!(parse(["--help"]).unwrap(), LaunchRequest::Help);
        assert_eq!(
            parse(["--placeholder"]).unwrap(),
            LaunchRequest::Placeholder(WindowOptions::default())
        );
        assert_eq!(
            parse(Vec::<String>::new()).unwrap_err(),
            CliError::NoRecordRequested
        );
    }

    #[test]
    fn packages_without_a_record_are_a_usage_error_not_a_guess() {
        assert_eq!(
            parse(["--package", "/base.package"]).unwrap_err(),
            CliError::NoRecordRequested
        );
    }

    #[test]
    fn a_record_alone_produces_a_scene_request_with_one_object() {
        let request = scene(&["--record", "0x00e6bce5:0x40637e03:0x067a07f0"]);
        assert_eq!(
            request.records(),
            vec![ResourceKey::new(0x00E6_BCE5, 0x4063_7E03, 0x067A_07F0)]
        );
        assert!(request.packages.is_empty());
        assert_eq!(request.scale, 1.0);
        assert_eq!(request.layout, Layout::Row);
        assert_eq!(request.spacing, DEFAULT_GAP);
        assert!(!request.flip_normal_y);
    }

    #[test]
    fn records_and_presets_accumulate_into_several_objects() {
        // The difference between a preset demo and an engine.
        let request = scene(&[
            "--preset",
            "creature",
            "--preset",
            "documented-asset",
            "--record",
            "0x01:0x02:0x03",
        ]);
        assert_eq!(request.objects.len(), 3);
        assert_eq!(request.records().len(), 3);
        assert!(request.window.title.contains("3 object"));
        let domain = request.to_scene();
        assert_eq!(domain.len(), 3);
        assert_eq!(domain.records().len(), 3);
    }

    #[test]
    fn the_same_record_twice_is_two_objects_and_one_decode() {
        let request = scene(&["--record", "1:2:3", "--record", "1:2:3"]);
        let domain = request.to_scene();
        assert_eq!(domain.len(), 2, "two objects on stage");
        assert_eq!(domain.records().len(), 1, "but one record to decode");
        assert_eq!(request.records().len(), 2);
    }

    #[test]
    fn a_creature_collects_the_parts_that_follow_it() {
        let request = scene(&[
            "--creature",
            "1:2:3",
            "--part",
            "4:5:6",
            "--part",
            "7:8:9",
            "--record",
            "10:11:12",
        ]);
        assert_eq!(request.objects.len(), 2, "the creature and the lone model");
        assert_eq!(request.objects[0].keys().len(), 3);
        assert_eq!(request.objects[0].kind(), "creature");
        assert_eq!(request.objects[1].kind(), "model");
        let domain = request.to_scene();
        assert_eq!(
            domain.records().len(),
            4,
            "three creature parts plus the lone model"
        );
        let creature = match &domain.objects[0] {
            SceneObject::Creature(creature) => creature,
            other => panic!("expected a creature, got {other:?}"),
        };
        assert_eq!(creature.len(), 3);
        assert_eq!(creature.distinct_records(), 3);
    }

    #[test]
    fn a_part_before_any_creature_is_refused_rather_than_attached_to_nothing() {
        assert_eq!(
            parse(["--part", "1:2:3"]).unwrap_err(),
            CliError::OrphanPart
        );
        assert_eq!(
            parse(["--record", "1:2:3", "--part", "4:5:6"]).unwrap_err(),
            CliError::OrphanPart
        );
    }

    #[test]
    fn the_creature_preset_is_distinct_from_the_documented_asset() {
        let creature = scene(&["--preset", "creature"]);
        assert_eq!(
            creature.records(),
            vec![ResourceKey::new(0x00E6_BCE5, 0x4062_7100, 0x067C_79D2)]
        );
        assert_ne!(
            creature.records()[0],
            ResourceKey::new(
                PRESET_DOCUMENTED_ASSET.0,
                PRESET_DOCUMENTED_ASSET.1,
                PRESET_DOCUMENTED_ASSET.2
            )
        );
        assert_eq!(
            creature.objects[0].origin(),
            RecordOrigin::Preset("creature")
        );
    }

    #[test]
    fn an_unknown_preset_is_named_in_the_error() {
        assert_eq!(
            parse(["--preset", "nope"]).unwrap_err(),
            CliError::UnknownPreset("nope".into())
        );
    }

    #[test]
    fn packages_accumulate_in_command_line_order() {
        let request = scene(&[
            "--package",
            "/base.package",
            "--package",
            "/patch.package",
            "--record",
            "1:2:3",
        ]);
        assert_eq!(
            request.packages,
            vec![
                PathBuf::from("/base.package"),
                PathBuf::from("/patch.package")
            ]
        );
    }

    #[test]
    fn a_flag_without_its_value_is_reported_by_name() {
        for flag in ["--record", "--package", "--creature", "--part", "--spacing"] {
            assert_eq!(
                parse([flag]).unwrap_err(),
                CliError::MissingValue {
                    flag: flag.to_owned()
                }
            );
        }
    }

    #[test]
    fn an_unknown_flag_is_refused_rather_than_ignored() {
        assert_eq!(
            parse(["--turbo"]).unwrap_err(),
            CliError::UnknownFlag("--turbo".into())
        );
    }

    #[test]
    fn malformed_values_are_refused_with_the_expectation_stated() {
        assert!(matches!(
            parse(["--record", "not-a-key"]).unwrap_err(),
            CliError::BadValue { flag, expected: "a T:G:I triple", .. } if flag == "--record"
        ));
        for bad in ["0", "-1", "inf"] {
            assert!(matches!(
                parse(["--scale", bad]).unwrap_err(),
                CliError::BadValue { flag, .. } if flag == "--scale"
            ));
        }
        assert!(matches!(
            parse(["--spacing", "-0.5"]).unwrap_err(),
            CliError::BadValue { flag, .. } if flag == "--spacing"
        ));
        assert!(matches!(
            parse(["--layout", "spiral"]).unwrap_err(),
            CliError::BadValue { flag, .. } if flag == "--layout"
        ));
    }

    #[test]
    fn info_only_is_orthogonal_to_everything_else() {
        assert!(scene(&["--record", "1:2:3", "--info"]).info_only);
    }

    #[test]
    fn the_window_title_names_the_scene_unless_it_was_set() {
        assert!(scene(&["--record", "1:2:3"])
            .window
            .title
            .contains("1 object"));
        assert_eq!(
            scene(&["--record", "1:2:3", "--title", "mine"])
                .window
                .title,
            "mine"
        );
    }

    #[test]
    fn the_assumed_texture_type_defaults_to_raster_and_is_visible() {
        let request = scene(&["--record", "1:2:3", "--texture"]);
        assert_eq!(
            request.assumed_texture_type,
            spore_core::record::type_id::RASTER
        );
        let overridden = scene(&[
            "--record",
            "1:2:3",
            "--texture",
            "--assumed-texture-type",
            "0x2F4E681B",
        ]);
        assert_eq!(overridden.assumed_texture_type, 0x2F4E_681B);
        assert!(overridden.textured);
        assert!(matches!(
            parse(["--record", "1:2:3", "--assumed-texture-type", "nope"]).unwrap_err(),
            CliError::BadValue { flag, .. } if flag == "--assumed-texture-type"
        ));
    }

    #[test]
    fn textures_are_off_unless_requested() {
        assert!(!scene(&["--record", "1:2:3"]).textured);
        assert!(scene(&["--record", "1:2:3", "--texture"]).textured);
    }

    #[test]
    fn the_normal_y_flip_is_opt_in_because_the_convention_is_unknown() {
        assert!(!scene(&["--record", "1:2:3"]).flip_normal_y);
        assert!(scene(&["--record", "1:2:3", "--flip-normal-y"]).flip_normal_y);
    }

    #[test]
    fn the_sporemodel_override_is_one_value_for_the_whole_scene() {
        let request = scene(&[
            "--record",
            "1:2:3",
            "--sporemodel",
            "0x2b978c46:0x40626200:0x05e3df13",
        ]);
        assert_eq!(
            request.sporemodel,
            Some(ResourceKey::new(0x2B97_8C46, 0x4062_6200, 0x05E3_DF13))
        );
        assert_eq!(scene(&["--record", "1:2:3"]).sporemodel, None);
        // Repeating it would make the winner depend on flag order, which is a
        // silent policy nobody expects from a flag that looks per-object.
        assert_eq!(
            parse([
                "--record",
                "1:2:3",
                "--sporemodel",
                "1:2:3",
                "--sporemodel",
                "4:5:6",
            ])
            .unwrap_err(),
            CliError::RepeatedSporemodel
        );
        assert!(matches!(
            parse(["--record", "1:2:3", "--sporemodel", "nope"]).unwrap_err(),
            CliError::BadValue { flag, .. } if flag == "--sporemodel"
        ));
    }

    #[test]
    fn sporemodel_packages_come_after_the_models_own_and_never_shadow_it() {
        let request = scene(&[
            "--package",
            "/content.package",
            "--sporemodel-package",
            "/game.package",
            "--sporemodel-package",
            "/patch.package",
            "--record",
            "1:2:3",
        ]);
        assert_eq!(request.packages, vec![PathBuf::from("/content.package")]);
        assert_eq!(
            request.sporemodel_packages,
            vec![
                PathBuf::from("/game.package"),
                PathBuf::from("/patch.package")
            ]
        );
        // Order matters: `Spore_Content` wins an identity it holds, and the
        // colour-only companion is consulted after it.
        assert_eq!(
            request.all_packages(),
            vec![
                PathBuf::from("/content.package"),
                PathBuf::from("/game.package"),
                PathBuf::from("/patch.package"),
            ]
        );
    }

    #[test]
    fn a_sporemodel_package_alone_is_still_a_usage_error_without_a_record() {
        // Naming game data but not what to show is still a typo, not a guess.
        assert_eq!(
            parse(["--sporemodel-package", "/game.package"]).unwrap_err(),
            CliError::NoRecordRequested
        );
    }

    #[test]
    fn usage_documents_every_flag_and_preset() {
        let text = usage();
        for flag in [
            "--package",
            "--record",
            "--preset",
            "--creature",
            "--part",
            "--layout",
            "--spacing",
            "--scale",
            "--width",
            "--height",
            "--title",
            "--info",
            "--texture",
            "--flip-normal-y",
            "--sporemodel",
            "--sporemodel-package",
            "--placeholder",
        ] {
            assert!(text.contains(flag), "usage must document {flag}: {text}");
        }
        for (name, _) in PRESETS {
            assert!(text.contains(name), "usage must document preset {name}");
        }
        assert!(
            text.contains("no type word"),
            "the assumption must be explained, not just offered: {text}"
        );
        assert!(
            text.contains("UNKNOWN"),
            "the normal-map convention must be stated as unknown: {text}"
        );
        assert!(
            text.contains("REPEATABLE"),
            "multi-object loading must be documented: {text}"
        );
        assert!(
            text.contains("Spore_Content holds none"),
            "why a colour package is needed must be stated, not just offered: {text}"
        );
        assert!(
            text.contains("INFERRED"),
            "the model-to-colour link must be stated as inferred: {text}"
        );
    }
}

#[cfg(test)]
mod replace_flag_tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<LaunchRequest, CliError> {
        super::parse(args.iter().map(|s| (*s).to_owned()))
    }

    fn base() -> Vec<&'static str> {
        vec![
            "--creator",
            "0x2b978c46:0x40626200:0xde727fa9",
            "--package",
            "SPORE/Data/Spore_Game.package",
        ]
    }

    #[test]
    fn a_replace_names_the_block_and_the_part() {
        let mut args = base();
        args.extend(["--creator-replace", "3:0x40626000,0x00013530"]);
        let Ok(LaunchRequest::Creator(request)) = parse(&args) else {
            panic!("expected a creator request, got {:?}", parse(&args));
        };
        let (index, id) = request.replace.expect("the replace was parsed");
        assert_eq!(index, 3);
        assert_eq!(id.group_id, 0x4062_6000);
        assert_eq!(id.instance_id, 0x0001_3530);
    }

    #[test]
    fn no_replace_flag_means_no_replacement() {
        let Ok(LaunchRequest::Creator(request)) = parse(&base()) else {
            panic!("expected a creator request");
        };
        assert_eq!(request.replace, None, "an absent flag is not a zero block");
    }

    #[test]
    fn two_replaces_are_refused_rather_than_one_silently_winning() {
        // Flag order would decide the winner otherwise, and the loser would be a
        // block the caller believed they had edited.
        let mut args = base();
        args.extend(["--creator-replace", "1:0x40626000,0x00013530"]);
        args.extend(["--creator-replace", "2:0x40626000,0x00013531"]);
        assert!(matches!(
            parse(&args),
            Err(CliError::RepeatedCreatorReplace { first: 1 })
        ));
    }

    #[test]
    fn a_malformed_replacement_is_refused_with_both_halves_named() {
        for bad in ["3", "3:", ":0x40626000,0x00013530", "x:0x40626000,0x0001"] {
            let mut args = base();
            args.extend(["--creator-replace", bad]);
            let Err(CliError::BadValue { flag, .. }) = parse(&args) else {
                panic!("{bad:?} should not parse");
            };
            assert_eq!(flag, "--creator-replace");
        }
    }

    #[test]
    fn a_replace_needs_a_creator_document() {
        assert!(matches!(
            parse(&["--creator-replace", "0:0x40626000,0x00013530"]),
            Err(CliError::OrphanCreatorFlag { .. })
        ));
    }
}
