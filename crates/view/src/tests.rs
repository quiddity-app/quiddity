use jiff::civil::Date;
use jiff::{Timestamp, ToSpan};
use quiddity_core::syntax::{Link, Value};
use quiddity_core::{BlockId, Graph};

use super::*;

fn text(s: &str) -> Option<Value> {
    Some(Value::Text(s.to_owned()))
}

fn today() -> Date {
    "2026-10-26".parse().unwrap()
}

/// A view on a graph that `build` has filled.
pub(crate) fn view_with(build: impl FnOnce(&Graph)) -> View {
    let graph = Graph::new();
    build(&graph);
    graph.commit();
    View::new(graph).unwrap()
}

fn note(graph: &Graph, parent: BlockId, text: &str) -> BlockId {
    graph.create_block(Some(parent), None, text).unwrap()
}

fn id_of(v: &View, title: &str) -> String {
    v.entities()
        .unwrap()
        .into_iter()
        .find(|e| e.title == title)
        .unwrap()
        .id
}

fn titles(refs: &[Reference]) -> Vec<&str> {
    refs.iter().map(|r| r.source.title.as_str()).collect()
}

fn texts(blocks: &[Block]) -> Vec<&str> {
    blocks.iter().map(|b| b.text.as_str()).collect()
}

/// An entity with the given top-level blocks, and the view on it.
fn page(blocks: &[&str]) -> (View, String, Vec<String>) {
    let mut ids = Vec::new();
    let v = view_with(|g| {
        let page = g.create_entity("Page", &[]).unwrap();
        ids = blocks
            .iter()
            .map(|t| note(g, page, t).to_string())
            .collect();
    });
    let page = id_of(&v, "Page");
    (v, page, ids)
}

fn top_level(v: &View, page: &str) -> Vec<String> {
    v.entity(page)
        .unwrap()
        .blocks
        .into_iter()
        .map(|b| b.text)
        .collect()
}

mod queries {
    use super::*;

    fn lots() -> View {
        view_with(|g| {
            g.create_entity("Zebra", &[]).unwrap();
            g.create_entity("apple", &[("favourite", text("true"))])
                .unwrap();
            g.create_entity("Mango", &[("favourite", text("no"))])
                .unwrap();
            let quiddity = g
                .create_entity("Quiddity", &[("favourite", text("Yes"))])
                .unwrap();
            let n = note(g, quiddity, "Built on [[apple]], [[Nope]]");
            note(g, n, "nested");
        })
    }

    #[test]
    fn lists_entities_by_name_ignoring_case() {
        let names: Vec<_> = lots()
            .entities()
            .unwrap()
            .into_iter()
            .map(|e| e.title)
            .collect();
        assert_eq!(names, ["apple", "Mango", "Quiddity", "Zebra"]);
    }

    #[test]
    fn favourites_come_from_the_property() {
        let names: Vec<_> = lots()
            .favourites()
            .unwrap()
            .into_iter()
            .map(|e| e.title)
            .collect();
        assert_eq!(names, ["apple", "Quiddity"]);
    }

    #[test]
    fn an_entity_has_nested_blocks_and_resolved_links() {
        let v = lots();
        let e = v.entity(&id_of(&v, "Quiddity")).unwrap();
        assert_eq!(e.title, "Quiddity");
        assert_eq!(e.blocks.len(), 2, "the properties block, then the note");
        assert_eq!(e.blocks[1].text, "Built on [[apple]], [[Nope]]");
        assert_eq!(e.blocks[1].children[0].text, "nested");
        let apple = v.entities().unwrap().remove(0).id;
        assert_eq!(e.links["apple"], LinkTarget::Entity { id: apple });
        assert_eq!(e.links["Nope"], LinkTarget::Missing);
    }

    #[test]
    fn only_entities_can_be_opened() {
        let v = lots();
        let id = v.entities().unwrap().remove(0).id;
        let block = v.entity(&id).unwrap().blocks[0].id.clone();
        assert!(v.entity(&block).is_err(), "a block is not an entity");
        assert!(v.entity("nonsense").is_err());
    }

    #[test]
    fn journals_run_newest_first_from_today() {
        let v = view_with(|g| {
            let index = quiddity_core::Index::build(g).unwrap();
            for date in [
                today().checked_sub(5.days()).unwrap(),
                today().checked_sub(1.day()).unwrap(),
                today().checked_add(1.day()).unwrap(),
            ] {
                quiddity_core::ensure_journal(g, &index, date).unwrap();
            }
            g.create_entity("Not a journal", &[]).unwrap();
        });
        let titles = |v: &View| -> Vec<String> {
            v.journals(today())
                .unwrap()
                .into_iter()
                .map(|e| e.title)
                .collect()
        };
        assert_eq!(
            titles(&v),
            ["2026-10-21", "2026-10-25"]
                .map(str::to_owned)
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
        );
        let made = v.ensure_journal(today()).unwrap();
        assert_eq!(made.title, "2026-10-26");
        assert_eq!(titles(&v), ["2026-10-26", "2026-10-25", "2026-10-21"]);
        assert_eq!(v.ensure_journal(today()).unwrap(), made, "created once");
    }

    #[test]
    fn references_are_grouped_by_entity_without_nesting_or_self_links() {
        let v = view_with(|g| {
            let alpha = g.create_entity("Alpha", &[]).unwrap();
            note(g, alpha, "talking to myself, [[Alpha]]");
            let beta = g.create_entity("Beta", &[]).unwrap();
            let outer = note(g, beta, "see [[Alpha]]");
            note(g, outer, "and again [[Alpha]]");
            note(g, beta, "unrelated");
            let to_alpha = Value::Link(Link {
                name: "Alpha".into(),
                qualifiers: Vec::new(),
            });
            let gamma = g
                .create_entity("Gamma", &[("type", Some(to_alpha))])
                .unwrap();
            note(g, gamma, "[[Alpha]] too, and [[Nope]]");
        });
        let refs = v.references(&id_of(&v, "Alpha")).unwrap();
        assert_eq!(titles(&refs), ["Beta", "Gamma"]);
        let beta = &refs[0];
        assert_eq!(
            beta.blocks.len(),
            1,
            "the inner block is shown inside the outer"
        );
        assert_eq!(beta.blocks[0].text, "see [[Alpha]]");
        assert_eq!(beta.blocks[0].children[0].text, "and again [[Alpha]]");
        let gamma = &refs[1];
        assert_eq!(gamma.blocks.len(), 2, "its property and its note");
        assert_eq!(gamma.links["Nope"], LinkTarget::Missing);
        assert!(matches!(gamma.links["Alpha"], LinkTarget::Entity { .. }));
    }

