type:: crate

- Entities as [[Contacts]]: reads them from a graph, and applies an edit made to a copy of one elsewhere. `crates/contacts`.
- It knows no platform. A contact is plain data that serialises, for whatever carries it to a phone's contacts and back.
- `contacts` lists them, `apply` makes an edit and `forget` takes a contact off an entity. `apply` is a three-way merge of the entries (what the copy was handed, what it is now, what the entity is now) so that a change made to the graph meanwhile isn't undone.
- Tested as a property: any contact made reads back as it was, and any edit to a contact gives the edited contact, over text with the characters the syntax treats specially. [[Testing]]
- `reconcile` keeps a `Store`, which is somebody's copy of the contacts, in step with the graph: edits go into the graph first, then the copy is made to match. The trait is what a platform implements, and the tests implement it as a phone does, with a row for each contact, edits and deletions made on it, and what it was handed over as. [[Contacts]]
