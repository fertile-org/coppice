## Install

Coppice bundles its own database and server, so you don't need Docker or Postgres. Agents run on your machine, so first install **Git** and the agent CLIs you plan to use (for example `claude` or `codex`) on the host and log in to each of them.

### macOS (Apple silicon: `mac-arm64`, Intel: `mac-x64`)

1. Open `Coppice-<version>-mac-<arch>.dmg` and drag **Coppice** to **Applications**.
2. Launch Coppice from Applications. The first launch takes a few seconds while it creates its database.

{{MAC_UNSIGNED}}
> **This build is not signed by Apple.** macOS will say Coppice "can't be opened" or "is damaged". Either right-click Coppice in Applications, choose **Open** and confirm, or run once:
>
> ```sh
> xattr -dr com.apple.quarantine /Applications/Coppice.app
> ```
{{/MAC_UNSIGNED}}

### Linux (Ubuntu 22.04+ / Debian 12+, `amd64` or `arm64`)

```sh
sudo apt install ./Coppice-<version>-linux-<arch>.deb
```

Then start **Coppice** from your applications menu, or run `coppice`.

### Your data

Boards, settings and logs live in `~/Library/Application Support/Coppice` on macOS and `~/.config/Coppice` on Linux. Uninstalling the app leaves them in place.

### Verify the download

Download `SHA256SUMS` next to the installer and run:

```sh
sha256sum --check --ignore-missing SHA256SUMS      # Linux
shasum -a 256 --check --ignore-missing SHA256SUMS  # macOS
```