    #[test]
    fn references_list_journals_first_and_newest_first() {
        let v = view_with(|g| {
            let index = quiddity_core::Index::build(g).unwrap();
            g.create_entity("Alpha", &[]).unwrap();
            let zed = g.create_entity("Zed", &[]).unwrap();
            note(g, zed, "[[Alpha]]");
            for days in [3, 1, 2] {
                let date = today().checked_sub(days.days()).unwrap();
                let day = quiddity_core::ensure_journal(g, &index, date).unwrap();
                note(g, day, "[[Alpha]]");
            }
        });
        let refs = v.references(&id_of(&v, "Alpha")).unwrap();
        assert_eq!(
            titles(&refs),
            ["2026-10-25", "2026-10-24", "2026-10-23", "Zed"]
        );
    }

    #[test]
    fn unlinked_references_are_whole_word_prose_mentions() {
        let v = view_with(|g| {
            g.create_entity("Alpha", &[("alias", text("Omega"))])
                .unwrap();
            let beta = g.create_entity("Beta", &[]).unwrap();
            note(g, beta, "alpha is mentioned");
            note(g, beta, "Alphabet soup");
            note(g, beta, "[[Alpha]] is linked");
            note(g, beta, "[[Alpha Centauri]] is somewhere else");
            note(g, beta, "alias: ((alpha)) is a reference");
            let gamma = g
                .create_entity("Gamma", &[("type", text("alpha"))])
                .unwrap();
            note(g, gamma, "ALPHA, again!");
            let delta = g.create_entity("Delta", &[]).unwrap();
            note(g, delta, "omega here, via an alias");
            g.create_entity("Alpha beta", &[]).unwrap();
        });
        let refs = v.unlinked_references(&id_of(&v, "Alpha")).unwrap();
        assert_eq!(titles(&refs), ["Beta", "Delta", "Gamma"]);
        let text = |r: &Reference| texts(&r.blocks).join("|");
        assert_eq!(text(&refs[0]), "alpha is mentioned");
        assert_eq!(text(&refs[1]), "omega here, via an alias");
        assert_eq!(text(&refs[2]), "ALPHA, again!", "not the type:: property");
    }

    #[test]
    fn aliases_written_as_links_count_too() {
        let omega = Value::Link(Link {
            name: "Omega".into(),
            qualifiers: Vec::new(),
        });
        let v = view_with(|g| {
            g.create_entity("Alpha", &[("alias", Some(omega))]).unwrap();
            let beta = g.create_entity("Beta", &[]).unwrap();
            note(g, beta, "omega, by its alias");
        });
        let refs = v.unlinked_references(&id_of(&v, "Alpha")).unwrap();
        assert_eq!(titles(&refs), ["Beta"]);
    }

    #[test]
    fn a_block_that_links_is_not_also_an_unlinked_mention() {
        let v = view_with(|g| {
            g.create_entity("Alpha", &[]).unwrap();
            let beta = g.create_entity("Beta", &[]).unwrap();
            note(g, beta, "[[Alpha]], and alpha again in prose");
        });
        let alpha = id_of(&v, "Alpha");
        assert_eq!(titles(&v.references(&alpha).unwrap()), ["Beta"]);
        assert!(v.unlinked_references(&alpha).unwrap().is_empty());
    }

    #[test]
    fn journals_have_no_unlinked_references() {
        let v = view_with(|_| {});
        let today = v.ensure_journal(today()).unwrap();
        assert!(v.unlinked_references(&today.id).unwrap().is_empty());
    }

    #[test]
    fn the_index_follows_edits_and_imports() {
        let v = view_with(|g| {
            let a = g.create_entity("A", &[]).unwrap();
            note(g, a, "no links yet");
            g.create_entity("B", &[]).unwrap();
        });
        let b = id_of(&v, "B");
        assert!(v.references(&b).unwrap().is_empty());
        let block = v.entity(&id_of(&v, "A")).unwrap().blocks[0].id.clone();
        v.splice_text(&block, 0, 2, "see [[B]] --").unwrap();
        assert_eq!(titles(&v.references(&b).unwrap()), ["A"]);

        // A change made behind the view's back is noticed once it says so.
        let other = Graph::new();
        other.import(&v.graph().snapshot().unwrap()).unwrap();
        let c = other.create_block(None, None, "C").unwrap();
        v.graph().import(&other.snapshot().unwrap()).unwrap();
        v.changed();
        assert!(v.entities().unwrap().iter().any(|e| e.id == c.to_string()));
    }
}

mod edits {
    use super::*;

    #[test]
    fn typing_splices_in_utf16_units() {
        let (v, page, ids) = page(&["a😀b"]);
        v.splice_text(&ids[0], 3, 1, "B").unwrap();
        v.splice_text(&ids[0], 0, 0, ">").unwrap();
        assert_eq!(top_level(&v, &page), [">a😀B"]);
        assert!(v.splice_text(&ids[0], 9, 0, "x").is_err());
        assert!(v.splice_text("junk", 0, 0, "x").is_err());
    }

    #[test]
    fn a_blocks_text_is_read_as_it_is_now() {
        let (v, _page, ids) = page(&["one"]);
        assert_eq!(v.block_text(&ids[0]).unwrap(), "one");
        v.splice_text(&ids[0], 3, 0, " two").unwrap();
        assert_eq!(v.block_text(&ids[0]).unwrap(), "one two");
        assert!(matches!(v.block_text("junk"), Err(Error::NotABlock(_))));
    }

    #[test]
    fn splitting_moves_the_tail_to_a_new_next_sibling() {
        let (v, page, ids) = page(&["hello world", "next"]);
        let new = v.split(&ids[0], 5).unwrap();
        assert_eq!(top_level(&v, &page), ["hello", " world", "next"]);
        assert_eq!(v.entity(&page).unwrap().blocks[1].id, new);
        let end = v.split(&ids[0], 5).unwrap();
        assert_eq!(top_level(&v, &page), ["hello", "", " world", "next"]);
        assert_ne!(end, new);
    }

    #[test]
    fn splitting_a_parent_makes_the_tail_its_first_child() {
        let (v, page, ids) = page(&["parent"]);
        let child = v.create_block(&ids[0], None, "child").unwrap();
        let new = v.split(&ids[0], 3).unwrap();
        let e = v.entity(&page).unwrap();
        assert_eq!(e.blocks.len(), 1);
        assert_eq!(e.blocks[0].text, "par");
        assert_eq!(texts(&e.blocks[0].children), ["ent", "child"]);
        assert_eq!(e.blocks[0].children[0].id, new);
        assert_eq!(e.blocks[0].children[1].id, child);
    }

    #[test]
    fn splitting_must_land_between_characters() {
        let (v, page, ids) = page(&["a😀b"]);
        assert!(matches!(v.split(&ids[0], 2), Err(Error::BadOffset(2))));
        assert!(matches!(v.split(&ids[0], 9), Err(Error::BadOffset(9))));
        assert_eq!(top_level(&v, &page), ["a😀b"], "nothing changed");
        v.split(&ids[0], 3).unwrap();
        assert_eq!(top_level(&v, &page), ["a😀", "b"]);
    }

