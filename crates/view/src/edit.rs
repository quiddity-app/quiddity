//! The write side: what typing and the outliner's keys do to a graph.
//!
//! Every edit is committed on its own, so it syncs and undoes as one unit.
//! Block ids are the strings the queries hand out.

use std::collections::BTreeSet;

use quiddity_core::BlockId;
use serde::Serialize;

use crate::{Error, Result, View};

/// The outcome of merging a block into the one before it.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Merged {
    /// The block that took the text.
    pub into: String,
    /// Where the merged text begins in it, in UTF-16 units: the caret.
    pub at: usize,
}

/// The byte offset of `utf16` code units into `text`, if that is between
/// characters.
fn byte_offset(text: &str, utf16: usize) -> Option<usize> {
    let mut seen = 0;
    for (byte, c) in text.char_indices() {
        if seen == utf16 {
            return Some(byte);
        }
        seen += c.len_utf16();
        if seen > utf16 {
            return None;
        }
    }
    (seen == utf16).then_some(text.len())
}

impl View {
    pub(crate) fn edited<T>(&self, result: Result<T>) -> Result<T> {
        self.graph.commit();
        self.changed();
        result
    }

    /// Edits other than typing are each a step of their own, so they end any
    /// burst of typing first.
    pub(crate) fn structural(&self) {
        self.end_typing();
    }

    /// Ends the burst of typing that [`View::splice_text`] opened, so the
    /// next typing is a new undo step. The UI calls it when typing pauses.
    pub fn end_typing(&self) {
        if self.typing.replace(false) {
            self.graph.end_undo_group();
        }
    }

    /// Undoes the latest step of this replica's edits. False if there is none.
    pub fn undo(&self) -> Result<bool> {
        self.end_typing();
        let undone = self.graph.undo();
        self.edited(undone.map_err(Error::from))
    }

    /// Redoes the step last undone. False if there is none.
    pub fn redo(&self) -> Result<bool> {
        self.end_typing();
        let redone = self.graph.redo();
        self.edited(redone.map_err(Error::from))
    }

    /// A block's current text.
    pub fn block_text(&self, id: &str) -> Result<String> {
        Ok(self.graph.text(self.block_id(id)?)?)
    }

    /// Replaces `len` UTF-16 units of a block's text from `pos` with `insert`.
    ///
    /// Typing between two calls to [`View::end_typing`] is one undo step.
    pub fn splice_text(&self, id: &str, pos: usize, len: usize, insert: &str) -> Result<()> {
        let block = self.block_id(id)?;
        if !self.typing.replace(true) {
            self.graph.begin_undo_group()?;
        }
        let result = self.graph.splice_text(block, pos, len, insert);
        self.edited(result.map_err(Error::from))
    }

    /// Creates a block under `parent` at `index`, or at the end.
    pub fn create_block(&self, parent: &str, index: Option<usize>, text: &str) -> Result<String> {
        self.structural();
        let parent = self.block_id(parent)?;
        let created = self.graph.create_block(Some(parent), index, text);
        self.edited(created.map(|b| b.to_string()).map_err(Error::from))
    }

    /// Creates an entity with an empty first block to type into.
    pub fn create_entity(&self, name: &str) -> Result<String> {
        self.structural();
        let made = self.graph.create_entity(name, &[]).and_then(|entity| {
            self.graph.create_block(Some(entity), None, "")?;
            Ok(entity)
        });
        self.edited(made.map(|e| e.to_string()).map_err(Error::from))
    }

    /// Renames an entity, keeping the old name as an alias so that links to it
    /// still resolve. Returns false if the name is the same. A journal or
    /// property entity is identified by its date or name and can't be renamed.
    pub fn rename_entity(&self, id: &str, name: &str) -> Result<bool> {
        self.structural();
        let entity = self.entity_id(id)?;
        let renamable = self
            .index()?
            .entity(entity)
            .is_some_and(|e| e.identity().is_none());
        if !renamable {
            return Err(Error::NotRenamable(id.to_owned()));
        }
        let renamed = self.graph.rename_entity(entity, name);
        self.edited(renamed.map_err(Error::from))
    }

    /// Deletes entities and everything in them, as one undo step. Nothing is
    /// deleted if any of `ids` isn't an entity. A journal or property entity
    /// goes with any duplicates merged into it, or one of them would take its
    /// place. Links to a deleted entity are left in the text and point nowhere.
    /// Returns how many entities went.
    pub fn delete_entities(&self, ids: &[String]) -> Result<usize> {
        self.structural();
        let targets = ids
            .iter()
            .map(|id| self.entity_id(id))
            .collect::<Result<BTreeSet<_>>>()?;
        let doomed: Vec<_> = {
            let index = self.index()?;
            self.graph
                .roots()
                .into_iter()
                .filter(|&root| targets.contains(&index.canonical(root)))
                .collect()
        };
        let deleted = doomed
            .into_iter()
            .try_for_each(|root| self.graph.delete_block(root));
        self.edited(deleted.map(|()| targets.len()).map_err(Error::from))
    }

