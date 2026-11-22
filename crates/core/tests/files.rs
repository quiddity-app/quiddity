#![expect(clippy::unwrap_used, reason = "a test failing to set up should panic")]
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use proptest::prelude::*;
use quiddity_core::files::{DecodedNames, decode_page_names, export_dir, import_dir};
use quiddity_core::syntax::{Link, Value};
use quiddity_core::{Graph, Index, Resolution, Result, ensure_journal};

fn text(s: &str) -> Option<Value> {
    Some(Value::Text(s.to_owned()))
}

// Round trips are property-tested in tests/system.rs. These pin down the
// directory layout and how other tools' graphs are read.

#[test]
fn export_layout() -> Result<()> {
    let graph = Graph::new();
    graph.create_entity("Jane", &[("type", text("person"))])?;
    graph.create_entity("jane", &[("type", text("book"))])?;
    graph.create_entity("Notes: misc/other", &[])?;
    let day = ensure_journal(
        &graph,
        &Index::build(&graph)?,
        "2026-10-25".parse().unwrap(),
    )?;
    graph.create_block(Some(day), None, "Wrote the exporter")?;
    graph.commit();

    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("out");
    assert_eq!(export_dir(&graph, &out)?, 4);
    assert!(out.join("quiddity.json").is_file());
    // Names differing only in case get distinct files, for
    // case-insensitive file systems.
    assert!(out.join("entities/Jane.md").is_file());
    assert!(out.join("entities/jane 2.md").is_file());
    assert!(out.join("entities/Notes_ misc_other.md").is_file());
    assert_eq!(
        fs::read_to_string(out.join("journals/2026-10-25.md"))?,
        "title:: 2026-10-25\n##journal(2026-10-25)\n\n- Wrote the exporter\n"
    );
    assert!(export_dir(&graph, &out).is_err(), "refuses to overwrite");
    Ok(())
}

/// What each file of an export holds, by name.
fn entity_files(out: &Path) -> BTreeMap<String, String> {
    fs::read_dir(out.join("entities"))
        .unwrap()
        .map(|e| {
            let path = e.unwrap().path();
            (
                path.file_name().unwrap().to_str().unwrap().to_owned(),
                fs::read_to_string(&path).unwrap(),
            )
        })
        .collect()
}

