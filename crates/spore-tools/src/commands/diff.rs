//! `diff`: record-level differences between two packages.
//!
//! # Why it exists
//!
//! Two facts about Spore's data make "what changed" a question that has to be
//! answered at record level rather than at file level:
//!
//! * **Patch packages override base.** `ContentStore` resolves first-match-wins,
//!   so which copy of an identity is live is a product decision made by the order
//!   packages are loaded in. Comparing two packages answers "what would change
//!   if I loaded these in the other order".
//! * **One package can hold two revisions of the same identity.** A stock install
//!   contains a base `globals` record of 264 bytes and a patched one of 276
//!   bytes, both at the *same* `(type, group, instance)`. A per-key lookup that
//!   keeps only the first row would call that record unchanged, and it is not.
//!
//! So the comparison is over **revision sets**: every identity is mapped to the
//! multiset of index rows that carry it, in each package, and the multisets are
//! compared. Two identical rows at one identity and one row plus a different one
//! are different facts, and this command can tell them apart.
//!
//! # Index-level by default, bytes on request
//!
//! Comparing content means extracting and decompressing every record of both
//! packages. Over `Spore_Content.package` that is the whole image, so it is not
//! the default: the default compares what the index states (stored size, memory
//! size, compression word), which is a pure function of the index and costs
//! nothing. `--bytes` turns on a content comparison, and the report then says
//! which of the two it did.
//!
//! The interesting property `--bytes` has is the disagreement count: records
//! whose index rows are identical but whose bytes are not. A non-zero count means
//! the index does not determine the content, which is worth knowing about any
//! pipeline that caches by `(type, group, instance, size)`.
//!
//! # Differences are the output, not a failure
//!
//! `diff` exits [`EXIT_OK`](crate::EXIT_OK) when the packages differ, exactly as
//! `verify` exits `0` when records fail to decode. The counts are the answer the
//! caller asked for.

use std::collections::BTreeMap;
use std::io::Write;

use spore_core::ResourceKey;
use spore_dbpf::DbpfEntry;

use crate::json::{hex_id, Json};
use crate::{DiffRequest, ToolError};

/// The default cap on each difference list. `0` means "every row".
pub const DEFAULT_ROW_LIMIT: usize = 100;

/// What one identity looks like inside one package: every index row carrying it,
/// ascending by `(offset, stored_size, memory_size, compression)`.
///
/// A `Vec` and not a single row is the whole point: a package may carry several
/// revisions of one identity, and which of them wins is a separate question this
/// command answers rather than guesses.
type Revisions = BTreeMap<ResourceKey, Vec<DbpfEntry>>;

/// One side's answer about one identity.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Side<'a> {
    /// The identity is in this package, with these rows.
    Present(&'a [DbpfEntry]),
    /// The identity is not in this package at all.
    Absent,
}

/// One identity's verdict, with the rows that produced it.
///
/// Owned rather than borrowed so the report can be rendered after the store's
/// borrow ends, and so the JSON renderer does not have to carry a lifetime.
#[derive(Debug, Clone)]
struct Row {
    key: ResourceKey,
    difference: Difference,
}

/// How two packages disagree, per identity.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Difference {
    /// Only in the left package.
    OnlyLeft(Vec<DbpfEntry>),
    /// Only in the right package.
    OnlyRight(Vec<DbpfEntry>),
    /// In both, with different index rows.
    Changed(Vec<DbpfEntry>, Vec<DbpfEntry>),
    /// In both, with identical index rows, but with different content.
    SameIndexDifferentBytes(Vec<DbpfEntry>, Vec<DbpfEntry>),
}

impl Difference {
    /// One word for the report.
    const fn label(&self) -> &'static str {
        match self {
            Self::OnlyLeft(_) => "only-in-left",
            Self::OnlyRight(_) => "only-in-right",
            Self::Changed(..) => "changed",
            Self::SameIndexDifferentBytes(..) => "same-index-different-bytes",
        }
    }
}

