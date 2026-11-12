use proptest::prelude::*;

use super::*;
use crate::tests::view_with;

/// Few words, so that blocks share them, with both cases and a non-ASCII one.
fn word() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "loro", "Loro", "lore", "iroh", "Crab", "crabs", "é1", "ß2",
    ])
    .prop_map(str::to_owned)
}

fn words() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(word(), 0..6)
}

fn page_with(blocks: &[String]) -> View {
    view_with(|g| {
        let page = g.create_entity("Page", &[]).unwrap();
        for text in blocks {
            g.create_block(Some(page), None, text).unwrap();
        }
    })
}

fn texts(hits: &[BlockHit]) -> Vec<&str> {
    hits.iter().map(|h| h.text.as_str()).collect()
}

#[test]
fn search_ignores_unicode_form_and_case() {
    let v = page_with(&["Caf\u{e9} in Stra\u{df}e".to_owned()]);
    for query in ["Cafe\u{301}", "STRASSE", "cafe\u{301} in"] {
        let hits = v.search_text(query, 10).unwrap();
        assert_eq!(hits.len(), 1, "{query:?}");
    }
    // A decomposed block is found by the composed query, and its range covers
    // the accent too.
    let v = page_with(&["Cafe\u{301}".to_owned()]);
    let hits = v.search_text("caf\u{e9}", 10).unwrap();
    assert_eq!(hits.len(), 1);
}

proptest! {
    /// Words are letters, digits and marks, folded, and the offsets are UTF-16.
    #[test]
    fn tokens_cover_their_words(text in "[a-cA-C1 ,.🦀é-]{0,20}") {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut last = 0;
        for t in tokens(&text) {
            prop_assert!(last <= t.start && t.start < t.end && t.end <= units.len());
            last = t.end;
            let word = String::from_utf16(&units[t.start..t.end]).unwrap();
            prop_assert!(word.chars().all(is_word_char));
            prop_assert_eq!(fold(&word), t.text);
        }
        let all: usize = tokens(&text).iter().map(|t| t.end - t.start).sum();
        let alnum: usize = text.chars().filter(|&c| is_word_char(c)).map(char::len_utf16).sum();
        prop_assert_eq!(all, alnum);
    }

    /// A block is found exactly when each query word is one of its words,
    /// or, for the last, the start of one.
    #[test]
    fn found_when_every_word_matches(block in words(), query in words()) {
        let text = block.join(" ");
        let v = page_with(std::slice::from_ref(&text));
        let hits = v.search_text(&query.join(" "), 10).unwrap();
        let have: Vec<String> = block.iter().map(|w| w.to_lowercase()).collect();
        let want: Vec<String> = query.iter().map(|w| w.to_lowercase()).collect();
        let expected = !want.is_empty()
            && want.iter().enumerate().all(|(i, w)| {
                have.iter().any(|h| h == w || (i + 1 == want.len() && h.starts_with(w.as_str())))
            });
        prop_assert_eq!(!hits.is_empty(), expected, "{:?} in {:?}", query, text);
    }

    /// The ranges are words of the block that match some word of the query.
    #[test]
    fn ranges_mark_matching_words(block in words(), query in words()) {
        let text = block.join(" ");
        let v = page_with(std::slice::from_ref(&text));
        let units: Vec<u16> = text.encode_utf16().collect();
        for hit in v.search_text(&query.join(" "), 10).unwrap() {
            prop_assert!(!hit.ranges.is_empty());
            for (s, e) in hit.ranges {
                let word = String::from_utf16(&units[s..e]).unwrap().to_lowercase();
                prop_assert!(
                    query.iter().any(|q| word.starts_with(&q.to_lowercase())),
                    "{word:?} for {query:?}"
                );
            }
        }
    }
}

#[test]
fn a_query_with_no_words_finds_nothing() {
    let v = page_with(&["loro".into()]);
    for q in ["", "  ", "-,."] {
        assert!(v.search_text(q, 10).unwrap().is_empty());
    }
}

#[test]
fn the_last_word_may_be_unfinished_but_the_others_may_not() {
    let v = page_with(&["notes on loro".into()]);
    assert_eq!(v.search_text("notes lo", 10).unwrap().len(), 1);
    assert!(v.search_text("not loro", 10).unwrap().is_empty());
}