#[test]
fn same_named_entities_are_numbered_in_creation_order() -> Result<()> {
    let graph = Graph::new();
    let janes: Vec<_> = (1..=3)
        .map(|n| graph.create_entity("Jane", &[("n", text(&n.to_string()))]))
        .collect::<Result<_>>()?;
    // Roots can be put in any order; the numbering mustn't follow.
    graph.move_block(janes[0], None, Some(2))?;
    graph.move_block(janes[2], None, Some(0))?;
    graph.commit();

    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("out");
    export_dir(&graph, &out)?;
    let files = entity_files(&out);
    for (name, n) in [("Jane.md", "1"), ("Jane 2.md", "2"), ("Jane 3.md", "3")] {
        assert!(
            files[name].contains(&format!("n:: {n}")),
            "{name}: {}",
            files[name]
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Where entities with one name sit among the roots doesn't change which
    /// file holds which, so a backup kept in version control shows only what
    /// changed.
    #[test]
    fn numbering_does_not_depend_on_root_order(
        count in 2usize..6,
        moves in prop::collection::vec((any::<prop::sample::Index>(), any::<prop::sample::Index>()), 0..8),
    ) {
        let graph = Graph::new();
        let ids: Vec<_> = (0..count)
            .map(|n| graph.create_entity("Jane", &[("n", text(&n.to_string()))]).unwrap())
            .collect();
        graph.commit();
        let tmp = tempfile::tempdir().unwrap();
        export_dir(&graph, &tmp.path().join("before")).unwrap();

        for (who, to) in &moves {
            graph.move_block(ids[who.index(count)], None, Some(to.index(count))).unwrap();
        }
        graph.commit();
        export_dir(&graph, &tmp.path().join("after")).unwrap();
        prop_assert_eq!(
            entity_files(&tmp.path().join("before")),
            entity_files(&tmp.path().join("after"))
        );
    }
}

/// File systems limit a name to 255 bytes, which 130 CJK characters or 70
/// emoji are over; the export used to stop at the first such entity.
#[test]
fn long_non_ascii_names_export_and_round_trip() -> Result<()> {
    let graph = Graph::new();
    let cjk = "日".repeat(130);
    let emoji = "🦀".repeat(70);
    graph.create_entity(&cjk, &[])?;
    graph.create_entity(&emoji, &[])?;
    // The same long name twice needs a number as well.
    graph.create_entity(&cjk.to_lowercase(), &[("type", text("copy"))])?;
    graph.commit();

    let tmp = tempfile::tempdir()?;
    let out = tmp.path().join("out");
    assert_eq!(export_dir(&graph, &out)?, 3);

    let copy = Graph::new();
    import_dir(&copy, &out)?;
    let index = Index::build(&copy)?;
    assert_eq!(index.named(&cjk).len(), 2);
    assert_eq!(index.named(&emoji).len(), 1);
    Ok(())
}

#[test]
fn imports_a_logseq_graph() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path();
    fs::create_dir_all(dir.join("pages"))?;
    fs::create_dir_all(dir.join("journals"))?;
    fs::create_dir_all(dir.join("logseq/bak"))?;
    fs::write(
        dir.join("pages/Jane.md"),
        "alias:: JD\n\n- A person\n\t- with a child block\n",
    )?;
    fs::write(dir.join("journals/2026_10_25.md"), "- Met [[JD]]\n")?;
    fs::write(dir.join("logseq/bak/Jane.md"), "- an old backup\n")?;

    let graph = Graph::new();
    assert_eq!(import_dir(&graph, dir)?.len(), 2);
    let index = Index::build(&graph)?;
    let day = index
        .journal("2026-10-25".parse().unwrap())
        .expect("journal inferred from the file name");
    assert_eq!(graph.entity_name(day)?, "2026-10-25");
    let Resolution::Unique(jane) = index.resolve(&Link {
        name: "JD".into(),
        qualifiers: vec![],
    }) else {
        panic!("alias resolves");
    };
    assert_eq!(graph.entity_name(jane)?, "Jane");
    assert_eq!(index.backlinks(jane).len(), 1);
    Ok(())
}

#[test]
fn logseq_namespaces_become_parents() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path();
    fs::create_dir_all(dir.join("pages"))?;
    // `___` is Logseq's file name for `/`; a title says the name itself.
    fs::write(
        dir.join("pages/Places___Scotland___Edinburgh.md"),
        "- A city\n",
    )?;
    fs::write(
        dir.join("pages/Places___Wales.md"),
        "title:: Places/Wales\n\n- Hills\n",
    )?;
    fs::write(
        dir.join("pages/Places___Ireland.md"),
        "parent:: [[Europe]]\n\n- Green\n",
    )?;

    let graph = Graph::new();
    import_dir(&graph, dir)?;
    let index = Index::build(&graph)?;
    let find = |name: &str| match index.resolve(&Link {
        name: name.into(),
        qualifiers: vec![],
    }) {
        Resolution::Unique(id) => id,
        other => panic!("{name}: {other:?}"),
    };
    let places = find("Places");
    let scotland = find("Places/Scotland");
    let edinburgh = find("Places/Scotland/Edinburgh");
    assert_eq!(graph.entity_name(edinburgh)?, "Places/Scotland/Edinburgh");
    // No order is promised.
    let mut children = index.inverse("children", places, true);
    children.sort();
    let mut expected = vec![scotland, find("Places/Wales")];
    expected.sort();
    assert_eq!(children, expected);
    assert_eq!(index.inverse("children", scotland, true), vec![edinburgh]);
    // A parent the page names itself is left alone.
    assert!(!children.contains(&find("Places/Ireland")));
    assert!(index.property_entity("parent").is_some());
    Ok(())
}

