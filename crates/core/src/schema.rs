//! Property schemas: how a property's values should be read, and how it
//! relates entities.
//!
//! A property's schema comes from the meta-properties (`##value-type(date)`,
//! `##transitive` and so on) in its property entity's properties block, over
//! a built-in schema for Quiddity's own properties. Schemas are advisory:
//! values that don't match are kept and flagged, never discarded.

use std::fmt;

use jiff::Timestamp;
use jiff::civil::Date;
use quiddity_syntax::{Link, MetaProperty, Query, Value, format_link, normalise_name, parse_query};

use crate::Flag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Text,
    Number,
    Boolean,
    Date,
    DateTime,
    /// A link to an entity, or a reference to a block.
    Ref,
}

/// How many values an entity may have of a property, counting each item of
/// a list and each instance: UML's multiplicity, `0..1` or `1..*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cardinality {
    pub min: usize,
    /// `None` for no limit.
    pub max: Option<usize>,
}

impl Cardinality {
    /// Any number, `*`.
    pub const ANY: Self = Self { min: 0, max: None };
    /// At most one, `0..1`.
    pub const OPTIONAL: Self = Self {
        min: 0,
        max: Some(1),
    };

    pub fn allows(self, count: usize) -> bool {
        count >= self.min && self.max.is_none_or(|max| count <= max)
    }

    /// Reads `*`, `2`, `0..1`, `1..*` or `2..5`.
    pub fn parse(s: &str) -> Option<Self> {
        let bound = |b: &str| match b.trim() {
            "*" => Some(None),
            n => n.parse().ok().map(Some),
        };
        let (min, max) = match s.split_once("..") {
            Some((min, max)) => (min.trim().parse().ok()?, bound(max)?),
            None => match bound(s)? {
                None => (0, None),
                Some(n) => (n, Some(n)),
            },
        };
        max.is_none_or(|max| min <= max)
            .then_some(Self { min, max })
    }
}

impl fmt::Display for Cardinality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.min, self.max) {
            (0, None) => write!(f, "*"),
            (min, Some(max)) if min == max => write!(f, "{min}"),
            (min, Some(max)) => write!(f, "{min}..{max}"),
            (min, None) => write!(f, "{min}..*"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertySchema {
    pub value_type: ValueType,
    /// `##values`: which entities a reference may refer to.
    pub values: Option<Query>,
    /// `##applies-to`: which entities may have the property.
    pub applies_to: Option<Query>,
    pub cardinality: Cardinality,
    /// An entity has its values' values of the property too.
    pub transitive: bool,
    /// An entity has the other properties of the entities it refers to.
    pub inherits: bool,
    /// The name of the relation the other way round.
    pub inverse: Option<String>,
    /// The relation holds both ways.
    pub symmetric: bool,
}

impl Default for PropertySchema {
    /// The schema for a property nobody has described: anything goes.
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl PropertySchema {
    pub const DEFAULT: Self = Self::of(ValueType::Text, Cardinality::ANY);

    /// A schema that only says how values are read and how many there are.
    pub const fn of(value_type: ValueType, cardinality: Cardinality) -> Self {
        Self {
            value_type,
            values: None,
            applies_to: None,
            cardinality,
            transitive: false,
            inherits: false,
            inverse: None,
            symmetric: false,
        }
    }

    /// Schemas for the properties Quiddity itself relies on.
    pub fn builtin(key: &str) -> Option<Self> {
        let key = normalise_name(key);
        BUILTIN
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, schema)| schema.clone())
    }

    /// The schema `meta` makes of `base`, with what couldn't be read. Each
    /// meta-property written overrides what `base` says.
    pub fn with_meta(mut self, meta: &[MetaProperty]) -> (Self, Vec<Flag>) {
        let mut flags = Vec::new();
        let mut typed = false;
        for m in meta {
            let Some(which) = lookup(&normalise_name(&m.key), META) else {
                flags.push(Flag::UnknownMeta { key: m.key.clone() });
                continue;
            };
            let parameter = m.parameter.as_deref();
            let problem = match (which, parameter) {
                (Meta::Transitive | Meta::Inherits | Meta::Symmetric, Some(_)) => {
                    Some("takes no parameter".to_owned())
                }
                (Meta::Transitive, None) => {
                    self.transitive = true;
                    None
                }
                (Meta::Inherits, None) => {
                    self.inherits = true;
                    None
                }
                (Meta::Symmetric, None) => {
                    self.symmetric = true;
                    None
                }
                (_, None) => Some("needs a parameter".to_owned()),
                (Meta::ValueType, Some(p)) => match lookup(&normalise_name(p), VALUE_TYPES) {
                    Some(t) => {
                        self.value_type = t;
                        typed = true;
                        None
                    }
                    None => {
                        Some("expected text, number, boolean, date, datetime or ref".to_owned())
                    }
                },
                (Meta::Values | Meta::AppliesTo, Some(p)) => match parse_query(p) {
                    Ok(q) => {
                        let slot = if which == Meta::Values {
                            &mut self.values
                        } else {
                            &mut self.applies_to
                        };
                        *slot = Some(q);
                        None
                    }
                    Err(e) => Some(e.to_string()),
                },
                (Meta::Cardinality, Some(p)) => match Cardinality::parse(p) {
                    Some(c) => {
                        self.cardinality = c;
                        None
                    }
                    None => Some("expected a multiplicity such as 1, 0..1 or 1..*".to_owned()),
                },
                (Meta::Inverse, Some(p)) => {
                    self.inverse = Some(p.to_owned());
                    None
                }
            };
            flags.extend(problem.map(|message| Flag::BadMeta {
                key: m.key.clone(),
                message,
            }));
        }
        let relates = self.values.is_some()
            || self.transitive
            || self.inherits
            || self.inverse.is_some()
            || self.symmetric;
        if relates && !typed {
            self.value_type = ValueType::Ref;
        }
        (self, flags)
    }
}

const BUILTIN: &[(&str, PropertySchema)] = {
    use {Cardinality as C, ValueType::*};
    &[
        ("type", PropertySchema::of(Ref, C::ANY)),
        ("alias", PropertySchema::of(Text, C::ANY)),
        ("date", PropertySchema::of(Date, C::OPTIONAL)),
        ("favourite", PropertySchema::of(Boolean, C::OPTIONAL)),
        ("id", PropertySchema::of(Text, C::OPTIONAL)),
        ("title", PropertySchema::of(Text, C::OPTIONAL)),
    ]
};

/// The meta-properties, by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Meta {
    ValueType,
    Values,
    AppliesTo,
    Cardinality,
    Transitive,
    Inherits,
    Inverse,
    Symmetric,
}

