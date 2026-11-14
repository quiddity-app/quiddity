use quiddity_core::{Graph, PublishError, Result, page_names};

fn graph(names: &[&str]) -> Result<Graph> {
    let graph = Graph::new();
    for name in names {
        graph.create_entity(name, &[])?;
    }
    graph.commit();
    Ok(graph)
}

#[test]
fn names_are_listed_in_order() -> Result<()> {
    let names = page_names(&graph(&["Zebra", "apple", "Café"])?).unwrap();
    assert_eq!(names, ["Café", "Zebra", "apple"]);
    Ok(())
}

/// Names that links treat as one are one page's worth of file, so they clash
/// however they differ: case, spacing or Unicode form.
#[test]
fn names_that_compare_equal_clash() -> Result<()> {
    let clash = |names: &[&str]| match page_names(&graph(names).unwrap()) {
        Err(PublishError::Clashes(groups)) => groups,
        other => panic!("expected a clash, got {other:?}"),
    };
    assert_eq!(clash(&["Jane", "jane", "Other"]), [["Jane", "jane"]]);
    assert_eq!(
        clash(&["Jane  Doe", "jane doe"]),
        [["Jane  Doe", "jane doe"]]
    );
    assert_eq!(clash(&["Caf\u{e9}", "Cafe\u{301}"]).len(), 1);
    Ok(())
}

#[test]
fn a_clash_is_reported_with_every_name_in_it() -> Result<()> {
    let Err(error) = page_names(&graph(&["A", "a", "B", "b", "C"])?) else {
        panic!("expected a clash");
    };
    let message = error.to_string();
    assert!(message.contains(r#"["A", "a"]"#) && message.contains(r#"["B", "b"]"#));
    assert!(!message.contains('C'));
    Ok(())
}

#[test]
fn a_name_must_be_one_path_segment() -> Result<()> {
    for bad in ["Places/Scotland", "a\\b", "..", ".", "tab\there"] {
        match page_names(&graph(&["Fine", bad])?) {
            Err(PublishError::Unroutable(names)) => assert_eq!(names, [bad]),
            other => panic!("{bad:?}: {other:?}"),
        }
    }
    Ok(())
}

#[test]
fn an_empty_graph_has_no_names() {
    assert!(page_names(&Graph::new()).unwrap().is_empty());
}
