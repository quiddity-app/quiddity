type:: tool

- jdx's tool for signed release manifests. A project signs a description of its release (each archive's platform, digest and size) as a Sigstore bundle named `packslip.sigstore.json`, attached to the release, and a client such as [[mise]] checks it against the repository's identity before installing. https://packslip.dev
- Quiddity publishes the headless node this way, with `jdx/packslip@v1` in the release workflow. [[Releases]]
