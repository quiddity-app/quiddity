//! Full-text search over what is written in blocks.
//!
//! Words are runs of letters and digits, compared ignoring case. A block is
//! found when every word of the query matches one of its words, the last
//! also as a prefix, because the query is usually still being typed. Blocks
//! are ranked by BM25 (rarer words and shorter blocks count for more), with a
//! bonus when the words are together and in order. Every query reads every
//! block, as the index does when it is rebuilt.

use serde::Serialize;

use quiddity_core::syntax::{fold, is_word_char};

use crate::query::EntityRef;
use crate::{Result, View};

/// BM25's weight of a word's repeats, and how far length counts.
const K1: f64 = 1.2;
const B: f64 = 0.75;
/// What a block with a query's words together and in order is worth more.
const PHRASE: f64 = 1.5;

/// How rare a word is: `total` blocks, `df` of which have it.
fn idf(total: f64, df: f64) -> f64 {
    (1.0 + (total - df + 0.5) / (df + 0.5)).ln()
}

/// What a word found `tf` times in a block is worth, given how rare it is and
/// the block's length as a multiple of the average.
fn term_score(idf: f64, tf: f64, length: f64) -> f64 {
    idf * tf * (K1 + 1.0) / (tf + K1 * (1.0 - B + B * length))
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Token {
    /// Folded, so that case and Unicode form don't matter.
    pub text: String,
    /// Where it is in the text, in UTF-16 units.
    pub start: usize,
    pub end: usize,
}

/// The words of `text`.
pub(crate) fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut units = 0;
    let mut current: Option<Token> = None;
    for c in text.chars() {
        let width = c.len_utf16();
        if is_word_char(c) {
            let token = current.get_or_insert_with(|| Token {
                text: String::new(),
                start: units,
                end: units,
            });
            token.text.push(c);
            token.end = units + width;
        } else if let Some(token) = current.take() {
            out.push(finish(token));
        }
        units += width;
    }
    out.extend(current.map(finish));
    out
}

/// Replaces a token's text, as written, with the form it is indexed and
/// matched in. Its place in the original text stays as it was.
fn finish(mut token: Token) -> Token {
    token.text = fold(&token.text);
    token
}

/// Whether a query word matches `token`; the last may be just its start.
fn matches(term: &str, last: bool, token: &Token) -> bool {
    token.text == term || (last && token.text.starts_with(term))
}

/// The words of `query`, each with whether it is the last.
fn terms(query: &str) -> Vec<(String, bool)> {
    let words = tokens(query);
    let n = words.len();
    words
        .into_iter()
        .enumerate()
        .map(|(i, t)| (t.text, i + 1 == n))
        .collect()
}

/// A block whose text matched a search.
#[derive(Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlockHit {
    /// The entity the block is on.
    pub entity: EntityRef,
    pub block: String,
    pub text: String,
    pub score: f64,
    /// The words that matched, as ascending `[start, end)` runs of `text` in
    /// UTF-16 units.
    pub ranges: Vec<(usize, usize)>,
}

struct Candidate<'a> {
    block: quiddity_core::BlockId,
    entity: quiddity_core::BlockId,
    text: &'a str,
    tokens: Vec<Token>,
}

impl View {
    /// The blocks whose text matches `query`, best first and at most `limit`.
    /// An entity's own name is a title, not text: see `search_titles`.
    pub fn search_text(&self, query: &str, limit: usize) -> Result<Vec<BlockHit>> {
        let terms = terms(query);
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let index = self.index()?;
        let mut candidates = Vec::new();
        let corpus = self.corpus()?;
        let mut entity = None;
        for scanned in corpus.iter() {
            if scanned.parent.is_none() {
                entity = Some(scanned.id);
                continue;
            }
            let Some(entity) = entity else {
                continue;
            };
            candidates.push(Candidate {
                block: scanned.id,
                entity,
                text: &scanned.text,
                tokens: tokens(&scanned.text),
            });
        }

        let total = candidates.len() as f64;
        let average = candidates.iter().map(|c| c.tokens.len()).sum::<usize>() as f64 / total;
        let found_in = |position: usize, c: &Candidate| {
            let (term, last) = &terms[position];
            c.tokens.iter().filter(|t| matches(term, *last, t)).count()
        };
        let idf: Vec<f64> = (0..terms.len())
            .map(|p| {
                let df = candidates.iter().filter(|c| found_in(p, c) > 0).count() as f64;
                idf(total, df)
            })
            .collect();

        let mut scored = Vec::new();
        for c in &candidates {
            let counts: Vec<usize> = (0..terms.len()).map(|p| found_in(p, c)).collect();
            if counts.contains(&0) {
                continue;
            }
            let length = c.tokens.len() as f64 / average;
            let mut score: f64 = counts
                .iter()
                .zip(&idf)
                .map(|(&tf, &idf)| term_score(idf, tf as f64, length))
                .sum();
            let together = c.tokens.windows(terms.len()).any(|w| {
                w.iter()
                    .zip(&terms)
                    .all(|(t, (term, last))| matches(term, *last, t))
            });
            if terms.len() > 1 && together {
                score *= PHRASE;
            }
            scored.push((c, score));
        }
        // Equal scores keep a fixed order, whatever order the blocks came in.
        scored.sort_by(|(a, x), (b, y)| y.total_cmp(x).then_with(|| a.block.cmp(&b.block)));
        scored.truncate(limit);

        let entities: Vec<_> = scored
            .iter()
            .map(|(c, _)| index.canonical(c.entity))
            .collect();
        drop(index);
        scored
            .into_iter()
            .zip(entities)
            .map(|((c, score), entity)| {
                let ranges = c
                    .tokens
                    .iter()
                    .filter(|t| terms.iter().any(|(term, last)| matches(term, *last, t)))
                    .map(|t| (t.start, t.end))
                    .collect();
                Ok(BlockHit {
                    entity: self.entity_ref(entity)?,
                    block: c.block.to_string(),
                    text: c.text.to_owned(),
                    score,
                    ranges,
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