    #[test]
    fn indenting_nests_under_the_sibling_before() {
        let (v, page, ids) = page(&["one", "two", "three"]);
        assert!(!v.indent(&ids[0]).unwrap(), "nothing before the first");
        assert!(v.indent(&ids[1]).unwrap());
        assert!(
            v.indent(&ids[2]).unwrap(),
            "under the one before, which is now one"
        );
        let e = v.entity(&page).unwrap();
        assert_eq!(texts(&e.blocks), ["one"]);
        assert_eq!(texts(&e.blocks[0].children), ["two", "three"]);
    }

    #[test]
    fn outdenting_follows_the_parent() {
        let (v, page, ids) = page(&["one", "two"]);
        let kid = v.create_block(&ids[0], None, "kid").unwrap();
        assert!(!v.outdent(&ids[0]).unwrap(), "already at the top");
        assert!(v.outdent(&kid).unwrap());
        assert_eq!(top_level(&v, &page), ["one", "kid", "two"]);
    }

    #[test]
    fn blocks_swap_with_their_neighbours() {
        let (v, page, ids) = page(&["a", "b", "c"]);
        assert!(!v.move_sibling(&ids[0], -1).unwrap());
        assert!(!v.move_sibling(&ids[2], 1).unwrap());
        assert!(v.move_sibling(&ids[0], 1).unwrap());
        assert_eq!(top_level(&v, &page), ["b", "a", "c"]);
        assert!(v.move_sibling(&ids[2], -1).unwrap());
        assert_eq!(top_level(&v, &page), ["b", "c", "a"]);
    }

    #[test]
    fn merging_joins_a_block_to_the_one_before_in_reading_order() {
        let (v, page, ids) = page(&["first", "second"]);
        let kid = v.create_block(&ids[0], None, "kid").unwrap();
        // "second" follows "kid" in reading order, not "first".
        let merged = v.merge_into_previous(&ids[1]).unwrap().unwrap();
        assert_eq!(merged, Merged { into: kid, at: 3 });
        let e = v.entity(&page).unwrap();
        assert_eq!(texts(&e.blocks), ["first"]);
        assert_eq!(texts(&e.blocks[0].children), ["kidsecond"]);
    }

    #[test]
    fn merging_leaves_parents_and_first_blocks_alone() {
        let (v, page, ids) = page(&["first", "second"]);
        v.create_block(&ids[1], None, "kid").unwrap();
        assert_eq!(
            v.merge_into_previous(&ids[0]).unwrap(),
            None,
            "nothing before it"
        );
        assert_eq!(
            v.merge_into_previous(&ids[1]).unwrap(),
            None,
            "it has children"
        );
        assert_eq!(top_level(&v, &page), ["first", "second"]);
    }

    #[test]
    fn an_empty_block_merges_away_leaving_the_caret_at_the_end_before() {
        let (v, page, ids) = page(&["keep", ""]);
        let merged = v.merge_into_previous(&ids[1]).unwrap().unwrap();
        assert_eq!(
            merged,
            Merged {
                into: ids[0].clone(),
                at: 4
            }
        );
        assert_eq!(top_level(&v, &page), ["keep"]);
    }

    #[test]
    fn new_entities_start_with_an_empty_block() {
        let v = view_with(|_| {});
        let id = v.create_entity("Fresh").unwrap();
        let e = v.entity(&id).unwrap();
        assert_eq!(e.title, "Fresh");
        assert_eq!(texts(&e.blocks), [""]);
        assert_eq!(v.entities().unwrap().len(), 1, "and the index saw it");
    }

    #[test]
    fn edits_travel_as_updates() {
        let (v, page, ids) = page(&["x"]);
        let replica = Graph::blank();
        replica.import(&v.graph().snapshot().unwrap()).unwrap();
        let before = v.graph().version();
        v.splice_text(&ids[0], 1, 0, "y").unwrap();
        v.split(&ids[0], 1).unwrap();
        replica
            .import(&v.graph().updates_since(&before).unwrap())
            .unwrap();
        assert_eq!(top_level(&View::new(replica).unwrap(), &page), ["x", "y"]);
    }
}

mod properties {
    use super::*;
    use crate::{Property, PropertyValue};

    /// The properties of a lone block with this text.
    fn of(text: &str) -> Vec<Property> {
        let (v, page, _) = page(&[text]);
        v.entity(&page).unwrap().blocks.remove(0).properties
    }

    fn text_value(s: &str) -> Option<PropertyValue> {
        Some(PropertyValue::Text { text: s.to_owned() })
    }

    #[test]
    fn line_properties_come_with_their_value_and_where_they_are() {
        let src = "prose\nstatus:: done\nrating:: 4";
        let props = of(src);
        assert_eq!(props.len(), 2);
        assert_eq!(props[0].key, "status");
        assert_eq!(props[0].value, text_value("done"));
        assert!(!props[0].inline);
        assert_eq!(&src[props[0].start..props[0].end], "status:: done");
        assert_eq!(&src[props[1].start..props[1].end], "rating:: 4");
    }

    #[test]
    fn inline_properties_are_marked_and_may_have_no_value() {
        let src = "see #urgent and #due(friday) ok";
        let props = of(src);
        let keys: Vec<_> = props.iter().map(|p| (p.key.as_str(), p.inline)).collect();
        assert_eq!(keys, [("urgent", true), ("due", true)]);
        assert_eq!(props[0].value, None);
        assert_eq!(props[1].value, text_value("friday"));
        assert_eq!(&src[props[1].start..props[1].end], "#due(friday)");
    }

    #[test]
    fn offsets_count_utf16_units_not_bytes() {
        let src = "😀 é\nkey:: v";
        let props = of(src);
        let units: Vec<u16> = src.encode_utf16().collect();
        let written = String::from_utf16(&units[props[0].start..props[0].end]).unwrap();
        assert_eq!(written, "key:: v");
        assert_eq!(
            props[0].start, 5,
            "emoji is two units, é one, a space, a newline"
        );
    }

