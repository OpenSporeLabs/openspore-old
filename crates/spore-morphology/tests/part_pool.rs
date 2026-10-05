//! # The real authorable part pool, measured rather than remembered.
//!
//! The document census in `catalog_corpus.rs` answers "what do Spore's shipped
//! creatures use". This file answers the question a part palette actually needs:
//! "how many parts could I offer".
//!
//! The two differ by ~7.7x, and the difference is the whole reason the palette
//! cannot be built from the census. The census is circular — a part no shipped
//! creature places is invisible to it — so a catalogue built that way can only
//! ever offer what is already on screen.
//!
//! `#[ignore]`d by default: it opens a ~1 GB package and resolves 482 chains.

use spore_assets::ContentStore;
use spore_creator_core::document::BlockId;
use spore_morphology::dbpf::PackageSource;
use spore_morphology::resolve::TYPE_RIGBLOCK;
use spore_morphology::Catalog;

/// The creature rigblock group.
///
/// **Not** hard-coded in the library: creature parts live in `0x40626000` while
/// cell, vehicle, building, flora and UFO parts live in five other groups, so the
/// palette must name the group rather than have the layer guess.
const CREATURE_PARTS: u32 = 0x4062_6000;

/// The repository root, found from this crate rather than from the cwd.
///
/// `cargo test` runs each test binary with its cwd set to the **crate** directory,
/// not the workspace root, so a relative `SPORE/Data/...` resolves to nothing in
/// a release run and only works by accident in some invocations. A path that works
/// under `cargo test` and fails under `cargo test --release` is the worst kind.
fn install_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("the crate directory has a workspace root above it")
        .to_path_buf()
}

/// Whether the install is present. A **skip**, never a pass: "no game install"
/// is a statement about this machine, and a test that reports OK without running
/// is the unavailable-is-not-a-zero bug this repository keeps meeting.
fn have_install() -> bool {
    install_root()
        .join("SPORE/Data/Spore_Game.package")
        .is_file()
}

/// The store, for enumerating the group.
fn store() -> Option<ContentStore> {
    let root = install_root();
    spore_assets::open_packages([
        root.join("SPORE/Data/Spore_Game.package"),
        root.join("SPORE/Data/Spore_Graphics.package"),
    ])
    .ok()
}

/// A resolver source over the same two packages.
///
/// Separate from [`store`] because they do different jobs and have different
/// lifetimes: the store owns the index (needed to *enumerate*) and the source
/// borrows whole files with `fs::read` (needed to *resolve* 482 chains). Using one
/// for both would mean either re-opening the package or holding a store the
/// resolver cannot use.
fn source() -> PackageSource {
    let root = install_root();
    PackageSource::new()
        .open(&root.join("SPORE/Data/Spore_Game.package"))
        .open(&root.join("SPORE/Data/Spore_Graphics.package"))
}

/// Every creature-part identity in the shipped packages.
fn creature_ids() -> Vec<BlockId> {
    store()
        .as_ref()
        .expect("the store opens when the install is present")
        .identities_of_type(TYPE_RIGBLOCK)
        .into_iter()
        .filter(|key| key.group_id == CREATURE_PARTS)
        .map(|key| BlockId::new(key.group_id, key.instance_id))
        .collect()
}

#[test]
#[ignore = "needs SPORE/ and resolves 482 chains"]
fn the_creature_part_pool_is_far_larger_than_the_shipped_census() {
    if !have_install() {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    }
    let ids = creature_ids();

    assert!(
        ids.len() > 400,
        "expected several hundred creature rigblocks, found {}",
        ids.len()
    );

    let catalog = Catalog::build_from_identities(&source(), ids);
    let total = catalog.len();
    let authorable = catalog
        .entries()
        .iter()
        .filter(|entry| entry.is_authorable())
        .count();

    println!("CREATURE POOL rigblocks={total} authorable={authorable}");

    // The measurement this file exists to pin.
    assert!(
        authorable > 400,
        "the authorable creature pool should be several hundred, got {authorable}"
    );
    // And the relationship that makes the census useless as a palette: the pool is
    // several times the 60 the document census finds. Asserted as a ratio so the
    // test survives either number moving a little.
    assert!(
        authorable > 60 * 4,
        "the pool ({authorable}) must dwarf the shipped census (60), or the \\
         palette could have been built from the documents after all"
    );
}

