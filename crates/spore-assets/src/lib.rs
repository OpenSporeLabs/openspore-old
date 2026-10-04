//! # spore-assets — the boundary where Spore data enters the engine.
//!
//! Everything above this crate is renderer-agnostic: it is written in terms of
//! `spore_gmdl::Mesh` and `spore_texture::RasterImage`, not Bevy types, Bevy
//! assets, or GPU handles. Everything below it is format knowledge that exists
//! nowhere else. That is the whole design intent, and it is why this crate can
//! be tested with no window, no GPU and no game installed.
//!
//! ```text
//!   spore-dbpf    DBPF v3 index, QFS/RefPack, record extraction
//!   spore-rw4     RW4 container section directory
//!   spore-gmdl    gmdl decode, mesh extraction, bounds
//!   spore-texture raster envelope, DXT5 decode
//!        |
//!  spore-assets   <-- THIS CRATE: package priority, identity resolution,
//!        |             typed decode, the canonical manifest
//!        v
//!   spore-engine   Bevy plugins, assets, scenes, rendering
//! ```
//!
//! # Clean-room boundary
//!
//! No EA or Maxis code, data or asset is linked, vendored or embedded here. The
//! container layouts are reimplementations from format analysis; the reference
//! material is this repository's own research (`docs/ASSET-PATH.md`,
//! `docs/RENDERWARE-RESEARCH.md`) and its independent Python oracles under
//! `tools/spore/`. Spore's own assets stay on the operator's disk and are
//! memory-mapped at read time, never copied into the repository.
//!
//! # What is *not* here, stated plainly
//!
//! * **RW4 section payloads.** The section directory is decoded; raster, mesh,
//!   skeleton and animation sections inside it are not. An `rw4` model loads
//!   and reports zero meshes rather than pretending otherwise.
//! * **gmdl versions other than 8.** Rejected by name.
//! * **Cell-stage gameplay records** (`cCellCellResource` and its ten
//!   siblings). The layouts are known and ported later; they are gameplay
//!   configuration, not geometry, and they do not unblock the render path.
//! * **Instance naming.** DBPF has no string table. An instance id is opaque
//!   and nothing here invents a name for one.
//!
//! # The graph layer
//!
//! [`graph`] builds the dependency edges *between* records — model to its
//! textures, model to the `rw4` records it names, model to the raster sharing
//! its identity — with each edge kind graded by how it was established and every
//! unresolved reference reported rather than dropped. See `docs/RESOURCE-GRAPH.md`
//! for the measured numbers and, in particular, for the one place where the
//! corpus says this crate's own dependency reads a gmdl table with the wrong byte
//! order.

// `unsafe` is DENIED crate-wide, not merely forbidden, so that the single
// exception below has to be an explicit, greppable `#[allow]` on one function
// with a written safety argument -- rather than a blanket hole in the policy.
#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]

pub mod color;
pub mod error;
pub mod graph;
pub mod manifest;
pub mod model;
pub mod package;
pub mod store;

pub use color::{
    extract as extract_model_colors, ColorError, ModelColors, Paint, PaintList, PaintRegion,
    UnitRgb,
};
pub use error::AssetError;
pub use graph::{
    read_reference_table, DanglingReference, Edge, EdgeKind, EdgeSummary, GraphOptions,
    ReferenceTableError, ResourceGraph, SkipReason, TraversalEntry, UnresolvedRecord,
};
pub use manifest::{probe_gmdl, DecodeStatus, ManifestBuilder, ManifestEvidence, ManifestRow};
pub use model::{LoadedModel, LoadedTexture, ModelFormat, ModelStore, TextureStore};
pub use package::{Package, PackageBytes};
pub use store::{ContentStore, RecordRef};

use spore_core::ResourceKey;

/// Loads a set of packages from disk in the order given.
///
/// The order **is** the resolution priority: earlier paths win when the same
/// record identity appears in more than one package. A typical Spore install
/// wants base content first and patches in front:
///
/// ```no_run
/// # fn main() -> Result<(), spore_assets::AssetError> {
/// use spore_assets::{ContentStore, Package};
///
/// let mut store = ContentStore::new();
/// store.push(Package::open("Spore_Content", "SPORE/Data/Spore_Content.package")?);
/// store.push(Package::open("PatchData", "SPORE/Data/PatchData.package")?);
/// store.push_front(Package::open("PatchData", "SPORE/Data/PatchData.package")?);
/// # Ok(())
/// # }
/// ```
pub fn open_packages<I, P>(paths: I) -> Result<ContentStore, AssetError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<std::path::Path>,
{
    let mut store = ContentStore::new();
    for path in paths {
        let path = path.as_ref();
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        store.push(Package::open(name, path)?);
    }
    Ok(store)
}

