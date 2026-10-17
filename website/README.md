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

Copy follows `POSITIONING.md` (M12 Beta). Download buttons link to `/download`. Asset URLs live in `src/content/downloads.ts`. Homepage and download wording live in `src/i18n/pages/en.ts`.

English keeps those URLs. `zh-CN`, `zh-TW`, `zh-HK`, `ja`, and `fr` use a lowercase prefix (`/ja/docs/concepts`). Untranslated locale URLs render the English page. See [I18N.md](I18N.md).

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

Canonical, `hreflang`, and sitemap URLs use the `site` value in `astro.config.mjs`. The site is built at the domain root.

```bash
cd website
npm ci
npm run build
npm run check:links
```

Vercel should use `website/` as the project root. `website/vercel.json` turns on clean URLs so `/docs` serves `docs.html`.

Docs search is a button in the docs header. It opens a dialog that searches the Pagefind index. The `astro-pagefind` integration builds that index from the static output at the end of `npm run build` (the same command Vercel runs). Only the docs article is indexed: `data-pagefind-body` is on that article, and pages without it — the homepage, download page, 404, and any locale URL that is still showing English — stay out of the index. Each locale has its own Pagefind index. The site header, nav, and footer are marked `data-pagefind-ignore`.

The marketing site is not published to GitHub Pages. Pages is the apt repository and the macOS update feed (`.github/workflows/apt-repo.yml`). Deploying this site there would replace those files. The website workflow builds and checks the site, then uploads the `website` artifact. It does not deploy.
