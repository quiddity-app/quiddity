use std::borrow::Cow;

use unicode_normalization::{UnicodeNormalization, char::is_combining_mark, is_nfc};

/// Text in the form Quiddity stores: Unicode NFC (canonical composition), so
/// that `é` typed as one character or as `e` plus an accent is stored the same
/// way. Borrows when the text is already NFC, which is nearly always.
pub fn nfc(text: &str) -> Cow<'_, str> {
    if is_nfc(text) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(text.nfc().collect())
    }
}

/// Whether `c` is a combining mark: part of the letter before it.
pub fn is_mark(c: char) -> bool {
    is_combining_mark(c)
}

/// Whether `c` is part of a word for names, tags and search: a letter or
/// digit, or a combining mark, which belongs to the letter before it.
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_mark(c)
}

/// Characters that take no space and are not part of a name: the zero-width
/// space, the word joiner and the byte order mark. (The zero-width joiner and
/// non-joiner are kept: emoji sequences and some scripts need them.)
fn is_invisible(c: char) -> bool {
    matches!(c, '\u{200b}' | '\u{2060}' | '\u{feff}')
}

/// The form in which words are compared and indexed: NFKC (so compatibility
/// forms such as `ﬁ` and `Ａ` match their plain spellings), case folded (so
/// `Straße` matches `STRASSE`), and without invisible characters.
pub fn fold(word: &str) -> String {
    let plain: String = word.chars().filter(|&c| !is_invisible(c)).nfkc().collect();
    // Folding can denormalise, so compose again.
    caseless::default_case_fold_str(&plain).nfkc().collect()
}

/// Normalises an entity or property name for comparison.
///
/// Names compare ignoring case, leading, trailing and repeated whitespace,
/// and differences of Unicode form (see [`fold`]), so `[[Jane  Doe]]` and
/// `[[jane doe]]` refer to the same names, and so do a composed and a
/// decomposed `Café`.
pub fn normalise_name(name: &str) -> String {
    // Fold before splitting: invisible characters can't hide or make a
    // boundary, and NFKC can itself make whitespace (`¯` is a space and a
    // combining macron), which must be collapsed too or a second pass would
    // find more to do.
    fold(name).split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn collapses_whitespace_and_case() {
        assert_eq!(normalise_name("  Jane \t Doe "), "jane doe");
        assert_eq!(normalise_name("ÉCOLE"), "école");
        assert_eq!(normalise_name(""), "");
    }

    #[test]
    fn identical_looking_names_are_the_same() {
        assert_eq!(normalise_name("Caf\u{e9}"), normalise_name("Cafe\u{301}"));
        assert_eq!(normalise_name("Straße"), normalise_name("STRASSE"));
        assert_eq!(normalise_name("a\u{200b}b"), "ab");
        assert_eq!(normalise_name("\u{fb01}x"), "fix");
        assert_eq!(normalise_name("\u{ff21}"), "a");
        assert_eq!(normalise_name("İ"), normalise_name("i\u{307}"));
    }

    #[test]
    fn whitespace_made_by_folding_is_collapsed_too() {
        // NFKC turns a spacing accent such as `¯` into a space and a combining
        // mark, so the space has to be dealt with after folding, not before.
        assert_eq!(normalise_name("\u{af}"), "\u{304}");
        assert_eq!(normalise_name("a\u{a8} "), "a \u{308}");
        assert_eq!(normalise_name("\u{a8}\u{b4}"), "\u{308} \u{301}");
    }

    #[test]
    fn nfc_composes_and_borrows() {
        assert_eq!(nfc("Cafe\u{301}"), "Caf\u{e9}");
        assert!(matches!(nfc("Caf\u{e9}"), Cow::Borrowed(_)));
        // Compatibility forms are not NFC's business: they stay as written.
        assert_eq!(nfc("\u{fb01}"), "\u{fb01}");
    }

    #[test]
    fn marks_are_word_characters() {
        assert!(is_word_char('\u{301}'));
        assert!(is_word_char('é'));
        assert!(!is_word_char(' '));
        assert!(!is_word_char('#'));
    }

    /// Strings weighted towards what normalisation changes.
    fn text() -> impl Strategy<Value = String> {
        proptest::collection::vec(
            prop_oneof![
                3 => proptest::char::range('a', 'f'),
                2 => proptest::sample::select(vec![
                    '\u{e9}', '\u{301}', '\u{327}', '\u{df}', 'ẞ', 'İ', '\u{fb01}', '\u{ff21}',
                    '\u{200b}', '\u{200d}', '\u{feff}', ' ', '\t', 'Σ', 'ς', '\u{212b}', 'Å',
                ]),
                1 => any::<char>(),
            ],
            0..12,
        )
        .prop_map(|cs| cs.into_iter().collect())
    }

    proptest! {
        #[test]
        fn normalising_a_name_is_idempotent(s in text()) {
            let once = normalise_name(&s);
            prop_assert_eq!(normalise_name(&once), once);
        }

        #[test]
        fn nfc_is_idempotent(s in text()) {
            let once = nfc(&s).into_owned();
            prop_assert_eq!(nfc(&once), Cow::Borrowed(once.as_str()));
        }

        #[test]
        fn canonically_equivalent_names_match(s in text()) {
            let decomposed: String = s.nfd().collect();
            prop_assert_eq!(normalise_name(&decomposed), normalise_name(&s));
        }
    }
}
