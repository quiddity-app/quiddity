//! The `##blob` line: reading what it says, in the block that starts with it.

use proptest::prelude::*;
use quiddity_core::syntax::parse_meta_line;
use quiddity_core::{Asset, AssetError, BlockId, Graph, Index, Result};

const HASH: &str = "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f";

/// File names weighted towards what a meta-property parameter treats
/// specially, and what the line's own parts are separated by.
fn name() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            4 => "[a-z0-9]",
            2 => proptest::sample::select(vec![
                "(", ")", "[", "]", "[[", "]]", "\\", ",", "\"", "#", "#a", " ", "\t", ".", "é",
                "&", "|", "~", "\\)", "))", "((",
            ])
            .prop_map(str::to_owned),
        ],
        1..12,
    )
    .prop_map(|parts| parts.concat().trim_matches([' ', '\t']).to_owned())
    .prop_filter("a name has something in it", |n| !n.is_empty())
}

fn asset() -> impl Strategy<Value = Asset> {
    (
        "[0-9a-f]{64}",
        proptest::sample::select(vec!["image/png", "application/pdf", "text/x-c++", "a/b"]),
        any::<u64>(),
        proptest::option::of(name()),
    )
        .prop_map(|(hash, mime, size, name)| Asset {
            hash,
            mime: mime.to_owned(),
            size,
            name,
        })
}

proptest! {
    /// What is written reads back as the same asset, through the parser
    /// that finds where the line's parameter ends.
    #[test]
    fn a_line_reads_back_as_the_asset_it_says(asset in asset()) {
        let line = asset.line();
        let (key, parameter) = parse_meta_line(&line).expect("a meta-property line");
        prop_assert_eq!(key, "blob");
        prop_assert_eq!(Asset::parse(&parameter.unwrap()), Ok(asset));
    }
}

#[test]
fn the_name_is_what_follows_the_third_comma() {
    let a = Asset::parse(&format!("{HASH}, IMAGE/PNG, 12, cat, sat, mat.png")).unwrap();
    assert_eq!(a.mime, "image/png");
    assert_eq!(a.size, 12);
    assert_eq!(a.name.as_deref(), Some("cat, sat, mat.png"));
    assert_eq!(
        Asset::parse(&format!("{HASH}, image/png, 12"))
            .unwrap()
            .name,
        None
    );
    assert_eq!(
        Asset::parse(&format!("{HASH}, image/png, 12, "))
            .unwrap()
            .name,
        None
    );
}

#[test]
fn each_part_is_checked() {
    let bad = |p: &str| Asset::parse(p).unwrap_err();
    assert_eq!(bad(""), AssetError::Hash);
    assert_eq!(bad(&HASH[1..]), AssetError::Hash);
    assert_eq!(
        bad(&format!("{}g, image/png, 1", &HASH[1..])),
        AssetError::Hash
    );
    assert_eq!(bad(HASH), AssetError::Mime);
    for mime in ["png", "image/", "/png", "image/png/x", "image/p ng", ""] {
        assert_eq!(
            bad(&format!("{HASH}, {mime}, 1")),
            AssetError::Mime,
            "{mime:?}"
        );
    }
    assert_eq!(bad(&format!("{HASH}, image/png")), AssetError::Size);
    for size in ["", "-1", "1.5", "big", "18446744073709551616"] {
        assert_eq!(
            bad(&format!("{HASH}, image/png, {size}")),
            AssetError::Size,
            "{size:?}"
        );
    }
}

/// A hash is kept in lower case, whichever the line is written in.
#[test]
fn a_hash_is_lower_case() {
    let a = Asset::parse(&format!("{}, image/png, 1", HASH.to_uppercase())).unwrap();
    assert_eq!(a.hash, HASH);
}

/// An entity with one block holding `text`, and the block.
fn block_with(text: &str) -> Result<(Index, BlockId, BlockId)> {
    let graph = Graph::new();
    let page = graph.create_block(None, None, "Page")?;
    let block = graph.create_block(Some(page), None, text)?;
    graph.commit();
    Ok((Index::build(&graph)?, page, block))
}

