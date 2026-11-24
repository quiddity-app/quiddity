alias:: link toast, link tooltip, link preview

- Pointing at a [[Links]] says where it goes, as a browser's status bar does.
	- A toast in the bottom-left corner of the window shows the destination while the pointer or keyboard focus is on a link, and goes when it leaves.
	- An entity's link shows its name. A link with words of its own, `[the library]([[Loro]])`, shows the entity's name and not its words. A link to nowhere, and an ambiguous one, show the name as written.
	- A link to the web or mail shows its URL.
	- A link to a page of the app, `[the graph](/graph/)`, shows the page's name as the sidebar has it: "Graph".
	- Links to a block have no toast yet, and the rest of the inert text isn't a link.
- An entity that has a properties block also gets a tooltip, after the pointer has rested on the link for about a third of a second: the entity's title, and the properties block, read-only. [[Properties]]
	- It is for looking at, not using: it doesn't take the pointer and its links aren't focusable. Escape dismisses it.
	- While it is showing, the link's own native tooltip, which holds the qualifiers, is left off so the two don't overlap.
	- An entity with no properties block gets the toast only.
- How it works
	- `lib/link-hint.ts` is a one-value store of what the toast says, and `useLinkHint` gives a link the handlers that set and clear it. `components/link-status.tsx` draws it from the root layout, so one toast serves every pane. [[The UI]]
	- `EntityLink` reads the entity's title when entered, so the toast is right whichever component drew the link, and draws the tooltip as a sibling of the link, not inside it.
	- Tested in `e2e/link-hint.spec.ts`.
