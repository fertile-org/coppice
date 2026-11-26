---
name: design-ui-options
description: Use when a ticket changes what people see. Read the project's design source, then publish 2–3 clickable HTML options for the human to choose before you plan or build.
---

<!-- Adapted from google-labs-code/design.md@db977b3c1d1f309269f43d5d17ae828d54bd6ae3 (Apache-2.0), anthropics/skills@41bbe19d1a1a7eaab5e7bb9050a417e5c6cffc8f (Apache-2.0), and pbakaus/impeccable@c475ffcec4045688323eb69b1e6a4f6c4655031c (Apache-2.0). See THIRD_PARTY_NOTICES.md. -->

On UI work call `design__list` first. Then read the project's design source, in this order, and stop when you have enough to match it:

1. `DESIGN.md` at the repo root, if it exists. It is plain markdown plus YAML design tokens (colors, typography, spacing). The token values win over the prose color names.
2. Theme and token files: CSS variables, `tokens.json`, or a Tailwind theme.
3. Two or three existing screens that this ticket touches. Reuse their components and spacing. Do not invent a second visual language.

Options must use those tokens and components. Two or three options, and they must differ in layout or structure, not only in color.

Each option covers the states the screen can be in, when they matter: empty, loading, and error. Empty tells the person what to do next. Loading shows that work is in progress. Error says what went wrong and how to fix it, in the product's voice.

Before `design__publish`, check the HTML and fix what fails:

- Body text contrast at least 4.5:1. Large text at least 3:1.
- Taps and clicks at least 44 by 44 pixels.
- The layout works on a phone and on a desktop.
- Labels and buttons say what happens, in plain language. The same action keeps the same name.

Then `design__create` → `design__write` files → `design__publish` 2–3 options. Stay under 512 KiB per file and 2 MiB per set. Do not put HTML in the ticket. Never use editor Write for these files. Pass `recommendedOptionId` with the option you recommend.

Put a `{{design:<setId>}}` marker under the plan section that choice decides (for example under `## UI design`), so Plan Review shows the picker there. On a first plan, if `design__list` already shows a selected option, plan around it and do not publish new options. On a plan revision, the newest change request alone decides whether to republish: if it alters what people see, republish 2–3 options and set `recommendedOptionId` to the option the revised plan follows; if it only picks an option ("Update the plan to use design option …") or does not touch what people see, keep the current set. When the plan has no visible change, including after a change request drops the UI work, submit the plan with `noVisibleChange: true` and do not publish options.

If there is no `DESIGN.md`, no token file, and no existing screen to match, do not pretend the options are the product's design system. Say that in the plan and propose a follow-up ticket to add a `DESIGN.md`. That ticket should load `design:design-system`.
