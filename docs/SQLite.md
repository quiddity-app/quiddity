type:: library

- An embedded SQL database that keeps a whole database in one file, and is meant as an application file format. https://sqlite.org/appfileformat.html
- [[quiddity-node]] keeps a graph in one SQLite file, through the `rusqlite` crate with SQLite compiled in, so the version and the options are ours. [[Storage]]
- Its security advice is for files that come from somewhere else, and a graph may: https://sqlite.org/security.html. Quiddity reads its file as untrusted. [[Storage]] says how.
