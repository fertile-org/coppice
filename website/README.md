# Coppice site

Astro site for the Beta marketing page and user docs. Contributor docs stay in the repo [`docs/`](../docs/) tree.

## Routes

| Route | Source |
| --- | --- |
| `/` | `src/pages/index.astro` |
| `/docs` | `src/pages/docs/index.md` |
| `/docs/install` | `src/pages/docs/install.md` |
| `/docs/concepts` | `src/pages/docs/concepts.md` |
| `/docs/providers` | `src/pages/docs/providers.md` |
| `/docs/faq` | `src/pages/docs/faq.md` |

Copy follows `POSITIONING.md` (M12 Beta). Download links are `#` until a beta GitHub Release exists. The only download targets are:

- macOS Apple Silicon — `Coppice-<version>-mac-arm64.dmg`
- Linux x64 — `Coppice-<version>-linux-x64.deb`

The release workflow also builds macOS Intel and Linux arm64. Those are not site CTAs, and there is no Windows build.

## Local preview

From the repo root:

```bash
cd website
npm ci
npm run dev       # http://127.0.0.1:4321
npm run build     # writes website/dist
npm run preview   # serves website/dist
```

Or `make website-dev` / `make website-build`.

## Deploy

No repository secrets are required.

Planned URL: `https://fertile-org.github.io/coppice/`

1. In the repo settings, set **Pages → Build and deployment → Source** to **GitHub Actions**.
2. Run the **Website** workflow manually (**Actions → Website → Run workflow**) with **Deploy to GitHub Pages at /coppice** checked.

That dispatch rebuilds with `SITE_BASE=/coppice` and `SITE_URL=https://fertile-org.github.io`, then deploys `website/dist` with the built-in `GITHUB_TOKEN` (`pages: write` and `id-token: write`). Pull requests only build and upload the `website` artifact, with `base` `/`, so opening the artifact does not require the `/coppice` prefix.

A custom domain can use the same artifact shape as local preview:

```bash
cd website
npm ci
SITE_URL=https://example.com npm run build
```

Leave `SITE_BASE` unset (it defaults to `/`). Point the host at `website/dist`.
