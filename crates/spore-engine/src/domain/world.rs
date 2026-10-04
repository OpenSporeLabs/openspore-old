//! The resource manager: the packages, and the store over them.
//!
//! One type owns every byte Spore hands the engine, and it owns them only for
//! as long as loading needs them. That is the whole lifetime story, and it is
//! worth writing down because the previous arrangement leaked it:
//!
//! * `spore-assets`' [`ContentStore`] memory-maps each package.
//!   `Spore_Content.package` is **995 268 083 bytes**.
//! * The renderer needs vertex buffers and texels, both of which are *copied*
//!   out of the mapping by the decoders.
//! * So nothing downstream of [`crate::domain::model`] reads the store.
//!
//! Consequently [`crate::prepare`] holds the world in a local, decodes
//! everything, and lets the mapping go. [`crate::loader::WorldResources`] exists
//! so the world can *also* be handed to a running app — for a future
//! streaming subsystem — but no system in this crate reads it, and the
//! renderer cannot therefore be holding the mapping alive by accident.

use std::path::Path;

use spore_assets::{AssetError, ContentStore, LoadedModel, ModelStore, Package};
use spore_core::ResourceKey;

/// Owns the opened packages and resolves identities against them.
#[derive(Debug)]
pub struct ResourceWorld {
    store: ContentStore,
    packages: Vec<String>,
}

impl ResourceWorld {
    /// Opens packages in the order given; the first match wins.
    ///
    /// Order is a product decision, not a format one — see
    /// `spore_assets::ContentStore`. A caller loading base content before a
    /// patch gets the patch's copy; see `docs/RUST-ENGINE.md` §4a.
    pub fn open<P: AsRef<Path>>(paths: &[P]) -> Result<Self, AssetError> {
        let mut store = ContentStore::new();
        for path in paths {
            store.push(Package::open(package_name(path.as_ref()), path.as_ref())?);
        }
        let packages = store
            .package_names()
            .into_iter()
            .map(str::to_owned)
            .collect();
        Ok(Self { store, packages })
    }

    /// The store, for the crates that resolve records against it.
    pub fn store(&self) -> &ContentStore {
        &self.store
    }

    /// Which packages were searched, in resolution order.
    pub fn package_names(&self) -> &[String] {
        &self.packages
    }

    /// Bytes of package image held in the mapping.
    ///
    /// Reported rather than hidden: it is the number that decides whether
    /// keeping the world alive past loading is affordable.
    pub fn mapped_bytes(&self) -> usize {
        self.store.packages().iter().map(|p| p.bytes().len()).sum()
    }

    /// Decodes one model record.
    ///
    /// Uses `spore-assets`' **recovering** parse, so a record whose tail names
    /// an undocumented shader-data id still yields complete geometry. Read
    /// [`crate::domain::model::DecodeStatus`] before treating a result as
    /// complete.
    pub fn load_model(&self, key: &ResourceKey) -> Result<LoadedModel, AssetError> {
        ModelStore::new(&self.store).load(key)
    }
}

/// The display name for a package path: its file stem.
fn package_name(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_world_resolves_nothing_and_says_so() {
        let world: ResourceWorld = ResourceWorld::open::<&str>(&[]).unwrap();
        assert!(world.package_names().is_empty());
        assert_eq!(world.mapped_bytes(), 0);
        assert!(world.store().is_empty());
        assert!(matches!(
            world.load_model(&ResourceKey::new(1, 2, 3)),
            Err(AssetError::EmptyStore)
        ));
    }

    #[test]
    fn opening_a_missing_package_is_an_error_naming_the_path() {
        let error =
            ResourceWorld::open(&[std::path::PathBuf::from("/definitely/not/here.package")])
                .unwrap_err();
        assert!(
            error.to_string().contains("/definitely/not/here.package"),
            "{error}"
        );
    }

    #[test]
    fn a_package_is_named_after_its_file_stem() {
        assert_eq!(
            package_name(Path::new("/game/Data/Spore_Content.package")),
            "Spore_Content"
        );
    }

    #[test]
    fn the_fixture_package_opens_and_reports_its_mapped_size() {
        // The one package this crate may depend on without the game install:
        // a committed synthetic DBPF under `tests/fixtures/`. No Spore asset
        // byte is read here.
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/fixtures/mini_package.dbpf"
        );
        let world = ResourceWorld::open(&[std::path::PathBuf::from(path)]).unwrap();
        assert_eq!(
            world.package_names(),
            ["mini_package"],
            "the display name is the file stem"
        );
        assert!(world.mapped_bytes() > 0);
    }
}
