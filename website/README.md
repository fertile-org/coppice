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

Copy follows `POSITIONING.md` (M12 Beta). Download links point at the pinned release assets in `DownloadLinks.astro` (bump `version` there each release). The only download targets are:

- macOS Apple Silicon — `Coppice-<version>-mac-arm64.dmg`
- Linux x64 — `Coppice-<version>-linux-amd64.deb`

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

Public site: [https://getcoppice.vercel.app/](https://getcoppice.vercel.app/), served from the domain root. No repository secrets are required for the build.

`BASE_PATH` (or the older `SITE_BASE`) sets Astro `base`. Leave it unset to build for `/`. `SITE_URL` sets the canonical origin when you want absolute OG and canonical URLs.

```bash
cd website
npm ci
SITE_URL=https://getcoppice.vercel.app npm run build
npm run check:links
```

Vercel should use `website/` as the project root. `website/vercel.json` turns on clean URLs so `/docs` serves `docs.html`.

GitHub Pages is an optional manual deploy, still at `/coppice`:

1. In the repo settings, set **Pages → Build and deployment → Source** to **GitHub Actions**.
2. Run the **Website** workflow manually (**Actions → Website → Run workflow**) with **Deploy to GitHub Pages at /coppice** checked.

That dispatch rebuilds with `BASE_PATH=/coppice` and `SITE_URL=https://fertile-org.github.io`, rewrites `docs.html` into `docs/index.html` (Pages does not apply Vercel clean URLs), then deploys `website/dist` with the built-in `GITHUB_TOKEN` (`pages: write` and `id-token: write`). Pull requests only build and upload the `website` artifact, with `base` `/`.
