//! Times the whole-graph costs on a made-up graph about the size of a large
//! Logseq import, so a change that makes them worse shows. Run it with
//! `cargo run --release -p quiddity-view --example scale [pages] [blocks per page] [save snapshot to]`.
//! The defaults (3,150 pages of 11 blocks each) make a 3.7 MB snapshot.

use std::time::Instant;

use quiddity_core::Graph;
use quiddity_view::View;

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    println!(
        "{what:<28}{:>9.1} ms",
        start.elapsed().as_secs_f64() * 1000.0
    );
    out
}

fn argument(n: usize, default: usize) -> usize {
    std::env::args()
        .nth(n)
        .and_then(|a| a.parse().ok())
        .unwrap_or(default)
}

/// Seven pages in ten are journals; each has a properties block and
/// `per_page` blocks with a child, linking to other pages.
fn graph(pages: usize, per_page: usize) -> Graph {
    let graph = Graph::new();
    for page in 0..pages {
        let name = if page % 10 < 7 {
            format!("2020-{:02}-{:02} #{page}", page % 12 + 1, page % 28 + 1)
        } else {
            format!("Topic {page}")
        };
        let root = graph.create_block(None, None, &name).unwrap();
        let props = format!("type:: note\nalias:: Alias {page}");
        graph.create_block(Some(root), None, &props).unwrap();
        for b in 0..per_page {
            let text = format!(
                "Block {b} of page {page} mentioning [[Topic {}]] and [[Alias {}]] with some ordinary words to search for #tag{}",
                (page * 7 + b) % pages,
                (page * 13 + b) % pages,
                b % 5
            );
            let parent = graph.create_block(Some(root), None, &text).unwrap();
            graph
                .create_block(Some(parent), None, "a nested child with words")
                .unwrap();
        }
    }
    graph.commit();
    graph
}

fn main() {
    let (pages, per_page) = (argument(1, 3150), argument(2, 5));
    let snapshot = graph(pages, per_page).snapshot().unwrap();
    // A third argument saves the snapshot, to time the WASM build on it.
    if let Some(path) = std::env::args().nth(3) {
        std::fs::write(path, &snapshot).unwrap();
    }
    println!(
        "{pages} pages, snapshot {:.1} MB",
        snapshot.len() as f64 / 1e6
    );

    let loaded = timed("Graph::from_snapshot", || {
        Graph::from_snapshot(&snapshot).unwrap()
    });
    let view = timed("View::new (first index)", || View::new(loaded).unwrap());
    view.graph().enable_undo();
    let typed_in = view.graph().children(view.graph().roots()[0])[1].to_string();

    timed("entities()", || view.entities().unwrap());
    timed("summaries()", || view.summaries().unwrap());
    timed("connections()", || view.connections().unwrap());
    timed("journals()", || {
        view.journals("2030-01-01".parse().unwrap()).unwrap()
    });
    timed("search_titles()", || {
        view.search_titles("topic 12", 20).unwrap()
    });
    timed("search_text()", || {
        view.search_text("ordinary words", 20).unwrap()
    });

    println!("-- after typing one character");
    timed("splice_text()", || {
        view.splice_text(&typed_in, 0, 0, "x").unwrap();
    });
    timed("first query (new index)", || {
        view.search_titles("a", 5).unwrap()
    });
    timed("summaries()", || view.summaries().unwrap());
    timed("search_text()", || {
        view.search_text("ordinary words", 20).unwrap()
    });

    println!("-- after an import (everything read again)");
    view.changed();
    timed("first query (new index)", || {
        view.search_titles("a", 5).unwrap()
    });
}
