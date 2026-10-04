//! Structural checks over a decoded document.
//!
//! # Validation reports, it does not repair
//!
//! Every issue carries a [`Severity`] and a path into the document. Nothing here
//! rewrites a document, drops a block, or fills a hole: a creator document is a
//! record of what the game stored, and a validator that "fixes" one is a second
//! answer to the same record.
//!
//! # What is checked, and why each one earns its place
//!
//! | check | because |
//! |---|---|
//! | child index in range | 601 corpus edges are all in range; an out-of-range one is a finding, not a clamp |
//! | no self-parent | 0 corpus self-edges |
//! | no cycle | 0 corpus cycles; the forest claim rests on it |
//! | in-degree ≤ 1 | max in-degree is 1 corpus-wide |
//! | declared roots | 79 roots over 68 documents — a document with none is malformed, a document with six is normal |
//! | symmetry reciprocity | reciprocal in 84 of 84 corpus relations |
//! | symmetry in range | an index that names nothing is a finding |
//! | paint presence vs family | `crt` has none in all 47 documents; finding one is a finding |
//! | handles non-empty | a written-but-empty `<handles>` is distinct from an absent one |
//!
//! A document that trips `Error` is not loadable into a runtime scene. A document
//! that trips `Warning` loads, and the warning travels with it.

use crate::document::{BlockIndex, CreatorDocument};
use crate::sporemodel::Family;

/// How much an issue matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// The document contradicts itself and cannot be composed.
    Error,
    /// The document is composable and something about it is worth saying.
    Warning,
}

/// One thing wrong with a document, located.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// How much it matters.
    pub severity: Severity,
    /// A closed-vocabulary code, safe to match on.
    pub code: &'static str,
    /// Which block, when the issue is about one.
    pub block: Option<BlockIndex>,
    /// A human-readable detail. Never a substitute for `code`.
    pub detail: String,
}

impl ValidationIssue {
    fn error(code: &'static str, block: Option<BlockIndex>, detail: impl Into<String>) -> Self {
        Self {
            severity: Severity::Error,
            code,
            block,
            detail: detail.into(),
        }
    }

    fn warning(code: &'static str, block: Option<BlockIndex>, detail: impl Into<String>) -> Self {
        Self {
            severity: Severity::Warning,
            code,
            block,
            detail: detail.into(),
        }
    }
}

