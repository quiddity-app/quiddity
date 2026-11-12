type:: prior art

- GitHub's secret scanning looks for credentials in repositories and tells their owner. It finds them by a pattern, and its own tokens are written to make that cheap and precise: a prefix that says what it is (`ghp_`), a random body, and a checksum at the end, so that a match can be checked without asking GitHub whether the token works. https://docs.github.com/en/code-security/secret-scanning
- [[Identity]] writes Quiddity's device key in that shape, so that one committed by mistake is found.
