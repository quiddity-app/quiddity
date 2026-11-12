//! Search: fuzzy matching of names, and the title search built on it.

use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::BTreeMap;

use quiddity_core::syntax::{
    Link, Qualifier, Value, fold, format_qualifier, is_mark, is_word_char, nfc,
};
use quiddity_core::{BlockId, EntityInfo, Index};
use serde::Serialize;

use crate::query::{EntityRef, link_key, link_qualifiers};
use crate::{Result, View};

/// What a matched character is worth, before bonuses.
const MATCH: i32 = 10;
/// For matching the first character of a word.
const WORD_START: i32 = 15;
/// For matching straight after the previous character, so that a run typed
/// together beats the same letters scattered.
const ADJACENT: i32 = 20;
/// For starting at the first character of the name.
const PREFIX: i32 = 10;
/// For each stretch of skipped characters between two matches.
const GAP: i32 = 8;
/// For matching the case that was typed, which settles ties.
const SAME_CASE: i32 = 1;

/// Where a query matched a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Match {
    /// Higher is better, and only means anything beside the scores of other
    /// names for the same query.
    pub score: i32,
    /// The matched text as `[start, end)` runs, in UTF-16 units of the name
    /// and ascending, for highlighting.
    pub ranges: Vec<(usize, usize)>,
}

struct QueryChar {
    /// What it matches, ignoring case.
    folded: char,
    /// As typed.
    typed: char,
    /// Must match the first character of a word (or, for a capital, the same
    /// capital): typed as a capital, or a letter or digit straight after a space.
    word_start: bool,
}

/// Pushes what `c` compares as (see `quiddity_core::syntax::fold`), which is
/// more than one character for `ß` and `ﬁ` and none for a zero-width space.
fn push_folded(c: char, out: &mut Vec<char>) {
    if c.is_ascii() {
        out.push(c.to_ascii_lowercase());
    } else {
        out.extend(fold(c.encode_utf8(&mut [0; 4])).chars());
    }
}

/// A character of the name as it is matched. One character of the name is
/// several when it folds to several, and none when it folds to nothing.
struct Unit {
    folded: char,
    /// The index in the name's characters, which tell what was typed, where
    /// words start, and where it is in the text.
    src: usize,
    /// Whether this is the first of the units of its character: the one that
    /// can start a word or be the same capital.
    first: bool,
}

/// The characters of `name` with where each is in `name`, in UTF-16 units.
///
/// A name that isn't NFC, as one made before text was stored composed may not
/// be, is composed here, a letter with its marks at a time. The characters of
/// one such group share its span, so a highlight covers whatever was written.
fn compose(name: &str) -> (Vec<char>, Vec<(usize, usize)>) {
    let mut chars = Vec::with_capacity(name.len());
    let mut spans = Vec::with_capacity(name.len());
    let mut units = 0;
    if matches!(nfc(name), Cow::Borrowed(_)) {
        for c in name.chars() {
            chars.push(c);
            spans.push((units, units + c.len_utf16()));
            units += c.len_utf16();
        }
        return (chars, spans);
    }
    let mut group = String::new();
    let mut start = 0;
    let flush = |group: &mut String,
                 start: usize,
                 end: usize,
                 chars: &mut Vec<char>,
                 spans: &mut Vec<(usize, usize)>| {
        for c in nfc(group).chars() {
            chars.push(c);
            spans.push((start, end));
        }
        group.clear();
    };
    for c in name.chars() {
        if !group.is_empty() && !is_mark(c) {
            flush(&mut group, start, units, &mut chars, &mut spans);
            start = units;
        }
        group.push(c);
        units += c.len_utf16();
    }
    flush(&mut group, start, units, &mut chars, &mut spans);
    (chars, spans)
}

