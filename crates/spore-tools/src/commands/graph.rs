//! `graph`: print the resource dependency graph around one record.
//!
//! # This command owns no graph logic
//!
//! [`spore_assets::ResourceGraph`] builds the edges, grades each kind with
//! [`spore_core::EvidenceLevel`], counts dangling references and reports every
//! record it could not read along with the reason. `traverse` is breadth-first
//! and ascending by [`spore_core::ResourceKey`], so two runs print the same
//! lines in the same order. None of that is re-implemented here; this module
//! chooses what to build, what to walk and how to render it.
//!
//! # `--kinds` selects what is *built*, not just what is followed
//!
//! Building `gmdl->texture` edges means extracting and walking every `gmdl`
//! record in the loaded packages — 4 209 records of a 995 MB package. A caller
//! who wants one kind's edges should not pay for the other three, so a `--kinds`
//! list narrows [`spore_assets::GraphOptions`] itself. The kind actually built is
//! printed, so a report can never imply an edge kind that was switched off.
//!
//! # A dangling edge is the normal case
//!
//! `gmdl->reference` and `gmdl->texture` are *declared* references: bytes inside
//! the model name them. Measured on `Spore_Content.package`, many of those name
//! records in another package that was not loaded, so dangling is expected and
//! is not an error. The exit code is [`EXIT_OK`](crate::EXIT_OK) whatever the
//! counts; the numbers are the output.
//!
//! # Which packages to load decides what dangles
//!
//! The graph resolves a reference against the *loaded* packages only, so
//! `graph` on one package legitimately reports a texture edge as dangling that a
//! run over four packages resolves. The report states the loaded set for exactly
//! that reason.

use std::collections::BTreeMap;
use std::io::Write;

use spore_assets::{DanglingReference, EdgeKind, GraphOptions, ResourceGraph, SkipReason};
use spore_core::ResourceKey;

use crate::json::Json;
use crate::{GraphRequest, ToolError};

/// How many rows the traversal and per-kind dangling samples print by default.
pub const DEFAULT_ROW_LIMIT: usize = 200;

/// The breadth-first walk depth used when `--depth` is absent.
pub const DEFAULT_DEPTH: u32 = 1;

