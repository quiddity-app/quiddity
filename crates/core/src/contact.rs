//! What `##contact(field:label)` says: which field of a contact a property
//! fills.
//!
//! The vocabulary is the fields every phone's contacts app has, spelt as
//! Quiddity spells them. Nothing here knows about any platform's contacts:
//! that is for whatever exposes them.

use std::fmt;

use quiddity_syntax::normalise_name;
use serde::{Deserialize, Serialize};

/// A field of a contact. An entity's name is the contact's name, and isn't
/// one of these.
/// Serialised as it is written in `##contact`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContactField {
    Phone,
    Email,
    Address,
    Organisation,
    JobTitle,
    Nickname,
    Website,
    Birthday,
    Note,
}

impl ContactField {
    /// Every field, in the order they are listed.
    pub const ALL: [Self; 9] = [
        Self::Phone,
        Self::Email,
        Self::Address,
        Self::Organisation,
        Self::JobTitle,
        Self::Nickname,
        Self::Website,
        Self::Birthday,
        Self::Note,
    ];

    /// How the field is written in `##contact(…)`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Phone => "phone",
            Self::Email => "email",
            Self::Address => "address",
            Self::Organisation => "organisation",
            Self::JobTitle => "job-title",
            Self::Nickname => "nickname",
            Self::Website => "website",
            Self::Birthday => "birthday",
            Self::Note => "note",
        }
    }

    /// Reads a field's name, ignoring case and spacing. `organization` is
    /// accepted for `organisation`.
    pub fn parse(s: &str) -> Option<Self> {
        let s = normalise_name(s);
        if s == "organization" {
            return Some(Self::Organisation);
        }
        Self::ALL.into_iter().find(|f| f.name() == s)
    }
}

impl fmt::Display for ContactField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The field a property fills, and the label its values carry: `phone:mobile`
/// is a phone number labelled mobile. A label is any text, and phones and
/// the like have the usual few (`home`, `work`, `mobile`) while anything else
/// is shown as written. `other` is no label: it is what a phone calls a number
/// that has none, so the two can't be told apart and aren't.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContactMapping {
    pub field: ContactField,
    /// As written, trimmed. Compared by [`ContactMapping::same_label`].
    pub label: Option<String>,
}

impl ContactMapping {
    /// Reads `phone`, `email:work` or `address: home`. A field with no
    /// label, an empty one or `other` has none.
    pub fn parse(s: &str) -> Option<Self> {
        let (field, label) = match s.split_once(':') {
            Some((field, label)) => (field, Some(label.trim())),
            None => (s, None),
        };
        Some(Self {
            field: ContactField::parse(field)?,
            label: label
                .filter(|l| !l.is_empty() && !l.eq_ignore_ascii_case("other"))
                .map(str::to_owned),
        })
    }

    /// Whether two labels are the same label, ignoring case and spacing.
    pub fn same_label(a: Option<&str>, b: Option<&str>) -> bool {
        a.map(normalise_name) == b.map(normalise_name)
    }
}

impl fmt::Display for ContactMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.label {
            Some(label) => write!(f, "{}:{label}", self.field),
            None => write!(f, "{}", self.field),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn mappings_are_read() {
        let m = |field, label: Option<&str>| {
            Some(ContactMapping {
                field,
                label: label.map(str::to_owned),
            })
        };
        assert_eq!(ContactMapping::parse("phone"), m(ContactField::Phone, None));
        assert_eq!(
            ContactMapping::parse(" Email : Work "),
            m(ContactField::Email, Some("Work")),
        );
        assert_eq!(
            ContactMapping::parse("Job  Title"),
            None,
            "the name has a hyphen, not a space"
        );
        assert_eq!(
            ContactMapping::parse("organization:"),
            m(ContactField::Organisation, None),
            "an empty label is none"
        );
        assert_eq!(
            ContactMapping::parse("phone:a:b"),
            m(ContactField::Phone, Some("a:b")),
            "a label is everything after the first colon"
        );
        assert_eq!(
            ContactMapping::parse("phone:Other"),
            m(ContactField::Phone, None),
            "other is no label"
        );
        assert_eq!(ContactMapping::parse("fax"), None);
        assert_eq!(ContactMapping::parse(""), None);
        assert_eq!(ContactMapping::parse(":work"), None);
    }

    #[test]
    fn labels_compare_like_names() {
        assert!(ContactMapping::same_label(Some("Work"), Some(" work ")));
        assert!(ContactMapping::same_label(None, None));
        assert!(!ContactMapping::same_label(Some("work"), None));
        assert!(!ContactMapping::same_label(Some("work"), Some("home")));
    }

    proptest! {
        /// A mapping reads back as it was written.
        #[test]
        fn mappings_round_trip(
            field in proptest::sample::select(ContactField::ALL.to_vec()),
            label in proptest::option::of("[a-z][a-z :]{0,8}[a-z]".prop_filter("not other", |l| !l.eq_ignore_ascii_case("other"))),
        ) {
            let mapping = ContactMapping { field, label };
            prop_assert_eq!(ContactMapping::parse(&mapping.to_string()), Some(mapping));
        }
    }
}
