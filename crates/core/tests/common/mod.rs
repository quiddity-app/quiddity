//! Shared strategies and helpers for the core property tests.
#![allow(dead_code, reason = "each test binary uses a different part")]

use proptest::prelude::*;
use proptest::sample::Index;
use quiddity_core::{BlockId, Graph, Result};

/// Block text weighted towards what matters to the outline format and the
/// parser: newlines, tabs, dashes, fences, and blocks made only of
/// property lines (which become an entity's properties block when first).
///
/// Keys and values are too short to spell `journal` or `property`, so no
/// generated entity has an identity and gets merged.
pub fn text() -> impl Strategy<Value = String> + Clone {
    prop_oneof![
        2 => r"[-\t \n`a-z:#\[\]()\r]{0,16}",
        1 => any::<String>(),
        1 => property_lines(),
    ]
}

/// Text that is a properties block: one or more `key:: value` lines.
pub fn property_lines() -> impl Strategy<Value = String> + Clone {
    proptest::collection::vec(("[a-z]{1,5}", "[a-z ]{0,5}"), 1..3).prop_map(|kvs| {
        kvs.iter()
            .map(|(k, v)| format!("{k}:: {v}").trim_end().to_owned())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// An edit to a graph. Blocks are chosen by position among the live blocks
/// when the edit is applied, so any sequence of edits is meaningful.
#[derive(Debug, Clone)]
pub enum Op {
    Create {
        parent: Option<Index>,
        at: Index,
        text: String,
    },
    Move {
        block: Index,
        parent: Option<Index>,
        at: Index,
    },
    Delete {
        block: Index,
    },
    SetText {
        block: Index,
        text: String,
    },
}

pub fn op() -> impl Strategy<Value = Op> {
    let parent = proptest::option::weighted(0.8, any::<Index>());
    prop_oneof![
        4 => (parent.clone(), any::<Index>(), text())
            .prop_map(|(parent, at, text)| Op::Create { parent, at, text }),
        2 => (any::<Index>(), parent, any::<Index>())
            .prop_map(|(block, parent, at)| Op::Move { block, parent, at }),
        1 => any::<Index>().prop_map(|block| Op::Delete { block }),
        2 => (any::<Index>(), text()).prop_map(|(block, text)| Op::SetText { block, text }),
    ]
}

/// Applies an op, choosing blocks from `graph.blocks()`. Ops that make no
/// sense in the current state (no blocks yet, or a move that would make a
/// cycle) are refused by the graph and ignored here.
pub fn apply(graph: &Graph, op: &Op) {
    let blocks = graph.blocks();
    let pick = |i: &Index| (!blocks.is_empty()).then(|| blocks[i.index(blocks.len())]);
    let siblings = |parent: Option<BlockId>| match parent {
        Some(p) => graph.children(p).len(),
        None => graph.roots().len(),
    };
    drop(match op {
        Op::Create { parent, at, text } => {
            let parent = parent.as_ref().and_then(pick);
            let at = at.index(siblings(parent) + 1);
            graph.create_block(parent, Some(at), text).map(drop)
        }
        Op::Move { block, parent, at } => {
            let (Some(block), parent) = (pick(block), parent.as_ref().and_then(pick)) else {
                return;
            };
            let here = usize::from(graph.parent(block) == parent);
            let at = at.index(siblings(parent) - here + 1);
            graph.move_block(block, parent, Some(at))
        }
        Op::Delete { block } => match pick(block) {
            Some(b) => graph.delete_block(b),
            None => return,
        },
        Op::SetText { block, text } => match pick(block) {
            Some(b) => graph.set_text(b, text),
            None => return,
        },
    });
    graph.commit();
}

/// Brings `b` up to date with `a` and vice versa.
pub fn sync(a: &Graph, b: &Graph) -> Result<()> {
    a.commit();
    b.commit();
    b.import(&a.updates_since(&b.version())?)?;
    a.import(&b.updates_since(&a.version())?)?;
    Ok(())
}

/// Syncs every pair, enough times for everything to reach everyone.
pub fn sync_all(replicas: &[Graph]) -> Result<()> {
    for _ in 0..2 {
        for i in 0..replicas.len() {
            for j in i + 1..replicas.len() {
                sync(&replicas[i], &replicas[j])?;
            }
        }
    }
    Ok(())
}

/// The whole tree as plain data, built only from the graph's own accessors
/// (not the outline code), for comparing graphs.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Tree {
    pub text: String,
    pub children: Vec<Tree>,
}

pub fn tree(graph: &Graph, block: BlockId) -> Tree {
    Tree {
        text: graph.text(block).expect("live block has text"),
        children: graph
            .children(block)
            .into_iter()
            .map(|c| tree(graph, c))
            .collect(),
    }
}

pub fn forest(graph: &Graph) -> Vec<Tree> {
    graph.roots().into_iter().map(|r| tree(graph, r)).collect()
}

/// Every block with its parent and text, in document order. Unlike
/// [`forest`], this includes IDs, which are the same on every replica.
pub fn layout(graph: &Graph) -> Vec<(BlockId, Option<BlockId>, String)> {
    graph
        .blocks()
        .into_iter()
        .map(|b| {
            (
                b,
                graph.parent(b),
                graph.text(b).expect("live block has text"),
            )
        })
        .collect()
}

/// Proptest settings for the slower tests: `cases` by default, or
/// `PROPTEST_CASES` from the environment for a deeper run.
pub fn config(cases: u32) -> proptest::test_runner::Config {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|c| c.parse().ok())
        .unwrap_or(cases);
    proptest::test_runner::Config::with_cases(cases)
}