/// `graph`: build, walk, report.
///
/// **Where the two documents go.** With `--json`, the JSON object is the whole of
/// stdout and the human report moves to stderr. Without it, the human report is
/// the whole of stdout. The rule is the tool's: stdout carries only the document
/// the caller asked for, so `graph ... --json | jq` works.
pub fn run(
    request: &GraphRequest,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<(), ToolError> {
    let store = super::open_store(&request.packages)?;
    if store.is_empty() {
        return Err(ToolError::NotFound(
            "no package was loaded, so no graph could be built".to_owned(),
        ));
    }

    let options = options_for(&request.kinds);
    let graph = ResourceGraph::build_with(&store, options);
    let reached = graph.traverse(&request.key, request.depth, &request.kinds);

    let mut report: Vec<u8> = Vec::new();
    if request.dependants {
        write_dependants(&mut report, &graph, &request.key, request.limit)?;
    }
    write_report(&mut report, &graph, &store, &request.key, &reached, request)?;

    if request.json {
        err.write_all(&report)?;
        writeln!(out, "{}", summary_json(&graph, &request.key, &reached))?;
    } else {
        out.write_all(&report)?;
    }
    Ok(())
}

/// The [`GraphOptions`] a `--kinds` list implies.
///
/// No list means [`GraphOptions::default`], which is every kind except
/// `group-sibling` — that one is off by default in `spore_assets` because it is
/// adjacency rather than a named reference, and this tool does not get to turn it
/// on behind the caller's back.
pub fn options_for(kinds: &[EdgeKind]) -> GraphOptions {
    if kinds.is_empty() {
        return GraphOptions::default();
    }
    let mut options = GraphOptions::none();
    options.texture_edges = kinds.contains(&EdgeKind::GmdlTexture);
    options.reference_edges = kinds.contains(&EdgeKind::GmdlReference);
    options.model_texture_sets = kinds.contains(&EdgeKind::ModelTextureSet);
    options.sibling_records = kinds.contains(&EdgeKind::SiblingRecord);
    options
}

/// Whether the root itself is present in the loaded packages.
///
/// Printed as a plain fact rather than as an error: the walk from a key that is
/// not loaded returns the root and nothing else, and a reader deserves to be
/// told that before they read the edge counts as "this model has no
/// dependencies".
fn root_present(graph: &ResourceGraph, key: &ResourceKey) -> bool {
    graph.contains(key)
}

fn write_report(
    out: &mut dyn Write,
    graph: &ResourceGraph,
    store: &spore_assets::ContentStore,
    key: &ResourceKey,
    reached: &[spore_assets::TraversalEntry],
    request: &GraphRequest,
) -> Result<(), ToolError> {
    let loaded = store.package_names().join(", ");
    writeln!(out, "root          {key}")?;
    writeln!(out, "packages      {loaded}")?;
    writeln!(out, "root present  {}", yes_no(root_present(graph, key)))?;
    writeln!(
        out,
        "records       {} distinct identities in the loaded package(s)",
        graph.known_records()
    )?;
    let built: Vec<&str> = EdgeKind::ALL
        .iter()
        .filter(|kind| graph.options().builds(**kind))
        .map(|kind| kind.as_str())
        .collect();
    writeln!(
        out,
        "edge kinds    {}",
        if built.is_empty() {
            "none built (--kinds named no buildable kind)".to_owned()
        } else {
            built.join(", ")
        }
    )?;
    writeln!(out, "depth         {}", request.depth)?;
    writeln!(
        out,
        "followed      {}",
        if request.kinds.is_empty() {
            "every built kind".to_owned()
        } else {
            request
                .kinds
                .iter()
                .map(|kind| kind.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        }
    )?;

    writeln!(out)?;
    writeln!(
        out,
        "edge summary  (declared is counted before de-duplication)"
    )?;
    writeln!(
        out,
        "  {:<20} {:>9} {:>7} {:>9}  evidence",
        "kind", "declared", "edges", "dangling"
    )?;
    let summaries = graph.edge_summary();
    for row in &summaries {
        writeln!(
            out,
            "  {:<20} {:>9} {:>7} {:>9}  {}",
            row.kind.as_str(),
            row.declared,
            row.edges,
            row.dangling,
            row.level
        )?;
    }
    if summaries.is_empty() {
        writeln!(
            out,
            "  (no edge kind was built, so there is nothing to summarise \u{2014} this is not a \
             measurement of zero)"
        )?;
    }

    writeln!(out)?;
    writeln!(out, "traversal")?;
    writeln!(
        out,
        "  breadth-first, ascending by (type, group, instance); {} record(s) within depth {}",
        reached.len(),
        request.depth
    )?;
    // `0` means every row, the same sentinel `list`, `stats` and `diff` use.
    let shown = if request.limit == 0 {
        reached.len()
    } else {
        request.limit.min(reached.len())
    };
    for entry in reached.iter().take(shown) {
        let note = if graph.contains(&entry.key) {
            ""
        } else {
            "  DANGLING: not in any loaded package"
        };
        writeln!(out, "  {}  {}{note}", entry.depth, entry.key.to_tgi())?;
    }
    if shown < reached.len() {
        writeln!(
            out,
            "  ({} further record(s) within depth, not shown \u{2014} raise --limit or pass 0 for all)",
            reached.len() - shown
        )?;
    }

    // The two sections below take the *request's* limit, not the traversal's
    // `shown`: they are separate lists, each capped independently.
    write_dangling(out, graph, request.limit)?;
    write_unresolved(out, graph, request.limit)
}

/// Prints the dangling references per edge kind, as counts plus a sample.
///
/// Counts are always printed even when the sample is capped: "how many" is the
/// question a caller has, and a truncated sample must not be mistaken for the
/// whole set. A kind that cannot dangle says so instead of printing `0`.
fn write_dangling(
    out: &mut dyn Write,
    graph: &ResourceGraph,
    limit: usize,
) -> Result<(), ToolError> {
    writeln!(out)?;
    writeln!(
        out,
        "dangling references  (a reference naming a record in no loaded package)"
    )?;
    let counts = graph.missing_reference_counts();
    if counts.is_empty() {
        writeln!(
            out,
            "  no edge kind was built, so nothing was looked for \u{2014} this is not a count of zero"
        )?;
        return Ok(());
    }
    let total: usize = counts.values().sum();
    for kind in EdgeKind::ALL {
        if !graph.options().builds(kind) {
            continue;
        }
        let count = counts.get(&kind).copied().unwrap_or(0);
        let mutating = if kind.is_declared_reference() {
            "declared by a record, so this is expected when the referring package is not loaded"
        } else {
            "adjacency, not a named reference, so this would mean the graph is inconsistent"
        };
        writeln!(out, "  {:<20} {count:>9}  {mutating}", kind.as_str())?;
        if !kind.is_declared_reference() && count > 0 {
            writeln!(
                out,
                "  {:<20} ^ this kind cannot dangle by construction; the count above is a defect",
                ""
            )?;
        }
        // `0` means every row, the same sentinel `list`, `stats` and `diff` use.
        // Reading it as "no rows" here would make `--limit 0` print the counts
        // and none of the evidence behind them.
        let shown = if limit == 0 { count } else { limit.min(count) };
        for dangling in graph.dangling_of_kind(kind).iter().take(shown) {
            writeln!(out, "      {}", dangling_text(dangling))?;
        }
        if shown < count {
            writeln!(
                out,
                "      ({} further dangling {kind} reference(s), not shown)",
                count - shown
            )?;
        }
    }
    writeln!(out, "  total          {total:>9}")?;
    Ok(())
}

/// One dangling reference as a line.
fn dangling_text(dangling: &DanglingReference) -> String {
    format!("{} -> {}", dangling.from.to_tgi(), dangling.to.to_tgi())
}

/// Prints the records that contributed no edges, grouped by skip reason.
///
/// This is the section that keeps "we did not look" apart from "we looked and
/// found nothing": a record listed here had its reference tables unread, so its
/// absence from the edge counts is *not* evidence that it has no dependencies.
fn write_unresolved(
    out: &mut dyn Write,
    graph: &ResourceGraph,
    limit: usize,
) -> Result<(), ToolError> {
    let unresolved = graph.unresolved();
    writeln!(out)?;
    if unresolved.is_empty() {
        writeln!(
            out,
            "unresolved    none: every record the built kinds could read was read"
        )?;
        return Ok(());
    }
    let mut by_reason: BTreeMap<&str, Vec<spore_assets::UnresolvedRecord>> = BTreeMap::new();
    for record in unresolved {
        by_reason
            .entry(record.reason.as_str())
            .or_default()
            .push(record.clone());
    }
    writeln!(
        out,
        "unresolved    {} record(s) contributed no edges; their absence from the edge counts is \
         NOT evidence that they have no dependencies",
        unresolved.len()
    )?;
    for (reason, records) in &by_reason {
        writeln!(out, "  {reason:<28} {}", records.len())?;
        // `0` means every row, the same sentinel every other section uses.
        let shown = if limit == 0 {
            records.len()
        } else {
            limit.min(records.len())
        };
        for record in records.iter().take(shown) {
            writeln!(
                out,
                "      {}  {}",
                record.key.to_tgi(),
                skip_detail(record)
            )?;
        }
        if shown < records.len() {
            writeln!(
                out,
                "      ({} further record(s) with this reason, not shown)",
                records.len() - shown
            )?;
        }
    }
    Ok(())
}

/// The decoder's own words for a skip, when it carried any.
///
/// A skip reason with no detail is a *finding* (`NoTextureSetDeclared`), so it
/// prints nothing rather than an empty parenthetical.
fn skip_detail(record: &spore_assets::UnresolvedRecord) -> String {
    match &record.reason {
        SkipReason::WalkFailed { detail }
        | SkipReason::PartialTextureSet { detail }
        | SkipReason::ReferenceTableUnreadable { detail } => detail.clone(),
        SkipReason::NoTextureSetDeclared => {
            "the record decoded and its material-info table declares no texture set".to_owned()
        }
    }
}

/// The `--dependants` section: which records name this one.
fn write_dependants(
    out: &mut dyn Write,
    graph: &ResourceGraph,
    key: &ResourceKey,
    limit: usize,
) -> Result<(), ToolError> {
    let dependants = graph.dependants(key);
    writeln!(out, "dependants")?;
    if dependants.is_empty() {
        // Empty is ambiguous on its own \u{2014} "no record names it" and "it is
        // not here" \u{2014} so both are stated.
        writeln!(
            out,
            "  no record in the loaded package(s) names {key}{}",
            if root_present(graph, key) {
                ""
            } else {
                " (and the root is itself absent from the loaded packages)"
            }
        )?;
        return Ok(());
    }
    writeln!(out, "  {} record(s) name it", dependants.len())?;
    let shown = if limit == 0 {
        dependants.len()
    } else {
        limit.min(dependants.len())
    };
    for edge in dependants.iter().take(shown) {
        writeln!(out, "      {}", edge.describe())?;
    }
    if shown < dependants.len() {
        writeln!(
            out,
            "  ({} further dependant(s), not shown)",
            dependants.len() - shown
        )?;
    }
    writeln!(out)?;
    Ok(())
}

/// `true` / `false`, spelled as words.
fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

/// The `--json` document: one object, fixed key order.
fn summary_json(
    graph: &ResourceGraph,
    key: &ResourceKey,
    reached: &[spore_assets::TraversalEntry],
) -> Json {
    let mut edges = Vec::new();
    for row in graph.edge_summary() {
        edges.push(
            Json::obj()
                .push("kind", row.kind.as_str())
                .push("declared", Json::from(row.declared))
                .push("edges", Json::from(row.edges))
                .push("dangling", Json::from(row.dangling))
                .push("evidence_level", row.level.as_str())
                .push(
                    "declared_reference",
                    Json::from(row.kind.is_declared_reference()),
                )
                .push("built", Json::from(true)),
        );
    }
    // Kinds that were not built are listed too, so a reader never has to infer
    // "absent" from a missing array element.
    for kind in EdgeKind::ALL {
        if graph.options().builds(kind) {
            continue;
        }
        edges.push(
            Json::obj()
                .push("kind", kind.as_str())
                .push("declared", Json::Null)
                .push("edges", Json::Null)
                .push("dangling", Json::Null)
                .push("evidence_level", kind.evidence_level().as_str())
                .push(
                    "declared_reference",
                    Json::from(kind.is_declared_reference()),
                )
                .push("built", Json::from(false)),
        );
    }

    let mut dangling: Vec<Json> = Vec::new();
    for (kind, count) in graph.missing_reference_counts() {
        dangling.push(
            Json::obj()
                .push("kind", kind.as_str())
                .push("count", Json::from(count)),
        );
    }

    let mut unresolved: Vec<Json> = Vec::new();
    for record in graph.unresolved() {
        unresolved.push(
            Json::obj()
                .push("record", record.key.to_tgi())
                .push("reason", record.reason.as_str())
                .push("detail", skip_detail(record)),
        );
    }

    Json::obj()
        .push("root", key.to_tgi())
        .push("root_present", Json::from(root_present(graph, key)))
        .push("known_records", Json::from(graph.known_records()))
        .push("edges", Json::Arr(edges))
        .push(
            "traversal",
            Json::Arr(
                reached
                    .iter()
                    .map(|entry| {
                        Json::obj()
                            .push("record", entry.key.to_tgi())
                            .push("depth", Json::from(entry.depth))
                            .push("present", Json::from(graph.contains(&entry.key)))
                    })
                    .collect(),
            ),
        )
        .push("dangling_by_kind", Json::Arr(dangling))
        .push("unresolved", Json::Arr(unresolved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use spore_assets::{Package, UnresolvedRecord};

    /// Builds an empty graph with the given options, so the option mapping can
    /// be checked without a package on disk.
    fn graph_with(kinds: &[EdgeKind]) -> ResourceGraph {
        let mut store = spore_assets::ContentStore::new();
        store.push(Package::from_vec("empty", minimal_package()).unwrap());
        ResourceGraph::build_with(&store, options_for(kinds))
    }

    /// A one-record package, so the store is not empty.
    fn minimal_package() -> Vec<u8> {
        let mut image = vec![0u8; 96];
        image[0..4].copy_from_slice(b"DBPF");
        image[4..8].copy_from_slice(&3u32.to_le_bytes());
        image[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
        image[0x40..0x44].copy_from_slice(&96u32.to_le_bytes());
        image.extend_from_slice(&0u32.to_le_bytes());
        image.extend_from_slice(&spore_core::record::type_id::PROP.to_le_bytes());
        image.extend_from_slice(&1u32.to_le_bytes());
        image.extend_from_slice(&2u32.to_le_bytes());
        image.extend_from_slice(&124u32.to_le_bytes());
        image.extend_from_slice(&(4u32 | 0x8000_0000).to_le_bytes());
        image.extend_from_slice(&4u32.to_le_bytes());
        image.extend_from_slice(&0u16.to_le_bytes());
        image.push(0);
        image.push(0);
        image.extend_from_slice(&[0xAB; 4]);
        image
    }

    #[test]
    fn no_kinds_list_means_the_asset_layers_own_default() {
        // Not `GraphOptions::all()`: switching on sibling adjacency behind the
        // caller's back would change what a default report means.
        let options = options_for(&[]);
        assert_eq!(options, GraphOptions::default());
        assert!(!options.sibling_records);
        assert!(options.texture_edges && options.reference_edges);
    }

    #[test]
    fn a_kinds_list_selects_what_is_built_not_only_what_is_followed() {
        let options = options_for(&[EdgeKind::GmdlTexture]);
        assert!(options.texture_edges);
        assert!(!options.reference_edges);
        assert!(!options.model_texture_sets);
        assert!(!options.sibling_records);

        let all = options_for(&EdgeKind::ALL);
        assert_eq!(all, GraphOptions::all());
    }

    #[test]
    fn the_default_kind_list_builds_everything_except_sibling_adjacency() {
        let graph = graph_with(&[]);
        assert_eq!(
            graph.edge_summary().len(),
            3,
            "the three default kinds are built"
        );
        assert_eq!(graph.edge_count(), 0);
        assert_eq!(
            graph.missing_reference_counts().len(),
            3,
            "a built kind is always counted, at 0 when nothing dangled"
        );
    }

    #[test]
    fn a_kind_that_cannot_dangle_reports_zero_without_a_defect_note() {
        let graph = graph_with(&[]);
        for kind in [
            EdgeKind::ModelTextureSet,
            EdgeKind::SiblingRecord,
            EdgeKind::GmdlTexture,
            EdgeKind::GmdlReference,
        ] {
            assert!(
                !kind.is_declared_reference()
                    || kind == EdgeKind::GmdlTexture
                    || kind == EdgeKind::GmdlReference,
                "the declaration flag must match the documented mechanism"
            );
        }
        // The property the report relies on: adjacency kinds cannot dangle, so a
        // non-zero count there would be a defect the report says so about.
        assert!(!EdgeKind::ModelTextureSet.is_declared_reference());
        assert!(!EdgeKind::SiblingRecord.is_declared_reference());
        assert!(graph.dangling_of_kind(EdgeKind::ModelTextureSet).is_empty());
        assert!(graph.dangling_of_kind(EdgeKind::SiblingRecord).is_empty());
    }

    #[test]
    fn a_skip_reason_without_a_detail_still_explains_itself() {
        let declared = UnresolvedRecord {
            key: ResourceKey::new(1, 2, 3),
            reason: SkipReason::NoTextureSetDeclared,
        };
        assert!(skip_detail(&declared).contains("declares no texture set"));

        let failed = UnresolvedRecord {
            key: ResourceKey::new(1, 2, 3),
            reason: SkipReason::WalkFailed {
                detail: "gmdl: empty input".to_owned(),
            },
        };
        assert_eq!(skip_detail(&failed), "gmdl: empty input");

        let table = UnresolvedRecord {
            key: ResourceKey::new(1, 2, 3),
            reason: SkipReason::ReferenceTableUnreadable {
                detail: "short header".to_owned(),
            },
        };
        assert_eq!(skip_detail(&table), "short header");
    }

    #[test]
    fn a_dangling_reference_names_both_ends_in_the_canonical_spelling() {
        let dangling = DanglingReference {
            kind: EdgeKind::GmdlTexture,
            from: ResourceKey::new(0x00e6_bce5, 0x4061_6201, 1),
            to: ResourceKey::new(0x2f4e_681c, 0x4066_2900, 2),
        };
        let text = dangling_text(&dangling);
        assert_eq!(
            text,
            "0x00e6bce5:0x40616201:0x00000001 -> 0x2f4e681c:0x40662900:0x00000002"
        );
    }

    #[test]
    fn a_zero_edge_graph_still_names_the_records_the_index_holds() {
        let graph = graph_with(&[]);
        assert_eq!(graph.known_records(), 1);
        let key = ResourceKey::new(spore_core::record::type_id::PROP, 1, 2);
        assert!(root_present(&graph, &key));
        let absent = ResourceKey::new(1, 2, 3);
        assert!(!root_present(&graph, &absent));
    }
}
