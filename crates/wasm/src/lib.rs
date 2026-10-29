//! A Quiddity graph replica for the browser.
//!
//! The UI holds its own copy of the graph, so typing never waits on the shell.
//! It loads a snapshot, answers every query from it, applies edits to it, and
//! sends the shell the updates those edits make. This wraps
//! [`quiddity_view::View`] for JavaScript: queries return plain objects, ids
//! are strings and offsets are UTF-16 code units, which is what JavaScript and
//! CodeMirror count in.

use quiddity_core::{Graph, Version};
use quiddity_view::{Error, View};
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// Converts to a plain JavaScript value: objects rather than `Map`s, and `null`
/// for a missing value (rather than `undefined`) to match the TypeScript types.
fn js<T: Serialize>(value: &T) -> Result<JsValue, JsError> {
    let serializer = serde_wasm_bindgen::Serializer::new()
        .serialize_maps_as_objects(true)
        .serialize_missing_as_null(true);
    Ok(value.serialize(&serializer)?)
}

fn date(text: &str) -> Result<jiff::civil::Date, Error> {
    text.parse().map_err(|_| Error::NotADate(text.to_owned()))
}

#[wasm_bindgen]
#[derive(Debug)]
pub struct Replica {
    view: View,
}

#[wasm_bindgen]
impl Replica {
    /// A replica of the graph in `snapshot`.
    #[wasm_bindgen(constructor)]
    pub fn new(snapshot: &[u8]) -> Result<Replica, JsError> {
        console_error_panic_hook::set_once();
        let view = View::new(Graph::from_snapshot(snapshot).map_err(Error::from)?)?;
        view.graph().enable_undo();
        Ok(Self { view })
    }

    /// A replica of a new, empty graph. For development outside the app.
    pub fn empty() -> Result<Replica, JsError> {
        console_error_panic_hook::set_once();
        let view = View::new(Graph::new())?;
        view.graph().enable_undo();
        Ok(Self { view })
    }

    /// The whole graph, to start another replica from.
    pub fn snapshot(&self) -> Result<Vec<u8>, JsError> {
        Ok(self.view.graph().snapshot().map_err(Error::from)?)
    }

    /// What this replica has seen, to pass to `exportUpdates` later.
    pub fn version(&self) -> Vec<u8> {
        self.view.graph().version().encode()
    }

    /// The edits made since `since` (or all of them), for another replica.
    #[wasm_bindgen(js_name = exportUpdates)]
    pub fn export_updates(&self, since: Option<Vec<u8>>) -> Result<Vec<u8>, JsError> {
        let since = match since {
            Some(bytes) => Version::decode(&bytes).map_err(Error::from)?,
            None => Version::default(),
        };
        Ok(self
            .view
            .graph()
            .updates_since(&since)
            .map_err(Error::from)?)
    }

    /// Applies updates or a snapshot from another replica.
    #[wasm_bindgen(js_name = import)]
    pub fn import(&self, bytes: &[u8]) -> Result<(), JsError> {
        self.view.graph().import(bytes).map_err(Error::from)?;
        self.view.changed();
        Ok(())
    }

    pub fn entities(&self) -> Result<JsValue, JsError> {
        js(&self.view.entities()?)
    }

    /// Entities whose titles match `query`, best first.
    #[wasm_bindgen(js_name = searchTitles)]
    pub fn search_titles(&self, query: &str, limit: usize) -> Result<JsValue, JsError> {
        js(&self.view.search_titles(query, limit)?)
    }

    /// Blocks whose text matches `query`, best first.
    #[wasm_bindgen(js_name = searchText)]
    pub fn search_text(&self, query: &str, limit: usize) -> Result<JsValue, JsError> {
        js(&self.view.search_text(query, limit)?)
    }

    /// Every entity with its tags, word count and incoming links, for listing.
    pub fn summaries(&self) -> Result<JsValue, JsError> {
        js(&self.view.summaries()?)
    }

