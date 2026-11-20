type:: tool

- A GitHub Action that saves a directory between CI runs and restores it by key. https://github.com/actions/cache
- CI uses it for [[Playwright]]'s browser, and for apt's downloads: the `apt-cache` action (`.github/actions/apt-cache`) restores `/var/cache/apt/archives` before a job installs packages. [[Testing]]
	- It exists because the Azure mirror the runners install from can take minutes. The UI job's system libraries are the packages [[Playwright]] asks for, and the shell's job installs Tauri's (`libwebkit2gtk-4.1-dev` and friends). `apt-get update` still asks the mirror, but the large part, the downloads, is local.
	- **A job runs `sudo scripts/apt-prune.sh` after its last install**, which the cache's post step then saves. It deletes every `.deb` that isn't installed at its own version, so the cache holds what the job needs and no more: not last week's versions, and not downloads that never installed. A job that forgets saves the lot.
	- The key is the runner's OS, architecture and Ubuntu release, the job's name, an input that changes with the packages (Playwright's version, or the workflow file), and the ISO week. An exact hit is never saved over, so the week makes the cache renew itself as the mirror moves on. A miss restores the nearest earlier entry, and apt downloads only what is missing from it.
	- The directory is root's, and the cache is restored and saved as the runner's user, so the action hands the directory over first and gives `partial`, which apt downloads into as `_apt`, back afterwards. `partial` is never saved.
	- Pull requests save their own entries as well as `main`, which a split restore and save could avoid, at the cost of every job saving explicitly.
