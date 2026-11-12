//! Virtual entities: a name that is linked to, and that no entity has, looks to
//! the UI in every way like an empty entity of that name, without being one.
//!
//! Nothing is stored. The index lists the names ([`quiddity_core::Index::mentions`])
//! and this module gives each an ID, `~` and the name as names are compared, so
//! a page, a row in All entities, a node in the graph, a search hit and a link
//! to it all work as they do for an entity. An entity becomes real by being
//! written to: adding a block, or renaming it, makes one named as the first
//! link to it wrote the name, as part of the same edit. The ID goes on working
//! after that and answers for the entity that now has the name, as the ID of a
//! merged duplicate does, so the page that was open stays open. When the last
//! link to a name goes, so does its entity, and there is nothing to clean up.

use std::collections::BTreeSet;

use quiddity_core::BlockId;
use quiddity_core::syntax::normalise_name;

use crate::{Error, Result, View};

/// What an ID starts with if it is a name's and not a block's.
const PREFIX: char = '~';

/// The ID of the virtual entity for `name`.
pub(crate) fn id_of(name: &str) -> String {
    format!("{PREFIX}{}", normalise_name(name))
}

/// The name an ID is for, if it is a virtual entity's.
pub(crate) fn name_of(id: &str) -> Option<&str> {
    id.strip_prefix(PREFIX)
}

/// A name that is linked to and that no entity has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Virtual {
    pub id: String,
    /// As the first block, in document order, that links to it writes it.
    pub title: String,
    /// The blocks that link to it, once for each link, in document order.
    pub sources: Vec<BlockId>,
}

/// What an ID for a name stands for now.
pub(crate) enum Named {
    /// An entity has the name: an ID that was a virtual entity's before it was
    /// written to, or one written by hand.
    Real(BlockId),
    Virtual(Virtual),
}

impl View {
    /// Every name that is linked to and that no entity has, in name order.
    pub(crate) fn virtuals(&self) -> Result<Vec<Virtual>> {
        let index = self.index()?;
        Ok(index
            .mentions()
            .map(|(name, mentions)| Virtual {
                id: id_of(name),
                title: mentions[0].name.clone(),
                sources: mentions.iter().map(|m| m.source).collect(),
            })
            .collect())
    }

    /// What `id` stands for, if it is a virtual entity's ID, and an error if
    /// nothing is called that and nothing links to it.
    pub(crate) fn named(&self, id: &str) -> Result<Option<Named>> {
        let Some(name) = name_of(id) else {
            return Ok(None);
        };
        let index = self.index()?;
        let mut real = index.named(name);
        real.sort();
        if let Some(&first) = real.first() {
            return Ok(Some(Named::Real(index.canonical(first))));
        }
        let mentions = index.mention(name);
        let Some(first) = mentions.first() else {
            return Err(Error::NotAnEntity(id.to_owned()));
        };
        Ok(Some(Named::Virtual(Virtual {
            id: id_of(name),
            title: first.name.clone(),
            sources: mentions.iter().map(|m| m.source).collect(),
        })))
    }

    /// The entity `id` is the ID of, made first if it is a virtual one: a root
    /// block named for it, in the edit being made. Any other ID is checked as
    /// an entity's is.
    pub(crate) fn entity_to_write(&self, id: &str) -> Result<BlockId> {
        match self.named(id)? {
            None => self.entity_id(id),
            Some(Named::Real(entity)) => Ok(entity),
            Some(Named::Virtual(v)) => Ok(self.graph.create_block(None, None, &v.title)?),
        }
    }

    /// The blocks linking to a virtual entity, as the set references are made of.
    pub(crate) fn sources_of(v: &Virtual) -> BTreeSet<BlockId> {
        v.sources.iter().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_id_is_the_tilde_and_the_name_as_names_are_compared() {
        assert_eq!(id_of("  New   York "), "~new york");
        assert_eq!(name_of("~new york"), Some("new york"));
        assert_eq!(name_of("12@3456"), None);
        assert_eq!(name_of(&id_of("A")), Some("a"));
    }
}
