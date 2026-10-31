type:: tool

- Mutation testing for Rust: it changes the code one small way at a time and checks that some test notices. https://mutants.rs
- CI runs it over the lines each pull request changes, and nothing may be missed. Each mutant runs against its own crate's tests only, so every [[Crate]] has to stand up on its own. [[Testing]]
- It copies the source tree, build output included, which has filled the disk more than once.
- Back to [[Tool]].
