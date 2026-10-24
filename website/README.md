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

English keeps those URLs. `vi`, `zh-CN`, `zh-TW`, `zh-HK`, `ja`, and `fr` use a lowercase prefix (`/ja/docs/concepts`). Untranslated locale URLs render the English page. See [I18N.md](I18N.md).

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

Public site: [https://getcoppice.app/](https://getcoppice.app/), served from the domain root. No repository secrets are required for the build.

Canonical, `hreflang`, and sitemap URLs use the `site` value in `astro.config.mjs`. The site is built at the domain root.

```bash
cd website
npm ci
npm run build
npm run check:links
npm run check:seo
```

`check:seo` reads `dist`: one title, description, and canonical per page, Open Graph and Twitter tags that match them, `robots.txt`, `sitemap.xml` (only indexable pages, `lastmod` from git, left out on shallow clones), the manifest, and JSON-LD. Every page carries one JSON-LD `@graph` (`src/lib/structured-data.ts`): Organization and WebSite everywhere, SoftwareApplication on the homepage and download page, TechArticle and BreadcrumbList on docs. The agent CLI names in it come from `src/content/agents.ts`. Keep that list in step with the Providers doc. The social card is `public/assets/og-card.png` (1200×630). Rebuild it with `npm run og:image` after changing the logo, the hero screenshot, or the card copy in `scripts/make-og-image.mjs`.

The page serves compressed copies, not the sources: the hero poster is `hero-screenshot-1440.webp`, the reduced-motion still picks AVIF or WebP at 720 or 1440 wide, and the header mark is `logo-72.webp` / `logo-108.webp`. Rebuild them with `npm run web:images` after changing `public/assets/hero-screenshot.png` or `public/logo.svg`. `check:seo` also fails any `<img>` over 200 KB that loads eagerly; the GIF fallback inside the hero `<video>` is lazy for that reason, since browsers fetch fallback images even when the video plays.

Vercel should use `website/` as the project root. Web Analytics is on for the project: `src/components/VercelAnalytics.astro` loads Vercel's cookieless script (no consent banner needed) only when the build runs on Vercel (`VERCEL=1`), so local and CI builds ship no tracking. Vercel preview deployments are tracked too; the dashboard filters by environment. It calls `inject()` rather than the package's Astro component, which would report `/docs/install.html` instead of `/docs/install`. `website/vercel.json` turns on clean URLs so `/docs` serves `docs.html`.

Docs search is a button in the docs header. It opens a dialog that searches the Pagefind index. The `astro-pagefind` integration builds that index from the static output at the end of `npm run build` (the same command Vercel runs). Only the docs article is indexed: `data-pagefind-body` is on that article, and pages without it — the homepage, download page, 404, and any locale URL that is still showing English — stay out of the index. Each locale has its own Pagefind index. The site header, nav, and footer are marked `data-pagefind-ignore`.

The marketing site is not published to GitHub Pages. Pages is the apt repository and the macOS update feed (`.github/workflows/apt-repo.yml`). Deploying this site there would replace those files. The website workflow builds and checks the site, then uploads the `website` artifact. It does not deploy.
