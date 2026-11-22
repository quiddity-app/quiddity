//! Journals written on devices that were offline from each other, merged in
//! arbitrary orders.
#![expect(clippy::unwrap_used, reason = "a test failing to set up should panic")]

mod common;

use common::{sync, sync_all};
use jiff::civil::Date;
use proptest::collection::vec;
use proptest::prelude::*;
use quiddity_core::files::export_dir;
use quiddity_core::{Graph, Identity, Index, ensure_journal, merge_duplicates};

const REPLICAS: usize = 3;

fn dates() -> [Date; 2] {
    ["2026-10-25".parse().unwrap(), "2026-10-26".parse().unwrap()]
}

#[derive(Debug, Clone)]
enum Step {
    /// Write to the day's journal, as the app would: find it through the
    /// replica's own index, creating it if that index has none.
    Write {
        replica: usize,
        day: usize,
    },
    Sync(usize, usize),
    Merge(usize),
}

fn step() -> impl Strategy<Value = Step> {
    prop_oneof![
        4 => (0..REPLICAS, 0..2usize).prop_map(|(replica, day)| Step::Write { replica, day }),
        2 => (0..REPLICAS, 0..REPLICAS).prop_map(|(a, b)| Step::Sync(a, b)),
        2 => (0..REPLICAS).prop_map(Step::Merge),
    ]
}

proptest! {
    #![proptest_config(common::config(64))]

    #[test]
    fn journals_merge_without_losing_writes(steps in vec(step(), 1..40)) {
        let replicas: Vec<Graph> = (0..REPLICAS).map(|_| Graph::blank()).collect();
        let mut written: [Vec<String>; 2] = Default::default();
        for step in &steps {
            match *step {
                Step::Write { replica, day } => {
                    let graph = &replicas[replica];
                    let journal = ensure_journal(graph, &Index::build(graph).unwrap(), dates()[day]).unwrap();
                    let text = format!("entry {day}.{}", written[day].len());
                    graph.create_block(Some(journal), None, &text).unwrap();
                    graph.commit();
                    written[day].push(text);
                }
                Step::Sync(a, b) => sync(&replicas[a], &replicas[b]).unwrap(),
                Step::Merge(r) => {
                    let graph = &replicas[r];
                    let before = common::layout(graph);
                    let moved = merge_duplicates(graph, &Index::build(graph).unwrap()).unwrap();
                    // The node relies on this to know whether to tell peers.
                    prop_assert_eq!(moved > 0, common::layout(graph) != before);
                }
            }
        }

        // Before merging is finished, an export still has every write, in
        // whichever duplicate it is.
        let dir = tempfile::tempdir().unwrap();
        export_dir(&replicas[0], dir.path()).unwrap();
        let mut exported = String::new();
        if let Ok(files) = std::fs::read_dir(dir.path().join("journals")) {
            for file in files {
                exported += &std::fs::read_to_string(file.unwrap().path()).unwrap();
            }
        }
        let held: Vec<String> = replicas[0].blocks().into_iter().map(|b| replicas[0].text(b).unwrap()).collect();
        for text in written.iter().flatten().filter(|t| held.contains(t)) {
            prop_assert!(exported.contains(&format!("- {text}\n")), "{} missing from\n{}", text, exported);
        }

        // As the app does after every sync: merge until there's nothing left.
        for _ in 0..4 {
            sync_all(&replicas).unwrap();
            let mut moved = 0;
            for graph in &replicas {
                moved += merge_duplicates(graph, &Index::build(graph).unwrap()).unwrap();
            }
            if moved == 0 {
                break;
            }
        }
        sync_all(&replicas).unwrap();

        let expected = common::layout(&replicas[0]);
        for graph in &replicas {
            prop_assert_eq!(&common::layout(graph), &expected);
            let index = Index::build(graph).unwrap();
            for (day, date) in dates().into_iter().enumerate() {
                let visible = index.entities().filter(|e| e.identity() == Some(Identity::Journal(date))).count();
                prop_assert_eq!(visible, usize::from(!written[day].is_empty()));
                let Some(journal) = index.journal(date) else { continue };
                let props = graph.properties_block(journal).unwrap();
                let mut texts: Vec<_> = graph
                    .children(journal)
                    .into_iter()
                    .filter(|&b| Some(b) != props)
                    .map(|b| graph.text(b).unwrap())
                    .collect();
                texts.sort();
                let mut want = written[day].clone();
                want.sort();
                prop_assert_eq!(texts, want);
            }
        }

        // Fully merged duplicates are left out of exports: one file per day.
        let dir = tempfile::tempdir().unwrap();
        export_dir(&replicas[0], &dir.path().join("out")).unwrap();
        let files = std::fs::read_dir(dir.path().join("out/journals")).unwrap().count();
        prop_assert_eq!(files, written.iter().filter(|w| !w.is_empty()).count());
    }
}
