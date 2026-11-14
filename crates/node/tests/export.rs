//! Blobs in an export: written to `assets/`, and read back.

mod common;

use std::fs;
use std::path::Path;
use std::process::Command;

use quiddity_core::{Asset, Graph, Index};
use quiddity_node::blobs::{Blobs, Exported, Hash, Imported};

fn describe(graph: &Graph, name: &str, bytes: &[u8]) -> Asset {
    let asset = Asset::new(
        &Hash::new(bytes).to_string(),
        "application/octet-stream",
        bytes.len() as u64,
        Some(name),
    )
    .unwrap();
    let page = graph.create_block(None, None, name).unwrap();
    graph.create_blob(page, &asset).unwrap();
    graph.commit();
    asset
}

fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Each file is written once, named for its hash and what it is, with its
/// own bytes; one this device lacks is counted and left out.
#[tokio::test]
async fn export_writes_each_file_once_and_counts_what_is_missing() {
    let (store, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let blobs = Blobs::open(store.path()).await.unwrap();
    let graph = Graph::new();
    let cat = describe(&graph, "cat.png", b"cat bytes");
    // The same bytes under another name make another entity, one file.
    describe(&graph, "copy of cat.png", b"cat bytes");
    describe(&graph, "away.txt", b"not here");
    let notes = describe(&graph, "notes", b"");
    blobs.put(&b"cat bytes"[..]).await.unwrap();
    blobs.put(&b""[..]).await.unwrap();

    let done = blobs.export_assets(&graph, out.path()).await.unwrap();
    assert_eq!(
        done,
        Exported {
            written: 2,
            missing: 1
        }
    );
    let files = listing(&out.path().join("assets"));
    assert_eq!(files.len(), 2, "{files:?}");
    assert_eq!(
        fs::read(out.path().join("assets").join(cat.file_name())).unwrap(),
        b"cat bytes"
    );
    assert_eq!(
        fs::read(out.path().join("assets").join(notes.file_name())).unwrap(),
        b""
    );
    blobs.shutdown().await.unwrap();
}

/// No assets, or none here, makes no `assets/` at all.
#[tokio::test]
async fn export_makes_no_directory_when_there_is_nothing_to_put_in_it() {
    let (store, out) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let blobs = Blobs::open(store.path()).await.unwrap();
    let graph = Graph::new();
    assert_eq!(
        blobs.export_assets(&graph, out.path()).await.unwrap(),
        Exported::default()
    );
    describe(&graph, "away.txt", b"not here");
    let done = blobs.export_assets(&graph, out.path()).await.unwrap();
    assert_eq!(
        done,
        Exported {
            written: 0,
            missing: 1
        }
    );
    assert!(!out.path().join("assets").exists());
    blobs.shutdown().await.unwrap();
}

/// Only a file named for the hash of what it holds is kept: a Logseq graph's
/// pictures, a damaged file and a stranger's are left where they are.
#[tokio::test]
async fn import_keeps_only_files_named_for_their_own_hash() {
    let (store, from) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let assets = from.path().join("assets");
    fs::create_dir_all(assets.join("a directory")).unwrap();
    let good = Hash::new(b"good");
    fs::write(assets.join(format!("{good}.txt")), b"good").unwrap();
    // The extension is only a convenience: the name's hash is what counts.
    let bare = Hash::new(b"bare");
    fs::write(assets.join(bare.to_string()), b"bare").unwrap();
    let damaged = Hash::new(b"what it should hold");
    fs::write(assets.join(format!("{damaged}.png")), b"what it does").unwrap();
    fs::write(assets.join("photo.png"), b"a logseq picture").unwrap();
    fs::write(
        assets.join(format!("{}.png", &good.to_string()[1..])),
        b"short",
    )
    .unwrap();

    let blobs = Blobs::open(store.path()).await.unwrap();
    let done = blobs.import_assets(from.path()).await.unwrap();
    assert_eq!(
        done,
        Imported {
            stored: 2,
            skipped: 3
        }
    );
    assert!(blobs.has(good).await.unwrap());
    assert!(blobs.has(bare).await.unwrap());
    assert!(!blobs.has(damaged).await.unwrap());
    assert!(!blobs.has(Hash::new(b"a logseq picture")).await.unwrap());
    // Again is the same.
    assert_eq!(blobs.import_assets(from.path()).await.unwrap(), done);
    // And a directory with no assets/ has nothing to say.
    let none = tempfile::tempdir().unwrap();
    assert_eq!(
        blobs.import_assets(none.path()).await.unwrap(),
        Imported::default()
    );
    blobs.shutdown().await.unwrap();
}

fn node(graph: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_quiddity-node"));
    command
        .arg("--graph")
        .arg(graph)
        .env("QUIDDITY_CONFIG_DIR", common::keys(graph));
    command
}

fn run(command: &mut Command) -> String {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{command:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// What a person would do: export a graph with files in it with the binary,
/// import the result into another, and find the same files and entities.
#[tokio::test]
async fn files_survive_an_export_and_an_import_with_the_command_line() {
    let tmp = tempfile::tempdir().unwrap();
    let (a, b, out) = (
        tmp.path().join("a"),
        tmp.path().join("b"),
        tmp.path().join("out"),
    );
    let bytes: Vec<u8> = (0..70_000u32).map(|i| (i % 241) as u8).collect();

    let store = common::store(&a);
    let graph = store.init(false).unwrap();
    let blobs = Blobs::open(&a).await.unwrap();
    let asset = describe(&graph, "data.bin", &bytes);
    describe(&graph, "elsewhere.txt", b"on another device");
    blobs.put(bytes.clone()).await.unwrap();
    store.save(&graph).unwrap();
    blobs.shutdown().await.unwrap();

    assert_eq!(
        run(node(&a).arg("export").arg(&out)),
        "exported 2 entities\nexported 1 files, 1 not on this device yet"
    );
    assert_eq!(
        fs::read(out.join("assets").join(asset.file_name())).unwrap(),
        bytes
    );
    // The entity's own file says what the file is, as text.
    let page = fs::read_to_string(out.join("entities/data.bin.md")).unwrap();
    assert!(page.contains(&asset.line()), "{page}");

    run(node(&b).arg("init"));
    assert_eq!(
        run(node(&b).arg("import").arg(&out)),
        "imported 2 entities\nimported 1 files"
    );
    let loaded = common::store(&b).load().unwrap();
    let index = Index::build(&loaded).unwrap();
    let mut assets: Vec<_> = index.assets().map(|(_, a)| a).collect();
    assets.sort_by(|x, y| x.name.cmp(&y.name));
    assert_eq!(assets.len(), 2);
    assert_eq!(assets[0], asset);

    let blobs = Blobs::open(&b).await.unwrap();
    assert_eq!(blobs.get(Hash::new(&bytes)).await.unwrap().to_vec(), bytes);
    assert!(!blobs.has(Hash::new(b"on another device")).await.unwrap());
    blobs.shutdown().await.unwrap();
}
