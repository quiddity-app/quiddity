type:: property

- What a block is: `type:: person`, `type:: journal`. Tags now say what something is: `#person`. [[tags]] [[Properties]]
- Its values are references, so bare text is a name: `type:: person` links to the entity called person. A block can have more than one type.
- Quiddity gives meaning to two: `journal`, for a [[Journal]], and `property`, for an entity that defines a property, like this one. Those two are what keeps `type` from being an ordinary property, and the plan is to identify them without it. [[Model redesign]]
- A link can ask for a type, to tell apart entities with the same name: `[[Jane#type(person)]]`. [[Links]]
- Table and board views by type are planned.
