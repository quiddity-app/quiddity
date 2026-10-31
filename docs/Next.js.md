type:: library

- A React framework. https://nextjs.org
- [[The UI]] is a Next.js static export: no server, just files that the shell's webview, a browser or GitHub Pages can load. [[Static build]]
- `next dev` renders on a server and the export doesn't, so a pane that reads the graph outside `ClientOnly` works in one and fails in the other. Some browser tests run against `next dev` for that reason. [[Testing]]
- Back to [[Library]].
