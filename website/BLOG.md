# Blog

Before you plan or write a post, read [.agents/skills/blog-writing/SKILL.md](../.agents/skills/blog-writing/SKILL.md). It covers the topic, the structure, and how much Coppice detail to include. This file covers the mechanics.

English articles live in `src/content/blog/en/`. The index at `/blog` and each article page are built from those files. A locale index lists only the notes published in that locale. Adding a note does not require an edit to the index, the sitemap, or the layout.

The example below is the schema, not a post.

## Routes

| URL | Source |
| --- | --- |
| `https://getcoppice.app/blog` | `src/pages/blog/index.astro` |
| `https://getcoppice.app/<locale>/blog` | the same index, listing notes in `src/content/blog/<locale>/` |
| `https://getcoppice.app/blog/<slug>` | `src/content/blog/en/<slug>.md` |
| `https://getcoppice.app/<locale>/blog/<slug>` | `src/content/blog/<locale>/<slug>.md` when that file is published |

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

JSON-LD on an article is `BlogPosting`: `datePublished` is `published`, and `dateModified` is `updated` or, when you omit `updated`, the same published date. The author is the `author` string. The canonical URL of an English note is `https://getcoppice.app/blog/<slug>`. A published translation canonicalizes to `https://getcoppice.app/<locale>/blog/<slug>`. A locale URL with no published translation renders the English note and canonicalizes to the English URL. Each locale index is a `Blog` and lists the notes published in that locale. With no notes, it does not emit an empty `blogPost` list.

## Images

Put screenshots in `public/blog/<slug>/` and refer to them from the root:

```markdown
![Demo board with two tickets in Backlog and In Progress.](/blog/short-heading/board.webp "Demo board: two tickets at once")
```

`alt` is required. The quoted title is the caption, rendered in a `figcaption`. The image is lazy-loaded. Capture notes and the limits of the current demo script are in [docs/development.md](../docs/development.md#marketing-screenshots).

## Translation

Add `src/content/blog/<locale>/<slug>.md` only when that translation exists. `<locale>` is `vi`, `zh-cn`, `zh-tw`, `zh-hk`, `ja`, or `fr`. Use the same slug as the English file. `hreflang` and the sitemap then include that locale. A locale with no published file is not listed on the article. The index at `/<locale>/blog` lists only that locale's published notes. It does not link to a missing translation. Opening a missing translation directly, for example `/ja/blog/<slug>`, renders the English note and points the canonical URL at `https://getcoppice.app/blog/<slug>`.

`draft: true` is per file. A draft translation is not an alternate, and it does not appear on that locale's index.

Labels on the index and the article page (`Published`, `Updated`, the index description, and the empty state) come from `src/i18n/ui/<locale>.ts`. `Blog` in the nav and the breadcrumb is `nav.blog` in that same file. Headings inside the note, including "Which one to pick", stay in the markdown.

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
