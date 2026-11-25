type:: tool

- Mutation testing for Rust: it changes the code one small way at a time and checks that some test notices. https://mutants.rs
- How CI runs it, and what to do with a mutant that survives, is in [[Testing]]. Each mutant runs against its own crate's tests only, so every [[Crate]] has to stand up on its own.
- It copies the source tree, build output included, which has filled the disk more than once.
