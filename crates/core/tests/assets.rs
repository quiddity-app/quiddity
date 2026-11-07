//! The `##asset` line: reading what it says, and flagging what it gets wrong.

use proptest::prelude::*;
use quiddity_core::syntax::parse_meta_line;
use quiddity_core::{Asset, AssetError, BlockId, Flag, Graph, Index, Result};

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
        prop_assert_eq!(key, "asset");
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

fn entity(graph: &Graph, name: &str, props: &str) -> Result<BlockId> {
    let id = graph.create_block(None, None, name)?;
    graph.create_block(Some(id), None, props)?;
    Ok(id)
}

fn index_with(props: &str) -> Result<(Index, BlockId)> {
    let graph = Graph::new();
    let id = entity(&graph, "cat.png", props)?;
    graph.commit();
    Ok((Index::build(&graph)?, id))
}

#[test]
fn an_entity_with_an_asset_line_is_an_asset() -> Result<()> {
    let (index, id) = index_with(&format!(
        "type:: image\n##asset({HASH}, image/png, 12, cat.png)"
    ))?;
    let assets: Vec<_> = index.assets().collect();
    assert_eq!(assets.len(), 1);
    assert_eq!(assets[0].0, id);
    assert_eq!(assets[0].1.name.as_deref(), Some("cat.png"));
    assert_eq!(index.flags(id), []);
    Ok(())
}

#[test]
fn an_asset_line_that_cannot_be_read_is_flagged_and_not_listed() -> Result<()> {
    for (props, message) in [
        ("##asset", AssetError::Missing),
        ("##asset(nothing)", AssetError::Hash),
        (
            &format!("##asset({HASH}, image/png, 1)\n##asset({HASH}, image/png, 2)") as &str,
            AssetError::Several,
        ),
    ] {
        let (index, id) = index_with(props)?;
        assert_eq!(index.assets().count(), 0, "{props}");
        assert_eq!(
            index.flags(id),
            [Flag::BadMeta {
                key: "asset".into(),
                message: message.to_string()
            }],
            "{props}"
        );
    }
    Ok(())
}

/// Other meta-properties still mean nothing here, and are still flagged,
/// without the asset line being taken for one of them.
#[test]
fn other_meta_properties_are_still_flagged_beside_an_asset_line() -> Result<()> {
    let (index, id) = index_with(&format!("##transitive\n##asset({HASH}, image/png, 1)"))?;
    assert_eq!(index.flags(id), [Flag::MetaOutsideProperty]);
    assert_eq!(index.assets().count(), 1);
    Ok(())
}
