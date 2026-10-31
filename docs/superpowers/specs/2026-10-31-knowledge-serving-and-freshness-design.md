# Knowledge — Serving, search quality, and freshness

**Status:** Draft for review
**Date:** 2026-10-31
**Amends:** [Agent compaction + full-text search](2026-09-28-knowledge-agent-compaction-fts-design.md). Governance (Inbox, immutable revisions, supersession, untrusted-data labelling) is unchanged.

## Problem

Governance is sound, but the pipeline around it has gaps:

- Agents get one generic line about `knowledge_search`, so they cannot tell a relevant rule exists.
- Full-text search misses `tests` vs `test`, `config.toml`, and file paths, and ranks filler above specific rules because common words weigh the same as rare ones.
- Nothing detects stale items, and agents cannot report a wrong one.
- Only Done tickets are mined. The working agent, which knows the most, cannot propose anything.
- `knowledge_search` always returns nothing in compaction runs (`board_id: None`), although the context tells the agent to use it.
- Compaction input omits the run summaries the previous spec promised.

## Decisions

1. **Start-of-run index.** Full and Human Agent ticket runs get up to `knowledge.retrieval.index_rows` (default 10; 0 disables) titles ranked against the ticket, capped at 3,200 bytes. Titles only, labelled as data. An index failure never fails a run.
2. **Two-step reads.** `knowledge_search` returns short rows (id, title, type, snippet, token estimate). New `knowledge_get` returns up to 5 full entries. Usage is logged on `knowledge_get` only, so `usage_count` means "opened".
3. **Better Postgres search, still no embeddings.** The search vector adds split tokens and an `english` stem layer next to `simple`. Ranking is summed inverse document frequency over the eligible set, with a title boost.
4. **Citations, verified at read time.** A revision may cite up to 10 repo-relative paths. The server anchors each to the default-branch commit when the file exists there. On read, search and get report `freshness`: `current`, `changed`, `missing`, `unknown`, or `uncited`. Git problems yield `unknown`, never an error. The index does not call git.
5. **Agent feedback.** `knowledge_flag` records `outdated`, `wrong`, or `confirmed`. `outdated` and `wrong` show on Inbox cards and never retire an item by themselves. `confirmed` re-anchors citations to the current default branch.
6. **Working agents can propose.** `knowledge_propose` creates Pending items only, at most 3 per run, on the run's board and with the run's ticket as source. A close match to an existing item returns that item instead.
7. **Compaction also cleans up.** Its context gains near-duplicate pairs among approved items, and a candidate may supersede one without a source ticket. Supersessions stay human-reviewed, and each item keeps a single replacement, so duplicates converge over cycles.
8. **Optional lighter review.** `knowledge.auto_save.allow_verified_citations` (default `false`) lets the existing allowlist auto-approve only candidates with at least one citation anchored to a commit. High-impact types, workspace scope, and supersessions still need a human. The status strip shows the age of the oldest Pending item.

## Compaction input

The ticket section gains the latest successful run's `submitted_result` summary and `changedFiles`. Comment selection keeps up to 4 `review_feedback` comments first, then fills to 8 with the latest others. The compaction run still has no repository: it cites paths it sees in this input and the server validates them.

## Out of scope

Embeddings, vector stores, graphs, per-tool-call capture, mining non-Done tickets directly (`knowledge_propose` covers those lessons), automatic retirement of items, and renaming "compaction".

## Measuring it

Evaluate on Coppice's own use before and after, by setting `index_rows` to 0 or 10: share of runs calling `knowledge_search` or `knowledge_get`, zero-result search rate, flags per opened item, Pending age, and merge or rework rate. Queries live in `docs/operations.md`. Consider embeddings only if logged zero-result searches show paraphrase misses.