    #[test]
    fn values_can_be_links_block_refs_and_lists() {
        let props = of("a:: [[Some Page]]\nb:: ((abc))\nc:: [x, [[Y]]]");
        assert_eq!(
            props[0].value,
            Some(PropertyValue::Link {
                name: "Some Page".into(),
                key: "Some Page".into()
            })
        );
        assert_eq!(
            props[1].value,
            Some(PropertyValue::Ref { id: "abc".into() })
        );
        let Some(PropertyValue::List { items }) = &props[2].value else {
            panic!("a list")
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], PropertyValue::Text { text: "x".into() });
        assert!(matches!(&items[1], PropertyValue::Link { name, .. } if name == "Y"));
    }

    #[test]
    fn a_links_key_finds_it_in_the_entitys_links() {
        let v = view_with(|g| {
            g.create_entity("Target", &[]).unwrap();
            let e = g.create_entity("Source", &[]).unwrap();
            note(g, e, "ref:: [[Target#type(thing)]]");
        });
        let e = v.entity(&id_of(&v, "Source")).unwrap();
        let Some(PropertyValue::Link { key, .. }) = &e.blocks[0].properties[0].value else {
            panic!("a link value")
        };
        assert!(e.links.contains_key(key), "{key:?} in {:?}", e.links.keys());
    }

    #[test]
    fn code_is_not_properties() {
        assert!(of("```\nnot:: a property\n```").is_empty());
        assert!(of("plain prose").is_empty());
    }

    #[test]
    fn references_carry_them_too() {
        let v = view_with(|g| {
            g.create_entity("Alpha", &[]).unwrap();
            let b = g.create_entity("Beta", &[]).unwrap();
            note(g, b, "[[Alpha]] mentioned\nstate:: open");
        });
        let refs = v.references(&id_of(&v, "Alpha")).unwrap();
        assert_eq!(refs[0].blocks[0].properties[0].key, "state");
    }
}

mod rename {
    use super::*;

    #[test]
    fn links_to_the_old_name_still_resolve() {
        let v = view_with(|g| {
            g.create_entity("Old name", &[]).unwrap();
            let other = g.create_entity("Other", &[]).unwrap();
            note(g, other, "see [[Old name]]");
        });
        let id = id_of(&v, "Old name");
        assert!(v.rename_entity(&id, "New name").unwrap());
        let e = v.entity(&id).unwrap();
        assert_eq!(e.title, "New name");
        assert_eq!(e.id, id, "the same entity");
        assert_eq!(titles(&v.references(&id).unwrap()), ["Other"]);
        let other = v.entity(&id_of(&v, "Other")).unwrap();
        assert_eq!(other.links["Old name"], LinkTarget::Entity { id });
    }

    #[test]
    fn the_old_name_is_visible_as_an_alias_in_the_properties() {
        let (v, page, _) = page(&["a note"]);
        v.rename_entity(&page, "Renamed").unwrap();
        let e = v.entity(&page).unwrap();
        assert_eq!(e.blocks[0].text, "alias:: Page");
        assert_eq!(e.blocks[0].properties[0].key, "alias");
        assert_eq!(texts(&e.blocks), ["alias:: Page", "a note"]);
    }

    #[test]
    fn the_same_name_and_an_empty_one_do_nothing() {
        let (v, page, _) = page(&["x"]);
        assert!(!v.rename_entity(&page, "Page").unwrap());
        assert!(v.rename_entity(&page, "  ").is_err());
        assert_eq!(v.entity(&page).unwrap().title, "Page");
        assert_eq!(top_level(&v, &page), ["x"], "no alias was added");
    }

    #[test]
    fn journals_and_property_entities_cannot_be_renamed() {
        let v = view_with(|g| {
            g.create_entity("rating", &[("type", text("property"))])
                .unwrap();
            g.create_entity("Ordinary", &[]).unwrap();
        });
        let day = v.ensure_journal(today()).unwrap();
        let rating = id_of(&v, "rating");
        for id in [&day.id, &rating] {
            assert!(!v.entity(id).unwrap().renamable);
            assert!(matches!(
                v.rename_entity(id, "x"),
                Err(Error::NotRenamable(_))
            ));
        }
        assert_eq!(v.entity(&day.id).unwrap().title, "2026-10-26", "untouched");
        assert!(v.entity(&id_of(&v, "Ordinary")).unwrap().renamable);
    }

    #[test]
    fn only_entities_can_be_renamed() {
        let (v, _page, ids) = page(&["x"]);
        assert!(matches!(
            v.rename_entity(&ids[0], "y"),
            Err(Error::NotAnEntity(_))
        ));
        assert!(v.rename_entity("junk", "y").is_err());
    }

    #[test]
    fn a_rename_ends_a_burst_of_typing_and_undoes_as_one_step() {
        let (v, page, ids) = page(&["x"]);
        v.graph().enable_undo();
        v.splice_text(&ids[0], 1, 0, "y").unwrap();
        v.rename_entity(&page, "Renamed").unwrap();
        assert!(v.undo().unwrap());
        let e = v.entity(&page).unwrap();
        assert_eq!(e.title, "Page", "the name and the alias came back together");
        assert_eq!(texts(&e.blocks), ["xy"], "and the typing was its own step");
        assert!(v.redo().unwrap());
        assert_eq!(v.entity(&page).unwrap().title, "Renamed");
    }
}

mod new_links {
    use super::*;

    /// Replaces a block's text as if typed, and saves it. Returns what saving
    /// made.
    fn edit(v: &View, block: &str, to: &str) -> Vec<String> {
        let before = v.block_text(block).unwrap();
        retype(v, block, to);
        v.commit_block(block, &before).unwrap()
    }

    fn retype(v: &View, block: &str, to: &str) {
        let len = v.block_text(block).unwrap().encode_utf16().count();
        v.splice_text(block, 0, len, to).unwrap();
    }

    fn names(v: &View) -> Vec<String> {
        let mut names: Vec<_> = v.entities().unwrap().into_iter().map(|e| e.title).collect();
        names.sort();
        names
    }

    fn only(made: Vec<String>) -> String {
        assert_eq!(made.len(), 1, "{made:?}");
        made.into_iter().next().unwrap()
    }

    #[test]
    fn following_a_link_makes_the_entity_once() {
        let (v, _page, _) = page(&[]);
        let id = v.follow_link("Nowhere").unwrap();
        let e = v.entity(&id).unwrap();
        assert_eq!(e.title, "Nowhere");
        assert_eq!(texts(&e.blocks), [""], "a block to type into");
        assert_eq!(v.follow_link("Nowhere").unwrap(), id);
        assert_eq!(v.follow_link("nowhere").unwrap(), id, "names ignore case");
        assert_eq!(names(&v), ["Nowhere", "Page"]);
    }

    #[test]
    fn following_a_link_to_what_exists_changes_nothing() {
        let (v, page, _) = page(&["x"]);
        assert_eq!(v.follow_link("Page").unwrap(), page);
        assert_eq!(top_level(&v, &page), ["x"]);
    }

    #[test]
    fn a_made_entity_has_the_properties_the_link_asked_for() {
        let (v, _page, _) = page(&[]);
        let id = v.follow_link("Jane #type(person)").unwrap();
        let e = v.entity(&id).unwrap();
        assert_eq!(e.title, "Jane");
        assert_eq!(e.blocks[0].text, "type:: person");
        assert_eq!(
            v.follow_link("Jane #type(person)").unwrap(),
            id,
            "it matches"
        );
        assert_ne!(v.follow_link("Jane #type(cat)").unwrap(), id);
    }