/// Logseq writes what a file name can't hold as `%XX`. A title says the name
/// itself, and `___` is a `/` before anything is decoded.
#[test]
fn logseq_file_names_are_decoded() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path();
    fs::create_dir_all(dir.join("pages"))?;
    fs::write(dir.join("pages/What%3F.md"), "- Who knows\n")?;
    fs::write(dir.join("pages/Places___St%3A Kilda.md"), "- Far\n")?;
    fs::write(dir.join("pages/a%5F%5F%5Fb.md"), "- Underscores\n")?;
    fs::write(
        dir.join("pages/Kept%3F.md"),
        "title:: Kept%3F\n\n- As said\n",
    )?;
    let graph = Graph::new();
    import_dir(&graph, dir)?;
    let index = Index::build(&graph)?;
    let mut names: Vec<_> = graph
        .roots()
        .into_iter()
        .map(|r| graph.entity_name(r))
        .collect::<Result<_>>()?;
    names.sort();
    assert_eq!(
        names,
        [
            "Kept%3F",
            "Places",
            "Places/St: Kilda",
            "What?",
            "a___b",
            "parent"
        ]
    );
    assert!(index.property_entity("parent").is_some());
    Ok(())
}

fn link(name: &str) -> Link {
    Link {
        name: name.into(),
        qualifiers: vec![],
    }
}

/// The names of every entity, in order.
fn names(graph: &Graph) -> Result<Vec<String>> {
    let mut names = graph
        .roots()
        .into_iter()
        .map(|r| graph.entity_name(r))
        .collect::<Result<Vec<_>>>()?;
    names.sort();
    Ok(names)
}

/// The page the entity's own `parent` names, if it names one.
fn parent_of(graph: &Graph, name: &str) -> Result<Option<String>> {
    let index = Index::build(graph)?;
    let Resolution::Unique(id) = index.resolve(&link(name)) else {
        panic!("{name} isn't one entity");
    };
    Ok(graph
        .entity_properties(id)?
        .into_iter()
        .find(|p| p.key == "parent")
        .and_then(|p| match p.value {
            Some(Value::Link(l)) => Some(l.name),
            _ => None,
        }))
}

/// A graph imported before names were decoded is put right once, without the
/// old name kept, and left alone after that. Nothing changes on a dry run.
#[test]
fn encoded_names_can_be_decoded_after_the_fact() -> Result<()> {
    let graph = Graph::new();
    let what = graph.create_entity("What%3F", &[])?;
    graph.create_entity("Plain", &[])?;
    graph.create_entity("Taken?", &[])?;
    graph.create_entity("Taken%3F", &[])?;
    graph.create_entity("100%", &[])?;
    let day = ensure_journal(
        &graph,
        &Index::build(&graph)?,
        "2026-10-25".parse().unwrap(),
    )?;
    // A journal is its day, whatever its first line says.
    graph.set_text(day, "2026-10-25%3F")?;

    let before = names(&graph)?;
    let dry = decode_page_names(&graph, true)?;
    assert_eq!(dry.renamed, [("What%3F".to_owned(), "What?".to_owned())]);
    assert_eq!(dry.clashing, [("Taken%3F".to_owned(), "Taken?".to_owned())]);
    assert!(dry.reparented.is_empty());
    assert_eq!(names(&graph)?, before);

    let done = decode_page_names(&graph, false)?;
    assert_eq!(done, dry);
    assert_eq!(graph.entity_name(what)?, "What?");
    assert_eq!(graph.entity_name(day)?, "2026-10-25%3F");
    // The old name is gone, and nothing was added to say it was there.
    assert!(graph.entity_properties(what)?.is_empty());
    let index = Index::build(&graph)?;
    assert!(!matches!(
        index.resolve(&link("What%3F")),
        Resolution::Unique(_)
    ));

    let again = decode_page_names(&graph, false)?;
    assert!(again.renamed.is_empty());
    assert!(again.reparented.is_empty());
    Ok(())
}

