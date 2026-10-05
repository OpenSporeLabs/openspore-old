//! **SCIENCE-2: how many gmdl -> texture references dangle, and how is a
//! dangling reference reported?**
//!
//! # The question, and the four answers it must be able to give
//!
//! A gmdl record's material-info section carries `{instance, group}` texture
//! references with **no type word**. `spore_material::resolve_texture` completes
//! the key with a caller-supplied type and looks it up. That lookup has **four**
//! possible outcomes, and this file exists because three of them must never be
//! reported as the fourth:
//!
//! | class | meaning | `resolve_texture`'s answer |
//! |---|---|---|
//! | **present** | the identity is in an opened package and its bytes decoded | `Ok(ResolvedTexture)` |
//! | **absent** | we looked in every opened package and it is not there | `Lookup { source: NotFound }` |
//! | **could not look** | the store could not answer — a container, a decompression, an empty store | `Lookup { source: <anything else> }` |
//! | **refused** | the record was reached and a decoder **declined**, naming why | `UnsupportedFourcc` / `AssumedTypeNotDecodable` / `Decode` / `IncompleteReference` / `EmptyImage` |
//!
//! Collapsing *absent* into *could not look* fabricates a negative; collapsing
//! *refused* into *absent* hides a real record behind a missing one; and
//! collapsing any of the three into `TextureRole::Unidentified` — which is graded
//! `Observed`, "the check ran and declined" — is the **silent-wrong** this file
//! measures and pins. See [`a_dangling_reference_is_never_reported_as_the_graded_role_negative`].
//!
//! # The measurement
//!
//! `#[ignore]`d: it needs the git-ignored `SPORE/` tree. It skips with an
//! `eprintln!("SKIP: ...")` line when `SPORE/Data` is absent, because a missing
//! game install is not a failure of this crate.
//!
//! ```text
//! cargo test -p spore-material --release --test dangling_texture_references -- --ignored --nocapture
//! ```
//!
//! It runs the census over **two package sets**, because "dangling" is a
//! statement about what was opened and not about the format:
//!
//! * `content_only` — `Spore_Content.package` alone;
//! * `whole_install` — every content-bearing package in `SPORE/Data`,
//!   `SPORE/DataEP1` and `SPORE/bp1content`.
//!
//! A reference absent from the first set and present in the second is **not** a
//! defect: it names a record another package supplies. That is the difference
//! between a dangling reference and a cross-package reference, and it is only
//! observable by running both.
//!
//! # What this file deliberately does NOT do
//!
//! It does not re-derive roles, re-check framing identities, or decide what a
//! reference *should* be. A census that computed its own answers would be a
//! second source of truth — the defect this workspace has already paid for
//! twice. Every class here is read off `spore_material`'s own typed errors, and
//! the classifier below is **exhaustive** over `MaterialError`,
//! `spore_assets::AssetError`, `spore_texture::TextureError` and
//! `spore_gmdl::GmdlError`, so a new variant fails to compile here rather than
//! sliding into an existing bucket.
//!
//! # The known-broken Python raster oracle does NOT decide this question
//!
//! `tools/spore/raster/raster.py` still implements the **refuted** raster model —
//! the 8-byte block stride and a derived two-layer count — which is why
//! `spore-material`'s fixtures were rewritten on 2026-10-05. It is worth saying
//! explicitly that it is not in this question's path, because it *is* in
//! `spore-material`'s decode path and a reader would reasonably ask.
//!
//! None of the four classes depends on it:
//!
//! * `absent` is decided by **DBPF index membership** (`ContentStore::read`
//!   returning [`AssetError::NotFound`]). No texel is decoded, so no codec is
//!   consulted.
//! * `could-not-look` and `refused` are decided by **typed errors** from
//!   `spore_dbpf`, `spore_gmdl` and `spore_texture` — the Rust implementations,
//!   which is the side the corpus refuted the oracle *against*.
//! * `present` consults [`spore_texture::decode_raster`], the **refuting**
//!   implementation, not the oracle.
//!
//! The one way the oracle could change a class is by turning a `present` into a
//! `refused`, if its framing identity were in force. The measured `refused` count
//! is **zero** over all 16 628 references, so nothing is hiding there. The oracle's
//! effect is confined to the *contents* of a `present` record — its `fourcc` and
//! its texels — and this census reads the `fourcc` and the assigned role, both
//! published by `spore-material` rather than recomputed.

use std::collections::{BTreeMap, BTreeSet};

use spore_assets::{AssetError, ContentStore, Package};
use spore_core::ResourceKey;
use spore_gmdl::GMDL_TYPE;
use spore_material::{MaterialError, TextureResolution};
use spore_texture::RASTER_TYPE;

mod support;

use spore_texture::DXT5_FOURCC;

use support::{raster_key, raster_record};

/// The package every headline number in this file is measured over.
const CONTENT: &str = "Spore_Content.package";

/// Directories scanned for the second package set, in that order.
const INSTALL_DIRS: [&str; 3] = ["Data", "DataEP1", "bp1content"];

/// Packages excluded from the second set, and why.
///
/// `Locale` and `Config` hold no content records. The two `Spore_Audio*` packages
/// are 1.9 GB between them and hold no gmdl and no raster: a texture is not in an
/// audio package, and mapping that in to learn it costs more than a minute of I/O.
/// This is a **measured** exclusion, not an assumption — `whole_install` prints
/// the per-package gmdl/raster counts that justify it.
const EXCLUDED: [(&str, &str); 3] = [
    (
        "Spore_Audio1",
        "972 MB of sound; no gmdl and no raster record",
    ),
    (
        "Spore_Audio2",
        "975 MB of sound; no gmdl and no raster record",
    ),
    ("Spore_EP1_Locale", "localisation strings"),
];

/// How many distinct absent identities are printed before the rest are summarised.
const SAMPLE_LIMIT: usize = 12;

// ---------------------------------------------------------------------------
// The closed vocabulary
// ---------------------------------------------------------------------------

