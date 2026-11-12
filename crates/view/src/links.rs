//! Links to entities that don't exist yet. A name nobody has made is a virtual
//! entity and following it makes nothing; a link with
//! qualifiers makes its entity when followed, or when a block that has just
//! gained it is saved.

use std::collections::HashSet;

use quiddity_core::syntax::{Link, PropertyStyle, Value, parse_block};
use quiddity_core::{BlockId, Resolution};

use crate::query::link_key;
use crate::{Error, Result, View};

/// The distinct links in `text`, in the order they first appear, each with the
/// key it is looked up by.
fn links_in(text: &str) -> Vec<(String, Link)> {
    let mut seen = HashSet::new();
    let parsed = parse_block(text);
    // A tag is a link to the entity it names.
    let tags = parsed
        .properties
        .iter()
        .filter(|p| p.style == PropertyStyle::Inline)
        .map(|p| quiddity_core::tags::link(&p.key));
    parsed
        .links
        .iter()
        .cloned()
        .chain(tags)
        .filter_map(|link| {
            let key = link_key(&link);
            seen.insert(key.clone()).then_some((key, link))
        })
        .collect()
}

impl View {
    /// The entity a link goes to. `key` is the text between the brackets, as
    /// [`crate::Entity::links`] has it. A name nobody has made goes to its
    /// virtual entity, which is made by being written to, not by being
    /// followed; a link with qualifiers makes its entity, since they have to be
    /// written somewhere. A link that matches several entities is an error:
    /// there is no one to open.
    pub fn follow_link(&self, key: &str) -> Result<String> {
        let (_, link) = links_in(&format!("[[{key}]]"))
            .into_iter()
            .next()
            .ok_or_else(|| Error::NotALink(key.to_owned()))?;
        let found = {
            let index = self.index()?;
            match index.resolve(&link) {
                Resolution::Unique(id) => Some(index.canonical(id)),
                Resolution::Ambiguous(_) => return Err(Error::AmbiguousLink(key.to_owned())),
                Resolution::Missing => None,
            }
        };
        let id = match found {
            Some(id) => id.to_string(),
            None if link.qualifiers.is_empty() => crate::virtual_entity::id_of(&link.name),
            None => {
                self.structural();
                let made = self.make_linked_entity(&link);
                self.edited(made)?.to_string()
            }
        };
        Ok(id)
    }

    /// The entity that holds the block whose `##id` is `id`, which is where a
    /// block reference goes until blocks can be shown in place. Nothing is
    /// made for a reference to nowhere: there is no way to say what it was
    /// meant to be.
    pub fn follow_block_ref(&self, id: &str) -> Result<String> {
        let index = self.index()?;
        let block = index
            .resolve_block_ref(id)
            .ok_or_else(|| Error::NoSuchBlockRef(id.to_owned()))?;
        Ok(index.canonical(self.graph.entity_of(block)).to_string())
    }

    /// The entity that defines the property `key`, made first if nobody has
    /// defined it: a property entity, named for the key, with an empty block
    /// to describe it in. Following a property's key goes here.
    pub fn define_property(&self, key: &str) -> Result<String> {
        let found = {
            let index = self.index()?;
            index.property_entity(key).map(|id| index.canonical(id))
        };
        let id = match found {
            Some(id) => id,
            None => {
                self.structural();
                let made = self.make_property_entity(key);
                self.edited(made)?
            }
        };
        Ok(id.to_string())
    }

    /// Settles a block after it has been edited, with `before` as its text
    /// when editing began. Every link with qualifiers that it gained and that
    /// goes nowhere gets an entity, with the properties the qualifiers ask for,
    /// and the ids of those entities are returned. A link without qualifiers
    /// makes nothing: a name nobody has made is an entity already, until it is
    /// written to (a virtual entity).
    ///
    /// All of it is one step to undo, separate from the typing.
    pub fn commit_block(&self, id: &str, before: &str) -> Result<Vec<String>> {
        let block = self.block_id(id)?;
        let after = self.graph.text(block)?;
        if after == before {
            return Ok(Vec::new());
        }
        let had: HashSet<String> = links_in(before).into_iter().map(|(key, _)| key).collect();
        self.structural();
        let mut made = Vec::new();
        for (key, link) in links_in(&after) {
            if had.contains(&key)
                || link.qualifiers.is_empty()
                || !matches!(self.index()?.resolve(&link), Resolution::Missing)
            {
                continue;
            }
            made.push(self.make_linked_entity(&link)?.to_string());
            self.changed();
        }
        Ok(made)
    }

    fn make_property_entity(&self, key: &str) -> Result<BlockId> {
        let key = key.trim();
        if key.is_empty() {
            return Err(quiddity_core::Error::EmptyName.into());
        }
        let kind = Value::Text("property".to_owned());
        let entity = self.graph.create_entity(key, &[("type", Some(kind))])?;
        self.graph.create_block(Some(entity), None, "")?;
        Ok(entity)
    }

    /// An entity for `link` to go to: named for it, with the properties its
    /// qualifiers ask for, and an empty block to type into.
    fn make_linked_entity(&self, link: &Link) -> Result<BlockId> {
        let properties: Vec<_> = link
            .qualifiers
            .iter()
            .map(|q| (q.key.as_str(), q.value.clone()))
            .collect();
        let entity = self.graph.create_entity(&link.name, &properties)?;
        self.graph.create_block(Some(entity), None, "")?;
        Ok(entity)
    }
}
