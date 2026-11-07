# English review evidence

Record of the independent second pass and of the acceptance decision for [SKILL.md](SKILL.md). Source identities and URLs are in [corpus-manifest.md](corpus-manifest.md). This file does not reproduce the articles.

Review date: 7 November 2026. The reviewer was a second reader, not the first researcher, and read the 30 current bodies before reading the research analysis or the draft skill.

## What was accepted

The skill is installed under the publication-date criterion, which is the criterion requested for this repository:

- Original publication strictly before 2020-11-07.
- Originally authored English, in the sense recorded in the manifest.
- Two full reads of all 30 current original textual bodies.

The stricter gate, an immutable pre-cutoff snapshot for every sentence, was not met and is not claimed. Later revisions mean some wording on the live pages is not proven to predate 2020. One archived body (EN25) was read in full. The other 29 were not. EN17 and EN24 remain excluded from support even though their first-publication dates pass.

This corpus is not 30 verified untouched pre-generative-AI texts. It is not a set of native-speaker biographies. A later revision date is uncertainty about wording age. It is not an allegation that any author used a generative model.

## What each pass actually read

- Researcher: all 30 current article texts, including available notes. Primary HTML was retained for that pass and is not committed here.
- Independent reviewer: all 30 current article texts again, including EN04's duplicated notes counted once, notes on EN18, EN19, EN21, EN23, and EN30, and the boundaries below. The reviewer checked that each stored article was wholly contained in its reading batch.
- The reviewer reported that 60 stored HTML and Markdown hashes matched the research manifest. Those files and hashes are not in this repository. This install does not re-check them.
- EN25 only: one additional complete archived body, fetched and read from `https://web.archive.org/web/20190126235336id_/https://blog.codinghorror.com/regular-expressions-now-you-have-two-problems/` (snapshot 27 January 2019). The explanatory prose and numbered tips used for the reviewed mechanisms are substantially present there. Punctuation, links, image migration, and an HTML-escaping defect differ from the current page. That is not character-for-character identity, and it is not a validation of the article's sanitizer.
- EN17 archive candidate `https://web.archive.org/web/20190116180350id_/https://css-tricks.com/the-trick-to-viewport-units-on-mobile/` (snapshot 17 January 2019) was not read; the fetch timed out. Availability metadata is not a reading.
- EN24 archive candidate `https://web.archive.org/web/20100214030959id_/http://www.codinghorror.com:80/blog/2007/10/embracing-languages-inside-languages.html` (snapshot 14 February 2010) was not read. A "closest" hit on the newer URL pointed at 2021 and was rejected as pre-cutoff evidence.
- A later raw-archive download of those three URLs received HTTP 429. No raw archive HTML is stored. EN25's successful read is separate from that failed download.

No replacement article was researched to fill EN17 or EN24. The skill does not need them. Do not promote them to support without a new two-pass reading of an accepted historical body.

## Boundaries that change what may be copied

- Navigation, newsletters, bios, donation asks, comments, sponsor blocks, and share prompts are not author style.
- A quotation is not the surrounding author's sentence habit. EN05 credits Meyer. EN06 credits Meszaros. EN14 quotes a specification. EN28 quotes a maintainer. EN12 uses blockquote markup for its own facts; the tag alone does not mean "third party."
- EN16's live browser-support table is excluded. EN18 and EN19's 2026 series navigation is excluded.
- EN07's withdrawn opening guarantee is a counterexample for product promises, not a pattern to copy.
- EN08's historical encoding claims, EN21's secret-handling steps, EN23's commands, and EN25's sanitizer are not current technical advice.
- EN24 and EN25 contain damaged code in the publisher HTML. EN28's nested list follows malformed markup. Do not imitate those defects, and do not insert errors to look human.
- EN22's 50/72 figures are Git and email line lengths, not a prose ceiling.
- Diagrams and demos were not executed, so this record says nothing about visual design or about whether an example still runs.

## Rule bounds

Support is qualitative. A counterexample limits a recommendation. It is not a judgment that the author writes badly. Locators name the article and the part of it that was checked. The articles themselves are at the URLs in the manifest.