/// The characters that must be matched: whitespace separates and is never
/// matched itself, but makes the next character start a word.
fn query_chars(query: &str) -> Vec<QueryChar> {
    let mut out = Vec::new();
    let mut after_space = false;
    let mut folded = Vec::new();
    for c in nfc(query).chars() {
        if c.is_whitespace() {
            after_space = true;
            continue;
        }
        let word_start =
            c.is_uppercase() || (after_space && c.is_alphanumeric() && !out.is_empty());
        folded.clear();
        push_folded(c, &mut folded);
        // Only the first of the characters a letter folds to starts a word.
        for (k, &f) in folded.iter().enumerate() {
            out.push(QueryChar {
                folded: f,
                typed: c,
                word_start: word_start && k == 0,
            });
        }
        after_space = false;
    }
    out
}

/// Whether each character begins a word: the first letter or digit after
/// anything else, a capital after a lower-case letter (`camelCase`), and the
/// last capital of a run that a lower-case letter follows (`HTTPServer`).
fn word_starts(chars: &[char]) -> Vec<bool> {
    (0..chars.len())
        .map(|i| {
            let c = chars[i];
            let Some(&prev) = i.checked_sub(1).map(|p| &chars[p]) else {
                return c.is_alphanumeric();
            };
            c.is_alphanumeric()
                && (!is_word_char(prev)
                    || (c.is_uppercase() && prev.is_lowercase())
                    || (c.is_uppercase()
                        && prev.is_uppercase()
                        && chars.get(i + 1).is_some_and(|n| n.is_lowercase())))
        })
        .collect()
}

/// Matches `query` against `name` the way an IDE's "go to" does: the query's
/// characters must appear in order, ignoring case, except that
///
/// - a capital must be the first letter of a word (or the same capital), so
///   `QD` finds `Quiddity Design`, and
/// - a character after a space must also start a word, so `quid d` doesn't
///   find `Quiddity`.
///
/// Of the ways to line the query up, the best scoring is chosen: runs typed
/// together, word starts and the start of the name all count for more.
/// `None` if it doesn't match, or the query is only whitespace.
#[must_use]
pub fn fuzzy_match(query: &str, name: &str) -> Option<Match> {
    let query = query_chars(query);
    let (chars, spans) = compose(name);
    let starts = word_starts(&chars);
    let mut units = Vec::with_capacity(chars.len());
    let mut folded = Vec::new();
    for (src, &c) in chars.iter().enumerate() {
        folded.clear();
        push_folded(c, &mut folded);
        units.extend(folded.iter().enumerate().map(|(k, &folded)| Unit {
            folded,
            src,
            first: k == 0,
        }));
    }
    let (m, n) = (query.len(), units.len());
    if m == 0 || m > n {
        return None;
    }

    let fits = |i: usize, j: usize| {
        let (q, u) = (&query[i], &units[j]);
        q.folded == u.folded
            && (!q.word_start
                || (u.first
                    && (starts[u.src] || (q.typed.is_uppercase() && chars[u.src] == q.typed))))
    };
    let here = |i: usize, j: usize| {
        let u = &units[j];
        MATCH
            + if u.first && starts[u.src] {
                WORD_START
            } else {
                0
            }
            + if u.first && chars[u.src] == query[i].typed {
                SAME_CASE
            } else {
                0
            }
    };

    // best[i][j]: the best score with query[i] matched at chars[j]; from[i][j]
    // is where query[i - 1] was in that alignment.
    let mut best = vec![vec![None::<i32>; n]; m];
    let mut from = vec![vec![0usize; n]; m];
    for j in (0..n).filter(|&j| fits(0, j)) {
        best[0][j] = Some(here(0, j) + if j == 0 { PREFIX } else { 0 });
    }
    for i in 1..m {
        // The best earlier match that leaves a gap before j, as j advances.
        let mut gapped: Option<(i32, usize)> = None;
        for j in i..n {
            if j >= 2 {
                let earlier = best[i - 1][j - 2].map(|s| (s, j - 2));
                gapped = gapped.into_iter().chain(earlier).max_by_key(|&(s, _)| s);
            }
            if !fits(i, j) {
                continue;
            }
            let adjacent = best[i - 1][j - 1].map(|s| (s + ADJACENT, j - 1));
            let gapped = gapped.map(|(s, k)| (s - GAP, k));
            let Some((score, k)) = [adjacent, gapped]
                .into_iter()
                .flatten()
                .max_by_key(|&(s, _)| s)
            else {
                continue;
            };
            best[i][j] = Some(score + here(i, j));
            from[i][j] = k;
        }
    }

    // The earliest of the equally good ends, so the answer doesn't depend on
    // iteration order.
    let (end, score) = (0..n).filter_map(|j| best[m - 1][j].map(|s| (j, s))).fold(
        None,
        |top: Option<(usize, i32)>, (j, s)| match top {
            Some((_, t)) if t >= s => top,
            _ => Some((j, s)),
        },
    )?;

    let mut at = vec![0; m];
    at[m - 1] = end;
    for i in (1..m).rev() {
        at[i - 1] = from[i][at[i]];
    }
    Some(Match {
        // Shorter names beat longer ones that match the same way.
        score: score - chars.len() as i32,
        ranges: utf16_runs(&at.iter().map(|&u| spans[units[u].src]).collect::<Vec<_>>()),
    })
}