    #[test]
    fn a_link_that_is_not_one_or_goes_to_several_places_is_refused() {
        let v = view_with(|g| {
            g.create_entity("Twin", &[]).unwrap();
            g.create_entity("Twin", &[]).unwrap();
        });
        assert!(matches!(
            v.follow_link("Twin"),
            Err(Error::AmbiguousLink(_))
        ));
        for key in ["", "  "] {
            assert!(
                matches!(v.follow_link(key), Err(Error::NotALink(_))),
                "{key:?}"
            );
        }
        assert_eq!(names(&v), ["Twin", "Twin"], "nothing was made");
    }

    #[test]
    fn saving_makes_an_entity_for_each_link_it_gained() {
        let (v, page, ids) = page(&["start"]);
        let made = edit(&v, &ids[0], "see [[Alpha]] and [[Beta]] and [[Alpha]]");
        assert_eq!(made.len(), 2, "Alpha once");
        assert_eq!(names(&v), ["Alpha", "Beta", "Page"]);
        let links = v.entity(&page).unwrap().links;
        assert!(matches!(links["Alpha"], LinkTarget::Entity { .. }));
        assert!(matches!(links["Beta"], LinkTarget::Entity { .. }));
    }

    #[test]
    fn saving_leaves_alone_links_that_go_somewhere_or_were_already_there() {
        let (v, _page, ids) = page(&["x"]);
        v.follow_link("Known").unwrap();
        assert!(edit(&v, &ids[0], "[[Known]] [[known]]").is_empty());
        // Written elsewhere, and not touched by this edit.
        retype(&v, &ids[0], "[[Gone]] x");
        assert!(v.commit_block(&ids[0], "[[Gone]] x").unwrap().is_empty());
        assert!(edit(&v, &ids[0], "[[Gone]] xy").is_empty());
        assert_eq!(names(&v), ["Known", "Page"]);
    }

    #[test]
    fn saving_without_a_change_does_nothing_at_all() {
        let (v, _page, ids) = page(&["[[Gone]]"]);
        v.graph().enable_undo();
        assert!(v.commit_block(&ids[0], "[[Gone]]").unwrap().is_empty());
        assert!(!v.undo().unwrap(), "no step was made");
    }

    #[test]
    fn a_link_with_no_name_makes_nothing() {
        let (v, _page, ids) = page(&["x"]);
        assert!(edit(&v, &ids[0], "[[ ]] [[]]").is_empty());
        assert_eq!(names(&v), ["Page"]);
    }

    #[test]
    fn a_link_qualified_in_the_text_makes_a_matching_entity() {
        let (v, page, ids) = page(&["x"]);
        let id = only(edit(&v, &ids[0], "[[Jane #type(person)]]"));
        assert_eq!(v.entity(&id).unwrap().blocks[0].text, "type:: person");
        let links = v.entity(&page).unwrap().links;
        assert_eq!(links.len(), 1, "{links:?}");
        assert_eq!(
            links.into_values().next().unwrap(),
            LinkTarget::Entity { id }
        );
    }

    #[test]
    fn correcting_a_new_link_renames_its_entity_without_an_alias() {
        let (v, page, ids) = page(&["x"]);
        let id = only(edit(&v, &ids[0], "see [[Foo]]"));
        assert!(edit(&v, &ids[0], "see [[Fooo]]").is_empty(), "nothing new");
        let e = v.entity(&id).unwrap();
        assert_eq!(e.title, "Fooo");
        assert_eq!(texts(&e.blocks), [""], "and no alias");
        assert_eq!(names(&v), ["Fooo", "Page"]);
        let links = v.entity(&page).unwrap().links;
        assert_eq!(links["Fooo"], LinkTarget::Entity { id });
    }

    #[test]
    fn only_the_link_that_was_edited_is_corrected() {
        let (v, _page, ids) = page(&["x"]);
        let made = edit(&v, &ids[0], "[[A]] [[B]] [[C]]");
        assert_eq!(made.len(), 3);
        let (a, b, c) = (&made[0], &made[1], &made[2]);
        edit(&v, &ids[0], "[[A]] [[Bee]] [[C]]");
        assert_eq!(v.entity(a).unwrap().title, "A");
        assert_eq!(v.entity(b).unwrap().title, "Bee");
        assert_eq!(v.entity(c).unwrap().title, "C");
        // A link added in front of the others is a new one, not a rename.
        let d = only(edit(&v, &ids[0], "[[D]] [[A]] [[Bee]] [[C]]"));
        assert_eq!(v.entity(&d).unwrap().title, "D");
        assert_eq!(names(&v), ["A", "Bee", "C", "D", "Page"]);
        // Two edited at once are corrected in the order written.
        edit(&v, &ids[0], "[[D]] [[AA]] [[Bee]] [[CC]]");
        assert_eq!(v.entity(a).unwrap().title, "AA");
        assert_eq!(v.entity(c).unwrap().title, "CC");
        assert_eq!(names(&v), ["AA", "Bee", "CC", "D", "Page"]);
    }

    #[test]
    fn an_entity_that_has_been_used_keeps_its_name_when_the_link_changes() {
        let (v, _page, ids) = page(&["x"]);
        let id = only(edit(&v, &ids[0], "[[Foo]]"));
        let block = v.entity(&id).unwrap().blocks[0].id.clone();
        v.splice_text(&block, 0, 0, "notes").unwrap();
        let again = only(edit(&v, &ids[0], "[[Fob]]"));
        assert_ne!(again, id, "a new entity");
        assert_eq!(v.entity(&id).unwrap().title, "Foo");
    }

    #[test]
    fn an_entity_linked_from_elsewhere_keeps_its_name_when_the_link_changes() {
        let (v, _page, ids) = page(&["x", "y"]);
        let id = only(edit(&v, &ids[0], "[[Foo]]"));
        edit(&v, &ids[1], "[[Foo]]");
        let again = only(edit(&v, &ids[0], "[[Fob]]"));
        assert_ne!(again, id);
        assert_eq!(v.entity(&id).unwrap().title, "Foo");
    }

    #[test]
    fn a_property_value_linking_to_it_counts_as_use() {
        let (v, _page, ids) = page(&["x", "state:: [[Foo]]"]);
        let id = only(edit(&v, &ids[0], "[[Foo]]"));
        let again = only(edit(&v, &ids[0], "[[Fob]]"));
        assert_ne!(again, id);
        assert_eq!(v.entity(&id).unwrap().title, "Foo");
    }

    #[test]
    fn a_link_that_reached_its_entity_by_an_alias_does_not_rename_it() {
        let mut block = String::new();
        let v = view_with(|g| {
            let real = g.create_entity("Real", &[("alias", text("Nick"))]).unwrap();
            g.create_block(Some(real), None, "").unwrap();
            let host = g.create_entity("Host", &[]).unwrap();
            block = note(g, host, "[[Nick]]").to_string();
        });
        let made = only(edit(&v, &block, "[[Nik]]"));
        assert_ne!(made, id_of(&v, "Real"));
        assert_eq!(names(&v), ["Host", "Nik", "Real"]);
    }

