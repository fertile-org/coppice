# Blog

English articles live in `src/content/blog/en/`. The index at `/blog` and each article page are built from those files. Adding a note does not require an edit to the index, the sitemap, or the layout.

This directory ships with no articles. The example below is the schema, not a post.

## Routes

| URL | Source |
| --- | --- |
| `https://getcoppice.app/blog` | `src/pages/blog/index.astro` |
| `https://getcoppice.app/blog/<slug>` | `src/content/blog/en/<slug>.md` |

`<slug>` is the filename without `.md`. Use lowercase words and hyphens: `worktrees.md` is `/blog/worktrees`. `index` is reserved. A directory named `<slug>` with `index.md` inside is the same article, for notes that keep files beside the markdown.

The page title is the frontmatter `title`, unless `metaTitle` is set. `Base.astro` appends ` · Coppice` when the text does not already include Coppice. `npm run check:seo` keeps that full title within 60 display columns and the description within 100–160. CJK and fullwidth characters count as two.

The article template renders one `h1` from `title`, then the date line, then `answer`, then the markdown body. Do not put a top-level heading in the body. Start at `##`.

`answer` is the opening paragraph. It has to be 40–500 characters. Write the direct answer there. The body can expand on it.

## Frontmatter

```yaml
---
title: Short heading
metaTitle: Optional search title
description: One or two sentences, 100 to 160 display columns, used as the meta description.
answer: The direct answer, 40 to 500 characters, shown above the body.
published: "2026-03-02"
updated: "2026-03-04"
author: The byline you want on the page
authorType: Person
image: /blog/short-heading/board.webp
draft: false
keywords:
  - worktree
---
```

| Field | Required | Notes |
| --- | --- | --- |
| `title` | yes | Heading and breadcrumb. |
| `metaTitle` | no | Replaces `title` in `<title>` only. |
| `description` | yes | Meta description. 100–160 display columns. |
| `answer` | yes | Opening paragraph. 40–500 characters. |
| `published` | yes | Quote it as `YYYY-MM-DD`. The build does not pick a date. |
| `updated` | no | Same format. Must be the same day or later. Shown only when it differs from `published`. |
| `author` | yes | The visible byline. There is no default name. 80 characters or fewer. |
| `authorType` | no | `Person` (the default in the JSON-LD) or `Organization`. |
| `image` | no | A site path starting with `/`, or an `https` URL. Used in `BlogPosting` JSON-LD. The social card stays the site card. |
| `draft` | no | `true` keeps the file out of the index, the route, `hreflang`, and the sitemap. A draft English article has no public URL, and a draft translation is not listed as an alternate. |
| `keywords` | no | Up to 8 strings. Omitted from the page when absent. |

Unknown keys fail the build. A translation file with no English article at the same slug also fails the build.

JSON-LD on an article is `BlogPosting`: `datePublished` is `published`, and `dateModified` is `updated` or, when you omit `updated`, the same published date. The author is the `author` string. The canonical URL is `https://getcoppice.app/blog/<slug>`. The index is a `Blog` and lists published English notes. With no notes, it does not emit an empty `blogPost` list.

## Images

Put screenshots in `public/blog/<slug>/` and refer to them from the root:

```markdown
![Demo board with two tickets in Backlog and In Progress.](/blog/short-heading/board.webp "Demo board: two tickets at once")
```

`alt` is required. The quoted title is the caption, rendered in a `figcaption`. The image is lazy-loaded. Capture notes and the limits of the current demo script are in [docs/development.md](../docs/development.md#marketing-screenshots).

## Translation

Add `src/content/blog/<locale>/<slug>.md` only when that translation exists. `<locale>` is `vi`, `zh-cn`, `zh-tw`, `zh-hk`, `ja`, or `fr`. Use the same slug as the English file. `hreflang` and the sitemap then include that locale. A locale with no file is not listed. The index at `/blog` stays English until a separate index translation exists; `/ja/blog` currently shows the English index and points its canonical URL at `https://getcoppice.app/blog`.

Labels on the article page (`Published`, `Updated`, `Blog`) are English in this foundation.

## Checks

From `website/`:

```bash
npm ci
npm run build
npm run check:seo
npm run check:i18n
node --experimental-strip-types --test src/lib/blog.test.ts
```

`check:i18n` copies `scripts/fixtures/blog/` into the content tree for a second build, then deletes those copies. Do not commit them under `src/content/blog/`.