/// The spans of the matched characters, in order, as runs: spans that touch
/// or share text make one.
fn utf16_runs(matched: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for &(start, end) in matched {
        match runs.last_mut() {
            Some(run) if start <= run.1 => run.1 = run.1.max(end),
            _ => runs.push((start, end)),
        }
    }
    runs
}

/// A property of an entity, for showing beside it in a list of choices.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HitProperty {
    pub key: String,
    /// The value as plain text, or `None` for a bare `#tag`.
    pub value: Option<String>,
}

/// An entity whose title matched a search.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TitleHit {
    pub entity: EntityRef,
    pub score: i32,
    /// As [`Match::ranges`], in the title, or in `alias` when there is one.
    pub ranges: Vec<(usize, usize)>,
    /// The alias that matched, when it beat the title: what the entity was
    /// called, or is also called. The `link` is still to the title, which
    /// the alias reaches.
    pub alias: Option<String>,
    /// The entity's properties, without repeats, in the order written.
    pub properties: Vec<HitProperty>,
    /// The `#key(value)` qualifiers that make a link to this title reach this
    /// entity and no other, as they are written after the name. Empty when
    /// the title is not shared, or when nothing tells the entities apart.
    pub qualifiers: String,
    /// What to write between the brackets to link here: the title, escaped
    /// as needed, then the qualifiers.
    pub link: String,
}

/// A value as plain text.
fn plain(value: &Value) -> String {
    match value {
        Value::Text(t) => t.clone(),
        Value::Link(l) => l.name.clone(),
        Value::BlockRef(id) => format!("(({id}))"),
        Value::List(items) => items.iter().map(plain).collect::<Vec<_>>().join(", "),
    }
}

/// The properties of `id` as shown in a list of choices.
fn properties_of(index: &Index, id: BlockId) -> Vec<HitProperty> {
    let mut out: Vec<HitProperty> = Vec::new();
    for p in index.entity(id).map_or(&[][..], |e| &e.properties) {
        let shown = HitProperty {
            key: p.key.clone(),
            value: p.value.as_ref().map(plain),
        };
        if !out.contains(&shown) {
            out.push(shown);
        }
    }
    out
}