    #[test]
    fn qualified_links_make_entities_but_are_not_corrected_in_place() {
        let (v, _page, ids) = page(&["x"]);
        let id = only(edit(&v, &ids[0], "[[Jane #type(person)]]"));
        let again = only(edit(&v, &ids[0], "[[Jan #type(person)]]"));
        assert_ne!(again, id);
        assert_eq!(v.entity(&id).unwrap().title, "Jane");
    }

    #[test]
    fn a_link_that_gains_a_qualifier_makes_an_entity_that_matches_it() {
        let (v, page, ids) = page(&["x"]);
        let plain = only(edit(&v, &ids[0], "[[Foo]]"));
        let typed = only(edit(&v, &ids[0], "[[Foo #type(person)]]"));
        assert_ne!(typed, plain, "the plain Foo can't match the qualifier");
        let links = v.entity(&page).unwrap().links;
        assert_eq!(
            links.into_values().next().unwrap(),
            LinkTarget::Entity { id: typed }
        );
        assert_eq!(v.entity(&plain).unwrap().title, "Foo");
    }

    #[test]
    fn saving_is_a_step_of_its_own_and_undoes_whole() {
        let (v, page, ids) = page(&["x"]);
        v.graph().enable_undo();
        v.splice_text(&ids[0], 1, 0, " [[Foo]]").unwrap();
        let made = v.commit_block(&ids[0], "x").unwrap();
        assert_eq!(made.len(), 1);
        assert!(v.undo().unwrap());
        assert_eq!(names(&v), ["Page"], "the entity went");
        assert_eq!(top_level(&v, &page), ["x [[Foo]]"], "the typing stayed");
        assert!(v.undo().unwrap());
        assert_eq!(top_level(&v, &page), ["x"]);
        assert!(v.redo().unwrap() && v.redo().unwrap());
        assert_eq!(names(&v), ["Foo", "Page"]);
    }

    #[test]
    fn a_correction_undoes_whole() {
        let (v, _page, ids) = page(&["x"]);
        v.graph().enable_undo();
        let id = only(edit(&v, &ids[0], "[[Foo]]"));
        edit(&v, &ids[0], "[[Fooo]]");
        assert!(v.undo().unwrap(), "the correction");
        assert_eq!(v.entity(&id).unwrap().title, "Foo");
    }
}

mod connections {
    use super::*;

    fn edges(v: &View) -> Vec<(String, String)> {
        let title = |id: &str| {
            let nodes = v.connections().unwrap().nodes;
            nodes.into_iter().find(|n| n.id == id).unwrap().title
        };
        v.connections()
            .unwrap()
            .edges
            .iter()
            .map(|e| (title(&e.source), title(&e.target)))
            .collect()
    }

    #[test]
    fn an_edge_is_one_entity_linking_to_another_however_often() {
        let v = view_with(|g| {
            let a = g.create_entity("A", &[]).unwrap();
            let b = g.create_entity("B", &[]).unwrap();
            g.create_entity("C", &[]).unwrap();
            let top = note(g, a, "see [[B]] and [[b]]");
            note(g, top, "again [[B]]");
            note(g, a, "owner:: [[C]]");
            note(g, b, "back to [[A]], and to [[B]] itself, and [[Nowhere]]");
        });
        let nodes: Vec<_> = v
            .connections()
            .unwrap()
            .nodes
            .into_iter()
            .map(|n| n.title)
            .collect();
        assert_eq!(
            nodes,
            ["A", "B", "C"],
            "as `entities`, and Nowhere isn't one"
        );
        let mut edges = edges(&v);
        edges.sort();
        let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
        assert_eq!(edges, [pair("A", "B"), pair("A", "C"), pair("B", "A")]);
    }

    #[test]
    fn a_duplicate_journals_links_belong_to_the_one_it_is_merged_into() {
        let props = [("type", text("journal")), ("date", text("2026-10-26"))];
        let v = view_with(|g| {
            let kept = g.create_entity("2026-10-26", &props).unwrap();
            let dup = g.create_entity("2026-10-26", &props).unwrap();
            let target = g.create_entity("Target", &[]).unwrap();
            note(g, kept, "kept");
            note(g, dup, "writes [[Target]]");
            let _ = target;
        });
        let connections = v.connections().unwrap();
        assert_eq!(
            connections.nodes.len(),
            2,
            "the duplicate is one with its twin"
        );
        assert_eq!(edges(&v), [("2026-10-26".to_owned(), "Target".to_owned())]);
    }

    #[test]
    fn an_empty_graph_has_no_edges() {
        let v = view_with(|_| {});
        assert_eq!(v.connections().unwrap().edges, []);
    }
}

mod summaries {
    use std::collections::BTreeSet;

    use super::*;
    use crate::query::distinguishing;

    fn row(v: &View, title: &str) -> Summary {
        v.summaries()
            .unwrap()
            .into_iter()
            .find(|s| s.title == title)
            .unwrap()
    }

    #[test]
    fn words_count_prose_but_not_the_title_or_property_lines() {
        let v = view_with(|g| {
            let a = g.create_entity("A long title indeed", &[]).unwrap();
            note(g, a, "alias:: other name");
            let top = note(g, a, "three words here");
            note(g, top, "  and  two ");
        });
        assert_eq!(row(&v, "A long title indeed").words, 5);
    }

    #[test]
    fn incoming_counts_linking_blocks_in_other_entities() {
        let v = view_with(|g| {
            let a = g.create_entity("A", &[]).unwrap();
            let b = g.create_entity("B", &[]).unwrap();
            note(g, a, "to [[B]]");
            note(g, a, "again [[B]]");
            note(g, b, "to itself [[B]] and [[A]]");
        });
        assert_eq!(row(&v, "B").incoming, 2);
        assert_eq!(row(&v, "A").incoming, 1);
    }

    #[test]
    fn tags_only_appear_to_tell_same_titled_entities_apart() {
        let v = view_with(|g| {
            let a = g.create_entity("Mercury", &[]).unwrap();
            note(g, a, "#planet #hot");
            // A property that isn't `tags` has a value, but it isn't a tag.
            note(g, a, "orbit:: fast");
            let b = g.create_entity("Mercury", &[]).unwrap();
            note(g, b, "tags:: [element, cold]");
            let c = g.create_entity("Venus", &[]).unwrap();
            note(g, c, "#planet");
        });
        let mercuries: Vec<_> = v
            .summaries()
            .unwrap()
            .into_iter()
            .filter(|s| s.title == "Mercury")
            .map(|s| s.tags)
            .collect();
        assert_eq!(mercuries, [["hot"], ["cold"]]);
        assert_eq!(row(&v, "Venus").tags, Vec::<String>::new());
    }