| Rule | Status in the skill | Independent support | Bound |
| --- | --- | --- | --- |
| R01 concrete contract | Supported, early enough for the task | EN09 opening tool identity; EN21 task and assumptions; EN18 goal and protocol; EN01 observed failure | EN08 uses history before setup. EN06 opens on vocabulary. Not a first-sentence law, and not a ban on history. |
| R02 actor and action | Supported. Noun-stack repair is an English editorial heuristic | EN03 process attributes; EN04 client, supplier, abstraction; EN30 names the enqueuer | EN06 keeps a passive clause whose topic is the object. EN04's repeated frame is useful. Do not force every passive into active voice. |
| R03 stable nouns | Supported. Article check is contextual, not a first-mention algorithm | EN12 on the ambiguity of "changing x"; EN13 shared row versus recomputed rows; EN05 introduces synonyms on purpose | EN15 uses pronouns where the referent is clear. No repetition quota. Do not put prose articles on identifiers or headings. |
| R04 information load | Supported flexibility. No length ceiling | EN12 short facts then long demonstration; EN14 dense clauses then a step-by-step; EN30 condition and consequence together | EN06 stays short. EN08 uses long paragraphs. EN04 repeats a sentence opening. EN22's 50/72 numbers are not prose limits. |
| R05 known to new | Supported, with a lookup exception | EN10 reuses one database and feeds output into the next step; EN13 one board; EN30 one service through new failures | EN06's separate categories need no narrative bridge. |
| R06 real relationships | Supported. Do not rewrite if as when | EN15 waiting and other work; EN27 source order and assistive-technology consequence; EN05 because queries do not modify state | EN06's definitions often need no connector. A possibility must not become an expected event. |
| R07 one mental step | Supported teaching pattern. Delayed correction is rejected for instructions | EN03 misconception then mechanism; EN18 baseline then trace; EN12 binding then mutation | EN07 states a TCP guarantee and withdraws it later. Do not import that into a procedure, a promise, or a security note. EN06 is a taxonomy, not a tutorial outline. |
| R08 examples as evidence | Supported, sized to the stated purpose | EN01 command, bytes, field; EN13 near-identical syntax with different behavior; EN19 workload held steady; EN21 coverage can fail while the build passes | EN06 uses brief examples rather than a full fixture. Do not invent output. Do not treat a historical command as a Coppice instruction. |
| R09 modal force | Supported as editing prompts, not as a private dictionary | EN14 may for rendering rather than a guarantee; EN20 must for hash invariants, separate from design advice; EN30 idempotency condition and the at-least-once note | EN05's should is a principle the author allows himself to break. EN28's ideally can be aspiration. Specification definitions win. |
| R10 evidence boundary | Supported as an editorial default | EN05 scopes C++ uncertainty; EN27 points at specialist explanations; EN28 marks eight hours as arbitrary and the cache as process-local; EN29 states security uncertainty | EN19 and EN07 show that confident wording can overclaim. Removing a hedge is allowed only when the meaning survives. |
| R11 jargon at the boundary | Supported | EN06 umbrella term then categories; EN12 optional analogy; EN18 explains the sequential case; EN01 scopes a formal definition to the task | EN21 assumes GitHub, Linux, and tox. EN15 links prior knowledge. Do not expand every acronym, and do not replace a precise term with a vague one. |
| R12 ordinary tone | Editorial default across divergent voices. Not a test of human authorship | EN10 and EN15 give direct steps; EN05 is restrained; EN23 uses I for a personal preference | EN14 jokes; EN08 scolds; EN29 is enthusiastic; archived EN25 encourages theatrically. Do not imitate a signature, invent a narrator, or ban enthusiasm. |
| R13 shape | Conventional default, explicitly editorial for tables | EN06 parallel categories; EN02 staged demos; EN21 assumptions, core, and bonus; EN22 action headings | EN26 numbers links that are not ordered steps. EN28's broken nesting is bad markup. Do not force every list item to start with the same word. |
| R14 verifiable effects | Supported concreteness. Restraint is a default, not a corpus law | EN09 names input, output, and account boundaries; EN10 shows formats; EN27 names a user consequence | EN09 also uses promotional adjectives. Archived EN25 praises tools. Do not invent a more specific benefit to replace a vague one. Keep a locked Coppice slogan. |
| R15 genre fit | Editorial transfer, except where a source is actually that shape | EN06 terminology; EN21 task steps; EN09 product orientation; EN12 a long conceptual model | EN14 is not a button or README pattern. EN23 is not a general procedure. Check the Coppice page in front of you. |
| R16 preserve the contract | Editorial safeguard motivated by the sources | EN12 rebinding versus mutation; EN14 modal and ordering conditions; EN20 equality and hash requirements; EN30 at-least-once note | EN07 oversimplifies on purpose. EN23 disagrees with itself about a ref name. EN25's raw code escaping is already damaged. Flag a bad literal. Do not silently repair it or treat preservation as proof. |

EN17's resize caveat and EN24's same-operation contrast were redundant with other articles. They are omitted here on purpose.

These English applications were checked against the corpus: finite verbs, noun modifiers, articles and definiteness, pronoun reference, contractions, if versus when, modal force and negation, and parallel grammar. They are not a finding that other languages can skip their own research. No cross-author frequency count was performed.

## Checks that changed the skill

The reviewer rejected sample rewrites that looked clearer while changing the claim. The miniature examples in the skill are the ones that keep a stated brief. Do not revive the rejected moves:

- Do not drop a real phase, such as initiating a retry versus performing it, because a shorter sentence sounds cleaner.
- Do not narrow "all files" to "files this command created" unless that scope is given.
- Do not invent a flag, a format compatibility claim, or a spreadsheet result.
- Do not guess which noun a pronoun names.
- Do not turn "always faster" into a measured cache claim. A mechanism-plus-unknown rewrite is a different brief.
- Do not invent a cause or a recovery for an undiagnosed save failure.
- Do not replace "retries are safe" with a specific idempotency design that was not supplied. The at-least-once example is usable only when that whole contract is given.
- Do not paraphrase marketing into an upload or sync behavior unless that behavior is the brief.

The reviewer also required these limits, which the skill keeps:

- Check the intended result when success of the process is not success of the task.
- Branch the next step on the actual condition. Do not offer one universal retry.
- Keep a contrast that is the point of the explanation, including repeated nouns.
- Read footnotes. Move a decision-critical limit next to the claim. Do not make footnotes mandatory on Coppice pages.
- Classify quotations by who is speaking.
- Do not copy corrupted literals as style.

No SEO, conversion, detector, or technical-correctness result is claimed from this research.