/// Qualifiers that, written after the title, pick `id` out from every other
/// entity a link to that title could reach (those with the same name or
/// alias). Greedy: each is the one that rules out most of the entities still
/// in the running, preferring `#key(value)` to a bare `#key`, which a rival
/// may come to have. Empty if there are no rivals, or some can't be told apart.
fn qualifiers_for(index: &Index, id: BlockId, title: &str) -> Vec<Qualifier> {
    let Some(own) = index.entity(id) else {
        return Vec::new();
    };
    let mut rivals: Vec<_> = index
        .named(title)
        .into_iter()
        .filter(|r| index.canonical(*r) != id)
        .filter_map(|r| index.entity(r))
        .collect();
    if rivals.is_empty() {
        return Vec::new();
    }
    // Every way of narrowing a link that this entity satisfies, by how it is
    // written. A reference to a block can't be written into a link.
    let mut candidates: BTreeMap<String, Qualifier> = BTreeMap::new();
    for p in &own.properties {
        let mut add = |value: Option<Value>| {
            let written = format_qualifier(&p.key, value.as_ref());
            candidates.entry(written).or_insert(Qualifier {
                key: p.key.clone(),
                value,
            });
        };
        add(None);
        let items = match &p.value {
            Some(Value::List(items)) => items.clone(),
            Some(v) => vec![v.clone()],
            None => vec![],
        };
        for item in items
            .into_iter()
            .filter(|i| !matches!(i, Value::BlockRef(_)))
        {
            add(Some(item));
        }
    }
    let mut chosen = Vec::new();
    while !rivals.is_empty() {
        let best = candidates
            .iter()
            .map(|(written, q)| {
                let ruled_out = rivals.iter().filter(|r| !r.matches(q)).count();
                (ruled_out, q.value.is_some(), Reverse(written), q)
            })
            .filter(|(n, ..)| *n > 0)
            .max_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
        let Some((.., q)) = best else {
            return Vec::new();
        };
        chosen.push(q.clone());
        rivals.retain(|r| r.matches(q));
    }
    chosen
}

impl View {
    /// The entities whose titles or aliases match `query` (see
    /// [`fuzzy_match`]), best first and at most `limit`. An entity is found
    /// once, by whichever of its names matches best, the title if it ties.
    /// Equal scores are in title order.
    pub fn search_titles(&self, query: &str, limit: usize) -> Result<Vec<TitleHit>> {
        let mut hits: Vec<(EntityRef, Match, Option<String>)> = Vec::new();
        let index = self.index()?;
        for entity in self.entities()? {
            // A name nobody has made has no aliases.
            let aliases = if crate::virtual_entity::name_of(&entity.id).is_some() {
                Vec::new()
            } else {
                let id = self.entity_id(&entity.id)?;
                index
                    .entity(id)
                    .map(EntityInfo::aliases)
                    .unwrap_or_default()
            };
            let mut best = fuzzy_match(query, &entity.title).map(|m| (m, None));
            for alias in aliases {
                let alias = plain(alias);
                let Some(m) = fuzzy_match(query, &alias) else {
                    continue;
                };
                if best.as_ref().is_none_or(|(b, _)| m.score > b.score) {
                    best = Some((m, Some(alias)));
                }
            }
            if let Some((m, alias)) = best {
                hits.push((entity, m, alias));
            }
        }
        // Stable, and `entities` is in title order.
        hits.sort_by_key(|(_, m, _)| Reverse(m.score));
        hits.truncate(limit);
        // A name nobody has made has no properties and no rivals to tell apart.
        let ids = hits
            .iter()
            .map(|(e, ..)| {
                if crate::virtual_entity::name_of(&e.id).is_some() {
                    Ok(None)
                } else {
                    self.entity_id(&e.id).map(Some)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let index = self.index()?;
        Ok(hits
            .into_iter()
            .zip(ids)
            .map(|((entity, m, alias), id)| {
                let link = Link {
                    name: entity.title.clone(),
                    qualifiers: id
                        .map_or_else(Vec::new, |id| qualifiers_for(&index, id, &entity.title)),
                };
                TitleHit {
                    properties: id.map_or_else(Vec::new, |id| properties_of(&index, id)),
                    qualifiers: link_qualifiers(&link),
                    link: link_key(&link),
                    entity,
                    score: m.score,
                    ranges: m.ranges,
                    alias,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests;
