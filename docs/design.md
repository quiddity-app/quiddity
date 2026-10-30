# Quiddity: initial design

Status: draft, for discussion. Nothing here is implemented yet.

Quiddity is a local-first second brain in the vein of Logseq, Obsidian and
Roam. It should feel familiar to people who have used those tools, run
primarily as a desktop and mobile app, sync between a user's devices without a
central service, export to and import from a sensible text format, and store
typed binaries.

## Goals

- **Local first.** Every device holds the whole graph and works fully offline.
- **Familiar.** Outliner editing, pages, journals, `[[links]]`, `#tags`,
  backlinks.
- **Peer-to-peer sync.** No central service. A headless node the user runs
  themselves fills in when portable devices aren't online at the same time.
- **Text in, text out.** A Markdown export that is readable on its own and good
  enough to re-import as a backup.
- **Extensible.** Types and properties are data, not code. New value types and
  block types can be added without breaking stored graphs.

## Non-goals (for now)

- A hosted web version. Next.js is purely the UI shell.
- Sharing part of a graph. Sharing is whole-graph only.
- Large-scale collaboration. The main case is one user with several devices.
  Multiple users on one graph should work, but it isn't what we're optimising
  for.
- Older app versions reading data written by newer ones (see
  [Versioning](#versioning-and-migration)).

## Architecture

```
┌──────────────── Tauri 2 app (desktop + iOS + Android) ────────────────┐
│                                                                        │
│  UI: Next.js (static export) + TypeScript                              │
│   ├─ its own replica: the core compiled to WASM, answering queries     │
│   ├─ editor (CodeMirror 6) bound to that replica                       │
│   └─ talks to the core over Tauri IPC (snapshot in, Loro updates out)  │
│                                                                        │
│  Core: Rust                                                            │
│   ├─ Loro document (one per graph): source of truth                    │
│   ├─ SQLite: persisted snapshot + update log, derived indexes, FTS     │
│   ├─ blob store: content-addressed binaries (iroh-blobs, BLAKE3)       │
│   └─ iroh endpoint: sync, presence and blob transfer between devices   │
└────────────────────────────────────────────────────────────────────────┘

┌──────── quiddity-node (headless) ────────┐
│  the same Rust core, without the UI;     │
│  always online, keeps full history       │
│  and all blobs                           │
└──────────────────────────────────────────┘
```

- **Tauri 2** for every platform. Networking, storage and indexing run in
  native Rust. A WebView can't reliably listen for connections or do local
  network discovery, and iOS evicts browser storage when it likes.
- **The UI holds its own Loro replica.** It exchanges updates with the core over
  IPC, using the same protocol peers use with each other. Keystrokes are applied
  locally in the UI, so typing never waits on IPC. The replica is the Rust core
  itself compiled to WASM (`quiddity-wasm`, over `quiddity-view`) rather than
  Loro's JavaScript bindings, so the UI has one implementation of the graph
  model, the index and the outliner's edits, and answers its own queries
  (entities, references, favourites) without a round trip. The shell hands it a
  snapshot at startup and applies the updates it sends back, a moment after the
  last edit, saving each time.
- **The headless node is the core run as a daemon.** It is a full peer owned by
  the user, not a dumb relay. It follows the same rules as every other
  device; being always on, it is what lets devices that are never online
  together sync.

Repository layout (a Cargo workspace for the crates, a pnpm workspace for the
apps):

```
crates/syntax    block Markdown syntax: properties, links, block refs
crates/core      graph model, index, journals, text import/export
crates/sync      sync protocol: messages, framing, per-connection sessions
crates/node      sync over iroh, and the quiddity-node CLI and headless node
crates/view      what a UI shows of a graph (queries) and its edits (outliner)
crates/wasm      the graph replica for the browser: view, compiled to WASM
fuzz/            cargo-fuzz targets
crates/cli       import/export/fsck tooling (later)
apps/app         Tauri 2 shell (not part of the Cargo workspace: needs webkit)
apps/ui          Next.js UI, static export
docs/
```

The Markdown syntax is needed on both sides: Rust for indexing, import and
export, and TypeScript for the editor. `crates/syntax` is the reference
implementation. The UI will use it compiled to WASM rather than keeping a
second parser in step.

## Data model

### Blocks

The block is the unit of everything. Each block has:

| Field  | Storage                     | Notes                                     |
|--------|-----------------------------|-------------------------------------------|
| id     | `LoroTree` node ID          | stable and unique, but internal to a graph |
| parent | the tree structure          | none for roots                            |
| order  | the tree's fractional index | concurrent moves can't create cycles      |
| text   | `LoroText` holding Markdown | merges character by character             |

All blocks live in one `LoroTree` inside one Loro document per graph. A move
between entities is then just a tree move, and Loro handles concurrent moves
for us, cycles included.

**The Markdown text is the source of truth for properties.** Nothing about a
block's properties is stored outside its text. Properties, links and
backlinks are all derived by parsing, and that derived state lives in an
index that can be rebuilt at any time and is never synced. The upshots:

- Export and import are lossless with no special cases.
- A property edit is a text edit, so it merges character by character like
  any other. Two devices changing the same value concurrently can therefore
  produce a merged string rather than one winner. With one user on several
  devices that's rare, and the result is visible and fixable, rather than
  silently dropping one side.
- The schema can't reject anything. Values that don't fit it are flagged.

### Properties

Properties are written in two ways, both parsed from the text:

- **Property lines**, as in Logseq's original format: `key:: value` on a line
  of its own. A key starts with a letter, digit or underscore, may contain
  spaces, and may not contain `:`, `#`, brackets or parentheses.
- **Inline properties**, anywhere in running text: `#key` or `#key(value)`.

A block's **type** is the value of its `type` property.

The two shapes of multiple values are distinguished explicitly:

| Written                          | Meaning                                   |
|----------------------------------|-------------------------------------------|
| `#tag(a) #tag(b)`, or two `tag::` lines | two instances of `tag`, one value each |
| `#steps([a, b, c])`, `steps:: [a, b, c]` | one instance holding an ordered list |

Lists are always written with brackets, and don't nest. In a property line a
comma means nothing special: `note:: a, b` is the text "a, b".

**Property names are unique.** A property key is an entity of type
`property` whose name is the key. If two devices create the same property
entity concurrently, the duplicates are merged like journals (below). Each
property entity can describe its values:

| Schema property | Values                                                       |
|-----------------|--------------------------------------------------------------|
| `value-type`    | `text`, `number`, `boolean`, `date`, `datetime`, `ref` (more later, e.g. `blob`) |
| `form`          | `scalar` or `list`                                           |
| `cardinality`   | `one` or `many`                                              |

A property with no entity, or one whose entity doesn't say, is untyped text,
scalar, any number of instances. Quiddity's own properties (`type`, `alias`,
`date`, `favourite`, `id`, `title`, and the three above) have built-in
schemas, so no bootstrap blocks or genesis snapshot are needed. Because names are unique,
a built-in is found by its name alone. A property entity for one of them is
optional and only adds documentation.

### Values

At the syntax level a value is one of:

- **text**: written bare, or `"quoted"` when it contains delimiters
- **a link**: `[[Name#qualifier(value)]]`
- **a block reference**: `((id))`
- **a list** of the above: `[a, "b, c", [[D]]]`

The property's schema then reads it as text, a number (an IEEE-754 double), a
boolean (`true`/`false`/`yes`/`no`), a date (ISO 8601 calendar date), a
datetime (RFC 3339, offset required) or a reference. For a `ref` property,
bare text is a name, so `type:: person` means `type:: [[person]]`.

Escapes: `\` before any of `\ ( ) [ ] , " #` makes it literal. Any other
backslash is literal, so `path:: C:\Users` reads as written. Quoted strings
also accept `\n` for a newline.

### Entities

An entity is the equivalent of a Logseq or Roam page: **a root block**. Its
text is its name and its children are the page body. If the first child
consists only of `key:: value` lines, it is the entity's **properties block**
and holds the entity's own properties. This mirrors the properties block at
the top of a Logseq page.

```
Jane                         ← root block: the name
├─ type:: person             ← properties block
│  born:: 1970-01-01
├─ Met at [[Summit]]         ← body
│  └─ …
```

- **Names are not unique.** An entity is identified by its name together with
  its properties.
- **Renaming** keeps the old name as an `alias` value, so links written
  against the old name (including by a device that was offline) still resolve.
- **Duplicates.** Some entities are fully identified by their properties:
  journals (type and date) and property keys (type and name). When two
  devices create the same one concurrently, the copy with the lowest ID is
  kept and the content of the others moves into it. The others are **not
  deleted**, because a device that hasn't yet synced may still be adding to
  its copy, and anything added under a deleted block is lost. Instead the
  index hides them, and the merge runs after every sync, moving anything new
  across. Every device makes the same moves, so concurrent merges converge.
  For other entities with the same name, the UI offers to merge.

### Journals and dates

A journal page is an entity named for its date, with a properties block of
`type:: journal` and `date:: 2026-10-25`.

### Block text and link syntax

| Syntax                                     | Meaning                                          |
|--------------------------------------------|--------------------------------------------------|
| `key:: value`                              | a property, on its own line                      |
| `#tag`                                     | property `tag` with no value (a plain tag)       |
| `#prop(value)`                             | one instance of `prop` with `value`              |
| `#prop([a, b])`                            | one instance holding a list                      |
| `#[[Multi word]](value)`                   | a property whose name contains spaces            |
| `[[Name]]`                                 | link to an entity called `Name`                  |
| `[[Name#type(person)#born(1970-01-01)]]`   | link narrowed by the target's properties         |
| `#author([[Jane#type(person)]])`           | a reference value, by name                       |
| `((id))`                                   | reference to the block with that `id` property   |

A qualifier `#key(value)` in a link matches an entity with an instance of
`key` whose value is equal to `value` (or contains it, for a list). A bare
`#key` matches an entity with any instance of `key`. Names, and values being
compared, ignore case and repeated whitespace.

Links in text are **resolved when followed**, not when written:

- Exactly one match: go to it.
- Several matches: show a disambiguation page listing the candidates and the
  properties that tell them apart, with an option to narrow the link in place.
- No match: offer to create the entity, taking its initial properties from the
  link.

The index records what each link resolves to, so backlinks include ambiguous
links (marked as such).

**Block references.** Block IDs are internal to a graph and don't survive
export and import. A block that something refers to gets an `id:: <uuid>`
property line, added when it's first referred to (as Logseq does), and
`((uuid))` resolves through that property.

Lexing rules:

- `#` starts a property only at the start of a line, after whitespace or
  `(`, or straight after another inline property (so `#a(1)#b(2)` is two).
  `# Heading`, `foo#bar` and URL fragments are not properties.
- All-digit keys (`#123`) are not properties, so issue numbers stay as text.
  `#fff` is a property; we can live with that.
- A trailing full stop isn't part of a bare key: `see #todo.` is `todo`.
- Code spans and fenced code blocks are opaque.
- Inside a link, `#` followed by a key starts a qualifier. Write `\#` for a
  literal one. `[[C# notes]]` is fine as it is.
- Anything malformed is plain text. Parsing never fails.

### Binaries

A binary is referenced from text by content hash, in a property whose
`value-type` is `blob` (a value type still to come). The value is a BLAKE3
hash, with the MIME type, size and an optional file name alongside. The bytes
live in the iroh-blobs store, not in the Loro document.

- Portable devices fetch blobs **when they're opened** and may evict them
  later. The headless node keeps them all.
- An `asset` block type holds a blob together with metadata such as captions
  and dimensions, so that embedding is just a block reference.
- Garbage collection: unreferenced blobs are deleted after a grace period, and
  only once every device's acknowledged version is past the point where the
  last reference was removed.

## Storage

- The **Loro document** is the only source of truth. It is persisted in SQLite
  as the latest snapshot plus an append-only log of updates, compacted
  periodically.
- **Derived indexes** live in SQLite tables, can be rebuilt from the document
  at any time, and are never synced:
  - name and alias → block IDs
  - property key and value → block IDs
  - links and backlinks, with how each resolved
  - full-text search (FTS5)
- **Blobs** live on disk in the iroh-blobs store.

## Sync

### Transport: iroh

- Each device is an iroh endpoint and is identified by its node ID (an Ed25519
  public key).
- Connections are authenticated QUIC. They use hole punching where possible and
  fall back to a relay; we can run our own relays.
- Protocols are separated by ALPN:
  - `quiddity/sync/2`: Loro version-vector exchange and updates, and
    presence on the same stream
  - iroh-blobs ALPN: binaries

#### Presence

Which blocks each device has open is ephemeral, so it is kept out of the
graph, and so out of its history, and travels as messages on the sync
connection. Each device owns a record of the blocks it has open and the
devices it is directly connected to, and tells each peer about every live
device the peer hasn't heard of, so records reach the whole connected
network. A device is live if it is reachable over links that both ends
list. A peer that drops therefore cuts off whatever hung from it, even if it
never reports having lost its own connections. Liveness is the transport's
own: iroh pings connections and closes quiet ones, so there are no
timeouts of ours.
### Devices and pairing

- **The graph's device list is the list of devices allowed to connect.** It
  lives in the graph, keyed by node ID, and is the same list compaction uses
  (see below). A peer accepts a connection from a device listed in its own
  copy of the graph, whatever else it has been told. The list decides who may
  *connect*, not whose edits are kept: updates are merged whoever made them.
- **Authorising a device is a graph edit** (`quiddity-node allow`, or
  `Peer::allow`): it lists the device, and the entry syncs to every other
  device, which then lets it connect too. A device a peer has been told
  about explicitly (`--allow`) is also let in, and listed when it
  connects, which is how the first is let in. A blank replica can't
  authorise anyone until it has joined.
- Not built yet: pairing by QR code and one-time token (an existing device
  shows its node ID and a token; the new one presents it), and revocation.
  Eviction (silence for 30 days) affects only compaction, not whether a
  device may connect.

### Protocol

Implemented in `crates/sync` (independent of transport) and `crates/node`
(over iroh):

1. On connecting, each side sends a **hello**: its graph's schema version
   (none, for a blank replica), its device ID, where its history starts,
   and its Loro version vector.
2. A side that can't read the other's schema stops there, so a device that
   is behind never imports data it doesn't understand. So does a side
   behind where the other's history starts (below). Otherwise it vouches
   for the other if it hasn't heard of it, and sends the updates the other
   is missing, or a snapshot to a blank peer it can't bring up to date with
   updates.
3. From then on, each side sends new updates whenever its graph changes,
   whether by local edits or by updates from another peer. So live editing
   syncs character by character, and a device in the middle relays. Each
   session tracks what its peer has, including what the peer sent, so
   nothing is sent twice or echoed back.
4. Messages are length-prefixed frames on one QUIC stream. Unknown message
   types are ignored, so newer versions can add messages.
5. Acknowledgements aren't messages: each device records what it has
   stored in the graph itself (below), which syncs like any edit.

### Live edits versus stored history

Every update is sent live, at full detail, to whichever peers are online.
Stored history is compacted, and every device follows the same rule: none
is special, the headless node included.

- **Each device records what it has stored.** The graph holds a map from
  device ID to the version that device last durably stored and when. It is
  sync metadata, so it lives beside the blocks rather than in them, and is
  never exported. (Authorising devices is separate; see above.) A device
  writes its record just before saving, so the record is saved with what it
  describes, and writes only its own entry, so entries never conflict.
- **Newcomers are vouched for.** A device handing the graph to one it
  hasn't heard of records the newcomer at the newcomer's own version, until
  the newcomer records itself. The device that creates a graph records
  itself from the start. Then any device that might compact past a
  newcomer's history has heard of it: records made before the vouch are
  included in what the newcomer received, and later ones arrive with it.
- **A device compacts at what every device has stored**, by replacing its
  stored document with a Loro shallow snapshot taken there, but only once it
  has everything any device has stored. An edit can be made on history a
  device has dropped only if it was made before its author recorded
  storing it, and by then the compacting device has it. Undo will bound
  this further: never past the start of the local undo stack.
- **A replica can't update a peer behind its history**, since the updates
  would never apply. A blank peer is sent a snapshot instead. Any other peer
  behind is one that was evicted (below).
- **Eviction.** A device that hasn't recorded anything for N days
  (configurable; 30 by default) stops holding back compaction. If it
  returns, edits it made on history others have dropped can't be applied.
  It re-syncs from a snapshot, and its unsynced edits are saved as a diff
  against the fresh snapshot and offered for re-application. Exactly how is
  an open question.

### Undo

Undo uses Loro's `UndoManager` on each device and covers that device's own
edits only. The stack is kept in memory and limited in depth, which also sets
the local compaction horizon. A multi-block operation (indent, move, paste a
tree) is a single Loro transaction, so it syncs and undoes as one unit. Because
it's one document, there are no non-atomic edits across documents to worry
about.

In the UI, undo is the graph's rather than an editor's: Ctrl/Cmd-Z and
Ctrl/Cmd-Shift-Z (or Ctrl-Y) work whichever block has the focus, or none, and
undo structure as well as text. Each structural edit is one step. Typing is
one step until it pauses for about a second, so a sentence undoes together; a
structural edit ends any burst of typing first. What the app does itself, such
as creating today's journal at startup, is not on the stack.

Loro brings a deleted block back, text and all, but **as a new block**: an id
held across an undo of its deletion (or a redo of its creation) is dead, and
edits another device made concurrently to the old block are not carried over to
the new one. Nothing may therefore treat a block id as stable across an undo.
(Entity ids in URLs are not affected unless the entity's creation is undone.)

## Versioning and migration

- The document carries a `schemaVersion`.
- **New clients read old data.** Migration only goes forwards. It is done **on
  read** through version lenses, and writes always use the current format. We
  don't rewrite the document wholesale: two devices running the same
  migration concurrently would produce duplicate edits.
- When a migration must change stored data (rare, e.g. restructuring the
  meta-schema), it is written as a deterministic, idempotent transform keyed by
  migration ID and recorded in the document, so a second run does nothing.
- **Older clients don't read newer data.** The sync handshake compares schema
  versions. A device that is behind keeps working on its own data and stays
  read-only for synced data until it is upgraded. It never imports updates it
  can't understand.
- **Unknown value tags and unknown properties are always kept**, so that within
  one schema version newer value types can be introduced without breaking
  anything.

## Text export and import

Export is a directory:

```
quiddity-export/
  quiddity.json            marks a Quiddity export, with its format version
  entities/
    Jane.md
    Jane 2.md              same name, different entity: file name suffix only
  journals/
    2026-10-25.md
  assets/                  (with blobs) <blake3>.<ext>
```

Each file is an outline in the style of Logseq's original format:

```markdown
title:: Jane
type:: person
born:: 1970-01-01

- Met at [[Platform Summit#type(event)]] #status(done)
  a second line of the same block
	- Child block with a ((0198f0c4-…)) reference
- Quotable
  id:: 0198f0c4-…
```

- The lines before the first bullet are the entity's name (`title::`, so
  names needn't be valid or unique file names) followed by its properties
  block.
- Child bullets are indented with one tab per level. Continuation lines get
  the bullet's tabs plus two spaces.
- Since text is the source of truth, block properties need no special
  handling: they're simply part of each block's text.
- Re-importing an export is lossless (tested as a round trip). With the
  manifest present, indentation is read strictly. Without it, as for a
  Logseq graph, it's judged by width, fenced code is respected, `logseq/` and
  hidden directories are skipped, and files in `journals/` named
  `2026_10_25.md` or `2026-10-25.md` become journal pages.
- Exports skip duplicates that have been fully merged. Importing the same
  export twice creates duplicates of everything except journals and property
  entities, which merge.

## UI model

- **Three panes.** Entrypoints, favourites and recents on the left; the
  current entity in the middle; open secondaries on the right. The panes live
  in the root layout, so the middle pane is the routed page.
- **The left pane**: fixed entrypoints (journals, all entities); favourites,
  which are the entities with `favourite:: true`, so they live in the graph and
  sync; and recents, which are per device and kept locally.
- **View state is the URL**: `/entity/?id=<main>&open=<a>,<b>`. A static
  export cannot enumerate entities for dynamic path segments, and the right
  pane is a list rather than a route. Click opens in the main pane,
  shift-click opens at the side.
- **Nothing blocks anything else.** Reading the URL suspends in a static
  export, so each pane, and each secondary within the right pane, has its own
  Suspense and error boundary. A card's header sits outside its content's
  boundary, so it can always be closed.
- Slate palette (light 50/800, dark 900/200, following the system), IBM Plex
  Sans bundled with the app.

- **Outliner**: every bullet is a block. A block shows rendered Markdown and
  becomes a CodeMirror 6 editor bound to its `LoroText` when focused, much like
  Logseq's edit-raw/view-rendered toggle. What is typed goes into the text as
  the splice that was typed (in UTF-16 units, which is what CodeMirror
  counts), not as a diff, so concurrent edits merge as the typists meant.
  Keys: Enter splits the block at the caret (the tail becomes the next
  sibling, or the first child if there are children), Shift-Enter inserts a
  newline, Tab and Shift-Tab indent and outdent, Backspace at the start joins
  the block to the one before, Alt-Up and Alt-Down move it among its siblings,
  the arrows move between blocks at the edges, and Escape leaves the editor.
  Undo is document-wide (see [Undo](#undo)).
- **Views stay in step.** The same block can be open in the main pane and a
  secondary. Changes from elsewhere reach an open editor as the smallest edit
  that turns its text into the graph's. Typing notifies other views after a
  pause, and structure changes at once, so a view is not rebuilt per
  keystroke.
- **Pages** are entities. **Journals** are the default home: today's page, then
  earlier days as you scroll.
- **Properties** are rendered as chips when a block isn't being edited, and
  as their Markdown when it is. `key:: value` lines come out of the text and
  are drawn as pills below it (a link value is a live link, a list is a
  comma-separated run); inline `#tag` and `#prop(value)` become pills where
  they were written. The view returns each block's properties with where they
  are written, in UTF-16 units, so the UI does not parse them itself.
- **Block text is Markdown** (CommonMark with GFM: tables, task lists,
  strikethrough) when not being edited, drawn by a renderer that makes React
  elements rather than HTML strings, and that ignores raw HTML. Text comes from
  other devices, so it is treated as untrusted: raw HTML shows as text; only
  `http(s)` and `mailto` links are links, and they open in the system's browser
  rather than replacing the app; remote images are not fetched; and `[[links]]`
  and `((refs))` are looked for only in prose, not in code. The schema drives the editing aids (date
  picker, reference autocomplete, etc), which write Markdown into the text.
- **Linked and unlinked references** appear at the foot of every entity page,
  grouped by the entity they are in (journals first, newest first). Unlinked
  references are whole-word, case-insensitive mentions of the entity's name or
  an alias in prose: not inside a link, not in a properties block, and not in a
  journal's own date. They are found on request, since that means reading every
  block. Disambiguation pages are as described above.
- **Presence**: other devices or users editing the same block appear as
  cursors, via presence messages on the sync connection.
- Later: queries over properties, table and board views by type, and a
  plugin API.

## Decisions so far

- Properties: Logseq-style `key:: value` lines, with an entity's own
  properties in its initial block, plus inline `#prop(value)`. The Markdown is
  the source of truth.
- Property names are unique.
- Lists are explicit: `#prop([a, b])`.
- `crates/syntax` (Rust) is the reference parser. The UI uses it via WASM.

## Open questions

1. **Unsynced edits on eviction.** What happens to edits made on a device that
   was evicted: diff and re-apply, or kept as a separate entity to sort out by
   hand?
2. **At-rest encryption on the headless node.** Is the user's own node trusted
   with plaintext, or should it eventually run "blind" and store only encrypted
   snapshots?
3. **Concurrent edits to one property value.** Since properties are text, two
   devices editing the same value offline get a character-level merge. Is that
   acceptable, or should the UI detect a value that changed on both sides and
   ask?