#[test]
fn rarer_words_shorter_blocks_and_phrases_rank_higher() {
    // "crab" is in all but one block; "loro" is rare.
    let v = page_with(&[
        "crab crab crab".into(),
        "crab and a long wander about before loro".into(),
        "crab loro".into(),
    ]);
    // Both words: the shorter block wins, where each has the words once.
    let hits = v.search_text("crab loro", 10).unwrap();
    assert_eq!(
        texts(&hits),
        ["crab loro", "crab and a long wander about before loro"]
    );

    let v = page_with(&[
        "alpha x beta".into(),
        "alpha beta".into(),
        "beta alpha".into(),
    ]);
    let hits = v.search_text("alpha beta", 10).unwrap();
    assert_eq!(texts(&hits)[0], "alpha beta");

    // The rarer word counts for more than the common one.
    let v = page_with(&[
        "common common common common".into(),
        "common rare".into(),
        "common".into(),
        "common".into(),
    ]);
    assert_eq!(
        texts(&v.search_text("common rare", 10).unwrap()),
        ["common rare"]
    );
    let rare = v.search_text("rare", 10).unwrap()[0].score;
    let common = v.search_text("common", 10).unwrap()[0].score;
    assert!(rare > common);
}

#[test]
fn hits_say_which_entity_and_block_and_where() {
    let v = view_with(|g| {
        let a = g.create_entity("Alpha", &[]).unwrap();
        g.create_block(Some(a), None, "nothing here").unwrap();
        let b = g.create_entity("Beta", &[]).unwrap();
        let parent = g.create_block(Some(b), None, "top").unwrap();
        g.create_block(Some(parent), None, "a 🦀 Crab").unwrap();
    });
    let hits = v.search_text("crab", 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].entity.title, "Beta");
    assert_eq!(hits[0].text, "a 🦀 Crab");
    assert_eq!(hits[0].ranges, vec![(5, 9)]);
    assert!(!hits[0].block.is_empty());
}

#[test]
fn a_name_is_not_text_and_the_limit_applies() {
    let v = view_with(|g| {
        let page = g.create_entity("Loro", &[]).unwrap();
        for text in ["loro one", "loro two", "loro three"] {
            g.create_block(Some(page), None, text).unwrap();
        }
    });
    // The empty first block of the entity has no words; the title isn't a block.
    let all = v.search_text("loro", 10).unwrap();
    assert_eq!(all.len(), 3);
    assert_eq!(v.search_text("loro", 2).unwrap().len(), 2);
    // Best first.
    let scores: Vec<f64> = all.iter().map(|h| h.score).collect();
    assert!(scores.windows(2).all(|w| w[0] >= w[1]));
}

fn close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-9, "{a} is not {b}");
}

#[test]
fn the_weights_are_bm25s() {
    // Worked by hand with K1 = 1.2 and B = 0.75.
    close(idf(10.0, 1.0), (1.0f64 + 9.5 / 1.5).ln());
    close(idf(10.0, 10.0), (1.0f64 + 0.5 / 10.5).ln());
    // 2 * 3 * 2.2 / (3 + 1.2 * (0.25 + 0.75 * 2)).
    close(term_score(2.0, 3.0, 2.0), 13.2 / 5.1);
    close(term_score(2.0, 3.0, 1.0), 13.2 / 4.2);
    // No repeats and an average length: just the idf.
    close(term_score(2.0, 1.0, 1.0), 2.0);
}

#[test]
fn a_score_is_the_idf_and_term_score_of_the_corpus() {
    // The page's own first block is empty, so count what is there.
    let v = page_with(&["x y".into(), "z".into()]);
    let blocks = v.graph().blocks().len() - v.graph().roots().len();
    let words = 3.0;
    let average = words / blocks as f64;
    let hit = &v.search_text("x", 10).unwrap()[0];
    close(
        hit.score,
        term_score(idf(blocks as f64, 1.0), 1.0, 2.0 / average),
    );
    // Together and in order is worth more, for a phrase: the same words, in
    // the same number, and only the order differs.
    let score = |text: &str| page_with(&[text.into()]).search_text("x y", 10).unwrap()[0].score;
    close(score("x y z"), score("y x z") * PHRASE);
}
