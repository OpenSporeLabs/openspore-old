//! One module per subcommand.
//!
//! Each `run` takes a fully parsed request and a `&mut dyn Write`, returns a
//! [`ToolError`], and holds no global state. That is the whole contract: the
//! argument parser lives in [`crate`], the command bodies here, and the binary in
//! `main.rs` is a shim. Nothing in this module reads the clock, the environment
//! or the terminal except where a behaviour is explicitly about a terminal
//! (`verify`'s progress line).
//!
//! # The decoded / type-level rule
//!
//! Every printed line that makes a claim about a record is one of two things,
//! and the wording says which:
//!
//! * **type-level** — a fact about the *type table*: "this build has a decoder
//!   family for `gmdl`", "`png` is a container this build does not decode".
//!   Cheap, computed from the type id alone, and true before any record is read.
//! * **decoded** — a fact about *these bytes*: "1 mesh, 50 vertices, stride 16",
//!   only ever printed after a decoder returned a value.
//!
//! A line that is neither must say so explicitly rather than borrow the
//! formatting of one that is. [`describe`] is where the distinction earns its
//! keep: a `plt` record prints its index extent and the sentence "this build has
//! no decoder for type 0x011989b7", never a mesh count.

pub mod describe;
pub mod diff;
pub mod extract;
pub mod find;
pub mod graph;
pub mod list;
pub mod manifest;
pub mod stats;
pub mod texture;
pub mod types;
pub mod verify;

use std::path::Path;

use spore_assets::{ContentStore, Package};
use spore_core::record::{group_name, RecordType};
use spore_dbpf::DbpfEntry;

use crate::ToolError;

/// Opens one package, naming it after its file stem.
///
/// The file stem is the name Spore's own asset tooling uses to refer to a
/// package (`Spore_Content`, `PatchData`), so it is the name that appears in
/// error messages and in a `find` report's "which package answered" line.
pub fn load_package(path: &Path) -> Result<(String, Package), ToolError> {
    let name = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let package = Package::open(name.clone(), path)?;
    Ok((name, package))
}

/// Opens a list of packages into one store, in the order given.
///
/// The order is the resolution priority: earlier wins on a duplicate identity.
/// `manifest` is the only command that accepts several, and it documents that
/// order as part of its contract.
pub fn open_store(paths: &[impl AsRef<Path>]) -> Result<ContentStore, ToolError> {
    let mut store = ContentStore::new();
    for path in paths {
        let path = path.as_ref();
        let (_, package) = load_package(path)?;
        store.push(package);
    }
    Ok(store)
}

/// Opens one package into a store, so a single-package command still goes
/// through the same priority resolution a multi-package one does.
///
/// Going through the store rather than calling [`spore_dbpf::find_entry`]
/// directly is what gives a miss its near-miss candidates, and it means the
/// "which package answered" line is the store's answer rather than a second
/// lookup policy written here.
pub fn single_package_store(path: &Path) -> Result<ContentStore, ToolError> {
    let mut store = ContentStore::new();
    let (_, package) = load_package(path)?;
    store.push(package);
    Ok(store)
}

/// The canonical type name for an id, or `None` when this build has none.
///
/// An unknown id is `None`, never a guess and never a four-character code: the
/// fourcc of a Spore type id is mostly punctuation (`gmdl` renders as `____`)
/// and would read as a name it is not.
pub fn type_name_of(type_id: u32) -> Option<&'static str> {
    RecordType::new(type_id).name()
}

/// The canonical group name for an id, or `None`.
pub fn group_name_of(group_id: u32) -> Option<&'static str> {
    group_name(group_id)
}

/// `0x%08x`.
pub fn hex(value: u32) -> String {
    format!("0x{value:08x}")
}

/// Renders a record type for a human line: `0x00e6bce5 (gmdl)`, or just the hex
/// when the id has no name in this build.
pub fn describe_type(type_id: u32) -> String {
    match type_name_of(type_id) {
        Some(name) => format!("{} ({name})", hex(type_id)),
        None => hex(type_id),
    }
}

/// Renders a compression word, distinguishing the three states the format
/// actually has.
///
/// `entry.compressed` is `false` for an *unsupported* word, so a two-way
/// "compressed yes/no" column silently calls an undecodable record "stored".
/// This keeps the third state visible.
pub fn compression_text(entry: &DbpfEntry) -> String {
    match entry.compression {
        spore_dbpf::COMPRESSION_NONE => "none".to_owned(),
        spore_dbpf::COMPRESSION_QFS => "qfs".to_owned(),
        other => format!("unsupported({other:#06x})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_renders_with_its_canonical_name_or_without_one() {
        assert_eq!(describe_type(0x00e6_bce5), "0x00e6bce5 (gmdl)");
        assert_eq!(describe_type(0xdead_beef), "0xdeadbeef");
        // An unknown id must never be rendered as its fourcc: a Spore type id
        // mostly spells as punctuation.
        assert!(!describe_type(0x00e6_bce5).contains("____"));
    }

    #[test]
    fn the_compression_word_keeps_three_states_apart() {
        let none = DbpfEntry::new(1, 2, 3, 0, 4, 4, spore_dbpf::COMPRESSION_NONE, false);
        assert_eq!(compression_text(&none), "none");
        let qfs = DbpfEntry::new(1, 2, 3, 0, 4, 4, spore_dbpf::COMPRESSION_QFS, false);
        assert_eq!(compression_text(&qfs), "qfs");
        // The interesting one: `compressed` is false, but the record is not
        // merely "stored" either -- extraction refuses it.
        let odd = DbpfEntry::new(1, 2, 3, 0, 4, 4, 0x1234, false);
        assert!(!odd.compressed);
        assert_eq!(compression_text(&odd), "unsupported(0x1234)");
    }

    #[test]
    fn a_missing_package_is_an_io_failure() {
        let err = load_package(Path::new("/nonexistent/definitely-not-here.package")).unwrap_err();
        assert_eq!(err.code(), crate::EXIT_IO);
    }
}