    #[test]
    fn created_and_modified_follow_the_history() {
        let mut b_text = None;
        let v = view_with(|g| {
            g.create_entity("A", &[]).unwrap();
            let b = g.create_entity("B", &[]).unwrap();
            b_text = Some(note(g, b, "first"));
        });
        let before = Timestamp::now().as_second();
        let (a, b) = (row(&v, "A"), row(&v, "B"));
        let made = a.created.expect("recorded");
        // Loro rounds to the nearest second, so it may be a second ahead.
        assert!(made.abs_diff(before) <= 2, "made just now");
        assert_eq!(b.created, a.created, "made in the same commit");
        assert_eq!(a.modified, a.created, "nothing since");

        std::thread::sleep(std::time::Duration::from_millis(1100));
        v.splice_text(&b_text.unwrap().to_string(), 0, 0, "re")
            .unwrap();
        let (a, b) = (row(&v, "A"), row(&v, "B"));
        assert_eq!(a.modified, a.created, "A was left alone");
        assert_eq!(
            b.created, a.created,
            "editing doesn't change when it was made"
        );
        assert!(b.modified > b.created, "{b:?}");
    }

    #[test]
    fn a_compacted_graph_has_no_created_time_but_still_summarises() {
        let g = Graph::new();
        let a = g.create_entity("A", &[]).unwrap();
        let text = note(&g, a, "one");
        g.commit();
        let at = g.version();
        g.splice_text(text, 0, 0, "x").unwrap();
        g.commit();
        let snapshot = g.shallow_snapshot(&at).unwrap();

        let v = View::new(Graph::from_snapshot(&snapshot).unwrap()).unwrap();
        let before = row(&v, "A");
        assert_eq!(before.created, None, "that history is gone");
        assert!(before.modified.is_some(), "the edit after it isn't");

        let text = v.entity(&id_of(&v, "A")).unwrap().blocks[0].id.clone();
        v.splice_text(&text, 0, 0, "y").unwrap();
        assert!(row(&v, "A").modified >= before.modified);
    }

    #[test]
    fn identical_tags_cannot_be_told_apart_and_show_none() {
        let v = view_with(|g| {
            for _ in 0..2 {
                let e = g.create_entity("Twin", &[]).unwrap();
                note(g, e, "#same");
            }
        });
        assert!(v.summaries().unwrap().iter().all(|s| s.tags.is_empty()));
    }

    #[test]
    fn distinguishing_takes_the_tag_that_rules_out_most_rivals() {
        let set = |t: &[&str]| t.iter().map(|s| (*s).to_owned()).collect::<BTreeSet<_>>();
        let own = set(&["a", "b", "c"]);
        let (r1, r2, r3) = (set(&["a", "b"]), set(&["a", "c"]), set(&["a"]));
        // "b" and "c" each rule out two rivals, "a" none; "b" first, then "c".
        assert_eq!(distinguishing(&own, &[&r1, &r2, &r3]), ["b", "c"]);
        assert!(distinguishing(&own, &[]).is_empty());
    }
}

mod property_links {
    use super::*;

    fn definitions(v: &View, page: &str) -> Vec<Option<String>> {
        let blocks = v.entity(page).unwrap().blocks;
        blocks
            .iter()
            .flat_map(|b| &b.properties)
            .map(|p| p.definition.clone())
            .collect()
    }

    #[test]
    fn a_property_points_at_the_entity_that_defines_it() {
        let mut status = String::new();
        let v = view_with(|g| {
            let defined = g
                .create_entity("Status", &[("type", text("property"))])
                .unwrap();
            status = defined.to_string();
            let page = g.create_entity("Page", &[]).unwrap();
            note(g, page, "status:: done");
            note(g, page, "later #STATUS(open) and #mood");
        });
        let page = id_of(&v, "Page");
        assert_eq!(
            definitions(&v, &page),
            [Some(status.clone()), Some(status), None],
            "found by name, ignoring case; mood has no definition"
        );
    }

    #[test]
    fn defining_a_property_makes_its_entity_once() {
        let (v, page, _) = page(&["mood:: calm"]);
        assert_eq!(definitions(&v, &page), [None]);
        let id = v.define_property("mood").unwrap();
        let e = v.entity(&id).unwrap();
        assert_eq!(e.title, "mood");
        assert!(!e.renamable, "a property is identified by its name");
        assert_eq!(texts(&e.blocks), ["type:: property", ""]);
        assert_eq!(definitions(&v, &page), [Some(id.clone())]);
        assert_eq!(v.define_property(" Mood ").unwrap(), id, "once");
        assert_eq!(v.entities().unwrap().len(), 2);
    }

    #[test]
    fn defining_a_property_that_is_defined_changes_nothing() {
        let v = view_with(|g| {
            g.create_entity("Status", &[("type", text("property"))])
                .unwrap();
        });
        let id = id_of(&v, "Status");
        assert_eq!(v.define_property("status").unwrap(), id);
        assert_eq!(v.entities().unwrap().len(), 1);
    }

    #[test]
    fn a_property_with_no_name_cannot_be_defined() {
        let (v, _page, _) = page(&[]);
        assert!(v.define_property("  ").is_err());
        assert_eq!(v.entities().unwrap().len(), 1);
    }

    #[test]
    fn defining_a_property_is_one_undo_step() {
        let (v, _page, _) = page(&[]);
        v.graph().enable_undo();
        v.define_property("mood").unwrap();
        assert!(v.undo().unwrap());
        assert_eq!(v.entities().unwrap().len(), 1);
    }
}

mod undo {
    use super::*;

    /// A page whose edits can be undone.
    fn undoable(blocks: &[&str]) -> (View, String, Vec<String>) {
        let (v, page, ids) = page(blocks);
        v.graph().enable_undo();
        (v, page, ids)
    }

    #[test]
    fn a_burst_of_typing_is_one_step_until_it_pauses() {
        let (v, page, ids) = undoable(&["x"]);
        for (i, c) in ["a", "b", "c"].iter().enumerate() {
            v.splice_text(&ids[0], 1 + i, 0, c).unwrap();
        }
        v.end_typing();
        v.splice_text(&ids[0], 4, 0, "!").unwrap();
        assert_eq!(top_level(&v, &page), ["xabc!"]);
        assert!(v.undo().unwrap());
        assert_eq!(
            top_level(&v, &page),
            ["xabc"],
            "the pause started a new step"
        );
        assert!(v.undo().unwrap());
        assert_eq!(top_level(&v, &page), ["x"], "and the burst came away whole");
        assert!(!v.undo().unwrap());
        assert!(v.redo().unwrap() && v.redo().unwrap());
        assert_eq!(top_level(&v, &page), ["xabc!"]);
    }

