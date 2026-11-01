alias:: meta-property

- A meta-property says something about a property itself: which values it takes, what it may be applied to, how many values it may have, and how it relates entities to one another. They are how a graph builds its own taxonomies (parents and children, kinds and subkinds) out of [[Properties]] it defines, with nothing in code. **Read, but not acted on yet:** the lines are parsed, and nothing uses them.
- Writing them
	- `##key` or `##key(parameter)`, on a line of its own in the properties block of a property's entity, the one with `type:: property` that defines the property. [[Entity]]
		- ```
		  parent                       ← the property's entity
		  └─ type:: property           ← its properties block
		     ##values(#type(place))
		     ##applies-to(#type(place))
		     ##cardinality(0..1)
		     ##transitive
		     ##inverse(children)
		  ```
	- The set is fixed: Quiddity gives each one its meaning, and they can't be defined in the graph. `##` is a namespace of its own, so `##inverse` has nothing to do with a property called `inverse`.
	- Anywhere else, a `##` line has no effect, and is flagged.
	- They are meant to be written once, when a property is set up, and seldom touched after. Changing one changes what every use of the property means, and nothing is rewritten.
- The meta-properties
	- | Written | Says |
	  |---|---|
	  | `##value-type(date)` | how values are read: `text`, `number`, `boolean`, `date`, `datetime` or `ref` |
	  | `##values(query)` | which entities a value may refer to |
	  | `##applies-to(query)` | which entities may have the property |
	  | `##cardinality(0..1)` | how many values an entity may have |
	  | `##transitive` | an entity has the property's values' values too |
	  | `##inherits` | an entity has the other properties of the entities it refers to |
	  | `##inverse(label)` | the name of the relation the other way round |
	  | `##symmetric` | the relation holds both ways |
	- `##values` and `##applies-to` take a query. [[Queries]]
	- `##cardinality` is written as UML writes multiplicity: `1`, `0..1`, `1..*`, `2..5`, or `*` for any number. It counts values, not lines: `#steps([a, b, c])` is three, and so are three `#step` lines. A list keeps its order; that is all that makes it different.
	- `##values`, `##transitive`, `##inherits`, `##inverse` and `##symmetric` only make sense for references, so a property with any of them is a `ref` property unless `##value-type` says otherwise. As now, a bare name in a reference is a name: with `parent` a reference, `parent:: Scotland` means `parent:: [[Scotland]]`. [[Links]]
- Defaults
	- A property with no meta-properties is text, with any number of values, on anything.
	- Quiddity's own properties ([[type]], [[alias]], [[date]], [[favourite]], [[id]] and [[title]]) have built-in meta-properties, so a new graph needs nothing written into it. A property entity overrides each one it writes, so they are editable like any other.
	- Quiddity defines no taxonomy of its own. There is no built-in `supertype` or `parent`: a graph makes the ones it wants.
- Transitive properties
	- With `##transitive` on [[type]], `Oak` with `type:: tree`, and `tree` with `type:: plant`, Oak's types are tree and plant, and the query `#type(plant)` finds both Oak and tree.
	- Values are followed to the entities they refer to, as far as they go. A cycle, an entity that is its own ancestor, is followed once round and flagged.
- Inheriting
	- With `##inherits` on a property, an entity has the properties of each entity the property refers to, as well as its own. The entities it inherits from may themselves inherit, and so on.
	- A taxonomy, for example: `supertype`, with `##transitive`, `##inherits` and `##inverse(subtypes)`. If `tree` has `supertype:: plant` and `plant` has `#alive`, then tree is `#alive` too, a query for `#alive` finds it, and plant's page lists tree under subtypes.
	- Its own values come first: a property an entity writes itself is not inherited, so a `cardinality` of one isn't broken by inheritance.
	- Not inherited: the inheriting property itself (that is what `##transitive` is for), and what identifies an entity: [[title]], [[alias]], [[id]] and [[date]]. What an entity is identified by is read from its own properties only, so inheriting `type:: property` doesn't make a property entity. [[Duplicates]]
	- Inherited values are shown on the entity's page apart from its own, saying where each comes from.
- Inverses
	- `##inverse(children)` on `parent` gives each entity a list headed children: the entities whose `parent` is it. It is shown above the references at the foot of its page, as a tree of children's children, expanded by default up to 100 entities, and collapsed past that.
	- Queries can use the label: `#children(Scotland)` finds entities with Scotland as a parent. For a transitive property, that includes grandparents, as `#parent` includes grandchildren.
	- The label can't be written as a property (a `children:: Edinburgh` line is an ordinary `children` property, unrelated to `parent`), and a property entity named for a label is flagged.
	- `##symmetric` is for relations like `sibling` or `spouse`: if Ann's sibling is Bob, Bob's sibling is Ann, whichever of them it is written on. A symmetric property is its own inverse.
- Flags
	- Like the rest of a schema, meta-properties never reject anything: the text is what it is, and what doesn't fit is flagged where it is written.
	- A value that isn't of the `##value-type`.
	- A reference whose target doesn't match `##values`. A link to nowhere is not flagged: it has no target to judge.
	- An entity that has a property but doesn't match its `##applies-to`. Only the properties it writes itself are checked.
	- More values than `##cardinality` allows, or fewer.
	- A cycle in a transitive property.
	- A meta-property Quiddity doesn't know, a parameter it can't read, or one outside a property entity's properties block.
- How it works
	- [[quiddity-syntax]] reads a `##key(…)` line as a meta-property, keeping the parameter as text. It finds the closing parenthesis by reading the parameter as the query parser does, so a query needs no escaping, and a line that is a meta-property isn't scanned for `#tags`. Links in a parameter that reads as a query are links like any other, and show in references. A properties block may hold meta-property lines as well as property lines.
		- A parameter ending in a backslash, `##k(C:\ )`, needs the space: `\)` would be a literal parenthesis.
	- Still to build: [[quiddity-core]] reads each property's meta-properties into its schema, and, after building the index, works out each entity's effective properties (its own, inherited, transitive and symmetric) by repeating until nothing changes, which copes with cycles. [[Queries]] are answered against the effective properties; the checks look at what is written.
	- They replace the `value-type::`, `form::` and `cardinality::` lines that a property entity is read for today. `form` goes: a list is always allowed.
