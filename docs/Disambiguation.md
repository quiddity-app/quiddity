alias:: disambiguation page, ambiguous link

- Names are not unique, so a [[Links]] such as `[[Mercury]]` may match several entities. Following one opens a page that lists them, so you can see which is which and choose.
	- The page is titled "Which Mercury?", and is at `/disambiguate/?link=Mercury`, the link as written between its brackets. A link's qualifiers are part of it, so `[[Mercury #type(planet)]]` lists only the planets.
	- Each entity it matches is listed with its title, which opens it, and its properties block, which is what tells it apart from the others. An entity with none says so. [[Properties]]
	- A link reached by clicking it, or by Enter in the editor on a link whose list was not offered, opens the page. Nothing is made or changed.
	- If the matches change while it is open, for example because one is renamed, the page shows the new list, and says when nothing matches any more.
- Planned, not built
	- Narrowing the link in place, by writing the qualifiers that pick the chosen entity into the block it came from.
	- Offering to merge the entities. [[Duplicates]]
- How it works
	- `View::link_matches(key)` in `quiddity-view` resolves a link as `follow_link` does and returns every entity it matches, as the index orders them, rather than failing when there are several. It makes nothing. [[quiddity-view]]
	- The page is `components/disambiguation.tsx`. It reads each match with `graph.entity` and draws the block flagged `propertiesBlock`, read-only. The link is part of the view's URL (`View.link`), so opening something at the side keeps it. [[The UI]]
	- Tested in `e2e/disambiguation.spec.ts`.