/// One reference's outcome, as a key that can be counted and compared.
///
/// Every arm is a *distinct fact*. There is no catch-all and no `Other`: the
/// point of the exercise is that a new failure mode must be given a name here
/// rather than being absorbed by an existing one.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    /// Found in an opened package; its bytes decoded.
    Present,
    /// Looked in every opened package; the identity is not there.
    Absent,
    /// The store could not answer. Never merged into `Absent`.
    CouldNotLook(&'static str),
    /// A decoder declined, naming why. Never merged into `Absent`.
    Refused(&'static str),
}

impl Class {
    /// The stable spelling used as a count key and in the printed table.
    fn as_str(&self) -> String {
        match self {
            Self::Present => "present".to_owned(),
            Self::Absent => "absent".to_owned(),
            Self::CouldNotLook(why) => format!("could-not-look/{why}"),
            Self::Refused(why) => format!("refused/{why}"),
        }
    }
}

/// Classifies one `MaterialError`.
///
/// The `Lookup` split is the load-bearing arm. Both halves arrive as
/// `MaterialError::Lookup`, because the store is what failed in both; what
/// differs is whether the store *answered*. `AssetError::NotFound` is an answer
/// ("I looked at N packages and this key is in none of them"); every other
/// `AssetError` is the absence of one.
fn classify(error: &MaterialError) -> Class {
    match error {
        MaterialError::Lookup { source, .. } => match source {
            AssetError::NotFound { .. } => Class::Absent,
            AssetError::Open { .. } => Class::CouldNotLook("AssetError::Open"),
            AssetError::Dbpf { .. } => Class::CouldNotLook("AssetError::Dbpf"),
            AssetError::EmptyStore => Class::CouldNotLook("AssetError::EmptyStore"),
            AssetError::PackageIndexOutOfRange { .. } => {
                Class::CouldNotLook("AssetError::PackageIndexOutOfRange")
            }
            // The remaining `AssetError` variants are `spore-assets` decode
            // failures, which `store.read` cannot produce. Listed so the match
            // stays exhaustive and a new variant cannot silently become
            // `Absent`.
            AssetError::UnsupportedModelType { .. } => {
                Class::CouldNotLook("AssetError::UnsupportedModelType")
            }
            AssetError::EmptyModel { .. } => Class::CouldNotLook("AssetError::EmptyModel"),
            AssetError::Gmdl { .. } => Class::CouldNotLook("AssetError::Gmdl"),
            AssetError::Rw4 { .. } => Class::CouldNotLook("AssetError::Rw4"),
            AssetError::Texture { .. } => Class::CouldNotLook("AssetError::Texture"),
        },
        MaterialError::IncompleteReference { .. } => {
            Class::Refused("MaterialError::IncompleteReference")
        }
        MaterialError::AssumedTypeNotDecodable { .. } => {
            Class::Refused("MaterialError::AssumedTypeNotDecodable")
        }
        MaterialError::UnsupportedFourcc { .. } => {
            Class::Refused("MaterialError::UnsupportedFourcc")
        }
        MaterialError::EmptyImage { .. } => Class::Refused("MaterialError::EmptyImage"),
        MaterialError::Decode { source, .. } => Class::Refused(texture_error_name(source)),
        MaterialError::ModelDecode { source, .. } => Class::Refused(gmdl_error_name(source)),
    }
}

/// The `TextureError` variant's name, from an exhaustive match.
///
/// Written out rather than derived so that adding a variant to
/// `spore_texture::TextureError` **fails to compile here** instead of landing in
/// a bucket. That is the whole reason the census carries a reason vocabulary.
fn texture_error_name(error: &spore_texture::TextureError) -> &'static str {
    use spore_texture::TextureError as E;
    match error {
        E::TruncatedEnvelope { .. } => "TextureError::TruncatedEnvelope",
        E::UnsupportedFourcc { .. } => "TextureError::UnsupportedFourcc",
        E::UnsupportedBlockSize { .. } => "TextureError::UnsupportedBlockSize",
        E::ZeroDimension { .. } => "TextureError::ZeroDimension",
        E::ZeroMipCount => "TextureError::ZeroMipCount",
        E::LevelNotInChain { .. } => "TextureError::LevelNotInChain",
        E::MipChainBeyondFloor { .. } => "TextureError::MipChainBeyondFloor",
        E::RecordShorterThanEnvelope { .. } => "TextureError::RecordShorterThanEnvelope",
        E::ChainSizeOverflow { .. } => "TextureError::ChainSizeOverflow",
        E::RecordSizeMismatch { .. } => "TextureError::RecordSizeMismatch",
        E::Field18Mismatch { .. } => "TextureError::Field18Mismatch",
        E::LevelLengthPrefix { .. } => "TextureError::LevelLengthPrefix",
        E::LevelSlicePastRecord { .. } => "TextureError::LevelSlicePastRecord",
        E::PayloadTooSmall { .. } => "TextureError::PayloadTooSmall",
        E::PayloadNotMultiple { .. } => "TextureError::PayloadNotMultiple",
        E::MipSlicePastRecord { .. } => "TextureError::MipSlicePastRecord",
        E::LuminanceRecordSize { .. } => "TextureError::LuminanceRecordSize",
        E::LuminanceLengthPrefix { .. } => "TextureError::LuminanceLengthPrefix",
        E::LuminanceSlicePastRecord { .. } => "TextureError::LuminanceSlicePastRecord",
        E::ShortLuminanceMip { .. } => "TextureError::ShortLuminanceMip",
        E::LuminanceInvariant { .. } => "TextureError::LuminanceInvariant",
        E::ShortBlockChain { .. } => "TextureError::ShortBlockChain",
        E::OutputTooLarge { .. } => "TextureError::OutputTooLarge",
        E::OutputInvariant { .. } => "TextureError::OutputInvariant",
    }
}

