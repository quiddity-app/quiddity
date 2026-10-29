# quiddity-view

What a UI shows of a graph (queries) and the edits it makes to it (the
outliner's). It owns a `Graph` and an `Index` over it, and returns plain data.
`quiddity-wasm` compiles it for the browser, where the UI holds its own copy
of the graph; nothing here knows about JavaScript or Tauri.

## The index

- **It is rebuilt lazily.** Edits mark the view stale (`changed`); the next
  query rebuilds. Anything done to the graph directly (an import, say) must
  be followed by `View::changed`, or queries answer from the old index.
- **Don't hold the index across a call that may rebuild it.** It sits in a
  `RefCell`, and a rebuild needs the borrow exclusively, so a `Ref` kept while
  another method runs panics once an edit has made it stale. Take it, use it,
  drop it; `block` shows the pattern.
- A rebuild reads every block, so an edit followed by a query costs time in
  proportion to the graph (about 18 ms at 10,000 blocks). The UI therefore
  asks for fresh answers after a pause in typing, not per keystroke.
- There is no clock. Anything that depends on the date (`journals`,
  `ensure_journal`) takes `today`.

## Wire format

The types in `query.rs` are what crosses to JavaScript and are mirrored by
`apps/ui/lib/model.ts`: change both together. IDs are strings. Names are
camelCase. **Offsets are UTF-16 code units** (`Property::start`/`end`, the
`pos` of `splice_text`, `split`), because that is what JavaScript strings and
CodeMirror count in; Rust's byte offsets never leave the crate. A missing
value is `None` here and `null` in JavaScript (the wasm layer sets that).

## Entities and blocks

- `entity()` accepts the ID of an entity that has been merged into another
  and answers for the one it was merged into, whose ID it reports. Callers
  should use the ID in the answer.
- **Block IDs don't survive an undo** of a deletion or redo of a creation
  (see `quiddity-core`). Tests compare text, not IDs, across an undo.
- `delete_entities` checks every ID before deleting any, and **deletes the
  duplicates merged into an entity with it**, or a journal's twin would show
  in its place. It leaves links to the entity alone: they become links to
  nowhere, as if the entity had never been made.
- Links are looked up in `Entity::links` by the text between their brackets as
  the syntax crate writes it (`link_key`). A link written some other way
  isn't found and renders as unresolved.
- Each entry (`ResolvedLink`) also carries the link's `name` and its written
  `qualifiers`, so the UI never parses link text: it shows the name and puts
  the qualifiers in a tooltip.

## Edits

- **Each edit is one commit, so one undo step** (a split is a splice and a
  create). Typing is the exception: `splice_text` opens an undo group, so a
  burst is one step, until `end_typing`.
- **Every other edit must call `structural()` first**, which ends any open
  burst. A new edit that forgets it would be merged into the typing before it.
- Edits that can't apply change nothing: `split` checks the offset is between
  characters before touching the text, and `merge_into_previous` says `None`
  rather than failing when there is nothing to merge into.
- **Journals and property entities can't be renamed**: they are identified by
  their date or name, so a rename would make them a different entity (and
  `Entity::renamable` says so, for the UI). Everything else keeps its old name
  as an alias; see `quiddity-core`.
- Reading order is depth first; "the block before" a block is the previous one
  in that order, which for a first child is its parent.

## Connections

`connections()` is the whole graph for drawing: every entity (as `entities`)
and one `Edge` per pair of entities where one links to the other from anywhere
in its blocks, however many links there are, including property values. The
source is the entity a link's block belongs to, as merged duplicates go, and an
entity's links to itself and links to nowhere make no edge. It walks every
backlink, so like the other queries it is a whole-graph cost the UI pays once
per revision.

## Summaries

`summaries()` is the list of every entity, in the order of `entities()`, with
what the All entities table shows.

- **Words** are whitespace-separated words in the entity's blocks at any depth,
  not its title and not blocks of nothing but property lines.
- **Incoming** counts blocks in *other* entities that link to it (a duplicate
  merged into the entity counts as the entity), so it is not the number of
  edges `connections()` draws.
- **Tags** are properties with no value (`#tag`, anywhere in the entity) and the
  values of a `tags` property (`tags:: [a, b]`; a bare `tags:: a, b` is one text
  value), compared by `normalise_name`. They are shown only to tell entities
  with the same title (ignoring case) apart: `distinguishing` picks, greedily,
  the fewest of an entity's tags that every same-titled rival lacks at least
  one of. Entities that are indistinguishable show none.

- **Created and modified** are seconds since the epoch, or `None` when the graph
  has no time for it (see Times in `quiddity-core`). Created is when the entity's
  root was made. Modified is the latest of when any of its blocks was added,
  moved or had its text edited; deleting a block doesn't count. Merging
  duplicate journals moves blocks, so it counts as a move. The text edit times
  are read from history once and then only what is new (`Activity`), kept for
  the life of the `View`.

## Links to nowhere

A `[[link]]` with no entity is made one in two ways (`links.rs`):

- **`follow_link(key)`** when the UI follows it: the entity, named for the
  link, with the properties its qualifiers ask for and an empty block. An
  ambiguous link is an error.
- **`commit_block(id, before)`** when editing a block ends; the UI passes the
  text it had when editing began. It makes an entity for each link the edit
  *gained*. Links already in `before` are left alone, so a block that arrived
  from another device with a dangling link makes nothing until the link is
  followed.
- **Correcting a link renames the entity it made.** The links either side of
  the edit are set aside and the ones in between are paired in order; a pair
  whose old link goes to an entity that is empty, linked from nothing but this
  block, and unqualified gets `correct_entity_name` (no alias) instead of a
  second entity. "Empty" is what rules out journals, property entities and a
  link that reached its entity by an alias: each has a properties block. Anything
  more than that (notes written under it, another link to it) makes the
  entity real, and it keeps its name.
- Saving is one undo step of its own, apart from the typing before it.

A property's key goes to the entity that defines it: `Property::definition` is
that entity's id (found by name as `Index::property_entity` does) or `None`, and
`define_property(key)` makes one, a property entity with an empty block, when
there is none. Like a link to nowhere, the UI makes it on following.

## Search

`fuzzy_match(query, name)` is the one matcher: characters in order, ignoring
case, but **a capital must start a word (or be that same capital)** and so must
anything typed after a space, as an IDE's "go to" does. Of the possible
alignments the best scoring wins (adjacent runs, word starts and the start of
the name count for more). Its ranges are UTF-16 units, for highlighting.
`search_titles` runs it over every entity's title. Whitespace in a query is
never matched itself. Tests use the weights by name, so retuning them doesn't
break the tests; the ranking tests are the ones to read when changing them.

`search_text` (`fts.rs`) is full-text search over block text: words are runs
of letters and digits, compared ignoring case; a block is found when every
query word matches one of its words, **the last also as a prefix** (the query
is usually still being typed). Blocks are ranked by BM25 with a bonus when a
multi-word query's words are together and in order. **It reads every block on
every query** (no inverted index yet), like an index rebuild; an inverted
index kept beside `Index` is the next step if that gets slow. Entity names
aren't text: `search_titles` has them.

## Tests

`tests.rs` builds graphs in memory with `view_with` and `page`. The queries,
the edits, properties and undo are separate modules. Offsets are checked
against text with emoji and accents, where bytes and UTF-16 units differ.
Mutation testing covers the whole crate and should stay clean.
