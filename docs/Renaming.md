alias:: rename

- Renaming an entity keeps the old name as an `alias`, so links written against it still work, including ones written by a device that was offline. [[alias]]
	- The alias is a line in the entity's properties block, which is made if there is none.
	- No alias is added if the old name already is one (aliases may be a list, matched as names are) or if only the case or spacing changed.
- Where: the main pane's title is a field when the entity can be renamed. Enter or leaving the field keeps the name and Escape drops it. Side cards and the journals show plain titles. [[The UI]]
- Journals and property entities cannot be renamed.
	- They are identified by their date or name, so a rename would make them a different entity. The index says which, and the view checks. [[Properties]]
- The name is one line, the first line of what is given, and cannot be empty.
- A name nothing has relied on is corrected without an alias. `correct_entity_name` is the rename for when the caller knows that, such as an entity made from a link a moment ago. [[Links to nowhere]]
- Undoing a rename takes back the name and the alias together. [[Undo]]