    /// Splits a block at `at` (UTF-16 units): the text after it moves to a new
    /// block, which is the first child if the block has children and the next
    /// sibling if not. Returns the new block.
    pub fn split(&self, id: &str, at: usize) -> Result<String> {
        self.structural();
        let block = self.block_id(id)?;
        let text = self.graph.text(block)?;
        let byte = byte_offset(&text, at).ok_or(Error::BadOffset(at))?;
        let tail = &text[byte..];
        let tail_len = self.graph.text_len_utf16(block)? - at;
        let parent = self
            .graph
            .parent(block)
            .ok_or(Error::NotABlock(id.to_owned()))?;
        let created = (|| {
            self.graph.splice_text(block, at, tail_len, "")?;
            if self.graph.children(block).is_empty() {
                let siblings = self.graph.children(parent);
                let here = siblings.iter().position(|&s| s == block).unwrap_or(0);
                self.graph.create_block(Some(parent), Some(here + 1), tail)
            } else {
                self.graph.create_block(Some(block), Some(0), tail)
            }
        })();
        self.edited(created.map(|b| b.to_string()).map_err(Error::from))
    }

    /// Makes a block the last child of the sibling before it. False if it has
    /// none.
    pub fn indent(&self, id: &str) -> Result<bool> {
        self.structural();
        let block = self.block_id(id)?;
        let parent = self
            .graph
            .parent(block)
            .ok_or(Error::NotABlock(id.to_owned()))?;
        let siblings = self.graph.children(parent);
        let here = siblings.iter().position(|&s| s == block).unwrap_or(0);
        let Some(&before) = here.checked_sub(1).and_then(|i| siblings.get(i)) else {
            return Ok(false);
        };
        let moved = self.graph.move_block(block, Some(before), None);
        self.edited(moved.map(|()| true).map_err(Error::from))
    }

    /// Makes a block the next sibling of its parent. False if its parent is
    /// the entity itself.
    pub fn outdent(&self, id: &str) -> Result<bool> {
        self.structural();
        let block = self.block_id(id)?;
        let parent = self
            .graph
            .parent(block)
            .ok_or(Error::NotABlock(id.to_owned()))?;
        let Some(grandparent) = self.graph.parent(parent) else {
            return Ok(false);
        };
        let uncles = self.graph.children(grandparent);
        let at = uncles
            .iter()
            .position(|&u| u == parent)
            .map_or(0, |i| i + 1);
        let moved = self.graph.move_block(block, Some(grandparent), Some(at));
        self.edited(moved.map(|()| true).map_err(Error::from))
    }

    /// Swaps a block with the sibling `delta` places away (-1 is up). False
    /// if there is none.
    pub fn move_sibling(&self, id: &str, delta: isize) -> Result<bool> {
        self.structural();
        let block = self.block_id(id)?;
        let parent = self
            .graph
            .parent(block)
            .ok_or(Error::NotABlock(id.to_owned()))?;
        let siblings = self.graph.children(parent);
        let here = siblings.iter().position(|&s| s == block).unwrap_or(0);
        let Some(to) = here
            .checked_add_signed(delta)
            .filter(|&t| t < siblings.len())
        else {
            return Ok(false);
        };
        let moved = self.graph.move_block(block, Some(parent), Some(to));
        self.edited(moved.map(|()| true).map_err(Error::from))
    }

    /// Appends a childless block's text to the block before it in reading
    /// order, and deletes it. Nothing happens, and `None` comes back, if it
    /// has children or nothing is before it in its entity.
    pub fn merge_into_previous(&self, id: &str) -> Result<Option<Merged>> {
        self.structural();
        let block = self.block_id(id)?;
        if !self.graph.children(block).is_empty() {
            return Ok(None);
        }
        let root = self.graph.entity_of(block);
        let mut order = Vec::new();
        self.reading_order(root, &mut order);
        let at = order.iter().position(|&b| b == block);
        let Some(&before) = at.and_then(|i| i.checked_sub(1)).and_then(|i| order.get(i)) else {
            return Ok(None);
        };
        let merged = (|| {
            let text = self.graph.text(block)?;
            let end = self.graph.text_len_utf16(before)?;
            self.graph.splice_text(before, end, 0, &text)?;
            self.graph.delete_block(block)?;
            Ok::<_, quiddity_core::Error>(Merged {
                into: before.to_string(),
                at: end,
            })
        })();
        self.edited(merged.map(Some).map_err(Error::from))
    }

    /// The blocks under `block`, depth first.
    fn reading_order(&self, block: BlockId, out: &mut Vec<BlockId>) {
        for child in self.graph.children(block) {
            out.push(child);
            self.reading_order(child, out);
        }
    }
}