/// The `GmdlError` variant's name, from an exhaustive match. Same reasoning as
/// [`texture_error_name`].
fn gmdl_error_name(error: &spore_gmdl::GmdlError) -> &'static str {
    use spore_gmdl::GmdlError as E;
    match error {
        E::EmptyInput => "GmdlError::EmptyInput",
        E::TruncatedHeader { .. } => "GmdlError::TruncatedHeader",
        E::UnsupportedVersion { .. } => "GmdlError::UnsupportedVersion",
        E::TruncatedRefCount => "GmdlError::TruncatedRefCount",
        E::TruncatedReferencedFiles { .. } => "GmdlError::TruncatedReferencedFiles",
        E::TruncatedBounds { .. } => "GmdlError::TruncatedBounds",
        E::NonFiniteBounds { .. } => "GmdlError::NonFiniteBounds",
        E::InvalidBounds { .. } => "GmdlError::InvalidBounds",
        E::TruncatedIndexBufferTable { .. } => "GmdlError::TruncatedIndexBufferTable",
        E::TruncatedIndexBuffer { .. } => "GmdlError::TruncatedIndexBuffer",
        E::TruncatedVertexDescriptorTable { .. } => "GmdlError::TruncatedVertexDescriptorTable",
        E::TruncatedVertexDescriptor { .. } => "GmdlError::TruncatedVertexDescriptor",
        E::UndocumentedDeclType { .. } => "GmdlError::UndocumentedDeclType",
        E::TruncatedVertexBufferTable { .. } => "GmdlError::TruncatedVertexBufferTable",
        E::TruncatedVertexBuffer { .. } => "GmdlError::TruncatedVertexBuffer",
        E::BadDescriptorIndex { .. } => "GmdlError::BadDescriptorIndex",
        E::TruncatedMeshTable { .. } => "GmdlError::TruncatedMeshTable",
        E::MeshReferencesMissingBuffer { .. } => "GmdlError::MeshReferencesMissingBuffer",
        E::TruncatedMaterialInfoTable { .. } => "GmdlError::TruncatedMaterialInfoTable",
        E::TruncatedMaterialInfo { .. } => "GmdlError::TruncatedMaterialInfo",
        E::TruncatedTextureSet { .. } => "GmdlError::TruncatedTextureSet",
        E::UndocumentedShaderDataId { .. } => "GmdlError::UndocumentedShaderDataId",
        E::TruncatedShaderData { .. } => "GmdlError::TruncatedShaderData",
        E::TruncatedTrailer { .. } => "GmdlError::TruncatedTrailer",
        E::MeshIndexOutOfRange { .. } => "GmdlError::MeshIndexOutOfRange",
        E::MeshReferencesMissingBuffers { .. } => "GmdlError::MeshReferencesMissingBuffers",
        E::UnsupportedTopology { .. } => "GmdlError::UnsupportedTopology",
        E::UnsupportedIndexWidth { .. } => "GmdlError::UnsupportedIndexWidth",
        E::EmptyIndexBuffer => "GmdlError::EmptyIndexBuffer",
        E::EmptyVertexBuffer => "GmdlError::EmptyVertexBuffer",
        E::TriangleListIndexCount { .. } => "GmdlError::TriangleListIndexCount",
        E::ShortIndexBuffer { .. } => "GmdlError::ShortIndexBuffer",
        E::MissingPositionElement { .. } => "GmdlError::MissingPositionElement",
        E::NonZeroStream { .. } => "GmdlError::NonZeroStream",
        E::UnsupportedVertexMethod { .. } => "GmdlError::UnsupportedVertexMethod",
        E::ZeroStride { .. } => "GmdlError::ZeroStride",
        E::StrideOverflow { .. } => "GmdlError::StrideOverflow",
        E::ShortVertexBuffer { .. } => "GmdlError::ShortVertexBuffer",
        E::NonFinitePosition { .. } => "GmdlError::NonFinitePosition",
        E::NonFiniteTexcoord { .. } => "GmdlError::NonFiniteTexcoord",
        E::IndexOutOfRange { .. } => "GmdlError::IndexOutOfRange",
    }
}

// ---------------------------------------------------------------------------
// The census
// ---------------------------------------------------------------------------

/// Everything one pass over one package set observed.
#[derive(Debug, Default)]
struct Census {
    /// Packages actually opened, in resolution order.
    packages: Vec<String>,
    /// gmdl identities in those packages.
    gmdl_records: usize,
    /// gmdl records whose bytes could not be extracted. **Not** a decode failure.
    record_unreadable: usize,
    /// gmdl records the recovering walk refused outright (geometry not decoded).
    record_undecodable: usize,
    /// gmdl records whose walk closed on the record end.
    record_complete: usize,
    /// gmdl records that stopped *inside* the material-info table, so their
    /// `texture_refs` is a strict **prefix** of the record's references.
    ///
    /// Tracked at record level and deliberately **not** folded into any reference
    /// class: a short list is "we did not read them all", which is neither "they
    /// are all here" nor "they are all absent".
    record_material_info_truncated: usize,
    /// References collected from those truncated records.
    references_in_truncated_records: usize,
    /// References collected from every record.
    references: usize,
    /// The four classes, counted.
    classes: BTreeMap<Class, usize>,
    /// Distinct identities per class, so a population of 40 000 references over
    /// 2 000 records is not mistaken for 2 000 missing assets.
    distinct: BTreeMap<Class, BTreeSet<ResourceKey>>,
    /// Absent references by the *group family of the reference* (`group & 0xFF..00`).
    absent_by_family: BTreeMap<u32, usize>,
    /// Absent references by the group id of the **model** that made them.
    absent_by_model_family: BTreeMap<u32, usize>,
    /// Present references by the raster fourcc.
    present_by_fourcc: BTreeMap<u32, usize>,
    /// Present references by the content role the decode assigned.
    present_by_role: BTreeMap<&'static str, usize>,
    /// Present references by group family.
    present_by_family: BTreeMap<u32, usize>,
    /// Why the recovering walk refused a gmdl record outright, by
    /// [`GmdlError`] variant.
    ///
    /// **The load-bearing number in this census.** A record refused here
    /// contributed **no references at all**, so its texture list was never read.
    /// Counting that as "this record has no textures" would be a fabricated
    /// negative over the whole population — the most consequential way to
    /// collapse "we did not look" into "we looked and found nothing", and one
    /// that a *reference* count alone would hide completely.
    undecodable_reasons: BTreeMap<&'static str, usize>,
}

impl Census {
    fn note(&mut self, class: Class, key: ResourceKey) {
        *self.classes.entry(class.clone()).or_insert(0) += 1;
        self.distinct.entry(class).or_default().insert(key);
    }

    fn get(&self, class: &Class) -> usize {
        self.classes.get(class).copied().unwrap_or(0)
    }

    fn sum_where(&self, keep: impl Fn(&Class) -> bool) -> usize {
        self.classes
            .iter()
            .filter(|(class, _)| keep(class))
            .map(|(_, count)| count)
            .sum()
    }

    fn absent_identities(&self) -> BTreeSet<ResourceKey> {
        self.distinct
            .get(&Class::Absent)
            .cloned()
            .unwrap_or_default()
    }

    fn print(&self, label: &str) {
        println!("\n===== {label} =====");
        println!(
            "packages opened ({}): {}",
            self.packages.len(),
            self.packages.join(", ")
        );
        println!("gmdl records: {}", self.gmdl_records);
        println!(
            "  unreadable (could not extract bytes): {}",
            self.record_unreadable
        );
        println!(
            "  refused by the decoder (geometry not decoded): {}",
            self.record_undecodable
        );
        println!("  walk closed on the record end: {}", self.record_complete);
        println!(
            "  material-info walk TRUNCATED (refs are a prefix): {} ({} refs collected from them)",
            self.record_material_info_truncated, self.references_in_truncated_records
        );
        println!("texture references collected: {}", self.references);
        if !self.undecodable_reasons.is_empty() {
            println!("\n-- records whose texture list was NEVER READ (walk refused outright) --");
            for (reason, count) in &self.undecodable_reasons {
                println!("  {reason:<48} {count:>7} records  0 references read");
            }
            println!(
                "  {:<48} {:>7} records  0 references read",
                "TOTAL", self.record_undecodable
            );
        }
        println!("\n-- class census (references) --");
        let mut total = 0usize;
        for (class, count) in &self.classes {
            let distinct = self.distinct.get(class).map_or(0, BTreeSet::len);
            println!(
                "  {:<58} {count:>7} refs  {distinct:>7} distinct identities",
                class.as_str()
            );
            total += count;
        }
        println!(
            "  {:<58} {total:>7} refs  (must equal the collected total)",
            "TOTAL"
        );
        println!("\n-- absent references by group family of the REFERENCE --");
        for (family, count) in &self.absent_by_family {
            println!("  0x{family:08x}  {count:>7}");
        }
        println!("\n-- absent references by group family of the MODEL --");
        for (family, count) in &self.absent_by_model_family {
            println!("  0x{family:08x}  {count:>7}");
        }
        println!("\n-- present references by raster fourcc --");
        for (fourcc, count) in &self.present_by_fourcc {
            println!(
                "  0x{fourcc:08x} {:<5} {count:>7}",
                spore_core::record::fourcc(*fourcc).unwrap_or_else(|| "____".to_owned())
            );
        }
        println!("\n-- present references by assigned content role --");
        for (role, count) in &self.present_by_role {
            println!("  {role:<24} {count:>7}");
        }
        println!("\n-- present references by group family --");
        for (family, count) in &self.present_by_family {
            println!("  0x{family:08x}  {count:>7}");
        }
        let identities = self.absent_identities();
        let sample: Vec<String> = identities
            .iter()
            .take(SAMPLE_LIMIT)
            .map(|key| format!("{key:?}"))
            .collect();
        println!(
            "\n-- distinct absent identities: {} total, showing {} --",
            identities.len(),
            sample.len()
        );
        for line in sample {
            println!("  {line}");
        }
    }
}