    #[test]
    fn typing_then_structure_are_separate_steps() {
        let (v, page, ids) = undoable(&["hello world"]);
        v.splice_text(&ids[0], 11, 0, "!").unwrap();
        v.split(&ids[0], 5).unwrap();
        assert_eq!(top_level(&v, &page), ["hello", " world!"]);
        assert!(v.undo().unwrap());
        assert_eq!(top_level(&v, &page), ["hello world!"], "the split alone");
        assert!(v.undo().unwrap());
        assert_eq!(top_level(&v, &page), ["hello world"], "then the typing");
    }

    #[test]
    fn a_merge_undoes_as_one_step_and_brings_both_blocks_back() {
        let (v, page, ids) = undoable(&["first", "second"]);
        v.merge_into_previous(&ids[1]).unwrap().unwrap();
        assert_eq!(top_level(&v, &page), ["firstsecond"]);
        assert!(v.undo().unwrap());
        assert_eq!(top_level(&v, &page), ["first", "second"]);
        assert!(v.redo().unwrap());
        assert_eq!(top_level(&v, &page), ["firstsecond"]);
    }

    #[test]
    fn moves_and_new_blocks_undo() {
        let (v, page, ids) = undoable(&["a", "b", "c"]);
        v.indent(&ids[1]).unwrap();
        v.move_sibling(&ids[2], -1).unwrap();
        v.create_block(&page, None, "d").unwrap();
        assert!(v.undo().unwrap());
        assert!(v.undo().unwrap());
        assert!(v.undo().unwrap());
        let e = v.entity(&page).unwrap();
        assert_eq!(texts(&e.blocks), ["a", "b", "c"]);
        assert!(e.blocks[0].children.is_empty(), "outdented again");
    }

    #[test]
    fn undo_refreshes_the_index() {
        let (v, _page, ids) = undoable(&["no link"]);
        v.create_entity("Target").unwrap();
        let target = id_of(&v, "Target");
        v.splice_text(&ids[0], 0, 7, "see [[Target]]").unwrap();
        assert_eq!(titles(&v.references(&target).unwrap()), ["Page"]);
        assert!(v.undo().unwrap());
        assert!(v.references(&target).unwrap().is_empty());
        assert!(v.redo().unwrap());
        assert_eq!(titles(&v.references(&target).unwrap()), ["Page"]);
    }

    #[test]
    fn a_new_entity_is_one_step() {
        let (v, _page, _ids) = undoable(&[]);
        v.create_entity("Fresh").unwrap();
        assert_eq!(v.entities().unwrap().len(), 2);
        assert!(v.undo().unwrap());
        assert_eq!(
            titles_of(&v),
            ["Page"],
            "entity and its first block together"
        );
    }

    fn titles_of(v: &View) -> Vec<String> {
        v.entities().unwrap().into_iter().map(|e| e.title).collect()
    }

    #[test]
    fn nothing_to_undo_is_not_an_error() {
        let (v, _page, _ids) = page(&["x"]);
        assert!(!v.undo().unwrap() && !v.redo().unwrap(), "undo not enabled");
        v.graph().enable_undo();
        assert!(!v.undo().unwrap() && !v.redo().unwrap(), "nothing yet");
    }
}

mod text {
    use super::super::query::{contains_word, without_links};

    #[test]
    fn links_and_block_refs_are_blanked_where_they_are() {
        assert_eq!(
            without_links("see [[Not alpha]] ok ((id)) end"),
            "see   ok   end"
        );
        assert_eq!(without_links("[[a]][[b]]tail"), "  tail");
        assert_eq!(without_links("an [[unclosed link"), "an [[unclosed link");
        assert_eq!(
            without_links("a ((open and [[x]] done"),
            "a ((open and   done"
        );
        assert_eq!(without_links("é[[é]]é"), "é é");
    }

    #[test]
    fn words_are_whole() {
        assert!(contains_word("an alpha here", "alpha"));
        assert!(contains_word("alpha", "alpha"));
        assert!(contains_word("(alpha)", "alpha"));
        assert!(!contains_word("alphabet", "alpha"));
        assert!(!contains_word("an_alpha", "alpha"));
        assert!(!contains_word("alphas", "alpha"));
        assert!(
            contains_word("alphabet alpha", "alpha"),
            "a later whole word counts"
        );
        assert!(contains_word("éalpha alpha", "alpha"));
    }
}

mod deleting_entities {
    use super::*;

    fn names(v: &View) -> Vec<String> {
        v.entities().unwrap().into_iter().map(|e| e.title).collect()
    }

    fn three() -> View {
        let v = view_with(|g| {
            for name in ["A", "B", "C"] {
                let e = g.create_entity(name, &[]).unwrap();
                note(g, e, "text");
            }
        });
        v.graph().enable_undo();
        v
    }

    #[test]
    fn the_chosen_entities_go_and_the_rest_stay() {
        let v = three();
        let gone = [id_of(&v, "A"), id_of(&v, "C")];
        assert_eq!(v.delete_entities(&gone).unwrap(), 2);
        assert_eq!(names(&v), ["B"]);
        assert!(v.entity(&gone[0]).is_err(), "and the entity is no more");
        assert_eq!(v.delete_entities(&[]).unwrap(), 0);
    }

    #[test]
    fn a_bad_id_stops_everything() {
        let (v, page, ids) = page(&["x"]);
        for bad in [ids[0].clone(), "junk".to_owned()] {
            assert!(matches!(
                v.delete_entities(&[page.clone(), bad]),
                Err(Error::NotAnEntity(_))
            ));
        }
        assert_eq!(
            names(&v),
            ["Page"],
            "a block isn't an entity, so nothing went"
        );
    }

    #[test]
    fn deleting_the_same_entity_twice_counts_it_once() {
        let v = three();
        let a = id_of(&v, "A");
        assert_eq!(v.delete_entities(&[a.clone(), a]).unwrap(), 1);
    }

    #[test]
    fn one_undo_brings_them_all_back() {
        let v = three();
        let gone = [id_of(&v, "A"), id_of(&v, "B")];
        v.delete_entities(&gone).unwrap();
        assert!(v.undo().unwrap());
        assert_eq!(names(&v), ["A", "B", "C"]);
        let a = id_of(&v, "A");
        assert_eq!(
            texts(&v.entity(&a).unwrap().blocks),
            ["text"],
            "with their blocks"
        );
        assert!(v.redo().unwrap());
        assert_eq!(names(&v), ["C"]);
    }

    #[test]
    fn a_duplicate_journal_goes_with_its_twin() {
        let props = [("type", text("journal")), ("date", text("2026-10-26"))];
        let v = view_with(|g| {
            g.create_entity("2026-10-26", &props).unwrap();
            g.create_entity("2026-10-26", &props).unwrap();
        });
        let day = id_of(&v, "2026-10-26");
        assert_eq!(v.delete_entities(&[day]).unwrap(), 1);
        assert_eq!(
            names(&v),
            Vec::<String>::new(),
            "or the twin would show instead"
        );
    }
}
