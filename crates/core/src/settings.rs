//! Settings for the whole graph, as opposed to any one entity.
//!
//! They live in their own map in the graph's document, so they sync with it,
//! but they aren't blocks: an export writes them to `quiddity.toml` beside the
//! Markdown, and an import reads that file back.
//!
//! ```toml
//! # Where the app opens: "journals", "all", "graph", or a link to an entity.
//! home = "[[Quiddity]]"
//! ```

use std::fmt;
use std::str::FromStr;

use loro::{LoroValue, ValueOrContainer};
use quiddity_syntax::{Link, format_link, parse_block};
use serde::{Deserialize, Serialize};

use crate::{Error, Graph, Result};

const SETTINGS: &str = "settings";
const HOME: &str = "home";

/// Where the UI opens when it is given nowhere in particular.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Home {
    /// Today's journal, then the days before it. The default.
    Journals,
    /// The list of every entity.
    All,
    /// The graph view.
    Graph,
    /// The entity a link goes to.
    Entity(Link),
}

impl fmt::Display for Home {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Home::Journals => f.write_str("journals"),
            Home::All => f.write_str("all"),
            Home::Graph => f.write_str("graph"),
            Home::Entity(link) => f.write_str(&format_link(link)),
        }
    }
}

impl FromStr for Home {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.trim() {
            "journals" => Ok(Home::Journals),
            "all" => Ok(Home::All),
            "graph" => Ok(Home::Graph),
            text if text.starts_with("[[") && text.ends_with("]]") => {
                let mut links = parse_block(text).links;
                match (links.pop(), links.is_empty()) {
                    (Some(link), true) => Ok(Home::Entity(link)),
                    _ => Err(Error::Settings(format!("{s:?} is not one link"))),
                }
            }
            _ => Err(Error::Settings(format!(
                "home is \"journals\", \"all\", \"graph\" or a [[link]], not {s:?}"
            ))),
        }
    }
}

/// The graph's settings. Each is `None` when it hasn't been set, which means
/// its default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    pub home: Option<Home>,
}

/// `quiddity.toml` as written: strings, checked when converted.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(skip_serializing_if = "Option::is_none")]
    home: Option<String>,
}

impl Settings {
    /// Reads `quiddity.toml`. An unknown key is an error rather than ignored,
    /// so that a misspelt setting doesn't silently do nothing.
    pub fn from_toml(text: &str) -> Result<Self> {
        let file: File = toml::from_str(text).map_err(|e| Error::Settings(e.to_string()))?;
        Ok(Self {
            home: file.home.as_deref().map(str::parse).transpose()?,
        })
    }

    /// Writes `quiddity.toml`: only the settings that are set.
    pub fn to_toml(&self) -> String {
        let file = File {
            home: self.home.as_ref().map(Home::to_string),
        };
        toml::to_string(&file).expect("a table of strings always serialises")
    }

    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Graph {
    /// The graph's settings. A stored value this build can't read (written by
    /// a newer one, say) is treated as unset.
    pub fn settings(&self) -> Settings {
        let map = self.doc().get_map(SETTINGS);
        let text = |key| match map.get(key) {
            Some(ValueOrContainer::Value(LoroValue::String(s))) => Some(s.to_string()),
            _ => None,
        };
        Settings {
            home: text(HOME).and_then(|s| s.parse().ok()),
        }
    }

    /// Sets each setting that `settings` has, and leaves the others as they
    /// are. Each is its own key, so devices setting different ones don't
    /// conflict.
    pub fn apply_settings(&self, settings: &Settings) -> Result<()> {
        let map = self.doc().get_map(SETTINGS);
        if let Some(home) = &settings.home {
            map.insert(HOME, home.to_string())?;
        }
        self.commit();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_this_build_cant_read_is_unset() {
        let graph = Graph::new();
        let map = graph.doc().get_map(SETTINGS);
        map.insert(HOME, "somewhere new").unwrap();
        assert_eq!(graph.settings(), Settings::default());
        map.insert(HOME, 7).unwrap();
        assert_eq!(graph.settings(), Settings::default());
    }
}
