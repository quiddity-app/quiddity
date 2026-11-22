# quiddity-contacts

Entities as contacts, and an edit to a contact applied back. Plain data in and
out, with no platform in it.

- **A contact is any entity with a value for a property `##contact` maps**
  (`contact_of`), from its own properties only, never what it inherits. There is
  no type to say it is a person, on purpose: the graph's owner chose that.
- **`apply` is a three-way merge, not a replace.** The copy a contacts app
  edited is compared with the copy it was handed (`base`), and only the
  difference is made to the entity as it is now. Never "set the entity to
  `desired`": that undoes what synced in since. Counts matter (two equal numbers
  are two); an entry the entity gained itself is not added a second time.
- **Rewriting is line by line.** `rewrite` rebuilds the properties block's
  text from its lines and sets it with `set_text`, which is a character diff,
  so concurrent edits elsewhere in the block still merge. Don't turn it back
  into byte ranges: deleting neighbouring lines then takes the same newline
  twice (the property test found it).
- **Blank values are never written**, and an entity is never made with nothing
  in a property: it wouldn't be a contact and would be gone at the next look.
- **`forget` takes the values off, not the entity.** Deleting a contact on a
  phone must never delete a page.
- Property tests need the characters the syntax treats specially (quotes,
  brackets, colons, commas, newlines), or `format_property_line`'s quoting goes
  unexercised.
- **`reconcile` takes edits first, then makes the store match the graph.**
  Don't swap them: the store would be overwritten with the graph's contacts
  before its edits were read. A row that was edited is always put again, even
  if the edit changed nothing, because putting it is what clears it.
- **Test it against a store that behaves as a phone does** (`tests/reconcile.rs`):
  it remembers what it was handed, so an edit can be told from what was there.
  The convergence property (a second reconcile does nothing, whatever happened
  on either side) is the check that matters; keep new operations in it.
