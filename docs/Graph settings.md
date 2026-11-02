alias:: settings, quiddity.toml

- Settings that belong to the whole [[Graph]] rather than to any [[Entity]]. Every device sees the same ones: they sync with the graph.
- There is one so far, `home`: where the app opens.
	- `"journals"`, the default: today's [[Journal]], then the days before it.
	- `"all"`: [[All entities]].
	- `"graph"`: the [[Graph view]].
	- A link, such as `"[[Quiddity]]"`: that entity. It is written as a [[Links]] link, qualifiers and all, and found the same way. If it finds no entity, or more than one, the app opens on the journals instead.
- They are set from a file, `quiddity.toml`, at the top of a directory being imported. The app has no way to change them yet. [[Export and import]]
	- ```toml
	  # Where the app opens: "journals", "all", "graph", or a link to an entity.
	  home = "[[Quiddity]]"
	  ```
	- A setting the file leaves out keeps whatever the graph had. A key it doesn't know, or a value it can't read, stops the import before anything is added, so a misspelling doesn't quietly do nothing.
	- An export writes the file when anything is set.
	- This site's own `docs/quiddity.toml` opens it on [[Quiddity]]. [[Publishing]]
- How it works
	- [[quiddity-core]] keeps them in a map of their own in the graph's [[Loro]] document, beside the blocks rather than in one, one key per setting so that devices changing different settings don't conflict. A value a device can't read (from a newer build, say) counts as unset.
	- [[quiddity-view]]'s `start()` turns `home` into somewhere to go, finding the entity for a link.
	- In [[The UI]], `/` goes wherever that says, replacing itself in the history; the journals have their own address, `/journals/`.
	- The file is read and written with [[toml]].
