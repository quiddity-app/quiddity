alias:: contact, ##contact

- An entity that has a value for a property mapped to a contact field is a contact, and is meant to appear in the phone's own contacts. A person, a company and a plumber all qualify. **Built, but never run on a phone**: the graph can say which entities are contacts and take an edit to one back, and there is an Android library to put them in the phone's, the shell keeps them in step, and the Devices page turns it on. These pages say what is built and what is planned. [[Android contacts]]
- Mapping properties to fields
	- A property entity's `##contact(field)` line says which field of a contact its values fill. `##contact(field:label)` adds a label, which says what kind of one it is. [[Built-in properties]]
		- ```
		  mobile                       ← the property's entity
		  └─ ##contact(phone:mobile)
		  work email
		  └─ ##contact(email:work)
		  ```
	- The fields are `phone`, `email`, `address`, `organisation`, `job-title`, `nickname`, `website`, `birthday` and `note`. A field is not case-sensitive, and `organization` is accepted. The entity's name is the contact's name, so there is no field for it.
	- A label is anything: `home`, `work` and `mobile` are what phones offer, and any other is shown as written. Labels are compared as names are, ignoring case and spacing. `other` is no label, because it is what a phone calls a number with none: `##contact(phone:other)` is `##contact(phone)`.
	- Several properties may fill one field, and one property fills one field. Each value of a property is one entry, whether written on lines of its own or as a list. [[Properties]]
	- A `##contact` that can't be read is flagged where it is written, like any meta-property, and nothing is mapped. [[Built-in properties]]
- Which entities are contacts
	- An entity that has a value for a mapped property, written by itself in its properties block, is a contact. Only what it writes counts: what it inherits doesn't make a contact. A property with no value, or an empty one, counts for nothing. [[Properties]]
	- The contact's name is the entity's name. A contact has no type: a company, a shop and a plumber are as much contacts as a person is, if they have a number to call.
	- Each value is one entry, in the order written. A link is shown as the name it links to, a date link as its day, and a block reference not at all.
- Edits coming back
	- A contacts app edits its own copy, so an edit arrives as three things: the contact as it was when it was handed over, the contact as it is now, and which entity it was. The change is made to the entity as it is *now*, which may have moved on:
		- an entry the copy gained is added, unless the entity has gained it too;
		- an entry the copy lost is removed, and what the copy never saw is left alone;
		- the name is changed only if the entity still has the name the copy started with, and the old name is kept as an [[alias]]. [[Renaming]]
	- A new entry goes in the property for its field and label. Failing that, the one for its field with no label, then any for the field. With none, it is dropped and the caller is told. A property whose name can't be written as a line's key is never used.
	- A value is written as text, one `key:: value` line for each, quoted where it needs to be: an address with line breaks is one value.
	- A removed value comes out of the line it is on, and a list that loses some of its items keeps the rest. A line left with nothing goes.
	- A contact with no entity, or whose entity has gone but was edited, makes one: named as it is (or for its first value, if it has no name) with its entries in the properties that fill them. A contact with nothing to put in a property makes nothing.
	- A contact deleted elsewhere removes its values and not the entity, so that deleting a contact never deletes a page. It is then no contact.
- Keeping a copy in step
	- A phone's contacts are somebody else's copy of these, and are kept as the graph's by reconciling, which is what runs when either side changes. The copy is a `Store`: it has a row for each contact, says which rows were edited, made or deleted since they were last put, and remembers each as it was handed over.
	- First the edits go into the graph, each as above, from the copy as it was handed over. Then the copy is made to have exactly the graph's contacts: a contact it lacks is put, one it has differently is replaced, and one the graph no longer has is removed. A row that was edited is always put again, as the graph now has it, so that the phone shows what the graph made of the edit.
	- A row made on the phone makes an entity, and from then on is that entity's. If there is nothing to make an entity of, because no property fills what was entered, the row is removed and what was lost is reported.
	- It converges: whatever was done on either side, a second reconcile finds nothing to do. That is the property the tests check, with edits and deletions on both sides in any order.
- How it works
	- [[quiddity-contacts]] reads contacts from the index and applies an edit, as plain data that serialises.
	- [[quiddity-core]] reads the line into the property's schema (`PropertySchema::contact`), and `Index::contact_keys` lists the properties that have one, by the name their entity has.