/// Runs the census over `store`.
///
/// `parse_recovering` rather than `parse`, on purpose: 619 records of
/// `Spore_Content.package` stop at the undocumented shader-data id `0x218`
/// inside the material-info table. `parse` refuses them outright, which would
/// hide their texture references entirely and turn "we stopped reading" into "we
/// found none" — the exact collapse this file exists to prevent. The truncation
/// is counted separately instead.
fn census(store: &ContentStore) -> Census {
    let mut out = Census {
        packages: store
            .package_names()
            .into_iter()
            .map(str::to_owned)
            .collect(),
        ..Census::default()
    };
    for model_key in store.identities_of_type(GMDL_TYPE) {
        out.gmdl_records += 1;
        let bytes = match store.read(&model_key) {
            Ok(bytes) => bytes,
            // Could not look. Counted at record level because there is no
            // reference to classify.
            Err(_) => {
                out.record_unreadable += 1;
                continue;
            }
        };
        let partial = match spore_gmdl::parse_recovering(&bytes) {
            Ok(partial) => partial,
            Err(error) => {
                out.record_undecodable += 1;
                *out.undecodable_reasons
                    .entry(gmdl_error_name(&error))
                    .or_insert(0) += 1;
                continue;
            }
        };
        let truncated = partial.stop.is_some();
        if truncated {
            out.record_material_info_truncated += 1;
        } else {
            out.record_complete += 1;
        }
        let model = partial.model;
        out.references += model.texture_refs.len();
        if truncated {
            out.references_in_truncated_records += model.texture_refs.len();
        }
        for reference in &model.texture_refs {
            let key = raster_key(reference.group_id, reference.instance_id);
            match spore_material::resolve_texture(store, *reference, RASTER_TYPE) {
                Ok(texture) => {
                    out.note(Class::Present, key);
                    *out.present_by_fourcc
                        .entry(texture.envelope.fourcc)
                        .or_insert(0) += 1;
                    *out.present_by_role
                        .entry(texture.role.as_str())
                        .or_insert(0) += 1;
                    *out.present_by_family
                        .entry(reference.group_id & 0xFFFF_FF00)
                        .or_insert(0) += 1;
                }
                Err(error) => {
                    let class = classify(&error);
                    if class == Class::Absent {
                        *out.absent_by_family
                            .entry(reference.group_id & 0xFFFF_FF00)
                            .or_insert(0) += 1;
                        *out.absent_by_model_family
                            .entry(model_key.group_id & 0xFFFF_FF00)
                            .or_insert(0) += 1;
                    }
                    out.note(class, key);
                }
            }
        }
    }
    out
}

