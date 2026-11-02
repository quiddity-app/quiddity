# quiddity-core

The graph on Loro, the index, schemas, journals, duplicate merging, and the
outline text format.

## Loro

- **Export commits pending edits.** `updates_since` and `snapshot` include
  uncommitted edits, so `Graph::commit` only marks a transaction boundary.
  With undo enabled that boundary matters: each commit is one undo step.
- **`version()` counts uncommitted edits too.**
- **Imports wait for their causes.** Updates whose earlier changes a replica
  hasn't seen are held pending without error. Exporting from a version the
  receiver doesn't have (say, from a fresh `Graph::new()` to a
  `Graph::blank()`) appears to import nothing. Test with replicas that share
  history, or send everything.
- `Graph::blank()` has no schema version until it syncs; `Graph::new()`
  writes one.

## Compaction

`Graph::shallow_snapshot(at)` keeps history only from `at` on. Loading it
gives a replica that:

- **can't update a peer behind `at`.** Loro would export updates that never
  apply, so `updates_since` refuses with `Error::BeforeHistory` instead;
  check `can_update` first. A blank peer can start from the snapshot.
- **can't apply edits made on the history it dropped.** Loro refuses them
  on import, also `Error::BeforeHistory`. So compacting at what every
  replica has is only safe once this replica has everything any replica
  has: an edit made on older history is then already here.
- **A Loro document that has imported anything ignores a compacted
  snapshot**, silently. So `from_snapshot` of a blank graph imports nothing,
  and `import` checks that what it imported shows in the version
  (`Error::NotApplied` if not), so nothing goes missing without a word.

`compaction_point` applies that rule using the device list
(`devices.rs`), which holds what each device has stored. It only works if
every device is in the list: the one that creates a graph records itself,
and a device vouches for each newcomer it hands the graph to
(`vouch_for`). A blank replica must not edit (or vouch) before it joins: an
edit with nothing beneath it can't be applied by a compacted replica.

## Undo

- **Opt-in.** `enable_undo` binds Loro's `UndoManager` to this replica; the
  shell and the headless node never call it, so they pay nothing. It only ever
  undoes this replica's own edits made after it was enabled, never another
  peer's. `undo`, `redo`, `clear_undo` and the group calls do nothing, and say
  so, until then.
- **One commit is one step.** A multi-block edit done in one commit (a split
  is a splice and a create) undoes as one unit. `begin_undo_group` and
  `end_undo_group` make the commits between them a single step, which is how a
  burst of typing is one step; `quiddity-view` decides where bursts begin and
  end.
- **Loro restores a deleted block as a new block.** Undoing a deletion, or
  redoing a creation, brings back the text and position under a new ID, and
  edits another device made concurrently to the old block are not carried
  over. An ID held across an undo is dead; tests compare text, not IDs, for
  this reason.
- `undo` and `redo` commit first, so edits made but not yet committed are a
  step of their own rather than lost.

## Times

Every replica records commit times (`Graph::blank` turns it on, so `new` and
`from_snapshot` have it too); Loro doesn't sync the setting. `created_at`,
`placed_at` (created or last moved) and `text_edits_since` read them back.

- **Times are to the second, and a change keeps its first edit's time.** Loro
  joins a replica's consecutive edits into one change, so the merge interval is
  0: the default of 1,000 seconds would leave a block's time up to a quarter of
  an hour behind its last edit.
- **Unknown is `None`, not zero.** Edits made before recording was on have a
  timestamp of 0, and history dropped by `shallow_snapshot` has no change to
  ask. **Compacting loses created dates**; modified dates survive only for what
  is edited afterwards.
- **Don't ask Loro for a diff of history from before a compaction.**
  `find_id_spans_between` unwraps a `None` and panics, which in the browser
  is an opaque `unreachable` from WASM. `text_edits_since` walks each peer's
  changes with `get_change` from no earlier than `history_start` instead.
- **A deletion isn't attributed to anything**: it leaves no block to ask.
- The text of a block is its own container, so a text edit isn't a move: ask
  `text_edits_since` for those and `placed_at` for the others. Reading history
  is the costly part, so callers keep what they read and ask only for what is
  new (`quiddity-view` does).

## Text

`splice_text` counts UTF-16 code units, the unit JavaScript and CodeMirror
use, and refuses an edit that reaches past the end. `set_text` is a diff and
is for whole-text replacement; typing should be a splice so that concurrent
edits merge as the typists meant.

## Renaming

`rename_entity` keeps the old name as an `alias` property line, so links
written against it still resolve, including ones written by a device that was
offline. It appends to the entity's properties block, or makes one first. No
alias is added if the old name is already one (aliases may be a list, and
names are matched as `normalise_name` does), or if only case or spacing
changed. It does not know which entities may be renamed: an entity identified
by its name or date (a property entity, a journal) must not be, and that is
the index's to say, so `quiddity-view` checks it.

`correct_entity_name` is the same without the alias, for a name nothing has
relied on yet (an entity made from a link a moment ago, whose link was then
mistyped). Anything that linked to the old name would stop finding the entity,
so the caller must know there is nothing.

## The model

- **Markdown is the source of truth.** Properties, links and block refs are
  parsed from block text. The `Index` is derived, rebuilt from the graph,
  and never synced.
- An entity is a root block; its text is its name. Its first child, if it
  holds only property lines, is its properties block.
- **`move_block`'s index is the final position**, so within the same parent
  it counts the siblings other than the block itself.
- **Duplicates are never deleted.** Entities with the same identity
  (journals by date, property entities by name) are merged into the one
  with the lowest ID by moving their content. A device that hasn't synced
  may still be writing to a duplicate, and anything under a deleted block
  would be lost. `merge_duplicates` runs after every sync and reports
  whether it moved anything. Spent duplicates are hidden and left out of
  exports.

## Schemas and effective properties

- **A schema is the built-in one with the property entity's `##` lines over
  it** (`PropertySchema::with_meta`). What can't be read is kept as flags
  for that entity and otherwise ignored. `Index::schema` borrows where it
  can: it is called in the inner loops.
- **Effective properties are worked out at build** (`effective.rs`), by
  repeating until no new value appears, so chains and cycles need no
  special case. Values are deduplicated by the entity they refer to, or by
  their text compared as names are. `from` is the entity a value is
  written on rather than the one it came through, which would depend on
  the order it was found in.
- **Only own properties identify an entity**, and only own properties are
  checked (`check.rs`); queries see effective ones, inverse labels included.
- A reference refers to an entity only if it resolves uniquely. Bare text
  refers only in a `ref` property.

## The outline format

- **`Indent::Tabs` is Quiddity's own format and is lossless.** It splits
  lines on `\n` only, so a `\r` in a block's text survives. `Indent::Auto`
  reads other tools' files (Logseq): it uses `lines()`, nests by width and
  respects code fences.
- Continuation lines are written as the bullet's tabs plus two spaces.
- Logseq sometimes writes page properties as the first bullet; that block
  is lifted into the outline's properties, splitting on `\n` only.

## Tests

`tests/common` has the shared generators (`text()`, `op()`) and replica
helpers. `common::config(n)` reads `PROPTEST_CASES`, so `n` is only the
default.
