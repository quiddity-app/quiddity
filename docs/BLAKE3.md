type:: library

- A fast cryptographic hash, with a mode that derives keys from other keys under a context string. https://github.com/BLAKE3-team/BLAKE3
- [[quiddity-identity]] uses only that mode (`derive_key`), to turn the device key and a salt into the endpoint key and the node mask. [[Identity]]