/// The two packages under comparison, named once.
///
/// Grouped into one struct rather than four parameters so the report function has
/// a signature a reader can hold: `out`, the two sides, the rows, the identical
/// count and the request.
struct Sides<'a> {
    left_name: &'a str,
    right_name: &'a str,
    left: &'a spore_assets::Package,
    right: &'a spore_assets::Package,
}

/// One index row, reduced to the fields a comparison is allowed to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct IndexRow {
    stored_size: u32,
    memory_size: u32,
    compression: u16,
}

impl IndexRow {
    fn of(entry: &DbpfEntry) -> Self {
        Self {
            stored_size: entry.stored_size,
            memory_size: entry.memory_size,
            compression: entry.compression,
        }
    }
}

/// Groups a package's index rows by identity, sorted so the result is a pure
/// function of the index and not of the filesystem.
fn revisions_by_identity(index: &spore_dbpf::PackageIndex) -> Revisions {
    let mut out: Revisions = BTreeMap::new();
    for entry in index.entries() {
        out.entry(entry.key).or_default().push(*entry);
    }
    for rows in out.values_mut() {
        rows.sort_by_key(|entry| (IndexRow::of(entry), entry.offset));
    }
    out
}

/// Compares two packages and prints the report.
///
/// Nothing is read from a payload unless `--bytes` was given: this is an index
/// comparison by default so it can run over the largest package in the install
/// in well under a second.
///
/// **Where the two documents go.** With `--json`, the JSON object is the whole of
/// stdout and the human report moves to stderr; without it, the human report is
/// the whole of stdout.
pub fn run(
    request: &DiffRequest,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), ToolError> {
    let (left_name, left) = super::load_package(&request.left)?;
    let (right_name, right) = super::load_package(&request.right)?;

    let left_index = revisions_by_identity(left.index());
    let right_index = revisions_by_identity(right.index());

    let mut rows: Vec<Row> = Vec::new();
    let mut same = 0usize;
    // One pass over the union of both identity sets, ascending: the report's
    // order is a function of the identities, not of which map was walked.
    let mut identities: Vec<ResourceKey> = left_index.keys().copied().collect();
    for key in right_index.keys() {
        if !left_index.contains_key(key) {
            identities.push(*key);
        }
    }
    identities.sort_unstable();
    identities.dedup();

    for key in identities {
        let left_side = left_index
            .get(&key)
            .map_or(Side::Absent, |rows| Side::Present(rows.as_slice()));
        let right_side = right_index
            .get(&key)
            .map_or(Side::Absent, |rows| Side::Present(rows.as_slice()));
        let difference = classify(
            &key,
            left_side,
            right_side,
            left.bytes(),
            right.bytes(),
            request.bytes,
        )?;
        let (owned, identical) = difference;
        if identical {
            same += 1;
        } else if let Some(difference) = owned {
            rows.push(Row { key, difference });
        } else {
            // Unreachable: `classify` returns either a difference or `identical`.
            // Falling back to counting it as identical would be the safe-but-wrong
            // direction, so it is a panic instead.
            unreachable!("classify reported neither a difference nor an identical verdict")
        }
    }

    // Most frequent verdict first, then by identity, so the order is stable and
    // the interesting classes are not buried under a long added list.
    rows.sort_by(|left, right| {
        left.difference
            .label()
            .cmp(right.difference.label())
            .then_with(|| left.key.cmp(&right.key))
    });

    let sides = Sides {
        left_name: &left_name,
        right_name: &right_name,
        left: &left,
        right: &right,
    };
    let mut report: Vec<u8> = Vec::new();
    write_report(&mut report, &sides, &rows, same, request)?;

    if request.json {
        err.write_all(&report)?;
        writeln!(
            out,
            "{}",
            report_json(&rows, same, sides.left_name, sides.right_name, request)
        )?;
    } else {
        out.write_all(&report)?;
    }
    Ok(())
}