#[test]
fn a_block_that_starts_with_a_blob_line_is_a_blob() -> Result<()> {
    let text = format!("##blob({HASH}, image/png, 12, cat.png)\nA cat, asleep");
    let (index, page, block) = block_with(&text)?;
    let blobs: Vec<_> = index.assets().collect();
    assert_eq!(blobs.len(), 1);
    assert_eq!(blobs[0].0, block);
    assert_eq!(blobs[0].1.name.as_deref(), Some("cat.png"));
    assert_eq!(Asset::caption(&text), "A cat, asleep");
    // The first child is a blob, which isn't a properties block, so the page
    // has no properties and nothing is flagged.
    let info = index.entity(page).unwrap();
    assert!((info.properties.is_empty() && info.meta.is_empty() && info.stray.is_empty()));
    assert_eq!(index.flags(page), []);
    Ok(())
}

#[test]
fn a_blob_is_a_block_wherever_it_is_and_not_an_entity() -> Result<()> {
    let graph = Graph::new();
    let page = graph.create_block(None, None, "Page")?;
    let first = graph.create_block(Some(page), None, "words")?;
    let line = format!("##blob({HASH}, image/png, 12)");
    let nested = graph.create_block(Some(first), None, &line)?;
    let later = graph.create_block(Some(page), None, &line)?;
    graph.commit();
    let index = Index::build(&graph)?;
    let listed: Vec<_> = index.assets().map(|(block, _)| block).collect();
    assert_eq!(listed, [nested, later], "in document order, one per block");
    assert_eq!(index.entities().count(), 1, "no entity is made");
    Ok(())
}

#[test]
fn only_a_first_line_that_can_be_read_makes_a_blob() -> Result<()> {
    let good = format!("##blob({HASH}, image/png, 1)");
    for (text, listed) in [
        (good.clone(), true),
        (format!("{good}\n"), true),
        (format!("  {good}"), true),
        (format!("words\n{good}"), false),
        (format!("\n{good}"), false),
        ("##blob".to_owned(), false),
        ("##blob(nothing)".to_owned(), false),
        (format!("##blob({HASH}, image/png)"), false),
        ("##blobs(x)".to_owned(), false),
        (format!("`{good}`"), false),
    ] {
        let (index, _, _) = block_with(&text)?;
        assert_eq!(index.assets().count(), usize::from(listed), "{text:?}");
    }
    Ok(())
}

/// Every `##blob` line is listed as written, the damaged ones too, so that
/// what a damaged line names isn't taken for something nothing refers to.
#[test]
fn every_blob_line_is_listed_as_written() -> Result<()> {
    let graph = Graph::new();
    let page = graph.create_block(None, None, "Page")?;
    let good = format!("{HASH}, image/png, 1");
    for text in [
        format!("##blob({good})\ncaption"),
        format!("##blob({HASH}, nonsense)"),
        "##blob".to_owned(),
        format!("words\n##blob({good})"),
        "no blob here".to_owned(),
    ] {
        graph.create_block(Some(page), None, &text)?;
    }
    graph.commit();
    let index = Index::build(&graph)?;
    assert_eq!(
        index.blob_lines().collect::<Vec<_>>(),
        [good.as_str(), &format!("{HASH}, nonsense"), ""]
    );
    assert_eq!(index.assets().count(), 1, "only the readable one is a blob");
    Ok(())
}

/// What `Asset::in_block` says of the first line, for the view to flag.
#[test]
fn a_blob_line_that_cannot_be_read_says_why() {
    let read = |text: &str| Asset::in_block(text);
    assert_eq!(read("words"), None);
    assert_eq!(read(""), None);
    assert_eq!(read("##blob"), Some(Err(AssetError::Missing)));
    assert_eq!(read("##blob(nothing)"), Some(Err(AssetError::Hash)));
    assert_eq!(
        read(&format!("##blob({HASH})")),
        Some(Err(AssetError::Mime))
    );
    assert_eq!(
        read(&format!("##blob({HASH}, image/png)")),
        Some(Err(AssetError::Size))
    );
    assert!(read(&format!("##blob({HASH}, image/png, 1)")).is_some_and(|r| r.is_ok()));
}

#[test]
fn the_caption_is_what_follows_the_first_line() {
    assert_eq!(Asset::caption("##blob(x)"), "");
    assert_eq!(Asset::caption("##blob(x)\n"), "");
    assert_eq!(
        Asset::caption("##blob(x)\n  A cat\nasleep \n"),
        "A cat\nasleep"
    );
    assert_eq!(Asset::caption("##blob(x)\n\n\n"), "");
}

