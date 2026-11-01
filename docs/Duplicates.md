alias:: merging

- Some entities are identified by their properties alone: a [[Journal]] by its date, and a property entity by its name. [[Properties]] When two devices make the same one at once, the graph holds two, and they are merged.
- The copy with the lowest ID is kept, and the content of the others moves into it. Every device makes the same moves, so merges made on several devices at once agree.
- The others are not deleted. A device that hasn't synced yet may still be adding to its copy, and anything added under a deleted block would be lost. Instead the index hides them, and the merge runs after every sync, moving across anything new.
- `quiddity-node serve` merges shortly after every change and tells the devices connected to it, so they needn't merge for themselves. [[quiddity-node]]
- What it means elsewhere
	- A link from a merged copy counts as a link from the one it was merged into, in [[All entities]] and the [[Graph view]].
	- Exports leave out copies that have been fully merged. Importing the same export twice makes two of everything except journals and property entities, which merge. [[Export and import]]
	- Deleting a journal or property entity deletes the copies merged into it, or they would take its place.
- Other entities may share a name: that is allowed, since an [[Entity]] is identified by its name and properties together. Offering to merge them in the UI is planned, not built.
- The code is `merge_duplicates` in [[quiddity-core]].