/// Classifies one identity. `Ok((None, true))` means "identical".
#[allow(clippy::type_complexity)]
fn classify<'a>(
    key: &ResourceKey,
    left: Side<'a>,
    right: Side<'a>,
    left_image: &[u8],
    right_image: &[u8],
    compare_bytes: bool,
) -> Result<(Option<Difference>, bool), ToolError> {
    match (left, right) {
        (Side::Present(left), Side::Absent) => {
            Ok((Some(Difference::OnlyLeft(left.to_vec())), false))
        }
        (Side::Absent, Side::Present(right)) => {
            Ok((Some(Difference::OnlyRight(right.to_vec())), false))
        }
        (Side::Absent, Side::Absent) => Ok((None, true)),
        (Side::Present(left), Side::Present(right)) => {
            let same_index = index_signature(left) == index_signature(right);
            if same_index && !compare_bytes {
                return Ok((None, true));
            }
            if same_index {
                // The index says these are the same record. Extracting both is
                // the only way to find out whether the *bytes* are, and a
                // disagreement here is worth a name of its own.
                let left_bytes = read_all(left_image, left, key)?;
                let right_bytes = read_all(right_image, right, key)?;
                if left_bytes == right_bytes {
                    Ok((None, true))
                } else {
                    Ok((
                        Some(Difference::SameIndexDifferentBytes(
                            left.to_vec(),
                            right.to_vec(),
                        )),
                        false,
                    ))
                }
            } else {
                Ok((
                    Some(Difference::Changed(left.to_vec(), right.to_vec())),
                    false,
                ))
            }
        }
    }
}

/// The index rows' comparable signature, order-insensitive.
///
/// Sorted because two packages may list the same two revisions in the opposite
/// index order, and that is not a difference in content.
fn index_signature(rows: &[DbpfEntry]) -> Vec<IndexRow> {
    let mut signature: Vec<IndexRow> = rows.iter().map(IndexRow::of).collect();
    signature.sort_unstable();
    signature
}

/// Extracts and decompresses every revision of one identity, in row order.
fn read_all(
    image: &[u8],
    rows: &[DbpfEntry],
    key: &ResourceKey,
) -> Result<Vec<Vec<u8>>, ToolError> {
    let mut out = Vec::with_capacity(rows.len());
    for entry in rows {
        let bytes = spore_dbpf::extract_record(image, entry).map_err(|error| {
            ToolError::Decode(format!("{key}: cannot read the record to compare: {error}"))
        })?;
        out.push(bytes);
    }
    Ok(out)
}

fn write_report(
    out: &mut dyn Write,
    sides: &Sides<'_>,
    rows: &[Row],
    same: usize,
    request: &DiffRequest,
) -> Result<(), ToolError> {
    writeln!(out, "left           {}", sides.left_name)?;
    writeln!(out, "right          {}", sides.right_name)?;
    writeln!(
        out,
        "comparison     {}",
        if request.bytes {
            "index rows AND decompressed content"
        } else {
            "index rows only (stored size, memory size, compression word). Pass --bytes to \
             compare content as well."
        }
    )?;
    writeln!(out, "left rows      {}", sides.left.record_count())?;
    writeln!(out, "right rows     {}", sides.right.record_count())?;
    writeln!(out, "identical      {same} identit(ies)")?;

    let only_left = rows
        .iter()
        .filter(|row| row.difference.label() == "only-in-left")
        .count();
    let only_right = rows
        .iter()
        .filter(|row| row.difference.label() == "only-in-right")
        .count();
    let changed = rows
        .iter()
        .filter(|row| row.difference.label() == "changed")
        .count();
    let index_disagreement = rows
        .iter()
        .filter(|row| row.difference.label() == "same-index-different-bytes")
        .count();

    writeln!(out, "only in left   {only_left}")?;
    writeln!(out, "only in right  {only_right}")?;
    writeln!(out, "changed        {changed}")?;
    if request.bytes {
        writeln!(
            out,
            "index says identical, bytes differ: {index_disagreement}"
        )?;
    }

    writeln!(out)?;
    let limit = request.limit;
    for verdict in [
        "changed",
        "same-index-different-bytes",
        "only-in-left",
        "only-in-right",
    ] {
        let selected: Vec<&Row> = rows
            .iter()
            .filter(|row| row.difference.label() == verdict)
            .collect();
        if selected.is_empty() {
            continue;
        }
        writeln!(out, "{verdict}  ({})", selected.len())?;
        let shown = if limit == 0 {
            selected.len()
        } else {
            limit.min(selected.len())
        };
        for row in selected.iter().take(shown) {
            writeln!(out, "  {}", row.key.to_tgi())?;
            write_rows(out, &row.difference)?;
        }
        if shown < selected.len() {
            writeln!(
                out,
                "  ({} further `{verdict}` identit(ies), not shown \u{2014} raise --limit or pass 0)",
                selected.len() - shown
            )?;
        }
        writeln!(out)?;
    }

    if request.bytes && index_disagreement > 0 {
        writeln!(
            out,
            "note           `same-index-different-bytes` means the DBPF index does not determine \
             the record's content. A cache keyed by (type, group, instance, size) would serve \
             the wrong bytes."
        )?;
    }

    Ok(())
}

