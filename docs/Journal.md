alias:: journals

- A journal is the page for a day: an [[Entity]] named for its date, with `type:: journal` and `date:: 2026-10-25` in its properties block. [[type]] [[date]]
- The journals are home.
	- The app opens on today's journal, with earlier days below it as you scroll. It is the first entrypoint on the left. [[The UI]]
	- Today's journal is made when the app starts, if there isn't one. That isn't an edit of yours, so it isn't on the undo stack. [[Undo]]
- A journal is identified by its date.
	- It can't be renamed, since that would make it a different day. [[Renaming]]
	- If two devices each make the same day's journal before they sync, the two are merged into one. [[Duplicates]]
- In the references at the foot of a page, journals come first, newest first. [[Links]]
- Importing a directory turns files in `journals/` named `2026-10-25.md`, or [[Logseq]]'s `2026_10_25.md`, into journals. [[Export and import]]
- This documentation has journals too: an entry for each day's work on Quiddity.