/// Typing in a blob's caption changes what the index lists, so the index is
/// built again rather than patched.
#[test]
fn a_blob_is_not_patched_in_place() -> Result<()> {
    let line = format!("##blob({HASH}, image/png, 1)");
    let (mut index, _, block) = block_with(&line)?;
    assert!(!index.update_text(block, &line, &format!("{line}\ncaption")));
    assert!(!index.update_text(block, "words", &line));
    assert!(!index.update_text(block, &line, "words"));
    assert!(index.update_text(block, "words", "more words"));
    Ok(())
}

proptest! {
    /// Building an asset from its parts gives the one its line reads back as.
    #[test]
    fn an_asset_built_from_parts_is_the_one_its_line_reads_as(asset in asset()) {
        let built = Asset::new(&asset.hash, &asset.mime, asset.size, asset.name.as_deref());
        prop_assert_eq!(built, Ok(asset));
    }
}

#[test]
fn building_an_asset_checks_its_parts_and_tidies_the_name() {
    let new = |h: &str, m: &str, n: Option<&str>| Asset::new(h, m, 1, n);
    assert_eq!(new("nope", "image/png", None), Err(AssetError::Hash));
    assert_eq!(new(HASH, "png", None), Err(AssetError::Mime));
    let a = new(&HASH.to_uppercase(), "Image/PNG", Some("  two\nlines\r\n ")).unwrap();
    assert_eq!((a.hash.as_str(), a.mime.as_str()), (HASH, "image/png"));
    assert_eq!(a.name.as_deref(), Some("two lines"));
    // A name with nothing in it is no name, and the line still reads back.
    let blank = new(HASH, "image/png", Some(" \n ")).unwrap();
    assert_eq!(blank.name, None);
    assert_eq!(Asset::parse(&blank.parameter()), Ok(blank));
}

proptest! {
    /// Whatever the name and type say, an export's file name is the hash and
    /// a short plain extension, so it is safe as a path.
    #[test]
    fn an_exports_file_name_is_always_safe(
        name in proptest::option::of("[ -~é/\\\\.]{0,24}"),
        mime in "[a-z]{1,6}/[a-z+.-]{1,12}",
    ) {
        let asset = Asset {
            hash: HASH.into(),
            mime,
            size: 1,
            name,
        };
        let file = asset.file_name();
        let (stem, extension) = file.split_once('.').unwrap();
        prop_assert_eq!(stem, HASH);
        prop_assert!((1..=8).contains(&extension.len()), "{file}");
        prop_assert!(
            extension.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit()),
            "{file}"
        );
    }
}

#[test]
fn an_export_names_a_file_for_what_it_is() {
    let file = |name: Option<&str>, mime: &str| {
        Asset::new(HASH, mime, 1, name).unwrap().file_name()[65..].to_owned()
    };
    // The name's own extension, lower-cased, wins; the last of several.
    assert_eq!(file(Some("Photo.JPEG"), "image/png"), "jpeg");
    assert_eq!(file(Some("a.tar.gz"), "application/gzip"), "gz");
    // Without one, or with one that isn't short and plain, the type's.
    assert_eq!(file(Some("README"), "text/markdown"), "md");
    assert_eq!(file(Some(".hidden"), "image/png"), "png");
    assert_eq!(file(Some("x.some-long-ext"), "application/pdf"), "pdf");
    assert_eq!(file(Some("x.wh y"), "image/jpeg"), "jpg");
    assert_eq!(file(Some("a/b.c/d"), "text/plain"), "txt");
    assert_eq!(file(None, "image/svg+xml"), "svg");
    // And, knowing neither, bin.
    assert_eq!(file(None, "application/x-odd"), "bin");
}

/// Every type the export knows, by the extension it gives it. Written out in
/// full, not read back from the code, so that forgetting one is noticed.
#[test]
fn an_export_knows_these_types() {
    for (mime, extension) in [
        ("image/png", "png"),
        ("image/jpeg", "jpg"),
        ("image/gif", "gif"),
        ("image/webp", "webp"),
        ("image/avif", "avif"),
        ("image/svg+xml", "svg"),
        ("application/pdf", "pdf"),
        ("application/json", "json"),
        ("text/plain", "txt"),
        ("text/markdown", "md"),
    ] {
        let asset = Asset::new(HASH, mime, 1, Some("no extension")).unwrap();
        assert_eq!(asset.file_name(), format!("{HASH}.{extension}"), "{mime}");
    }
}
