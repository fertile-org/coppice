# Coppice site

Astro site for the Beta marketing page and user docs. Contributor docs stay in the repo [`docs/`](../docs/) tree.

## Routes

| Route | Source |
| --- | --- |
| `/` | `src/pages/index.astro` |
| `/download` | `src/pages/download.astro` |
| `/docs` | `src/pages/docs/index.md` |
| `/docs/install` | `src/pages/docs/install.md` |
| `/docs/concepts` | `src/pages/docs/concepts.md` |
| `/docs/providers` | `src/pages/docs/providers.md` |
| `/docs/faq` | `src/pages/docs/faq.md` |

Copy follows `POSITIONING.md` (M12 Beta). Download buttons link to `/download`. The labels and asset URLs for that page live in `src/content/downloads.ts`.

- macOS (Apple Silicon) — `Coppice-mac-arm64.dmg`
- macOS (Intel) — `Coppice-mac-x64.dmg`
- Linux (x64) — `Coppice-linux-amd64.deb`
- Linux (ARM64) — `Coppice-linux-arm64.deb`

Windows 11 links to the WSL section of the install doc. There is no Windows build.

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

Docs search is a box in the docs header. The `astro-pagefind` integration builds the Pagefind index from the static output at the end of `npm run build` (the same command Vercel runs). Only the docs article is indexed: `data-pagefind-body` is on that article, and pages without it — the homepage, download page, and 404 — stay out of the index. The site header, nav, and footer are marked `data-pagefind-ignore`.

GitHub Pages is an optional manual deploy, still at `/coppice`:

1. In the repo settings, set **Pages → Build and deployment → Source** to **GitHub Actions**.
2. Run the **Website** workflow manually (**Actions → Website → Run workflow**) with **Deploy to GitHub Pages at /coppice** checked.

That dispatch rebuilds with `BASE_PATH=/coppice` and `SITE_URL=https://fertile-org.github.io`, rewrites `docs.html` into `docs/index.html` (Pages does not apply Vercel clean URLs), then deploys `website/dist` with the built-in `GITHUB_TOKEN` (`pages: write` and `id-token: write`). Pull requests only build and upload the `website` artifact, with `base` `/`.