/// A `%2F` in an old name is a namespace now, so the page is put under the
/// page above it, which is made if it has none. A `parent` that names a page
/// that was renamed follows it, wherever it is written; one the import
/// wouldn't have written is left.
#[test]
fn decoded_names_get_the_namespaces_they_now_have() -> Result<()> {
    let graph = Graph::new();
    let parent = |name: &str| ("parent", Some(Value::Link(link(name))));
    graph.create_entity("Places", &[])?;
    graph.create_entity("Places/X%3F", &[parent("Places")])?;
    graph.create_entity("Places/X%3F/Y", &[parent("Places/X%3F")])?;
    graph.create_entity("Foo", &[parent("Places/X%3F")])?;
    // The import named `Places` above this, when it was `Places/A%2FB`.
    graph.create_entity("Places/A%2FB", &[parent("Places")])?;
    graph.create_entity("Kept%2FIt", &[parent("Elsewhere")])?;
    graph.create_entity("Q%2FR%2FS", &[])?;
    // It has other properties, so the `parent` is added to them.
    graph.create_entity("Z%2FW", &[("type", text("thing"))])?;
    // A namespace with no `parent`: the import would have given it one, so it
    // was taken off, and stays off.
    graph.create_entity("Orphan/Page%3F", &[])?;

    let dry = decode_page_names(&graph, true)?;
    let mut expected: Vec<(String, String)> = [
        ("Foo", "Places/X?"),
        ("Places/A/B", "Places/A"),
        ("Places/X?/Y", "Places/X?"),
        ("Q/R/S", "Q/R"),
        ("Z/W", "Z"),
    ]
    .map(|(a, b)| (a.to_owned(), b.to_owned()))
    .into();
    expected.sort();
    let mut seen = dry.reparented.clone();
    seen.sort();
    assert_eq!(seen, expected);
    assert_eq!(names(&graph)?.len(), 9, "a dry run adds nothing");
    assert_eq!(parent_of(&graph, "Foo")?.as_deref(), Some("Places/X%3F"));

    decode_page_names(&graph, false)?;
    assert_eq!(
        names(&graph)?,
        [
            "Foo",
            "Kept/It",
            "Orphan/Page?",
            "Places",
            "Places/A",
            "Places/A/B",
            "Places/X?",
            "Places/X?/Y",
            "Q",
            "Q/R",
            "Q/R/S",
            "Z",
            "Z/W",
            "parent",
        ]
    );
    for (name, parent) in [
        ("Foo", Some("Places/X?")),
        ("Kept/It", Some("Elsewhere")),
        ("Places", None),
        ("Places/A", Some("Places")),
        ("Places/A/B", Some("Places/A")),
        ("Places/X?", Some("Places")),
        ("Places/X?/Y", Some("Places/X?")),
        ("Q", None),
        ("Q/R", Some("Q")),
        ("Q/R/S", Some("Q/R")),
        ("Orphan/Page?", None),
        ("Z", None),
        ("Z/W", Some("Z")),
    ] {
        assert_eq!(parent_of(&graph, name)?.as_deref(), parent, "{name}");
    }
    assert!(Index::build(&graph)?.property_entity("parent").is_some());
    let index = Index::build(&graph)?;
    let Resolution::Unique(zw) = index.resolve(&link("Z/W")) else {
        panic!("Z/W")
    };
    let keys: Vec<_> = graph
        .entity_properties(zw)?
        .into_iter()
        .map(|p| p.key)
        .collect();
    assert_eq!(keys, ["type", "parent"]);

    let again = decode_page_names(&graph, false)?;
    assert_eq!(again, DecodedNames::default());
    Ok(())
}

/// A file in `journals/` named for a day is that day's journal. One that
/// already says so, as an export does, isn't given a second line; one from an
/// export before `##journal` has its old lines left as they are, and the new
/// one added.
#[test]
fn journals_are_marked_with_a_journal_line_once() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path();
    fs::create_dir_all(dir.join("journals"))?;
    fs::write(dir.join("journals/2026_10_25.md"), "- Met someone\n")?;
    fs::write(
        dir.join("journals/2026-10-26.md"),
        "title:: 2026-10-26\n##journal(2026-10-26)\n\n- Wrote it\n",
    )?;
    fs::write(
        dir.join("journals/2026-10-27.md"),
        "title:: 2026-10-27\ntype:: journal\ndate:: 2026-10-27\n\n- An old export\n",
    )?;
    let graph = Graph::new();
    import_dir(&graph, dir)?;
    let index = Index::build(&graph)?;
    let first_block = |day: &str| -> Result<String> {
        let id = index.journal(day.parse().unwrap()).expect(day);
        graph.text(graph.children(id)[0])
    };
    assert_eq!(first_block("2026-10-25")?, "##journal(2026-10-25)");
    assert_eq!(first_block("2026-10-26")?, "##journal(2026-10-26)");
    assert_eq!(
        first_block("2026-10-27")?,
        "##journal(2026-10-27)\ntype:: journal\ndate:: 2026-10-27"
    );
    Ok(())
}