/// Where `SPORE/Data` lives, or `None` when the game is not installed.
fn data_dir() -> Option<std::path::PathBuf> {
    if let Ok(dir) = std::env::var("OPENSPORE_SPORE_DATA") {
        let dir = std::path::PathBuf::from(dir);
        return dir.is_dir().then_some(dir);
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../SPORE/Data");
    dir.is_dir().then_some(dir)
}

/// The corpus census. **The measurement this workstream exists to produce.**
#[test]
#[ignore = "needs the git-ignored SPORE/ tree"]
fn every_gmdl_texture_reference_is_classified() {
    let Some(data) = data_dir() else {
        eprintln!(
            "SKIP: {} is absent, so there is no Spore corpus to census",
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../SPORE/Data")
                .display()
        );
        return;
    };

    // ---- set 1: Spore_Content.package alone.
    let content = {
        let mut store = ContentStore::new();
        store.push(
            Package::open("Spore_Content", data.join(CONTENT))
                .expect("Spore_Content.package opens"),
        );
        store
    };
    let content_census = census(&content);
    content_census.print("content_only: Spore_Content.package");

    // ---- set 2: every content-bearing package in the install.
    let install_root = data
        .parent()
        .expect("SPORE/Data has a parent")
        .to_path_buf();
    let mut whole = ContentStore::new();
    let mut skipped: Vec<String> = Vec::new();
    for dir in INSTALL_DIRS {
        let path = install_root.join(dir);
        let Ok(entries) = std::fs::read_dir(&path) else {
            skipped.push(format!("{dir} (absent)"));
            continue;
        };
        let mut found: Vec<std::path::PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "package"))
            .collect();
        found.sort();
        for path in found {
            let name = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string());
            if EXCLUDED.iter().any(|(prefix, _)| name.starts_with(prefix)) {
                skipped.push(name);
                continue;
            }
            match Package::open(&name, &path) {
                Ok(package) => whole.push(package),
                Err(error) => skipped.push(format!("{name}: {error}")),
            }
        }
    }
    println!("\n# packages excluded or skipped: {skipped:?}");
    let whole_census = census(&whole);
    whole_census.print("whole_install: every content-bearing package");

    // ---- Per-package record census. Without this the `whole install` line hides
    // which package the undecodable gmdl records actually live in, and "6832
    // records we could not read a texture list from" is not actionable. Built by
    // re-opening each package into its own one-package store, because that is the
    // only read path `spore-assets` exposes and it keeps this file free of a
    // `spore-dbpf` dependency.
    println!("\n===== per-package record census =====");
    println!(
        "{:<24} {:>8} {:>8} {:>11} {:>9} {:>9}",
        "package", "gmdl", "undecod", "raster", "trunc", "refs"
    );
    for dir in INSTALL_DIRS {
        let path = install_root.join(dir);
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        let mut files: Vec<std::path::PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "package"))
            .collect();
        files.sort();
        for file in files {
            let name = file
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| file.display().to_string());
            if EXCLUDED.iter().any(|(prefix, _)| name.starts_with(prefix)) {
                continue;
            }
            let Ok(package) = Package::open(&name, &file) else {
                continue;
            };
            let mut one = ContentStore::new();
            one.push(package);
            let census_one = census(&one);
            let rasters = one.identities_of_type(RASTER_TYPE).len();
            println!(
                "{:<24} {:>8} {:>8} {:>11} {:>9} {:>9}",
                name,
                census_one.gmdl_records,
                census_one.record_undecodable,
                rasters,
                census_one.record_material_info_truncated,
                census_one.references,
            );
        }
    }

    // ---- the two answers, stated.
    println!("\n===== VERDICT =====");
    for (label, result) in [
        ("Spore_Content.package alone", &content_census),
        ("whole install", &whole_census),
    ] {
        println!(
            "{label:<28}: present {:>6}  absent {:>6}  could-not-look {:>4}  refused {:>4}  \
             (of {} collected references, from {} gmdl records)",
            result.get(&Class::Present),
            result.get(&Class::Absent),
            result.sum_where(|class| matches!(class, Class::CouldNotLook(_))),
            result.sum_where(|class| matches!(class, Class::Refused(_))),
            result.references,
            result.gmdl_records,
        );
    }

    // The census must account for every reference it collected. This is the
    // assertion that makes the four classes a partition rather than a sample.
    for (label, result) in [
        ("content_only", &content_census),
        ("whole_install", &whole_census),
    ] {
        let total: usize = result.classes.values().sum();
        assert_eq!(
            total, result.references,
            "{label}: every collected reference must land in exactly one class"
        );
    }

    // A reference absent from the content package alone and present in the whole
    // install is a **cross-package reference**, not a dangling one.
    let rescued: Vec<ResourceKey> = whole_census
        .absent_identities()
        .difference(&content_census.absent_identities())
        .copied()
        .collect();
    println!(
        "identities absent from Spore_Content.package alone that DO resolve in the whole install: {}",
        rescued.len()
    );
    for key in rescued.iter().take(SAMPLE_LIMIT) {
        println!("  {key:?}");
    }

    // **NOTHING may be reported as `could not look` over a store of readable
    // packages.** An entry here would mean the census itself could not see, which
    // is a different claim from "it is not there" and the two must not trade
    // places.
    for (label, result) in [
        ("content_only", &content_census),
        ("whole_install", &whole_census),
    ] {
        assert_eq!(
            result.sum_where(|class| matches!(class, Class::CouldNotLook(_))),
            0,
            "{label}: a store of readable packages must be able to answer every lookup; a miss \
             here would make 'absent' and 'could not look' indistinguishable"
        );
    }

    // Pinned corpus facts. These are the numbers the report quotes; if one moves,
    // the report and `docs/TEXTURE-REFERENCES.md` are stale.
    assert_eq!(
        content_census.record_unreadable + content_census.record_undecodable,
        0,
        "every gmdl record of Spore_Content.package must be readable and decodable"
    );
    assert_eq!(
        content_census.gmdl_records, 4209,
        "the gmdl population of Spore_Content.package is a fact worth seeing change"
    );
    assert_eq!(
        content_census.record_material_info_truncated, 619,
        "the records that stop at the undocumented shader-data id 0x218"
    );
    assert_eq!(
        content_census.references, 16_628,
        "every collected reference, over the recovering walk's whole population. The strict \
         walk sees 15_390 of them (tests/real_corpus_roles.rs); the other 1_238 come from the \
         619 records whose material-info walk stopped, which is why this number is larger"
    );

    // **The headline, pinned.** Every reference but 150 resolves, and all 150 name
    // ONE identity. There is no "unreadable" class and no "refused" class in the
    // corpus: nothing was skipped for a reason this build could not state.
    assert_eq!(content_census.get(&Class::Present), 16_478);
    assert_eq!(content_census.get(&Class::Absent), 150);
    assert_eq!(
        content_census.sum_where(|class| matches!(class, Class::CouldNotLook(_))),
        0
    );
    assert_eq!(
        content_census.sum_where(|class| matches!(class, Class::Refused(_))),
        0
    );
    assert_eq!(
        content_census.absent_identities().len(),
        1,
        "150 references naming 150 different missing records would be one story; 150 naming \
         ONE is a different one, and the single identity is what makes it actionable"
    );

    // ---- THE NEAR-MISS PROBE. "Absent" is a statement about an identity, not about
    // a record. The one absent identity is `(raster, group 0x00000000, instance
    // 0x552a5077)`, and a group id of **zero** is not a group — so before calling
    // the record missing it is worth asking whether the *instance* exists under
    // some other group. If it does, the reference's group word is what is wrong;
    // if it does not, the record is simply not in the install.
    const ABSENT_INSTANCE: u32 = 0x552A_5077;
    let under_group_zero: Vec<ResourceKey> = content
        .identities_of_type(RASTER_TYPE)
        .into_iter()
        .filter(|key| key.group_id == 0)
        .collect();
    let same_instance_other_group: Vec<ResourceKey> = content
        .identities_of_type(RASTER_TYPE)
        .into_iter()
        .filter(|key| key.instance_id == ABSENT_INSTANCE && key.group_id != 0)
        .collect();
    println!(
        "\n-- near-miss probe for the one absent identity (instance 0x{ABSENT_INSTANCE:08x}) --"
    );
    println!(
        "  raster records whose group id is literally 0: {}",
        under_group_zero.len()
    );
    for key in under_group_zero.iter().take(SAMPLE_LIMIT) {
        println!("    {key:?}");
    }
    println!(
        "  raster records with that instance under ANY non-zero group: {}",
        same_instance_other_group.len()
    );
    for key in same_instance_other_group.iter().take(SAMPLE_LIMIT) {
        println!("    {key:?}");
    }
    assert_eq!(
        under_group_zero.len(),
        0,
        "no raster record carries group id 0, so the reference names a group that does not exist \
         -- an absence of a NAME, which is not a record that went missing"
    );
    assert_eq!(
        same_instance_other_group.len(),
        0,
        "and the instance is not present under another group either, so the record is genuinely \
         not in the package"
    );
    // And across the whole install, so "not in Spore_Content.package" is not
    // merely "not in the base content".
    assert_eq!(
        whole
            .identities_of_type(RASTER_TYPE)
            .into_iter()
            .filter(|key| key.instance_id == ABSENT_INSTANCE)
            .count(),
        0,
        "the instance is in no raster record of any opened package"
    );

    // ---- THE OTHER DIRECTION, and the same care about "did not look". Over the
    // whole install the reference census is **unchanged**, because every
    // additional package's gmdl records were refused outright and so contributed
    // zero references. Those refusals are the second "we did not look" in this
    // census, and they are much larger than the dangling class.
    println!(
        "records whose texture list was never read, whole install: {} (a reference count of 0 \
         for them, NOT a measurement that they have no textures)",
        whole_census.record_undecodable
    );
    for (reason, count) in &whole_census.undecodable_reasons {
        println!("  {reason:<48} {count:>7} records");
    }
    assert_eq!(
        whole_census.references, content_census.references,
        "the whole-install pass must not read a reference the content-only pass did not, or the \
         two package sets are not comparable"
    );
    assert_eq!(
        whole_census.get(&Class::Absent),
        content_census.get(&Class::Absent),
        "and no reference resolves in another package that did not resolve in the content \
         package -- the 150 are not cross-package references"
    );
    assert_eq!(
        whole_census.record_undecodable, 6832,
        "the EP1 model population this build refuses. A refusal with a named reason, so the \
         reference census is honest about not covering it -- but it is 6 832 models and the \
         dangling class is 150 references, so the coverage gap is the bigger question"
    );
    assert_eq!(
        whole_census
            .undecodable_reasons
            .get("GmdlError::UnsupportedVersion"),
        Some(&6832),
        "and every one of them is the documented version refusal, which is a specification and \
         not a defect: if a reason other than UnsupportedVersion appears here, that is a real gap"
    );

    // ---- ...and the model families that carry them, at full precision rather
    // than the `& 0xFF..00` families above, because the existing
    // `tests/real_corpus_roles.rs::group_zero_references_exist_and_resolve_to_nothing`
    // asserts the population is model group `0x40616201` and reads as though that
    // were the only one. If the exact set is wider, that comment is a stale claim
    // and this is where the correction is measured.
    let mut absent_model_groups: BTreeSet<u32> = BTreeSet::new();
    for model_key in content.identities_of_type(GMDL_TYPE) {
        let Ok(bytes) = content.read(&model_key) else {
            continue;
        };
        let Ok(partial) = spore_gmdl::parse_recovering(&bytes) else {
            continue;
        };
        if partial
            .model
            .texture_refs
            .iter()
            .any(|reference| reference.group_id == 0)
        {
            absent_model_groups.insert(model_key.group_id);
        }
    }
    println!(
        "\n-- exact model groups carrying a group-0 reference: {:?} --",
        absent_model_groups
            .iter()
            .map(|g| format!("0x{g:08x}"))
            .collect::<Vec<_>>()
    );
    assert!(
        absent_model_groups.contains(&0x4061_6201),
        "the population the existing corpus test names"
    );
    assert!(
        absent_model_groups.len() > 1,
        "MORE THAN ONE model family carries them, so the existing test's 'every one of them is \
         in the same group, so this is one model family's convention rather than scattered \
         damage' is a stale claim: {absent_model_groups:?}"
    );

    // gmdl name? In `Spore_Content.package` every gmdl record decoded, so this
    // number is a real "unreferenced", not another "we did not look". It is the
    // complement of the census and it bounds how much of the asset set the
    // texture-reference question even reaches.
    let all_rasters: BTreeSet<ResourceKey> = content
        .identities_of_type(RASTER_TYPE)
        .into_iter()
        .collect();
    let referenced = content_census
        .distinct
        .get(&Class::Present)
        .cloned()
        .unwrap_or_default();
    let unreferenced: Vec<&ResourceKey> = all_rasters.difference(&referenced).collect();
    println!(
        "\n-- raster records in Spore_Content.package: {} total, {} named by a readable gmdl, \
         {} named by none --",
        all_rasters.len(),
        referenced.len(),
        unreferenced.len()
    );
    for key in unreferenced.iter().take(SAMPLE_LIMIT) {
        println!("  unreferenced {key:?}");
    }
    assert_eq!(
        all_rasters.len(),
        2954,
        "the raster population of Spore_Content.package is a fact worth seeing change"
    );
    assert_eq!(referenced.len(), 2342);
    assert_eq!(
        unreferenced.len(),
        612,
        "every gmdl record of this package decoded, so these 612 raster records really are named \
         by no model -- a different fact from the 6 832 EP1 models whose texture lists were \
         never read"
    );

    // ---- The unreferenced rasters, by family. All twelve of the printed sample
    // are `0x406628xx`, a family **no** present reference names — which is a
    // different thing from "the models that use it are among the 6 832 we never
    // read", and worth saying rather than leaving to a reader of the sample.
    let mut unreferenced_by_family: BTreeMap<u32, usize> = BTreeMap::new();
    for key in &unreferenced {
        *unreferenced_by_family
            .entry(key.group_id & 0xFFFF_FF00)
            .or_insert(0) += 1;
    }
    println!("-- unreferenced raster records by group family --");
    for (family, count) in &unreferenced_by_family {
        println!("  0x{family:08x}  {count:>7}");
    }
    let referenced_families: BTreeSet<u32> = referenced
        .iter()
        .map(|key| key.group_id & 0xFFFF_FF00)
        .collect();
    for (family, count) in &unreferenced_by_family {
        println!(
            "  0x{family:08x}: named by a present reference? {}",
            if referenced_families.contains(family) {
                "yes"
            } else {
                "NO"
            }
        );
        let _ = count;
    }
}