const META: &[(&str, Meta)] = &[
    ("value-type", Meta::ValueType),
    ("values", Meta::Values),
    ("applies-to", Meta::AppliesTo),
    ("cardinality", Meta::Cardinality),
    ("transitive", Meta::Transitive),
    ("inherits", Meta::Inherits),
    ("inverse", Meta::Inverse),
    ("symmetric", Meta::Symmetric),
];

/// How `##value-type` is spelt.
const VALUE_TYPES: &[(&str, ValueType)] = &[
    ("text", ValueType::Text),
    ("number", ValueType::Number),
    ("boolean", ValueType::Boolean),
    ("date", ValueType::Date),
    ("datetime", ValueType::DateTime),
    ("ref", ValueType::Ref),
];

fn lookup<T: Copy>(name: &str, table: &[(&str, T)]) -> Option<T> {
    table.iter().find(|(n, _)| *n == name).map(|&(_, v)| v)
}

/// A value read according to its schema.
#[derive(Debug, Clone, PartialEq)]
pub enum Typed {
    Text(String),
    Number(f64),
    Boolean(bool),
    Date(Date),
    DateTime(Timestamp),
    Link(Link),
    BlockRef(String),
    List(Vec<Typed>),
}

/// A value that doesn't fit its schema.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("expected {expected}, found {found:?}")]
pub struct Mismatch {
    pub expected: String,
    pub found: Value,
}

/// Reads a value according to a schema. A list is always allowed, and is
/// read item by item.
pub fn coerce(value: &Value, schema: &PropertySchema) -> Result<Typed, Mismatch> {
    match value {
        Value::List(items) => items
            .iter()
            .map(|v| coerce_scalar(v, schema.value_type))
            .collect::<Result<_, _>>()
            .map(Typed::List),
        _ => coerce_scalar(value, schema.value_type),
    }
}

fn coerce_scalar(value: &Value, value_type: ValueType) -> Result<Typed, Mismatch> {
    let parsed = match (value_type, value) {
        (ValueType::Text, Value::Text(s)) => Some(Typed::Text(s.clone())),
        (ValueType::Text, Value::Link(link)) => Some(Typed::Text(format_link(link))),
        (ValueType::Text, Value::BlockRef(id)) => Some(Typed::Text(format!("(({id}))"))),
        (ValueType::Number, Value::Text(s)) => s
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite())
            .map(Typed::Number),
        (ValueType::Boolean, Value::Text(s)) => match normalise_name(s).as_str() {
            "true" | "yes" => Some(Typed::Boolean(true)),
            "false" | "no" => Some(Typed::Boolean(false)),
            _ => None,
        },
        (ValueType::Date, Value::Text(s)) => s.trim().parse().ok().map(Typed::Date),
        (ValueType::DateTime, Value::Text(s)) => s.trim().parse().ok().map(Typed::DateTime),
        (ValueType::Ref, Value::Link(link)) => Some(Typed::Link(link.clone())),
        (ValueType::Ref, Value::BlockRef(id)) => Some(Typed::BlockRef(id.clone())),
        // `type:: person` means the same as `type:: [[person]]`.
        (ValueType::Ref, Value::Text(s)) => Some(Typed::Link(Link {
            name: s.clone(),
            qualifiers: Vec::new(),
        })),
        _ => None,
    };
    parsed.ok_or_else(|| mismatch(&format!("{value_type:?}").to_lowercase(), value))
}