/// Convenience: load one model record and return its meshes.
pub fn load_model_meshes(
    store: &ContentStore,
    key: &ResourceKey,
) -> Result<Vec<spore_gmdl::Mesh>, AssetError> {
    Ok(ModelStore::new(store).load(key)?.meshes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/mini_package.dbpf"
        );
        std::fs::read(path).expect("committed fixture must be present")
    }

    #[test]
    fn a_package_opens_from_bytes_without_touching_the_filesystem() {
        let package = Package::from_vec("mini", fixture()).unwrap();
        assert_eq!(package.record_count(), 3);
        assert_eq!(package.name(), "mini");
        assert!(package.path().is_none());
    }

    #[test]
    fn an_empty_store_refuses_every_lookup_by_name() {
        let store = ContentStore::new();
        let key = ResourceKey::new(spore_gmdl::GMDL_TYPE, 1, 2);
        assert_eq!(store.find(&key).unwrap_err(), AssetError::EmptyStore);
        assert_eq!(store.read(&key).unwrap_err(), AssetError::EmptyStore);
    }

    #[test]
    fn not_found_names_the_key_and_offers_near_misses() {
        let mut store = ContentStore::new();
        store.push(Package::from_vec("mini", fixture()).unwrap());
        let missing = ResourceKey::new(0xDEAD_BEEF, 1, 2);
        let err = store.find(&missing).unwrap_err();
        let AssetError::NotFound {
            key,
            searched,
            candidates,
        } = &err
        else {
            panic!("expected NotFound, got {err:?}");
        };
        assert_eq!(*key, missing);
        assert_eq!(*searched, 1);
        // A hint for an unknown *type* is impossible, so the list is empty --
        // and empty here must mean "no suggestion", not "no records exist".
        assert!(candidates.is_empty());
        assert_eq!(store.len(), 1);
    }

    #[test]
    fn not_found_for_a_known_type_offers_near_misses() {
        let mut store = ContentStore::new();
        store.push(Package::from_vec("mini", fixture()).unwrap());
        // Ask for the right type but an instance that does not exist.
        let present = store.packages()[0].index().entries()[0].key;
        let absent_instance = ResourceKey::new(present.type_id, present.group_id, 0xDEAD_BEEF);
        let AssetError::NotFound { candidates, .. } = store.find(&absent_instance).unwrap_err()
        else {
            panic!("expected NotFound");
        };
        assert!(
            !candidates.is_empty(),
            "a known type must produce a suggestion"
        );
    }

    #[test]
    fn priority_order_decides_which_package_answers() {
        let bytes = fixture();
        let key = {
            let probe = Package::from_vec("probe", bytes.clone()).unwrap();
            probe.index().entries()[0].key
        };

        let mut base_first = ContentStore::new();
        base_first.push(Package::from_vec("base", bytes.clone()).unwrap());
        base_first.push(Package::from_vec("patch", bytes.clone()).unwrap());
        assert_eq!(base_first.find(&key).unwrap().package_name, "base");

        let mut patch_first = ContentStore::new();
        patch_first.push(Package::from_vec("patch", bytes.clone()).unwrap());
        patch_first.push(Package::from_vec("base", bytes).unwrap());
        assert_eq!(
            patch_first.find(&key).unwrap().package_name,
            "patch",
            "an earlier package must win"
        );

        // `push_front` is the explicit override and outranks everything.
        let mut overlaid = ContentStore::new();
        overlaid.push(Package::from_vec("base", fixture()).unwrap());
        overlaid.push_front(Package::from_vec("patch", fixture()).unwrap());
        assert_eq!(overlaid.find(&key).unwrap().package_name, "patch");
    }

    #[test]
    fn read_from_bypasses_priority() {
        let bytes = fixture();
        let mut store = ContentStore::new();
        store.push(Package::from_vec("base", bytes.clone()).unwrap());
        store.push(Package::from_vec("patch", bytes).unwrap());
        let key = store.packages()[0].index().entries()[0].key;
        // The key exists in both packages, so a priority-blind read must agree
        // byte-for-byte with the priority-resolved one.
        let via_priority = store.read(&key).unwrap();
        let via_index = store.read_from(1, &key).unwrap();
        assert_eq!(via_priority, via_index);
    }

    #[test]
    fn read_from_reports_an_out_of_range_package_by_name() {
        let store = ContentStore::new();
        let key = ResourceKey::new(1, 2, 3);
        assert!(matches!(
            store.read_from(3, &key),
            Err(AssetError::PackageIndexOutOfRange { index: 3, count: 0 })
        ));
    }

    #[test]
    fn errors_expose_the_record_they_are_about() {
        let err = AssetError::UnsupportedModelType {
            key: ResourceKey::new(1, 2, 3),
            type_id: 9,
        };
        assert_eq!(err.key(), Some(ResourceKey::new(1, 2, 3)));
        assert_eq!(AssetError::EmptyStore.key(), None);
    }
}