/// Logseq links to a journal by the day as it writes it, which isn't what a
/// journal is named here, so such a journal answers to that as an alias.
#[test]
fn logseq_journals_answer_to_the_name_logseq_gives_them() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path();
    fs::create_dir_all(dir.join("pages"))?;
    fs::create_dir_all(dir.join("journals"))?;
    // The journals come before the pages that link to them, so the links have
    // to be read first.
    fs::write(
        dir.join("journals/2026_11_10.md"),
        "alias:: Tuesday\n\n- A day\n",
    )?;
    fs::write(dir.join("journals/2026_11_11.md"), "- Nothing links here\n")?;
    fs::write(
        dir.join("pages/Plan.md"),
        "- Due [[10th November 2026]]\n\t- and again #[[3rd December 2026]]\n",
    )?;
    fs::write(
        dir.join("pages/Review.md"),
        "due:: [[10th november 2026]]\n\n- Soon\n",
    )?;

    let graph = Graph::new();
    import_dir(&graph, dir)?;
    let index = Index::build(&graph)?;
    let find = |name: &str| match index.resolve(&Link {
        name: name.into(),
        qualifiers: vec![],
    }) {
        Resolution::Unique(id) => Some(id),
        _ => None,
    };
    let tenth = index.journal("2026-11-10".parse().unwrap()).unwrap();
    assert_eq!(find("10th November 2026"), Some(tenth));
    assert_eq!(find("10th november 2026"), Some(tenth));
    assert_eq!(find("Tuesday"), Some(tenth), "its own alias is kept");
    assert_eq!(index.backlinks(tenth).len(), 2);
    // Nothing links to the 11th by date, so it is left as it was.
    let eleventh = index.journal("2026-11-11".parse().unwrap()).unwrap();
    assert_eq!(find("11th November 2026"), None);
    assert_eq!(
        graph.text(graph.children(eleventh)[0])?,
        "##journal(2026-11-11)"
    );
    // A day linked to that has no file is made, so that the link isn't left
    // pointing nowhere.
    let third = index.journal("2026-12-03".parse().unwrap()).unwrap();
    assert_eq!(find("3rd December 2026"), Some(third));
    assert_eq!(graph.entity_name(third)?, "2026-12-03");
    Ok(())
}

/// A Quiddity export isn't Logseq's: a link there means what it says.
#[test]
fn a_quiddity_export_has_no_day_aliases_added() -> Result<()> {
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path();
    fs::create_dir_all(dir.join("entities"))?;
    fs::create_dir_all(dir.join("journals"))?;
    fs::write(
        dir.join("quiddity.json"),
        "{\"format\": \"quiddity\", \"version\": 1}\n",
    )?;
    fs::write(
        dir.join("journals/2026-11-10.md"),
        "title:: 2026-11-10\n##journal(2026-11-10)\n\n- A day\n",
    )?;
    fs::write(
        dir.join("entities/Plan.md"),
        "title:: Plan\n\n- Due [[10th November 2026]]\n",
    )?;

    let graph = Graph::new();
    import_dir(&graph, dir)?;
    let index = Index::build(&graph)?;
    let day = index.journal("2026-11-10".parse().unwrap()).unwrap();
    assert_eq!(graph.text(graph.children(day)[0])?, "##journal(2026-11-10)");
    assert_eq!(index.journal("2026-11-10".parse().unwrap()), Some(day));
    Ok(())
}