fn mismatch(expected: &str, found: &Value) -> Mismatch {
    Mismatch {
        expected: expected.to_owned(),
        found: found.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiddity_syntax::parse_block;

    fn t(s: &str) -> Value {
        Value::Text(s.to_owned())
    }

    /// The schema the meta-property lines in `text` make of the default.
    fn read(text: &str) -> (PropertySchema, Vec<Flag>) {
        PropertySchema::default().with_meta(&parse_block(text).meta)
    }

    #[test]
    fn meta_properties_are_read() {
        let (schema, flags) = read(
            "##value-type(Number)\n##applies-to(#type(film))\n##cardinality(0..1)\n##inverse(rated)",
        );
        assert_eq!(flags, []);
        assert_eq!(
            schema.value_type,
            ValueType::Number,
            "an explicit type wins"
        );
        assert_eq!(schema.applies_to, parse_query("#type(film)").ok());
        assert_eq!(schema.cardinality, Cardinality::OPTIONAL);
        assert_eq!(schema.inverse.as_deref(), Some("rated"));

        let (schema, flags) = read("##values(#type(place))\n##transitive\n##inherits\n##symmetric");
        assert_eq!(flags, []);
        assert_eq!(
            (schema.transitive, schema.inherits, schema.symmetric),
            (true, true, true)
        );
        assert_eq!(schema.values, parse_query("#type(place)").ok());
    }

    /// A property that relates entities is a reference unless it says
    /// otherwise.
    #[test]
    fn relating_implies_ref() {
        for meta in [
            "##values(x)",
            "##transitive",
            "##inherits",
            "##inverse(x)",
            "##symmetric",
        ] {
            assert_eq!(read(meta).0.value_type, ValueType::Ref, "{meta}");
            let typed = format!("{meta}\n##value-type(text)");
            assert_eq!(read(&typed).0.value_type, ValueType::Text, "{typed}");
        }
        for meta in ["##applies-to(x)", "##cardinality(1)", "##nonsense"] {
            assert_eq!(read(meta).0.value_type, ValueType::Text, "{meta}");
        }
    }

    #[test]
    fn unreadable_meta_properties_are_flagged() {
        let (schema, flags) = read(
            "##nonsense\n##transitive(yes)\n##values\n##values(a &)\n##value-type(colour)\n##cardinality(5..2)",
        );
        assert_eq!(
            schema,
            PropertySchema::default(),
            "nothing unreadable is used"
        );
        let keys: Vec<_> = flags
            .iter()
            .map(|f| match f {
                Flag::UnknownMeta { key } => format!("unknown {key}"),
                Flag::BadMeta { key, .. } => format!("bad {key}"),
                other => format!("{other:?}"),
            })
            .collect();
        assert_eq!(
            keys,
            [
                "unknown nonsense",
                "bad transitive",
                "bad values",
                "bad values",
                "bad value-type",
                "bad cardinality"
            ]
        );
    }

    #[test]
    fn builtin_types() {
        let date = PropertySchema::builtin("date").unwrap();
        assert_eq!(
            coerce(&t("2026-10-25"), &date),
            Ok(Typed::Date("2026-10-25".parse().unwrap()))
        );
        assert!(coerce(&t("2026-02-30"), &date).is_err());

        let at = PropertySchema::of(ValueType::DateTime, Cardinality::OPTIONAL);
        assert!(coerce(&t("2026-10-25T21:04:00+01:00"), &at).is_ok());
        assert!(
            coerce(&t("2026-10-25T21:04:00"), &at).is_err(),
            "an offset is required"
        );

        let ty = PropertySchema::builtin("type").unwrap();
        assert!(matches!(coerce(&t("person"), &ty), Ok(Typed::Link(l)) if l.name == "person"));

        let flag = PropertySchema::of(ValueType::Boolean, Cardinality::OPTIONAL);
        assert_eq!(coerce(&t("Yes"), &flag), Ok(Typed::Boolean(true)));
        assert_eq!(PropertySchema::builtin("Favourite"), Some(flag));

        for (key, cardinality) in [("id", Cardinality::OPTIONAL), ("alias", Cardinality::ANY)] {
            assert_eq!(
                PropertySchema::builtin(key).unwrap().cardinality,
                cardinality
            );
        }
    }

    #[test]
    fn cardinalities() {
        for (written, min, max) in [
            ("*", 0, None),
            ("1", 1, Some(1)),
            ("0..1", 0, Some(1)),
            (" 1 .. * ", 1, None),
            ("2..5", 2, Some(5)),
            ("3..3", 3, Some(3)),
        ] {
            assert_eq!(
                Cardinality::parse(written),
                Some(Cardinality { min, max }),
                "{written}"
            );
        }
        for bad in ["", "x", "*..1", "5..2", "-1", "1..2..3", "1.."] {
            assert_eq!(Cardinality::parse(bad), None, "{bad}");
        }
        let written: Vec<_> = [(0, None), (1, Some(1)), (0, Some(1)), (1, None)]
            .map(|(min, max)| Cardinality { min, max }.to_string())
            .into();
        assert_eq!(
            written,
            ["*", "1", "0..1", "1..*"],
            "as briefly as it can be"
        );
        let c = Cardinality {
            min: 1,
            max: Some(2),
        };
        assert_eq!(
            (0..4).map(|n| c.allows(n)).collect::<Vec<_>>(),
            [false, true, true, false]
        );
        assert!(Cardinality::ANY.allows(usize::MAX));
    }

    use proptest::prelude::*;

    /// A typed value, and how a person might write it.
    fn typed() -> impl Strategy<Value = (ValueType, Typed, Value)> {
        let link = "[a-z ]{1,8}".prop_map(|name| Link {
            name: name.trim().to_owned() + "x",
            qualifiers: Vec::new(),
        });
        prop_oneof![
            any::<String>().prop_map(|s| (ValueType::Text, Typed::Text(s.clone()), t(&s))),
            link.clone().prop_map(|l| (
                ValueType::Text,
                Typed::Text(format_link(&l)),
                Value::Link(l)
            )),
            "[a-z0-9-]{1,8}".prop_map(|id| (
                ValueType::Text,
                Typed::Text(format!("(({id}))")),
                Value::BlockRef(id)
            )),
            any::<f64>()
                .prop_filter("finite", |n| n.is_finite())
                .prop_map(|n| (ValueType::Number, Typed::Number(n), t(&n.to_string()))),
            (any::<bool>(), any::<bool>()).prop_map(|(b, short)| {
                let word = match (b, short) {
                    (true, false) => "true",
                    (true, true) => "Yes",
                    (false, false) => "False",
                    (false, true) => "no",
                };
                (ValueType::Boolean, Typed::Boolean(b), t(word))
            }),
            (1i16..=9999, 1i8..=12, 1i8..=28).prop_map(|(y, m, d)| {
                let date = Date::new(y, m, d).unwrap();
                (ValueType::Date, Typed::Date(date), t(&date.to_string()))
            }),
            (Timestamp::MIN.as_second()..=Timestamp::MAX.as_second()).prop_map(|secs| {
                let at = Timestamp::from_second(secs).unwrap();
                (ValueType::DateTime, Typed::DateTime(at), t(&at.to_string()))
            }),
            link.prop_map(|l| (ValueType::Ref, Typed::Link(l.clone()), Value::Link(l))),
            "[a-z0-9-]{1,8}".prop_map(|id| (
                ValueType::Ref,
                Typed::BlockRef(id.clone()),
                Value::BlockRef(id)
            )),
        ]
    }

    proptest! {
        /// Whatever is written for a type reads back as that value, alone or
        /// in a list.
        #[test]
        fn coerce_reads_what_was_written(cases in proptest::collection::vec(typed(), 1..4)) {
            let (value_type, expected, written) = cases[0].clone();
            let schema = PropertySchema::of(value_type, Cardinality::ANY);
            prop_assert_eq!(coerce(&written, &schema), Ok(expected));

            let same: Vec<_> = cases.iter().filter(|c| c.0 == value_type).collect();
            prop_assert_eq!(
                coerce(&Value::List(same.iter().map(|c| c.2.clone()).collect()), &schema),
                Ok(Typed::List(same.iter().map(|c| c.1.clone()).collect()))
            );
        }

        /// A cardinality is written as it is read.
        #[test]
        fn cardinalities_round_trip(min in 0usize..4, extra in proptest::option::of(0usize..4)) {
            let c = Cardinality { min, max: extra.map(|e| min + e) };
            prop_assert_eq!(Cardinality::parse(&c.to_string()), Some(c), "{}", c);
        }
    }
}
