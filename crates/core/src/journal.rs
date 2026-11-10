//! Journals: an entity with `##journal(2026-10-25)` in its properties block is
//! that day's journal, and the day is what identifies it.

use jiff::civil::Date;
use quiddity_syntax::{MetaProperty, format_meta_line, normalise_name};

use crate::{BlockId, Graph, Index, Result};

/// The `##` property that makes an entity a day's journal.
pub const KEY: &str = "journal";

/// Whether a `##` line is a `##journal` line.
pub fn is_journal(meta: &MetaProperty) -> bool {
    normalise_name(&meta.key) == KEY
}

/// The day of the first `##journal` line, if it reads as one.
pub fn date(meta: &[MetaProperty]) -> Option<Date> {
    let first = meta.iter().find(|m| is_journal(m))?;
    first.parameter.as_deref()?.parse().ok()
}

/// What is wrong with an entity's `##journal` lines, in words.
pub(crate) fn problems(meta: &[MetaProperty]) -> Vec<&'static str> {
    let mut lines = meta.iter().filter(|m| is_journal(m));
    let mut problems = Vec::new();
    if let Some(first) = lines.next() {
        match first.parameter.as_deref() {
            None => problems.push("needs a day: ##journal(2026-10-25)"),
            Some(p) if p.parse::<Date>().is_err() => {
                problems.push("must be a day written like 2026-10-25");
            }
            Some(_) => {}
        }
    }
    if lines.next().is_some() {
        problems.push("an entity is the journal of one day only");
    }
    problems
}

/// Returns the journal page for `date`, creating it if the index has none.
///
/// Two devices doing this offline will each create a page; once they sync,
/// [`crate::merge_duplicates`] folds them together.
pub fn ensure_journal(graph: &Graph, index: &Index, date: Date) -> Result<BlockId> {
    if let Some(id) = index.journal(date) {
        return Ok(id);
    }
    create_journal(graph, date)
}

/// Makes the journal page for `date`, whether or not there is one already.
pub fn create_journal(graph: &Graph, date: Date) -> Result<BlockId> {
    let date = date.to_string();
    let day = graph.create_entity(&date, &[])?;
    graph.create_block(Some(day), None, &format_meta_line(KEY, Some(&date)))?;
    Ok(day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_once() -> Result<()> {
        let graph = Graph::new();
        let date: Date = "2026-10-25".parse().unwrap();
        let day = ensure_journal(&graph, &Index::build(&graph)?, date)?;
        assert_eq!(graph.entity_name(day)?, "2026-10-25");
        assert_eq!(graph.text(graph.children(day)[0])?, "##journal(2026-10-25)");
        assert_eq!(ensure_journal(&graph, &Index::build(&graph)?, date)?, day);
        Ok(())
    }

    fn meta(text: &str) -> Vec<MetaProperty> {
        quiddity_syntax::parse_block(text).meta
    }

    #[test]
    fn the_first_journal_line_gives_the_day() {
        let day: Date = "2026-10-25".parse().unwrap();
        assert_eq!(date(&meta("##journal(2026-10-25)")), Some(day));
        assert_eq!(date(&meta("##Journal( 2026-10-25 )")), Some(day));
        assert_eq!(
            date(&meta("##journal(2026-10-25)\n##journal(2026-10-26)")),
            Some(day)
        );
        for not in [
            "",
            "##journal",
            "##journal(soon)",
            "##journal(2026-02-30)",
            "##journal:: 2026-10-25",
            "##journals(2026-10-25)",
        ] {
            assert_eq!(date(&meta(not)), None, "{not:?}");
        }
        assert_eq!(
            date(&meta("##journal(soon)\n##journal(2026-10-25)")),
            None,
            "only the first counts, so what is flagged is what is read"
        );
    }

    #[test]
    fn problems_are_said() {
        assert!(problems(&meta("##journal(2026-10-25)")).is_empty());
        assert!(problems(&meta("##tag:: [a]")).is_empty());
        assert_eq!(
            problems(&meta("##journal")),
            ["needs a day: ##journal(2026-10-25)"]
        );
        assert_eq!(
            problems(&meta("##journal(soon)")),
            ["must be a day written like 2026-10-25"]
        );
        assert_eq!(
            problems(&meta("##journal(2026-10-25)\n##journal(2026-10-26)")),
            ["an entity is the journal of one day only"]
        );
        assert_eq!(problems(&meta("##journal\n##journal(2026-10-26)")).len(), 2);
    }
}
