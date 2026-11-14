---
name: design-ui-options
description: Use when a ticket changes what people see. Publish 2–3 clickable HTML options for the human to choose before you plan or build.
---

On UI work call `design__list` first. Then `design__create` → `design__write` files → `design__publish` 2–3 options. Do not put HTML in the ticket. Next turn honor `selectedDesign` from `ticket_get` or `design__list` on a ticket, and `selected` in chat. Never use editor Write for these files. Pass `recommendedOptionId` with the option you recommend. On a plan revision, use only the newest change request: if it alters what people see, republish 2–3 options and set `recommendedOptionId` to the option the revised plan follows; if it only picks an option ("Update the plan to use design option …") or does not touch what people see, keep the current set. When the plan has no visible change, including after a change request drops the UI work, submit the plan with `noVisibleChange: true` and do not publish options.
