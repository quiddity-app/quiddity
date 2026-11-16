type:: tool

- Static analysis and code quality as a service, on the open-source plan. https://sonarcloud.io/project/overview?id=quiddity-app_quiddity
- CI runs the scanner in the `sonarcloud` job of `ci.yml`, with the Rust coverage from [[cargo-llvm-cov]]. It reports on every push to `main` and every pull request from this repository.
	- It is "CI-based" analysis, not SonarCloud's automatic analysis, because only CI can supply coverage. Automatic analysis must stay off in the project's settings, or the two collide.
	- The exclusions are in `sonar-project.properties`. The coverage report's path is an argument to the scan in `ci.yml`, and absolute, because SonarCloud treats each crate as a module and reads a relative path from each one's own directory. Generated and test-only code is left out of coverage.
	- SonarCloud analyses the whole project, so the file sets no `sonar.sources` or `sonar.tests`: it ignores them and warns. What it shouldn't read, such as binary fonts and icons (which it warns about as bad encoding), is excluded instead.
	- It has no secret: it is "secretless" analysis, which the organisation enables in SonarCloud. The job's `id-token: write` permission lets it prove its identity to SonarCloud with GitHub's OIDC token, and `sonarqube-auth-action` swaps that for a short-lived token just before the scan. A pull request from a fork gets no OIDC token, so it skips the job, and the `gate` counts a skip as a pass.
- It does not gate merging: the job fails only if the scan itself breaks. Whether to require SonarCloud's quality gate is a decision still to make. [[Testing]]
