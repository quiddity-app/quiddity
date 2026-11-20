//! The write side: what typing and the outliner's keys do to a graph.
//!
//! Every edit is committed on its own, so it syncs and undoes as one unit.
//! Block ids are the strings the queries hand out.

use std::collections::BTreeSet;

use quiddity_core::BlockId;
use quiddity_core::outline::{Indent, Outline, OutlineBlock, format_outline, parse_outline};
use serde::Serialize;

use crate::{Change, Error, Result, View};

/// The outcome of merging a block into the one before it.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Merged {
    /// The block that took the text.
    pub into: String,
    /// Where the merged text begins in it, in UTF-16 units: the caret.
    pub at: usize,
}

/// What pasting some text into a block being edited should do.
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "text", rename_all = "camelCase")]
pub enum PastedText {
    /// Not an outline: it goes in as the text it is.
    Plain,
    /// One block of an outline, with nothing under it. Its text goes in
    /// without the bullet, so a copied block pastes as what it said.
    Block(String),
    /// Several blocks, or one with children: they become blocks.
    Outline,
}

/// What pasting `text` into a block should do. Only text that starts with a
/// bullet is an outline, so a sentence with a dash in it is just text.
pub fn classify_paste(text: &str) -> PastedText {
    let first = text.trim_start().lines().next().unwrap_or("");
    let bulleted = first == "-" || first.starts_with("- ") || first.starts_with("-\t");
    if !bulleted {
        return PastedText::Plain;
    }
    let outline = parse_outline(text, Indent::Auto);
    // A bulleted first line always reads as at least one block, so what isn't
    // a lone childless one is several blocks, or one with children.
    match outline.blocks.as_slice() {
        [only] if only.children.is_empty() => PastedText::Block(only.text.clone()),
        _ => PastedText::Outline,
    }
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
    /// Ends an edit that may have changed anything.
    pub(crate) fn edited<T>(&self, result: Result<T>) -> Result<T> {
        self.edited_by(result, Change::Anything)
    }

    /// Ends an edit that changed only what `change` says, so that less is
    /// read again.
    pub(crate) fn edited_by<T>(&self, result: Result<T>, change: Change) -> Result<T> {
        self.graph.commit();
        self.changed_by(change);
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
        self.edited_by(result.map_err(Error::from), Change::Text(block))
    }

    /// Creates a block under `parent` at `index`, or at the end. A `parent`
    /// that is a name nobody has made makes the entity, in the same edit.
    pub fn create_block(&self, parent: &str, index: Option<usize>, text: &str) -> Result<String> {
        self.structural();
        let parent = if crate::virtual_entity::name_of(parent).is_some() {
            self.entity_to_write(parent)?
        } else {
            self.block_id(parent)?
        };
        let created = self.graph.create_block(Some(parent), index, text);
        self.edited_by(
            created.map(|b| b.to_string()).map_err(Error::from),
            Change::Structure,
        )
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

    /// Deletes `id` if it is an entity's first block, has no text and no
    /// children: what is left of an offer of a properties block when nothing
    /// was typed in it. Anything else is left alone. Returns whether it went.
    pub fn drop_empty_first_block(&self, id: &str) -> Result<bool> {
        self.structural();
        let block = self.block_id(id)?;
        let first = self.graph.parent(block).is_some_and(|parent| {
            self.graph.parent(parent).is_none()
                && self.graph.children(parent).first() == Some(&block)
        });
        let empty = self.graph.text(block)?.is_empty() && self.graph.children(block).is_empty();
        if !(first && empty) {
            return Ok(false);
        }
        let deleted = self.graph.delete_block(block);
        self.edited_by(
            deleted.map(|()| true).map_err(Error::from),
            Change::Structure,
        )
    }

    /// Adds a file the shell has kept to `page`, as a block at its end whose
    /// first line says what the file is: a blob. `hash` is the shell's answer
    /// for the bytes, and the parts are checked before anything is changed.
    /// The block is the file's place in the graph, with no page of its own.
    /// One undo step. Returns the new block's ID.
    pub fn attach_blob(
        &self,
        page: &str,
        hash: &str,
        mime: &str,
        size: u64,
        name: Option<&str>,
    ) -> Result<String> {
        let asset = quiddity_core::Asset::new(hash, mime, size, name)?;
        self.structural();
        let page = self.entity_id(page)?;
        let attached = self.graph.create_blob(page, &asset);
        self.edited(attached.map(|b| b.to_string()).map_err(Error::from))
    }

    /// Renames an entity, keeping the old name as an alias so that links to it
    /// still resolve. Returns false if the name is the same. A journal or
    /// property entity is identified by its date or name and can't be renamed.
    pub fn rename_entity(&self, id: &str, name: &str) -> Result<bool> {
        self.structural();
        // A name nobody has made is renamed by making it first, so its old
        // name is kept as an alias like any other, and links to it still work.
        let entity = if crate::virtual_entity::name_of(id).is_some() {
            let made = self.entity_to_write(id)?;
            // The index has to see the entity before it can be asked about it.
            self.changed();
            made
        } else {
            self.entity_id(id)?
        };
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
        // A name nobody has made has nothing to delete: it stays while
        // anything links to it, as a deleted entity's links go nowhere.
        let mut targets = BTreeSet::new();
        for id in ids {
            match self.named(id)? {
                Some(crate::virtual_entity::Named::Virtual(_)) => {}
                Some(crate::virtual_entity::Named::Real(entity)) => {
                    targets.insert(entity);
                }
                None => {
                    targets.insert(self.entity_id(id)?);
                }
            }
        }
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
    ///
    /// Splitting at the very start of a block with text moves nothing: the
    /// new block is an empty one *before* it, so the block keeps its text,
    /// children and place, rather than all of it becoming a child or sibling
    /// of an empty block.
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
            if at == 0 && !text.is_empty() {
                let siblings = self.graph.children(parent);
                let here = siblings.iter().position(|&s| s == block).unwrap_or(0);
                return self.graph.create_block(Some(parent), Some(here), "");
            }
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
        let indented = self.indent_one(block, &BTreeSet::new());
        self.edited_by(indented, Change::Structure)
    }

    /// Makes a block the next sibling of its parent. False if its parent is
    /// the entity itself.
    pub fn outdent(&self, id: &str) -> Result<bool> {
        self.structural();
        let block = self.block_id(id)?;
        let outdented = self.outdent_one(block);
        self.edited_by(outdented, Change::Structure)
    }

    /// Indents each of `ids` as [`View::indent`] does, with their children,
    /// in one undo step. Blocks inside another of `ids` go with it. The
    /// blocks nest under the sibling before the first of a run of siblings,
    /// so they stay in order; if that run has nothing before it, none of it
    /// moves. True if any block did.
    pub fn indent_blocks(&self, ids: &[String]) -> Result<bool> {
        self.structural();
        let tops = self.top_blocks(ids)?;
        let held: BTreeSet<_> = tops.iter().copied().collect();
        let moved = tops
            .into_iter()
            .try_fold(false, |any, block| Ok(self.indent_one(block, &held)? | any));
        self.edited_by(moved, Change::Structure)
    }

    /// Outdents each of `ids` as [`View::outdent`] does, in one undo step. A
    /// run of siblings follows their parent in the order they had, which
    /// takes the last first, since each lands straight after the parent. True
    /// if any block moved.
    pub fn outdent_blocks(&self, ids: &[String]) -> Result<bool> {
        self.structural();
        let tops = self.top_blocks(ids)?;
        let moved = tops
            .into_iter()
            .rev()
            .try_fold(false, |any, block| Ok(self.outdent_one(block)? | any));
        self.edited_by(moved, Change::Structure)
    }

    /// Deletes each of `ids` with its children, as one undo step. Blocks
    /// inside another of `ids` go with it and aren't counted. An entity is
    /// not a block to delete here and is left alone: [`View::delete_entities`]
    /// is for that. Links to what went are left in the text, and point
    /// nowhere. Returns how many blocks were deleted, not counting their
    /// children.
    pub fn delete_blocks(&self, ids: &[String]) -> Result<usize> {
        self.structural();
        let tops = self.top_blocks(ids)?;
        let deleted = tops
            .iter()
            .try_for_each(|&block| self.graph.delete_block(block));
        self.edited(deleted.map(|()| tops.len()).map_err(Error::from))
    }

    /// The text to put on the clipboard for `ids`: each block with its
    /// children as a Markdown outline, the same as in a file, so it reads as
    /// text anywhere and [`View::paste_blocks`] reads it back. Blocks inside
    /// another of `ids` come with it, once.
    pub fn copy_blocks(&self, ids: &[String]) -> Result<String> {
        fn outline_block(graph: &quiddity_core::Graph, block: BlockId) -> Result<OutlineBlock> {
            Ok(OutlineBlock {
                text: graph.text(block)?,
                children: graph
                    .children(block)
                    .into_iter()
                    .map(|child| outline_block(graph, child))
                    .collect::<Result<_>>()?,
            })
        }
        let blocks = self
            .top_blocks(ids)?
            .into_iter()
            .map(|block| outline_block(&self.graph, block))
            .collect::<Result<_>>()?;
        Ok(format_outline(&Outline {
            blocks,
            ..Outline::default()
        }))
    }

    /// Adds the blocks that `text` is an outline of as the siblings after
    /// `after`, as one undo step, and returns every block it made, children
    /// included, in reading order. Anything that isn't an outline is one block
    /// of that text, and nothing at all makes none. A title is left out, and
    /// properties are a block of their own.
    pub fn paste_blocks(&self, after: &str, text: &str) -> Result<Vec<String>> {
        self.structural();
        let anchor = self.block_id(after)?;
        let made = self.add_outline_after(anchor, text);
        self.edited(made)
    }

    /// Pastes `text` into the block being edited, `id`: its blocks go in after
    /// it, as [`View::paste_blocks`] does, except that a block with no text and
    /// no children is replaced, as it is a place to put them and not something
    /// to leave an empty block behind. One undo step. Returns every block made.
    pub fn paste_into_block(&self, id: &str, text: &str) -> Result<Vec<String>> {
        self.structural();
        let anchor = self.block_id(id)?;
        let replaced = (|| {
            let empty =
                self.graph.text(anchor)?.is_empty() && self.graph.children(anchor).is_empty();
            let made = self.add_outline_after(anchor, text)?;
            if empty && !made.is_empty() {
                self.graph.delete_block(anchor)?;
            }
            Ok(made)
        })();
        self.edited(replaced)
    }

    /// Adds the blocks of `text` after `anchor`, without ending the edit.
    fn add_outline_after(&self, anchor: BlockId, text: &str) -> Result<Vec<String>> {
        let parent = self
            .graph
            .parent(anchor)
            .ok_or_else(|| Error::NotABlock(anchor.to_string()))?;
        let at = self
            .graph
            .children(parent)
            .iter()
            .position(|&sibling| sibling == anchor)
            .map_or(0, |i| i + 1);
        let outline = parse_outline(text, Indent::Auto);
        let blocks: Vec<_> = outline
            .properties
            .map(|text| OutlineBlock {
                text,
                children: Vec::new(),
            })
            .into_iter()
            .chain(outline.blocks)
            .collect();
        fn add(
            graph: &quiddity_core::Graph,
            parent: BlockId,
            index: Option<usize>,
            block: &OutlineBlock,
            made: &mut Vec<String>,
        ) -> Result<()> {
            let id = graph.create_block(Some(parent), index, &block.text)?;
            made.push(id.to_string());
            for child in &block.children {
                add(graph, id, None, child, made)?;
            }
            Ok(())
        }
        let mut made = Vec::new();
        blocks
            .iter()
            .enumerate()
            .try_for_each(|(i, block)| add(&self.graph, parent, Some(at + i), block, &mut made))?;
        Ok(made)
    }

    /// The blocks of `ids` that aren't inside another of them, in reading
    /// order. Each is a whole subtree to act on, so what is inside one isn't
    /// acted on again.
    pub(crate) fn top_blocks(&self, ids: &[String]) -> Result<Vec<BlockId>> {
        let wanted = ids
            .iter()
            .map(|id| self.block_id(id))
            .collect::<Result<BTreeSet<_>>>()?;
        let mut entities = Vec::new();
        for &block in &wanted {
            let entity = self.graph.entity_of(block);
            if !entities.contains(&entity) {
                entities.push(entity);
            }
        }
        let inside_another = |block: BlockId| {
            std::iter::successors(self.graph.parent(block), |&b| self.graph.parent(b))
                .any(|ancestor| wanted.contains(&ancestor))
        };
        let mut tops = Vec::new();
        for entity in entities {
            let mut order = Vec::new();
            self.reading_order(entity, &mut order);
            tops.extend(
                order
                    .into_iter()
                    .filter(|&b| wanted.contains(&b) && !inside_another(b)),
            );
        }
        Ok(tops)
    }

    /// Indents a block, unless the sibling before it is one of `held`, which
    /// didn't move and so would be what it nested under.
    fn indent_one(&self, block: BlockId, held: &BTreeSet<BlockId>) -> Result<bool> {
        let parent = self
            .graph
            .parent(block)
            .ok_or_else(|| Error::NotABlock(block.to_string()))?;
        let siblings = self.graph.children(parent);
        let here = siblings.iter().position(|&s| s == block).unwrap_or(0);
        let Some(&before) = here.checked_sub(1).and_then(|i| siblings.get(i)) else {
            return Ok(false);
        };
        if held.contains(&before) {
            return Ok(false);
        }
        let moved = self.graph.move_block(block, Some(before), None);
        moved.map(|()| true).map_err(Error::from)
    }

    fn outdent_one(&self, block: BlockId) -> Result<bool> {
        let parent = self
            .graph
            .parent(block)
            .ok_or_else(|| Error::NotABlock(block.to_string()))?;
        let Some(grandparent) = self.graph.parent(parent) else {
            return Ok(false);
        };
        let uncles = self.graph.children(grandparent);
        let at = uncles
            .iter()
            .position(|&u| u == parent)
            .map_or(0, |i| i + 1);
        let moved = self.graph.move_block(block, Some(grandparent), Some(at));
        moved.map(|()| true).map_err(Error::from)
    }

    /// Moves a block one place up (-1) or down (1) among its siblings, and
    /// past the first or last of them into the next parent. False if it
    /// is at the very top or bottom of its entity.
    pub fn move_sibling(&self, id: &str, delta: isize) -> Result<bool> {
        self.structural();
        let block = self.block_id(id)?;
        let moved = self.move_one(block, delta, &BTreeSet::new());
        self.edited_by(moved, Change::Structure)
    }

    /// Moves each of `ids`, with its children, one place up (-1) or down (1),
    /// as one undo step. Blocks inside another of `ids` go with it.
    ///
    /// A block swaps with the sibling beyond it. At the end of its siblings it
    /// moves on into the neighbouring parent instead, as the last child of the
    /// parent before it or the first of the one after, or if there is none, out
    /// beside its own parent. Blocks that are chosen together stay together: a
    /// run of siblings moves as a run, and a run at the top level stops whole,
    /// rather than the rest passing its first block. True if any block moved.
    pub fn move_blocks(&self, ids: &[String], delta: isize) -> Result<bool> {
        self.structural();
        let mut tops = self.top_blocks(ids)?;
        // The leading blocks go first, so that each finds out whether the one
        // it would swap with is chosen and stuck, and so that a run leaving
        // its parent arrives in the order it had.
        if delta.is_positive() {
            tops.reverse();
        }
        let held: BTreeSet<_> = tops.iter().copied().collect();
        let moved = tops.into_iter().try_fold(false, |any, block| {
            Ok(self.move_one(block, delta, &held)? | any)
        });
        self.edited_by(moved, Change::Structure)
    }

    /// Swaps a block with the sibling beyond it, unless that is one of `held`,
    /// which didn't move, or moves it on into the next parent if there is no
    /// sibling that way and the parent beyond isn't one of `held` either.
    fn move_one(&self, block: BlockId, delta: isize, held: &BTreeSet<BlockId>) -> Result<bool> {
        let parent = self
            .graph
            .parent(block)
            .ok_or_else(|| Error::NotABlock(block.to_string()))?;
        let siblings = self.graph.children(parent);
        let here = siblings.iter().position(|&s| s == block).unwrap_or(0);
        let Some(to) = here
            .checked_add_signed(delta)
            .filter(|&t| t < siblings.len())
        else {
            return self.move_out(block, parent, delta, held);
        };
        if held.contains(&siblings[to]) {
            return Ok(false);
        }
        let moved = self.graph.move_block(block, Some(parent), Some(to));
        moved.map(|()| true).map_err(Error::from)
    }

    /// Moves the first or last child of `parent` (the direction `delta` says)
    /// on to where it goes next: into the end of the parent's previous sibling,
    /// or the start of its next, or beside the parent when there is no such
    /// sibling. An entity's properties block takes no children, so it doesn't
    /// count. Nothing moves from the top level, whose parent is the entity.
    fn move_out(
        &self,
        block: BlockId,
        parent: BlockId,
        delta: isize,
        held: &BTreeSet<BlockId>,
    ) -> Result<bool> {
        let Some(grandparent) = self.graph.parent(parent) else {
            return Ok(false);
        };
        let uncles = self.graph.children(grandparent);
        let at = uncles.iter().position(|&u| u == parent).unwrap_or(0);
        let properties = self
            .graph
            .properties_block(self.graph.entity_of(parent))
            .ok()
            .flatten();
        let neighbour = at
            .checked_add_signed(delta)
            .and_then(|i| uncles.get(i))
            .copied()
            .filter(|&n| Some(n) != properties);
        // A parent that is chosen and stuck isn't one to move into: the block
        // would end up inside another that is being moved.
        if neighbour.is_some_and(|n| held.contains(&n)) {
            return Ok(false);
        }
        let up = delta.is_negative();
        let moved = match neighbour {
            Some(n) if up => self.graph.move_block(block, Some(n), None),
            Some(n) => self.graph.move_block(block, Some(n), Some(0)),
            None if up => self.graph.move_block(block, Some(grandparent), Some(at)),
            None => self
                .graph
                .move_block(block, Some(grandparent), Some(at + 1)),
        };
        moved.map(|()| true).map_err(Error::from)
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
