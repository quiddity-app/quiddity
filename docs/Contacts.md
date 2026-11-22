alias:: contact, ##contact

- An entity that has a value for a property mapped to a contact field is a contact, and is meant to appear in the phone's own contacts. A person, a company and a plumber all qualify. **Not built yet**: only the mapping below is read. Nothing shows contacts anywhere, so these pages say what is planned, and are corrected as each part lands.
- Mapping properties to fields
	- A property entity's `##contact(field)` line says which field of a contact its values fill. `##contact(field:label)` adds a label, which says what kind of one it is. [[Built-in properties]]
		- ```
		  mobile                       ← the property's entity
		  └─ ##contact(phone:mobile)
		  work email
		  └─ ##contact(email:work)
		  ```
	- The fields are `phone`, `email`, `address`, `organisation`, `job-title`, `nickname`, `website`, `birthday` and `note`. A field is not case-sensitive, and `organization` is accepted. The entity's name is the contact's name, so there is no field for it.
	- A label is anything: `home`, `work`, `mobile` and `other` are what phones offer, and any other is shown as written. Labels are compared as names are, ignoring case and spacing.
	- Several properties may fill one field, and one property fills one field. Each value of a property is one entry, whether written on lines of its own or as a list. [[Properties]]
	- A `##contact` that can't be read is flagged where it is written, like any meta-property, and nothing is mapped. [[Built-in properties]]
- How it works
	- [[quiddity-core]] reads the line into the property's schema (`PropertySchema::contact`), and `Index::contact_keys` lists the properties that have one, by the name their entity has.