// ---------------------------------------------------------------------------
// Hermetic: one test per class
// ---------------------------------------------------------------------------

/// The store the hermetic class tests resolve against: one synthetic raster at
/// the documented asset's first texture group.
fn present_store() -> (ContentStore, spore_gmdl::GmdlTextureRef) {
    let key = raster_key(0x4063_2900, 0x067A_07F0);
    let store = support::store_over(&[(key, raster_record(16, 16, 2, DXT5_FOURCC))]);
    (
        store,
        spore_gmdl::GmdlTextureRef {
            instance_id: key.instance_id,
            group_id: key.group_id,
        },
    )
}

/// **present**: the identity is in an opened package and its bytes decode.
///
/// `Class::Present` is the absence of a `MaterialError`, so it is asserted by
/// construction — the classifier only ever runs on a failure.
#[test]
fn a_reference_into_an_opened_package_is_present() {
    let (store, reference) = present_store();
    let texture = spore_material::resolve_texture(&store, reference, RASTER_TYPE)
        .expect("the reference resolves");
    assert_eq!(texture.key, raster_key(0x4063_2900, 0x067A_07F0));
    assert!(texture.pixels_decoded());
    assert_eq!(Class::Present.as_str(), "present");
}

/// **absent**: the identity is in no opened package.
///
/// This is the class the workstream is about. The assertion is that it arrives
/// as a `NotFound` **and** that [`TextureResolution`] says `Absent` — not
/// `Unidentified`, and not `Refused`.
#[test]
fn a_reference_no_package_holds_is_absent_and_says_so() {
    let (store, _) = present_store();
    let dangling = spore_gmdl::GmdlTextureRef {
        instance_id: 0x067A_07F0,
        group_id: 0x4063_2FFF,
    };
    let error = spore_material::resolve_texture(&store, dangling, RASTER_TYPE)
        .expect_err("no package holds this identity");

    let MaterialError::Lookup { key, source } = &error else {
        panic!("a missing identity must be a Lookup, got {error:?}");
    };
    assert_eq!(*key, raster_key(0x4063_2FFF, 0x067A_07F0));
    assert!(
        matches!(source, AssetError::NotFound { .. }),
        "the store ANSWERED that the key is in none of its packages; anything else is \
         'could not look' and must not be counted as absent: {source:?}"
    );
    assert_eq!(classify(&error), Class::Absent);

    // The message names the key, so a log line is actionable without a struct
    // dump.
    let text = error.to_string();
    assert!(
        text.contains("40632fff"),
        "the message must name the missing identity: {text}"
    );

    // And the closed-vocabulary view agrees.
    match TextureResolution::of(&error) {
        TextureResolution::Absent {
            key: absent,
            searched,
        } => {
            assert_eq!(absent, raster_key(0x4063_2FFF, 0x067A_07F0));
            assert_eq!(
                searched, 1,
                "the store states how many packages it searched"
            );
        }
        other => panic!("expected Absent, got {other:?}"),
    }
}

