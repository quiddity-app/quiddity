alias:: journals

- A journal is the page for a day: an [[Entity]] named for its date, with `##journal(2026-10-25)` in its properties block. That line is what makes it one, and the day in it is what identifies it. [[Meta-properties]]
	- It is flagged if the day can't be read, if there is none, or if there is more than one line. Only the first line is read, so what is flagged is what is ignored.
	- `type:: journal` and `date::` meant this before, and mean nothing now: they are ordinary properties. Nothing rewrites an old graph.
	- Importing a file from `journals/` adds the line unless it has one. An export from before it keeps its old lines, with the new one added.
- The journals are home.
	- The app opens on today's journal, with earlier days below it as you scroll, unless the graph's `home` setting names somewhere else. It is the first entrypoint on the left. [[The UI]] [[Graph settings]]
	- Today's journal is made when the app starts, if there isn't one. That isn't an edit of yours, so it isn't on the undo stack. [[Undo]]
- A journal is identified by its date.
	- It can't be renamed, since that would make it a different day. [[Renaming]]
	- If two devices each make the same day's journal before they sync, the two are merged into one. [[Duplicates]]
- In the references at the foot of a page, journals come first, newest first. [[Links]]
- Importing a directory turns files in `journals/` named `2026-10-25.md`, or [[Logseq]]'s `2026_10_25.md`, into journals, by adding the `##journal` line. [[Export and import]]
- This documentation has journals too: an entry for each day's work on Quiddity, which people write in commit messages and a job saves here. [[Journal entries]]
