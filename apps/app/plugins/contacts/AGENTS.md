# tauri-plugin-quiddity-contacts

The phone's side of `quiddity-contacts`: a Tauri plugin whose Rust side is the
`Store` that `reconcile` keeps in step, and whose Android library puts the
contacts in the system's. See `docs/Android contacts.md` for the design.

- **An app can't be a contacts provider on Android; it keeps an account.**
  Contacts are raw contacts of the `uk.co.aylett.quiddity.contacts` account.
  The account needs an authenticator and a sync adapter to be listed and
  editable, so both are declared (`Services.kt`) and do nothing: the app
  reconciles itself, while it runs.
- **`SYNC1` is the contact as it was handed over**, as JSON. Without it an
  edit can't be told from what was already there, and `apply` can't merge.
  Don't use `SYNC1` for anything else, and don't drop it on an update.
- **Write as a sync adapter** (`asSyncAdapter`): otherwise the write marks the
  row dirty and comes back as an edit, forever, and a delete only flags.
- **A label must come back as it went** (`Fields.kt`). `other` is no label,
  on both sides, or the graph sees the phone as having edited a number.
- Only the kinds of data Quiddity has are replaced on a put (`MANAGED`), so a
  photo or ringtone the user gave a contact stays.
- **The library has never been compiled.** It was written without an Android
  SDK, from Tauri's Kotlin sources. Expect first-build fixes. The Rust side
  checks on every platform (it is all `Unavailable` off Android) but its
  Android half is only checked with `--target aarch64-linux-android`.
- Commands return objects (`{rows: [...]}` for lists): a result is an object.
- The plugin has no commands for the page. The shell calls it, and the page
  talks to the shell.
