//! The graph against a model: a plain in-memory tree. After every edit, the
//! graph's blocks, order, parents and texts must match the model's.

mod common;

use common::{Op, op};
use proptest::collection::vec;
use proptest::prelude::*;
use quiddity_core::syntax::nfc;
use quiddity_core::{BlockId, Graph};

#[derive(Debug, Default)]
struct Model {
    text: Vec<String>,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    roots: Vec<usize>,
}

impl Model {
    fn live(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack: Vec<usize> = self.roots.iter().rev().copied().collect();
        while let Some(n) = stack.pop() {
            out.push(n);
            stack.extend(self.children[n].iter().rev());
        }
        out
    }

    fn siblings(&mut self, parent: Option<usize>) -> &mut Vec<usize> {
        match parent {
            Some(p) => &mut self.children[p],
            None => &mut self.roots,
        }
    }

    /// Whether `a` is `b` or one of its ancestors.
    fn encloses(&self, a: usize, mut b: usize) -> bool {
        loop {
            if a == b {
                return true;
            }
            match self.parent[b] {
                Some(p) => b = p,
                None => return false,
            }
        }
    }

    fn create(&mut self, parent: Option<usize>, at: usize, text: String) -> usize {
        let n = self.text.len();
        self.text.push(text);
        self.parent.push(parent);
        self.children.push(Vec::new());
        self.siblings(parent).insert(at, n);
        n
    }

    fn detach(&mut self, n: usize) {
        let parent = self.parent[n];
        self.siblings(parent).retain(|&c| c != n);
    }
}

proptest! {
    #![proptest_config(common::config(128))]

    #[test]
    fn graph_behaves_like_a_tree(ops in vec(op(), 1..40)) {
        let graph = Graph::new();
        let mut model = Model::default();
        let mut ids: Vec<BlockId> = Vec::new();

        let mut ever: Vec<usize> = Vec::new();
        for op in &ops {
            let before = graph.version();
            // The blocks this op made or changed the text of, which is what
            // `text_edits_since` should name.
            let mut touched: Vec<usize> = Vec::new();
            let live = model.live();
            let pick = |i: &proptest::sample::Index| (!live.is_empty()).then(|| live[i.index(live.len())]);
            match op {
                Op::Create { parent, at, text } => {
                    let parent = parent.as_ref().and_then(pick);
                    let at = at.index(model.siblings(parent).len() + 1);
                    let id = graph.create_block(parent.map(|p| ids[p]), Some(at), text).unwrap();
                    let made = model.create(parent, at, nfc(text).into_owned());
                    // Making a block with no text writes nothing to its text.
                    if !nfc(text).is_empty() {
                        touched.push(made);
                    }
                    ids.push(id);
                }
                Op::Move { block, parent, at } => {
                    let Some(block) = pick(block) else { continue };
                    let parent = parent.as_ref().and_then(pick);
                    // The index is the block's final position among its
                    // new siblings, not counting itself.
                    let others = model.siblings(parent).iter().filter(|&&c| c != block).count();
                    let at = at.index(others + 1);
                    let result = graph.move_block(ids[block], parent.map(|p| ids[p]), Some(at));
                    if parent.is_some_and(|p| model.encloses(block, p)) {
                        prop_assert!(result.is_err(), "moving a block under itself must fail");
                        continue;
                    }
                    result.unwrap();
                    model.detach(block);
                    model.siblings(parent).insert(at, block);
                    model.parent[block] = parent;
                }
                Op::Delete { block } => {
                    let Some(block) = pick(block) else { continue };
                    graph.delete_block(ids[block]).unwrap();
                    model.detach(block);
                }
                Op::SetText { block, text } => {
                    let Some(block) = pick(block) else { continue };
                    graph.set_text(ids[block], text).unwrap();
                    let text = &nfc(text).into_owned();
                    if model.text[block] != *text {
                        touched.push(block);
                    }
                    model.text[block].clone_from(text);
                }
            }
            graph.commit();

            let expected: Vec<_> = model
                .live()
                .into_iter()
                .map(|n| (ids[n], model.parent[n].map(|p| ids[p]), model.text[n].clone()))
                .collect();
            prop_assert_eq!(common::layout(&graph), expected.clone(), "after {:?}", op);
            // The bulk read agrees with asking for each block in turn.
            let scanned: Vec<_> = graph
                .scan()
                .unwrap()
                .into_iter()
                .map(|b| (b.id, b.parent, b.text))
                .collect();
            prop_assert_eq!(scanned, expected, "scan after {:?}", op);

            ever.extend(touched.iter().copied());
            let live: std::collections::BTreeSet<_> = model.live().into_iter().collect();
            let want: std::collections::BTreeSet<_> =
                touched.into_iter().filter(|n| live.contains(n)).map(|n| ids[n]).collect();
            let named: std::collections::BTreeSet<_> =
                graph.text_edits_since(&before).unwrap().into_iter().map(|(b, _)| b).collect();
            prop_assert_eq!(named, want, "text edits after {:?}", op);

            // And over the whole history at once, where a block edited and
            // then deleted must not be named.
            let ever_live: std::collections::BTreeSet<_> =
                ever.iter().filter(|n| live.contains(n)).map(|&n| ids[n]).collect();
            let since_start: std::collections::BTreeSet<_> = graph
                .text_edits_since(&quiddity_core::Version::default())
                .unwrap()
                .into_iter()
                .map(|(b, _)| b)
                .collect();
            prop_assert_eq!(since_start, ever_live, "text edits since the start after {:?}", op);
        }
    }
}
