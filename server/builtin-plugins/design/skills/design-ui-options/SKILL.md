---
name: design-ui-options
description: Use when a ticket changes what people see. Publish 2–3 clickable HTML options for the human to choose before you plan or build.
---

On UI work call `design__list` first. Then `design__create` → `design__write` files → `design__publish` 2–3 options. Do not put HTML in the ticket. Next turn honor `selectedDesign` from `ticket_get` or `design__list` on a ticket, and `selected` in chat. Never use editor Write for these files. Pass `recommendedOptionId` with the option you recommend. On a plan revision, do not publish new options unless the change request asks for them.
