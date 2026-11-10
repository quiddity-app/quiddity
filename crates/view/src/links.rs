//! Links to entities that don't exist yet: following one makes the entity, and
//! so does saving a block that has just gained one.

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
    /// The entity a link goes to, made first if there is none. `key` is the
    /// text between the brackets, as [`crate::Entity::links`] has it. A link
    /// that matches several entities is an error: there is no one to open.
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
            Some(id) => id,
            None => {
                self.structural();
                let made = self.make_linked_entity(&link);
                self.edited(made)?
            }
        };
        Ok(id.to_string())
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
    /// when editing began. Every link it gained that goes nowhere gets an
    /// entity, and the ids of those entities are returned.
    ///
    /// If an entity is all that a link in the block made, and the link has
    /// been edited, the entity takes the link's new name rather than a second
    /// being made: its old name was never relied on, so no alias is kept. That
    /// is when the entity is empty, nothing but this block links to it, and
    /// neither link has qualifiers (the entity's properties would be wrong).
    ///
    /// All of it is one step to undo, separate from the typing.
    pub fn commit_block(&self, id: &str, before: &str) -> Result<Vec<String>> {
        let block = self.block_id(id)?;
        let after = self.graph.text(block)?;
        if after == before {
            return Ok(Vec::new());
        }
        let old = links_in(before);
        let new = links_in(&after);
        // What is left once the links either side of the edit are set aside:
        // the ones that were edited, in the order they were written.
        let start = old.iter().zip(&new).take_while(|(a, b)| a.0 == b.0).count();
        let end = old[start..]
            .iter()
            .rev()
            .zip(new[start..].iter().rev())
            .take_while(|(a, b)| a.0 == b.0)
            .count();
        let old = &old[start..old.len() - end];
        let new = &new[start..new.len() - end];

        self.structural();
        let mut made = Vec::new();
        let mut changed = false;
        for (i, (_, link)) in new.iter().enumerate() {
            if !matches!(self.index()?.resolve(link), Resolution::Missing) {
                continue;
            }
            let mistyped = match old.get(i) {
                Some((_, previous)) if link.qualifiers.is_empty() => {
                    self.unrelied_on(previous, block)?
                }
                _ => None,
            };
            match mistyped {
                Some(entity) => {
                    self.graph.correct_entity_name(entity, &link.name)?;
                }
                None => made.push(self.make_linked_entity(link)?.to_string()),
            }
            changed = true;
            self.changed();
        }
        if changed {
            self.edited(Ok(()))?;
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

    /// The entity `link` goes to, if it can still be renamed freely: it is
    /// empty, and nothing but `block` links to it. Being empty also rules out
    /// the entities a name must not change (a journal, a property) and one the
    /// link reached by an alias, as each has a properties block.
    fn unrelied_on(&self, link: &Link, block: BlockId) -> Result<Option<BlockId>> {
        if !link.qualifiers.is_empty() {
            return Ok(None);
        }
        let entity = {
            let index = self.index()?;
            let Resolution::Unique(entity) = index.resolve(link) else {
                return Ok(None);
            };
            let alone = index.backlinks(entity).iter().all(|b| b.source == block);
            if !alone {
                return Ok(None);
            }
            entity
        };
        for child in self.graph.children(entity) {
            if !self.graph.text(child)?.trim().is_empty() || !self.graph.children(child).is_empty()
            {
                return Ok(None);
            }
        }
        Ok(Some(entity))
    }
}