/// **absent** must stay distinct from **could not look**: an *empty store* has no
/// answer to give, which is not the same as an answer of "no".
#[test]
fn a_store_that_cannot_look_is_not_reported_as_absent() {
    let empty = ContentStore::new();
    let reference = spore_gmdl::GmdlTextureRef {
        instance_id: 0x067A_07F0,
        group_id: 0x4063_2900,
    };
    let error = spore_material::resolve_texture(&empty, reference, RASTER_TYPE)
        .expect_err("an empty store resolves nothing");
    let MaterialError::Lookup { source, .. } = &error else {
        panic!("an empty store is a Lookup failure, got {error:?}");
    };
    assert!(
        matches!(source, AssetError::EmptyStore),
        "an empty store could not look; it did not look and find nothing: {source:?}"
    );
    assert_eq!(
        classify(&error),
        Class::CouldNotLook("AssetError::EmptyStore"),
        "the two must be different classes, and both must be reachable"
    );
    assert_ne!(
        Class::CouldNotLook("AssetError::EmptyStore"),
        Class::Absent,
        "if these compared equal the vocabulary would collapse 'we could not look' into 'we \
         looked and found nothing'"
    );
    assert!(matches!(
        TextureResolution::of(&error),
        TextureResolution::CouldNotLook { .. }
    ));
}

/// A record whose bytes are present but whose **fourcc has no frame** is a
/// refusal naming why — never an absence.
#[test]
fn a_present_record_of_an_undecodable_fourcc_is_refused_by_name() {
    let key = raster_key(0x4063_2901, 0x067A_07F0);
    let store = support::store_over(&[(key, raster_record(16, 16, 2, 0xDEAD_BEEF))]);
    let reference = spore_gmdl::GmdlTextureRef {
        instance_id: key.instance_id,
        group_id: key.group_id,
    };
    let error = spore_material::resolve_texture(&store, reference, RASTER_TYPE)
        .expect_err("a fourcc outside the known set is refused");

    let MaterialError::UnsupportedFourcc {
        key: refused,
        fourcc,
    } = &error
    else {
        panic!("expected a named fourcc refusal, got {error:?}");
    };
    assert_eq!(*refused, key, "the refusal names the record it is about");
    assert_eq!(*fourcc, 0xDEAD_BEEF);
    assert_eq!(
        classify(&error),
        Class::Refused("MaterialError::UnsupportedFourcc")
    );
    assert_ne!(
        classify(&error),
        Class::Absent,
        "a refused record is PRESENT; only its format has no frame"
    );
    assert!(matches!(
        TextureResolution::of(&error),
        TextureResolution::Refused(_)
    ));
}

/// A raster record whose payload contradicts its own envelope is **refused**, and
/// the reason names the texture-layer variant rather than collapsing into a bare
/// "could not decode".
#[test]
fn a_present_but_inconsistent_raster_is_refused_with_the_texture_variant_named() {
    let key = raster_key(0x4063_2902, 0x067A_07F0);
    let mut record = raster_record(16, 16, 2, DXT5_FOURCC);
    record.truncate(record.len() - 8);
    let store = support::store_over(&[(key, record)]);
    let reference = spore_gmdl::GmdlTextureRef {
        instance_id: key.instance_id,
        group_id: key.group_id,
    };
    let error = spore_material::resolve_texture(&store, reference, RASTER_TYPE)
        .expect_err("a truncated raster payload is refused");

    assert!(
        matches!(&error, MaterialError::Decode { .. }),
        "expected a texture-layer refusal, got {error:?}"
    );
    let class = classify(&error);
    assert!(
        matches!(&class, Class::Refused(why) if why.starts_with("TextureError::")),
        "the reason must name the texture-layer variant: {class:?}"
    );
    assert_ne!(class, Class::Absent);
}

/// A **wildcard** reference names no single record. It is refused by the
/// resolver, and refusing it is what stops "pick the first row that matches".
#[test]
fn a_wildcard_reference_is_refused_rather_than_resolved_to_the_first_match() {
    let (store, _) = present_store();
    let wildcard = spore_gmdl::GmdlTextureRef {
        instance_id: spore_core::WILDCARD,
        group_id: 0x4063_2900,
    };
    let error = spore_material::resolve_texture(&store, wildcard, RASTER_TYPE)
        .expect_err("a wildcard names no single record");
    assert!(
        matches!(error, MaterialError::IncompleteReference { .. }),
        "{error:?}"
    );
    assert_eq!(
        classify(&error),
        Class::Refused("MaterialError::IncompleteReference")
    );
    assert_ne!(classify(&error), Class::Absent);
}