/// Every issue found, ordered by block then by discovery. Deterministic order is
/// required, not tidy: two runs over the same document must produce the same
/// list, or a test that asserts on it is a coin flip.
pub fn validate(document: &CreatorDocument, family: Option<Family>) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let count = document.blocks.len();

    for (index, block) in document.blocks.iter().enumerate() {
        if let Some(children) = &block.children {
            for &child in children {
                if child >= count {
                    issues.push(ValidationIssue::error(
                        "child_index_out_of_range",
                        Some(index),
                        format!("block {index} lists child {child}, but there are {count} blocks"),
                    ));
                }
                if child == index {
                    issues.push(ValidationIssue::error(
                        "self_parent",
                        Some(index),
                        format!("block {index} lists itself as a child"),
                    ));
                }
            }
            if children.is_empty() {
                issues.push(ValidationIssue::warning(
                    "empty_child_list",
                    Some(index),
                    format!(
                        "block {index} writes an empty <childlist>; the corpus writes none at all for a leaf"
                    ),
                ));
            }
        }

        if let Some(partner) = block.symmetric {
            if partner >= count {
                issues.push(ValidationIssue::error(
                    "symmetric_out_of_range",
                    Some(index),
                    format!("block {index} names mirror partner {partner}, out of range"),
                ));
            } else {
                // Reciprocity: the corpus holds it in 84 of 84 relations, so a
                // one-sided pairing is a finding about the record, not a shape
                // to normalise away.
                let reciprocal = document.blocks[partner].symmetric == Some(index);
                if !reciprocal {
                    issues.push(ValidationIssue::warning(
                        "symmetric_not_reciprocal",
                        Some(index),
                        format!(
                            "block {index} names mirror partner {partner}, which does not name it back"
                        ),
                    ));
                }
            }
        }

        if let Some(handles) = &block.handles {
            if handles.is_empty() {
                issues.push(ValidationIssue::warning(
                    "empty_handles",
                    Some(index),
                    format!("block {index} writes <handles> with no <weight> children"),
                ));
            }
        }

        if let Some(paint) = &block.paint {
            if paint.is_empty() {
                issues.push(ValidationIssue::warning(
                    "empty_paint_list",
                    Some(index),
                    format!("block {index} writes <paintlist> with no <paint> children"),
                ));
            }
            if let Some(family) = family {
                if !family.carries_paint() && !paint.is_empty() {
                    issues.push(ValidationIssue::warning(
                        "paint_in_a_family_without_paint",
                        Some(index),
                        format!(
                            "block {index} writes <paintlist>, which no {} document in the corpus does",
                            family.name()
                        ),
                    ));
                }
            }
        }
    }

    // In-degree: a block named as a child twice has two parents, which the
    // corpus never does.
    let mut in_degree = vec![0usize; count];
    for block in &document.blocks {
        for &child in block.child_indices() {
            if let Some(slot) = in_degree.get_mut(child) {
                *slot += 1;
            }
        }
    }
    for (index, &degree) in in_degree.iter().enumerate() {
        if degree > 1 {
            issues.push(ValidationIssue::error(
                "multiple_parents",
                Some(index),
                format!("block {index} is a child of {degree} blocks"),
            ));
        }
    }

    // Cycles: iterative colouring over the derived child edges.
    for index in cycle_starts(&in_degree, document) {
        issues.push(ValidationIssue::error(
            "cycle",
            Some(index),
            format!("block {index} is inside a parent cycle; the corpus forest has none"),
        ));
    }

    // Roots: derived, never stored, and a document with none has no composition.
    let roots = document.roots();
    if !count.eq(&0) && roots.is_empty() {
        issues.push(ValidationIssue::error(
            "no_root",
            None,
            "every block is somebody's child, so nothing can be attached to the world",
        ));
    }

    issues.sort_by(|a, b| {
        a.block
            .cmp(&b.block)
            .then_with(|| a.code.cmp(b.code))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    issues
}

/// The blocks that start a cycle, found without recursion.
fn cycle_starts(in_degree: &[usize], document: &CreatorDocument) -> Vec<BlockIndex> {
    // Colour 0 = unvisited, 1 = on the current chain, 2 = settled.
    let mut colour = vec![0u8; document.blocks.len()];
    let mut starts = Vec::new();
    for root in 0..document.blocks.len() {
        if colour[root] != 0 || in_degree.get(root).copied().unwrap_or(0) == 0 {
            continue;
        }
        let mut chain = Vec::new();
        let mut current = root;
        loop {
            if colour[current] == 1 {
                starts.push(current);
                break;
            }
            if colour[current] == 2 {
                break;
            }
            colour[current] = 1;
            chain.push(current);
            match document.parent_of(current) {
                Some(parent) if parent < colour.len() => current = parent,
                _ => break,
            }
        }
        for node in chain {
            colour[node] = 2;
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::UnitRgb;
    use crate::document::{Block, BlockId};

    fn block(id: u32) -> Block {
        Block {
            block_id: Some(BlockId::new(1, id)),
            ..Block::default()
        }
    }

    fn document(blocks: Vec<Block>) -> CreatorDocument {
        CreatorDocument {
            blocks,
            ..CreatorDocument::default()
        }
    }

    fn codes(issues: &[ValidationIssue]) -> Vec<&'static str> {
        issues.iter().map(|issue| issue.code).collect()
    }

    #[test]
    fn a_two_block_document_with_one_parent_is_clean() {
        let mut parent = block(1);
        parent.children = Some(vec![1]);
        let issues = validate(&document(vec![parent, block(2)]), None);
        assert!(issues.is_empty(), "{issues:?}");
    }

    #[test]
    fn a_leaf_that_writes_no_child_list_is_not_an_empty_list() {
        let issues = validate(&document(vec![block(1)]), None);
        assert!(!codes(&issues).contains(&"empty_child_list"));
    }

    #[test]
    fn an_out_of_range_child_is_an_error_not_a_clamp() {
        let mut parent = block(1);
        parent.children = Some(vec![9]);
        let issues = validate(&document(vec![parent]), None);
        assert!(codes(&issues).contains(&"child_index_out_of_range"));
        assert_eq!(issues[0].severity, Severity::Error);
        assert_eq!(document(vec![Block::default()]).roots().len(), 1);
    }

    #[test]
    fn a_self_parent_and_a_two_parent_block_are_both_errors() {
        let mut self_parent = block(1);
        self_parent.children = Some(vec![0]);
        assert!(codes(&validate(&document(vec![self_parent]), None)).contains(&"self_parent"));

        // Block 1 is a child of both 0 and 2: in-degree 2, which the corpus
        // never has.
        let mut first = block(1);
        let mut second = block(2);
        first.children = Some(vec![1]);
        second.children = Some(vec![1]);
        let middle = block(3);
        assert!(
            codes(&validate(&document(vec![first, middle, second]), None))
                .contains(&"multiple_parents")
        );
    }

    #[test]
    fn a_document_where_every_block_is_a_child_has_no_root_and_says_so() {
        let mut a = block(1);
        let mut b = block(2);
        a.children = Some(vec![1]);
        b.children = Some(vec![0]);
        let issues = validate(&document(vec![a, b]), None);
        assert!(codes(&issues).contains(&"no_root"));
        assert!(codes(&issues).contains(&"cycle"));
    }

    #[test]
    fn a_six_root_document_is_normal_not_a_warning() {
        let blocks = (0..6).map(block).collect();
        let issues = validate(&document(blocks), None);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(document((0..6).map(block).collect()).roots().len(), 6);
    }

    #[test]
    fn one_sided_symmetry_is_reported_and_a_reciprocal_pair_is_not() {
        let mut a = block(1);
        a.symmetric = Some(1);
        let issues = validate(&document(vec![a, block(2)]), None);
        assert!(codes(&issues).contains(&"symmetric_not_reciprocal"));

        let mut pair_a = block(1);
        let mut pair_b = block(2);
        pair_a.symmetric = Some(1);
        pair_b.symmetric = Some(0);
        assert!(validate(&document(vec![pair_a, pair_b]), None).is_empty());
    }

    #[test]
    fn paint_in_a_creature_document_is_reported_and_never_dropped() {
        let mut painted = block(1);
        painted.paint = Some(vec![crate::document::PaintRegion {
            region: 5,
            paint_id: 0xf596_7f30,
            color1: UnitRgb::new(0.0, 0.0, 0.0).expect("0.0 is in range"),
            color2: UnitRgb::new(1.0, 1.0, 1.0).expect("1.0 is in range"),
        }]);
        let issues = validate(&document(vec![painted]), Some(Family::Creature));
        assert!(codes(&issues).contains(&"paint_in_a_family_without_paint"));
        // Still valid, and still carrying the paint.
        assert_eq!(Severity::Warning, issues[0].severity);
    }

    #[test]
    fn validation_order_is_deterministic() {
        let mut a = block(1);
        a.symmetric = Some(7);
        let document = document(vec![a, block(2)]);
        let first = validate(&document, None);
        let second = validate(&document, None);
        assert_eq!(first, second);
    }

    #[test]
    fn an_empty_document_is_not_a_document_with_no_root() {
        assert!(validate(&document(Vec::new()), None).is_empty());
    }
}