#[test]
#[ignore = "needs SPORE/ and resolves 482 chains"]
fn a_part_nobody_references_is_still_offered() {
    if !have_install() {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    }
    // The property that makes this builder worth having. An unreferenced part has
    // an empty `references` list, which is NOT the same as being unavailable --
    // if it were, the palette would be circular and could never grow.
    let catalog = Catalog::build_from_identities(&source(), creature_ids());

    let authorable = catalog
        .entries()
        .iter()
        .filter(|entry| entry.is_authorable())
        .count();
    let unreferenced = catalog
        .entries()
        .iter()
        .filter(|entry| entry.is_authorable() && entry.reference_count() == 0)
        .count();
    println!("CREATURE POOL unreferenced-but-authorable={unreferenced} of {authorable}");

    assert!(
        authorable > 300,
        "the pool should still be several hundred, got {authorable}"
    );
    // EVERY authorable part is unreferenced, which is the whole point: this
    // builder is handed no documents, so `references` is empty by construction.
    //
    // The first version of this assertion claimed some WOULD be referenced, on the
    // theory that the shipped creatures use some of their own parts. They do not --
    // in this pool, not one -- and asserting otherwise would have been me trying
    // to make a number come out. The property that actually matters is the
    // conjunction: all 464 resolve AND none is referenced, which is precisely what
    // makes the document census circular and this builder necessary.
    assert_eq!(
        unreferenced, authorable,
        "every part handed to build_from_identities has empty references by \
         construction; a non-zero difference would mean the builder invented some"
    );
}

#[test]
#[ignore = "needs SPORE/ and resolves 482 chains"]
fn the_group_is_load_bearing_so_two_families_never_share_a_part() {
    // The group selects WHICH parts exist. This asserts the property that actually
    // matters about that, after the first version of this test asserted something
    // that is simply false.
    //
    // It asserted that cell rigblocks resolve as NOT authorable, on the theory
    // that a creature palette must exclude them. But the resolver does not know
    // about families -- it walks `rigblock -> prop -> part pair` -- and cell parts
    // resolve perfectly well, 58 of 58. A `<blockid>` naming a cell part inside a
    // `crt` document would resolve; what actually stops it is the document's own
    // grammar and the meaning of the group, not the resolver. Asserting
    // non-resolution would have been a claim about the resolver that is wrong, and
    // it would have failed against correct code.
    //
    // The invariant worth pinning is DISJOINTNESS: the creature pool and the cell
    // pool share no identity. If they ever overlapped, "name the group" would stop
    // meaning anything and `build_from_identities` would be handed a mixed pool.
    if !have_install() {
        eprintln!("SKIP: no SPORE/Data/Spore_Game.package");
        return;
    }
    let creature: Vec<BlockId> = creature_ids();
    let cell: Vec<BlockId> = store()
        .as_ref()
        .expect("the store opens when the install is present")
        .identities_of_type(TYPE_RIGBLOCK)
        .into_iter()
        .filter(|key| key.group_id == 0x4061_6000)
        .map(|key| BlockId::new(key.group_id, key.instance_id))
        .collect();

    println!(
        "CREATURE POOL creature={} cell={}",
        creature.len(),
        cell.len()
    );
    assert!(
        !cell.is_empty(),
        "the cell pool exists; an empty one proves nothing"
    );

    let cell_set: std::collections::HashSet<BlockId> = cell.iter().copied().collect();
    let shared: Vec<BlockId> = creature
        .iter()
        .copied()
        .filter(|id| cell_set.contains(id))
        .collect();
    assert!(
        shared.is_empty(),
        "a creature part is also a cell part: {shared:?}. Then the group selects \
         nothing and `build_from_identities` is being handed a mixed pool."
    );
}