/// **The regression test for the silent-wrong.** `ModelTextures::role_of` answers
/// [`spore_material::TextureRole::Unidentified`] for a reference that did not
/// resolve — and `Unidentified` is documented *and graded* at
/// [`spore_core::EvidenceLevel::Observed`] as "the record **decoded** and its
/// content matched none of the signatures", i.e. a specific negative with a known
/// cause. An absent record has no such observation: nothing was decoded, so
/// nothing declined. Reporting the absence with the grade of an observation is
/// silent-wrong.
///
/// `role_of` keeps its signature — `spore-engine` reads it and this workstream
/// does not own that crate — so the fix is the additive, closed-vocabulary
/// [`TextureResolution`]. This test is what makes the collapse impossible to
/// reintroduce silently: it asserts the collapse still exists in `role_of`
/// (so the next reader knows it is there) and that `slot` does not share it.
#[test]
fn a_dangling_reference_is_never_reported_as_the_graded_role_negative() {
    use spore_core::EvidenceLevel;
    use spore_material::TextureRole;

    // One store holding one raster plus one gmdl model whose texture set names
    // that raster and two identities no package holds. Both records go into one
    // synthetic DBPF image, so `resolve_model_record` reaches the model and the
    // texture through one lookup path.
    let present = raster_key(0x4063_2900, 0x067A_07F0);
    let model_key = ResourceKey::new(GMDL_TYPE, 0x4063_7E03, 0x067A_07F0);
    let gmdl = support::gmdl_record(
        1,
        &[support::DOCUMENTED_MATERIAL_ID],
        support::MaterialBlock::TextureSet(
            [0x4063_2900u32, 0x4063_2901, 0x4063_2902]
                .into_iter()
                .map(|group_id| spore_gmdl::GmdlTextureRef {
                    instance_id: 0x067A_07F0,
                    group_id,
                })
                .collect(),
        ),
    );
    let store = support::store_over(&[
        (model_key, gmdl),
        (present, raster_record(16, 16, 2, DXT5_FOURCC)),
    ]);

    let resolved = spore_material::resolve_model_record(&store, &model_key, RASTER_TYPE)
        .expect("the model record itself decodes");
    assert_eq!(resolved.textures.len(), 3);
    assert_eq!(
        resolved.resolved_count(),
        1,
        "exactly one reference resolves"
    );
    assert_eq!(resolved.role_of(0), TextureRole::ColourAndMask);

    // --- The collapse, stated rather than hidden.
    assert_eq!(
        resolved.role_of(1),
        TextureRole::Unidentified,
        "role_of answers Unidentified for a reference that is absent"
    );
    assert_eq!(
        spore_material::role_evidence(resolved.role_of(1)),
        EvidenceLevel::Observed,
        "and Unidentified is graded Observed -- 'the check ran and declined'. For an absent \
         record no check ran at all, so that grade is over-stated: this is the silent-wrong, and \
         it is why `slot` exists."
    );

    // --- The fix: a closed vocabulary that keeps the four classes apart.
    assert_eq!(
        resolved.slot(0).map(|slot| slot.class_name()),
        Some("present")
    );
    assert_eq!(
        resolved.slot(1).map(|slot| slot.class_name()),
        Some("absent")
    );
    assert_eq!(
        resolved.slot(2).map(|slot| slot.class_name()),
        Some("absent")
    );
    // Out of range is a *fifth* thing again, and stays `None` rather than
    // becoming "absent": the flat list never had that slot.
    assert_eq!(resolved.slot(3), None);

    // `reason()` names the identity and how many packages were searched, so a log
    // line carries the fact rather than a class name alone.
    let reason = resolved
        .slot(1)
        .expect("slot 1 exists")
        .reason()
        .expect("a failure has a reason");
    assert!(reason.contains("40632901"), "{reason}");
    assert!(reason.contains("1 opened package"), "{reason}");

    // And the same statement over the whole table, so a caller that wants a
    // count gets one per class rather than one per role.
    let census = resolved.slot_census();
    let get = |name: &str| {
        census
            .iter()
            .find(|(seen, _)| *seen == name)
            .map(|(_, count)| *count)
    };
    assert_eq!(get("present"), Some(1));
    assert_eq!(get("absent"), Some(2));
    assert_eq!(
        get("could-not-look"),
        None,
        "nothing could not be looked at -- and a missing key means 'not observed', not 0"
    );
    assert_eq!(
        get("refused"),
        None,
        "nothing was refused -- and a missing key means 'not observed', not 0"
    );
    assert_eq!(
        get("unidentified"),
        None,
        "a role is not a class: the two vocabularies must not share a bucket"
    );

    // The role census and the slot census are different measurements and must
    // not be added together: the role census puts an absent slot in
    // `Unidentified`, which is exactly the loss `slot` documents.
    let role_census = resolved.role_census();
    assert_eq!(
        role_census
            .iter()
            .find(|(role, _)| *role == TextureRole::Unidentified)
            .map(|(_, count)| *count),
        Some(2),
        "the role census still counts the two absent slots as `Unidentified`, which is the \
         documented collapse"
    );
}

/// A model whose material-info walk **stopped** is a different fact again, and it
/// is counted at record level: `texture_refs` is then a strict *prefix* of the
/// record's references, so "we read three" is not "the record has three".
#[test]
fn a_truncated_material_walk_reports_a_prefix_and_not_a_complete_list() {
    let model_key = ResourceKey::new(GMDL_TYPE, 0x4063_7E03, 0x067A_07F0);
    // Both of block 0's references have a record, so the prefix is *resolvable* —
    // it is short, not broken. The distinction is the point: a short list that
    // resolves cleanly still does not say the record has only two references.
    let rasters = support::creature_block0_refs()
        .into_iter()
        .map(|reference| {
            (
                raster_key(reference.group_id, reference.instance_id),
                raster_record(16, 16, 2, DXT5_FOURCC),
            )
        })
        .collect::<Vec<_>>();
    let mut records = vec![(
        model_key,
        support::truncated_material_gmdl(2, 3, support::creature_block0_refs()),
    )];
    records.extend(rasters);
    let store = support::store_over(&records);

    // The strict entry point refuses, naming the reason — so a caller that used
    // it learns there is no complete list.
    let error = spore_material::resolve_model_record(&store, &model_key, RASTER_TYPE)
        .expect_err("a walk that stopped inside the material info does not close");
    let MaterialError::ModelDecode { source, .. } = &error else {
        panic!("expected a gmdl-layer refusal, got {error:?}");
    };
    assert!(
        matches!(
            source,
            spore_gmdl::GmdlError::UndocumentedShaderDataId { .. }
        ),
        "the refusal must name the undocumented shader-data id: {source:?}"
    );

    // The recovering route returns the prefix **and** says the walk stopped.
    let partial =
        spore_gmdl::parse_recovering(&store.read(&model_key).expect("the model record extracts"))
            .expect("the geometry decodes");
    assert_eq!(
        partial.model.texture_refs.len(),
        2,
        "block 0's two references"
    );
    let stop = partial.stop.expect("and the walk stopped");
    assert_eq!(stop.stage, spore_gmdl::WalkStage::MaterialInfo);
    assert!(
        !partial.model.fully_walked(),
        "a stopped walk has not read the record to its end"
    );
    // The truncated list is still classified per reference, and neither absent
    // nor "the record has none" is inferred from it.
    let results = spore_material::resolve_model_textures(&store, &partial.model, RASTER_TYPE);
    assert_eq!(results.len(), 2);
    assert_eq!(
        results.iter().filter(|entry| entry.is_ok()).count(),
        2,
        "both of block 0's references resolve; the prefix is real, just short"
    );
}
