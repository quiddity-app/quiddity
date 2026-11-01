alias:: replica

- A graph is everything you keep in Quiddity: every [[Entity]] and [[Block]], and the history of how they came to be.
- Every device holds the whole graph, its own replica, and works on it offline. Replicas exchange their changes when they meet. [[Sync]]
- A graph is shared whole, or not at all. Which devices hold it is the [[Device list]].
- On disk a graph is a directory, made with `quiddity-node init`. The app keeps one in its data directory. [[Storage]] [[quiddity-node]]
- It is one [[Loro]] document, so an edit that touches many blocks, such as moving a tree, is one change that syncs and undoes as a unit. [[Undo]]
- Not to be confused with the [[Graph view]], which draws the entities and the links between them.