/// The revision rows of one identity, one per line, so a caller can see *which*
/// revision changed rather than only that something did.
fn write_rows(out: &mut dyn Write, difference: &Difference) -> Result<(), ToolError> {
    let labelled = match difference {
        Difference::OnlyLeft(rows) => (Some("left "), rows),
        Difference::OnlyRight(rows) => (Some("right"), rows),
        Difference::Changed(left, right) => {
            for entry in left {
                writeln!(out, "    left  {}", index_text(entry))?;
            }
            for entry in right {
                writeln!(out, "    right {}", index_text(entry))?;
            }
            return Ok(());
        }
        Difference::SameIndexDifferentBytes(left, right) => {
            for entry in left {
                writeln!(out, "    left  {}", index_text(entry))?;
            }
            for entry in right {
                writeln!(out, "    right {}", index_text(entry))?;
            }
            return Ok(());
        }
    };
    if let (Some(label), rows) = labelled {
        for entry in rows {
            writeln!(out, "    {label} {}", index_text(entry))?;
        }
    }
    Ok(())
}

/// One index row as a line: the three words a comparison reads, plus the offset.
///
/// `offset` is printed because two revisions of one identity are usually told
/// apart by where they sit, and the offset is what a caller needs to go and read
/// one of them.
fn index_text(entry: &DbpfEntry) -> String {
    format!(
        "offset={:#010x} stored={} memory={} compression={}",
        entry.offset, entry.stored_size, entry.memory_size, entry.compression
    )
}

fn report_json(
    rows: &[Row],
    same: usize,
    left_name: &str,
    right_name: &str,
    request: &DiffRequest,
) -> Json {
    let mut differences = Vec::new();
    for row in rows {
        let (left, right) = match &row.difference {
            Difference::OnlyLeft(left) => (Some(entries_json(left)), None),
            Difference::OnlyRight(right) => (None, Some(entries_json(right))),
            Difference::Changed(left, right) => {
                (Some(entries_json(left)), Some(entries_json(right)))
            }
            Difference::SameIndexDifferentBytes(left, right) => {
                (Some(entries_json(left)), Some(entries_json(right)))
            }
        };
        differences.push(
            Json::obj()
                .push("kind", row.difference.label())
                .push("record", row.key.to_tgi())
                .push("type_id", hex_id(row.key.type_id))
                .push("left", Json::from(left))
                .push("right", Json::from(right)),
        );
    }
    Json::obj()
        .push("left_package", left_name)
        .push("right_package", right_name)
        .push("compared_bytes", Json::from(request.bytes))
        .push("identical_identities", Json::from(same))
        .push("differences", Json::Arr(differences))
}