    /// Every entity and the links between them, for drawing as a graph.
    pub fn connections(&self) -> Result<JsValue, JsError> {
        js(&self.view.connections()?)
    }

    pub fn favourites(&self) -> Result<JsValue, JsError> {
        js(&self.view.favourites()?)
    }

    /// Journal pages from `today` (`YYYY-MM-DD`) back, newest first.
    pub fn journals(&self, today: &str) -> Result<JsValue, JsError> {
        js(&self.view.journals(date(today)?)?)
    }

    /// The journal page for `today`, created if there isn't one.
    #[wasm_bindgen(js_name = ensureJournal)]
    pub fn ensure_journal(&self, today: &str) -> Result<JsValue, JsError> {
        js(&self.view.ensure_journal(date(today)?)?)
    }

    pub fn entity(&self, id: &str) -> Result<JsValue, JsError> {
        js(&self.view.entity(id)?)
    }

    pub fn references(&self, id: &str) -> Result<JsValue, JsError> {
        js(&self.view.references(id)?)
    }

    #[wasm_bindgen(js_name = unlinkedReferences)]
    pub fn unlinked_references(&self, id: &str) -> Result<JsValue, JsError> {
        js(&self.view.unlinked_references(id)?)
    }

    /// Ends a burst of typing, so the next typing is a new undo step. Call
    /// when typing pauses.
    #[wasm_bindgen(js_name = endTyping)]
    pub fn end_typing(&self) {
        self.view.end_typing();
    }

    /// Undoes this replica's latest step. False if there is none.
    pub fn undo(&self) -> Result<bool, JsError> {
        Ok(self.view.undo()?)
    }

    pub fn redo(&self) -> Result<bool, JsError> {
        Ok(self.view.redo()?)
    }

    /// Forgets what could be undone: for edits that aren't the user's, such
    /// as creating today's journal when the app starts.
    #[wasm_bindgen(js_name = clearUndo)]
    pub fn clear_undo(&self) {
        self.view.graph().clear_undo();
    }

    /// A block's current text.
    #[wasm_bindgen(js_name = blockText)]
    pub fn block_text(&self, id: &str) -> Result<String, JsError> {
        Ok(self.view.block_text(id)?)
    }

    /// Replaces `len` UTF-16 units of a block's text from `pos` with `insert`.
    #[wasm_bindgen(js_name = spliceText)]
    pub fn splice_text(
        &self,
        id: &str,
        pos: usize,
        len: usize,
        insert: &str,
    ) -> Result<(), JsError> {
        Ok(self.view.splice_text(id, pos, len, insert)?)
    }

    #[wasm_bindgen(js_name = createBlock)]
    pub fn create_block(
        &self,
        parent: &str,
        index: Option<usize>,
        text: &str,
    ) -> Result<String, JsError> {
        Ok(self.view.create_block(parent, index, text)?)
    }

    #[wasm_bindgen(js_name = createEntity)]
    pub fn create_entity(&self, name: &str) -> Result<String, JsError> {
        Ok(self.view.create_entity(name)?)
    }

    /// Renames an entity, keeping the old name as an alias. False if the name
    /// is the same.
    #[wasm_bindgen(js_name = renameEntity)]
    pub fn rename_entity(&self, id: &str, name: &str) -> Result<bool, JsError> {
        Ok(self.view.rename_entity(id, name)?)
    }

    /// The entity a link goes to, made first if there is none. `key` is the
    /// text between the brackets, as the entity's `links` has it.
    #[wasm_bindgen(js_name = followLink)]
    pub fn follow_link(&self, key: &str) -> Result<String, JsError> {
        Ok(self.view.follow_link(key)?)
    }

    /// The entity that defines the property `key`, made first if there is
    /// none.
    #[wasm_bindgen(js_name = defineProperty)]
    pub fn define_property(&self, key: &str) -> Result<String, JsError> {
        Ok(self.view.define_property(key)?)
    }

