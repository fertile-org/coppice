---
name: blog-writing
description: Use when planning, drafting, reviewing, or rewriting a post for the Coppice blog (website/src/content/blog/). Covers what a post is for, how to pick the topic and the reader's real problem, the structure, how much Coppice detail to include, and the checks to run. Not for user docs, READMEs, or social posts.
---

# Blog writing

The Coppice blog exists for search. Each post answers a question people already type into Google about coding agents, then shows Coppice as one good way to solve it. A post is not a getting-started guide and not product documentation.

For English prose, also follow [english-technical-writing](../english-technical-writing/SKILL.md). For frontmatter, routes, images, and checks, see [website/BLOG.md](../../../website/BLOG.md).

## 1. Pick a topic people search for

- Start from a generic question, not a Coppice feature: "How to use Claude Code and Codex together on one project", not "Assigning agents in Coppice".
- Phrase the title the way someone would search for it. "How to …" and "… vs …" titles work well.
- One post answers one question.

## 2. Name the reader's real problem

Before you write, state the problem the reader actually has. Do not invent one to set up Coppice.

Check it against how people work today. Example: for using Claude Code and Codex together, the hard part is not telling who changed what. One agent can plan and another can build from that plan without trouble. The hard part is switching between terminals, and it gets worse with several tasks in parallel.

If you are not sure what the real problem is, ask the owner before you draft.

## 3. Structure

1. **`answer` (frontmatter):** the direct answer in two to four sentences. It mentions the options, including Coppice.
2. **The problem:** a short section on why this is hard, in the reader's terms.
3. **The options:** two to four ways to solve it. Most should work without Coppice, such as plain terminals, git worktrees, or a script. Give each one a sentence or two on when it works and where it stops working. Show a command only when it is the whole option, such as `git worktree add`.
4. **The Coppice option:** what Coppice does for this problem in one or two sentences, then at most five short steps to try it. Link to the docs for the details.
5. **Which one to pick:** one line per option, matched to a situation.

Keep the whole post short. A reader should finish it in a few minutes.

## 4. How much Coppice detail

Write for someone who has never opened Coppice.

- Leave out internal terms and settings: connectors, `auto_start_runs`, column rules, exact error messages, every button on a form. Say "pick Claude Code or Codex for each agent", not "choose a connector".
- Leave out edge cases and troubleshooting. Link to [Getting started](/docs/getting-started), [Concepts](/docs/concepts), or [Providers](/docs/providers) instead.
- Each Coppice claim must match the current docs or code. Check `website/src/pages/docs/` and, when in doubt, the server code. Do not promise a feature that is not shipped.

## 5. Other tools

- Generic approaches are fine to describe: terminals, git worktrees, shell scripts, the CLIs themselves.
- Do not recommend or pitch a competing product. If one is relevant, mention it in a few words at most and move on to Coppice.

## 6. Screenshots

- Use one or two at most, only where a picture saves words.
- Captions describe what the reader sees. Do not write "Demo data" or "fixture" in a caption.
- Delete any image the post no longer uses from `website/public/blog/<slug>/`.

## 7. Before you finish

From `website/`:

```bash
npm run build
npm run check:seo
npm run check:links
node --experimental-strip-types --test src/lib/blog.test.ts
```

Then read the post once as a new visitor who searched for its title. Ask:

- Does the first paragraph answer the question?
- Is the problem one the reader would recognize?
- Could they try the Coppice option from the steps alone, and find more in the linked docs?
- Is there any sentence that only makes sense to someone who already uses Coppice? Cut it.