fn entries_json(entries: &[DbpfEntry]) -> Json {
    Json::Arr(
        entries
            .iter()
            .map(|entry| {
                Json::obj()
                    .push("offset", hex_id(entry.offset))
                    .push("stored_size", Json::from(entry.stored_size))
                    .push("memory_size", Json::from(entry.memory_size))
                    .push("compression", Json::from(u32::from(entry.compression)))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(key: ResourceKey, stored: u32, memory: u32, offset: u32) -> DbpfEntry {
        DbpfEntry::new(
            key.type_id,
            key.group_id,
            key.instance_id,
            offset,
            stored,
            memory,
            0,
            false,
        )
    }

    fn key(instance: u32) -> ResourceKey {
        ResourceKey::new(0x2a3c_e5b7, 0x4061_6201, instance)
    }

    fn synthetic_image() -> Vec<u8> {
        // Two rows at one identity and one at another: the shape `diff` exists to
        // handle. Payloads are distinct so a content comparison can tell them
        // apart.
        let mut image = vec![0u8; 96];
        image[0..4].copy_from_slice(b"DBPF");
        image[4..8].copy_from_slice(&3u32.to_le_bytes());
        image[0x24..0x28].copy_from_slice(&3u32.to_le_bytes());
        image[0x40..0x44].copy_from_slice(&96u32.to_le_bytes());
        image.extend_from_slice(&0u32.to_le_bytes());
        let rows: [(u32, u32, u32); 3] = [
            (0x2a3c_e5b7, 0x4061_6201, 1),
            (0x2a3c_e5b7, 0x4061_6201, 1),
            (0x2a3c_e5b7, 0x4061_6201, 2),
        ];
        let sizes = [4u32, 8, 16];
        let mut offset = 96u32 + 4 + 28 * 3;
        for (index, (t, g, i)) in rows.iter().enumerate() {
            image.extend_from_slice(&t.to_le_bytes());
            image.extend_from_slice(&g.to_le_bytes());
            image.extend_from_slice(&i.to_le_bytes());
            image.extend_from_slice(&offset.to_le_bytes());
            image.extend_from_slice(&(sizes[index] | 0x8000_0000).to_le_bytes());
            image.extend_from_slice(&sizes[index].to_le_bytes());
            image.extend_from_slice(&0u16.to_le_bytes());
            image.push(0);
            image.push(0);
            offset += sizes[index];
        }
        image.extend_from_slice(&[0xAA; 4]);
        image.extend_from_slice(&[0xBB; 8]);
        image.extend_from_slice(&[0xCC; 16]);
        image
    }

    fn index_of(image: &[u8]) -> spore_dbpf::PackageIndex {
        spore_dbpf::PackageIndex::parse(image).expect("synthetic image must parse")
    }

    #[test]
    fn two_revisions_of_one_identity_are_both_kept() {
        let index = index_of(&synthetic_image());
        let grouped = revisions_by_identity(&index);
        assert_eq!(grouped.len(), 2, "two identities, not three rows");
        assert_eq!(
            grouped[&key(1)].len(),
            2,
            "the duplicate identity keeps both rows: the 264/276 case"
        );
        assert_eq!(grouped[&key(2)].len(), 1);
    }

    #[test]
    fn the_index_signature_ignores_list_order_but_not_content() {
        let a = vec![entry(key(1), 264, 264, 100), entry(key(1), 276, 276, 200)];
        let b = vec![entry(key(1), 276, 276, 200), entry(key(1), 264, 264, 100)];
        assert_eq!(
            index_signature(&a),
            index_signature(&b),
            "two packages may list the same revisions in opposite order"
        );
        let c = vec![entry(key(1), 264, 264, 100)];
        assert_ne!(
            index_signature(&a),
            index_signature(&c),
            "a different revision count is a different signature"
        );
    }

    #[test]
    fn identical_index_rows_are_identical_without_reading_a_byte() {
        let rows = vec![entry(key(1), 264, 264, 100)];
        // An empty image: any extraction would fail, so a pass here proves the
        // comparison did not read the payloads.
        let (difference, same) = classify(
            &key(1),
            Side::Present(&rows),
            Side::Present(&rows),
            &[],
            &[],
            false,
        )
        .unwrap();
        assert_eq!(difference, None);
        assert!(same);
    }

    #[test]
    fn one_side_only_is_reported_with_the_side_that_has_it() {
        let rows = vec![entry(key(1), 264, 264, 100)];
        let (difference, same) =
            classify(&key(1), Side::Present(&rows), Side::Absent, &[], &[], false).unwrap();
        assert!(!same);
        assert_eq!(difference.map(|d| d.label()), Some("only-in-left"));

        let (difference, same) =
            classify(&key(1), Side::Absent, Side::Present(&rows), &[], &[], false).unwrap();
        assert_eq!(difference.map(|d| d.label()), Some("only-in-right"));
        assert!(!same);
    }

    #[test]
    fn a_different_size_is_a_change_even_when_both_sides_have_two_revisions() {
        let left = vec![entry(key(1), 264, 264, 100), entry(key(1), 276, 276, 200)];
        let right = vec![entry(key(1), 264, 264, 100), entry(key(1), 280, 280, 200)];
        let (difference, same) = classify(
            &key(1),
            Side::Present(&left),
            Side::Present(&right),
            &[],
            &[],
            false,
        )
        .unwrap();
        assert_eq!(difference.map(|d| d.label()), Some("changed"));
        assert!(!same);
    }

    #[test]
    fn a_byte_comparison_of_identical_rows_over_the_same_image_agrees() {
        let image = synthetic_image();
        let rows = vec![entry(key(2), 16, 16, 96 + 4 + 28 * 3)];
        let (difference, same) = classify(
            &key(2),
            Side::Present(&rows),
            Side::Present(&rows),
            &image,
            &image,
            true,
        )
        .unwrap();
        assert_eq!(
            difference, None,
            "the same image cannot disagree with itself"
        );
        assert!(same);
    }

    #[test]
    fn a_comparison_that_cannot_read_a_record_is_a_decode_failure_not_a_difference() {
        // The rows claim bytes the image does not have, and `--bytes` was asked
        // for, so the tool must report it rather than call the records different.
        let rows = vec![entry(key(1), 4096, 4096, 900_000)];
        let error = classify(
            &key(1),
            Side::Present(&rows),
            Side::Present(&rows),
            &[],
            &[],
            true,
        )
        .unwrap_err();
        assert_eq!(error.code(), crate::EXIT_DECODE);
        assert!(error
            .to_string()
            .contains("cannot read the record to compare"));
    }

    #[test]
    fn an_identity_in_neither_package_is_not_a_difference() {
        let (difference, same) =
            classify(&key(9), Side::Absent, Side::Absent, &[], &[], false).unwrap();
        assert_eq!(difference, None);
        assert!(same);
    }

    #[test]
    fn a_side_names_its_two_states_rather_than_being_truthy() {
        // Two variants, not a bool: "in the left package, in the right" and
        // "in neither" are the three facts a comparison can report, and a
        // `present: bool` pair would make "in neither" and "in neither, and we
        // did not look" the same value.
        let rows = vec![entry(key(1), 1, 1, 0)];
        assert!(matches!(Side::Present(&rows), Side::Present(_)));
        assert!(matches!(Side::Absent, Side::Absent));
    }

    #[test]
    fn a_difference_row_prints_both_sides_with_the_words_the_index_states() {
        let left = vec![entry(key(1), 264, 264, 100)];
        let right = vec![entry(key(1), 276, 276, 200)];
        let difference = Difference::Changed(left, right);
        let mut out = Vec::new();
        write_rows(&mut out, &difference).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("left  offset=0x00000064 stored=264"),
            "{text}"
        );
        assert!(
            text.contains("right offset=0x000000c8 stored=276"),
            "{text}"
        );
    }
}