    /// Settles a block after editing, given its text from before: makes an
    /// entity for each link it gained that goes nowhere, or corrects the
    /// name of one made from a link just edited. Returns the ids made.
    #[wasm_bindgen(js_name = commitBlock)]
    pub fn commit_block(&self, id: &str, before: &str) -> Result<Vec<String>, JsError> {
        Ok(self.view.commit_block(id, before)?)
    }

    /// Splits a block at `at`; returns the new block.
    pub fn split(&self, id: &str, at: usize) -> Result<String, JsError> {
        Ok(self.view.split(id, at)?)
    }

    pub fn indent(&self, id: &str) -> Result<bool, JsError> {
        Ok(self.view.indent(id)?)
    }

    pub fn outdent(&self, id: &str) -> Result<bool, JsError> {
        Ok(self.view.outdent(id)?)
    }

    /// Swaps a block with the sibling `delta` places away.
    #[wasm_bindgen(js_name = moveSibling)]
    pub fn move_sibling(&self, id: &str, delta: i32) -> Result<bool, JsError> {
        Ok(self.view.move_sibling(id, delta as isize)?)
    }

    /// Joins a block to the one before it. Returns `{ into, at }`, or
    /// `undefined` if it can't be.
    #[wasm_bindgen(js_name = mergeIntoPrevious)]
    pub fn merge_into_previous(&self, id: &str) -> Result<JsValue, JsError> {
        match self.view.merge_into_previous(id)? {
            Some(merged) => js(&merged),
            None => Ok(JsValue::UNDEFINED),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the UI does with it, minus JavaScript: load a snapshot, edit,
    /// send the updates on.
    #[test]
    fn the_replica_undoes_its_own_edits_only() {
        let shell = Graph::new();
        let page = shell.create_entity("Page", &[]).unwrap();
        let block = shell.create_block(Some(page), None, "hi").unwrap();
        let replica = Replica::new(&shell.snapshot().unwrap()).unwrap();
        let id = block.to_string();

        assert!(!replica.undo().unwrap(), "nothing of its own yet");
        replica.splice_text(&id, 2, 0, "!").unwrap();
        replica.end_typing();
        assert!(replica.undo().unwrap());
        assert_eq!(replica.block_text(&id).unwrap(), "hi");
        assert!(replica.redo().unwrap());
        replica.clear_undo();
        assert!(!replica.undo().unwrap(), "cleared");
        assert_eq!(replica.block_text(&id).unwrap(), "hi!");
    }

    #[test]
    fn links_to_nowhere_get_an_entity_when_followed_or_saved() {
        let shell = Graph::new();
        let page = shell.create_entity("Page", &[]).unwrap();
        let block = shell.create_block(Some(page), None, "").unwrap();
        let replica = Replica::new(&shell.snapshot().unwrap()).unwrap();
        let id = block.to_string();

        let followed = replica.follow_link("Nowhere").unwrap();
        assert_eq!(replica.follow_link("Nowhere").unwrap(), followed);
        replica.splice_text(&id, 0, 0, "[[Else]]").unwrap();
        let made = replica.commit_block(&id, "").unwrap();
        assert_eq!(made.len(), 1);
        assert_ne!(made[0], followed);
    }

    #[test]
    fn updates_carry_edits_between_replicas() {
        let shell = Graph::new();
        let page = shell.create_entity("Page", &[]).unwrap();
        let block = shell.create_block(Some(page), None, "hi").unwrap();
        let replica = Replica::new(&shell.snapshot().unwrap()).unwrap();

        let before = replica.version();
        replica.splice_text(&block.to_string(), 2, 0, "!").unwrap();
        let updates = replica.export_updates(Some(before)).unwrap();
        shell.import(&updates).unwrap();
        assert_eq!(shell.text(block).unwrap(), "hi!");
        assert!(!replica.export_updates(None).unwrap().is_empty());
    }
}
