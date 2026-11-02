//! The graph's settings: how `home` is written, `quiddity.toml`, and keeping
//! them in the graph.

mod common;

use std::fs;

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use quiddity_core::files::{export_dir, import_dir};
use quiddity_core::syntax::{Link, Qualifier, Value};
use quiddity_core::{Graph, Home, Result, Settings};

/// A link name weighted towards what a link has to escape, and towards the
/// words that mean a view rather than an entity.
fn link_name() -> impl Strategy<Value = String> {
    prop_oneof![
        r#"[a-z#\[\]\\ "]{1,8}"#,
        Just("journals".to_owned()),
        r"[^\n\r]{1,12}",
    ]
    .prop_filter("trimmed", |n| !n.is_empty() && n.trim() == n)
}

fn home() -> impl Strategy<Value = Home> {
    let qualifier =
        ("[a-z]{1,5}", option::of(r#"[a-z \]\)"]{1,5}"#)).prop_map(|(key, value)| Qualifier {
            key,
            value: value.map(Value::Text),
        });
    let link = (link_name(), vec(qualifier, 0..2))
        .prop_map(|(name, qualifiers)| Home::Entity(Link { name, qualifiers }));
    prop_oneof![
        1 => Just(Home::Journals),
        1 => Just(Home::All),
        1 => Just(Home::Graph),
        3 => link,
    ]
}

proptest! {
    #![proptest_config(common::config(64))]

    #[test]
    fn home_reads_back_as_written(home in home()) {
        prop_assert_eq!(home.to_string().parse::<Home>().unwrap(), home);
    }

    #[test]
    fn settings_survive_toml(home in option::of(home())) {
        let settings = Settings { home };
        prop_assert_eq!(Settings::from_toml(&settings.to_toml()).unwrap(), settings);
    }

    #[test]
    fn settings_survive_the_graph(home in option::of(home())) {
        let graph = Graph::new();
        let settings = Settings { home };
        graph.apply_settings(&settings).unwrap();
        prop_assert_eq!(graph.settings(), settings);
    }
}

#[test]
fn home_is_a_view_or_one_link() {
    let entity = |name: &str| {
        Home::Entity(Link {
            name: name.to_owned(),
            qualifiers: vec![],
        })
    };
    assert_eq!(" graph ".parse::<Home>().unwrap(), Home::Graph);
    assert_eq!("[[Quiddity]]".parse::<Home>().unwrap(), entity("Quiddity"));
    assert_eq!("[[journals]]".parse::<Home>().unwrap(), entity("journals"));
    for bad in [
        "",
        "Journals",
        "Quiddity",
        "[[a]] and [[b]]",
        "[[a]][[b]]",
        "see [[a]]",
        "[[a]] too",
        "[[]]",
    ] {
        assert!(bad.parse::<Home>().is_err(), "{bad:?}");
    }
}

#[test]
fn toml_is_checked() {
    assert_eq!(Settings::from_toml("").unwrap(), Settings::default());
    assert!(Settings::from_toml("hom = \"graph\"").is_err(), "misspelt");
    assert!(Settings::from_toml("home = \"nowhere\"").is_err());
    assert!(Settings::from_toml("home = 1").is_err());
    assert_eq!(Settings::default().to_toml(), "");
}

#[test]
fn applying_leaves_what_is_unset() {
    let graph = Graph::new();
    let all = Settings {
        home: Some(Home::All),
    };
    graph.apply_settings(&all).unwrap();
    graph.apply_settings(&Settings::default()).unwrap();
    assert_eq!(graph.settings(), all);
}

#[test]
fn export_and_import_carry_settings() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let graph = Graph::new();
    graph.create_entity("Quiddity", &[])?;
    graph.commit();

    // Nothing set, no file.
    let plain = tmp.path().join("plain");
    export_dir(&graph, &plain)?;
    assert!(!plain.join("quiddity.toml").exists());

    graph.apply_settings(&Settings {
        home: Some("[[Quiddity]]".parse()?),
    })?;
    let out = tmp.path().join("out");
    export_dir(&graph, &out)?;
    assert_eq!(
        fs::read_to_string(out.join("quiddity.toml"))?,
        "home = \"[[Quiddity]]\"\n"
    );

    let copy = Graph::new();
    import_dir(&copy, &out)?;
    assert_eq!(copy.settings(), graph.settings());
    Ok(())
}

#[test]
fn a_bad_settings_file_stops_the_import() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    fs::write(tmp.path().join("Page.md"), "- a block\n")?;
    fs::write(tmp.path().join("quiddity.toml"), "home = \"Page\"\n")?;
    let graph = Graph::new();
    assert!(import_dir(&graph, tmp.path()).is_err());
    assert!(graph.roots().is_empty(), "nothing imported");

    // Nor is one that can't be read at all.
    fs::remove_file(tmp.path().join("quiddity.toml"))?;
    fs::create_dir(tmp.path().join("quiddity.toml"))?;
    assert!(import_dir(&graph, tmp.path()).is_err());
    assert!(graph.roots().is_empty(), "nothing imported");
    Ok(())
}
