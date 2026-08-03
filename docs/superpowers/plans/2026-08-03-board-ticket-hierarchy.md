# Board Ticket Hierarchy Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make direct parent-child ticket relationships immediately scannable on board cards and navigable from a child ticket's detail drawer without changing status placement or drag behavior.

**Architecture:** Build a pure hierarchy index from the project's existing ticket array and memoize it in `BoardPage`. Pass each ticket's derived relationship summary through `BoardColumn` into `TicketCard`, reuse it for the active drag overlay, and pass the resolved parent summary into the existing drawer/detail path.

**Tech Stack:** React 19, TypeScript, TanStack Query, React Router, dnd-kit, lucide-react, Tailwind CSS, Vitest, Testing Library.

---

### Task 1: Derive direct hierarchy metadata

**Files:**
- Create: `web/src/features/board/ticketHierarchy.ts`
- Test: `web/src/features/board/ticketHierarchy.test.ts`

- [ ] **Step 1: Write the failing hierarchy tests**

Cover an unrelated ticket, a child with a resolved parent, a parent with direct children in different columns, a child that is also a parent, exact `done` counting, and an unresolved parent. Build concise `Ticket` fixtures and assert the returned map entries.

- [ ] **Step 2: Run the hierarchy tests to verify they fail**

Run: `cd web && npm test -- src/features/board/ticketHierarchy.test.ts`

Expected: FAIL because `ticketHierarchy.ts` does not exist.

- [ ] **Step 3: Implement the hierarchy index**

Define `TicketParentSummary`, `TicketHierarchy`, and `buildTicketHierarchyIndex(tickets)`. Initialize one entry per ticket, resolve parents from a single ID map, mark missing parents unavailable, and count only immediate children whose status is exactly `done`.

- [ ] **Step 4: Run the hierarchy tests**

Run: `cd web && npm test -- src/features/board/ticketHierarchy.test.ts`

Expected: PASS.

### Task 2: Render and propagate board hierarchy cues

**Files:**
- Modify: `web/src/features/board/TicketCard.tsx`
- Modify: `web/src/features/board/BoardColumn.tsx`
- Modify: `web/src/features/board/BoardPage.tsx`
- Test: `web/src/features/board/TicketCard.test.tsx`
- Test: `web/src/features/board/BoardPage.test.tsx`

- [ ] **Step 1: Write failing card and board tests**

Render card fixtures for unrelated, child-only, parent-only, combined, and unresolved-parent variants. Assert the exact visible relationship labels, decorative icons, full parent title in accessible text, and no hierarchy content for unrelated tickets. Mock dnd-kit and board data in the board test, trigger a drag start for a cross-column child, and assert its hierarchy cue appears in both the resting card and overlay.

- [ ] **Step 2: Run the focused tests to verify they fail**

Run: `cd web && npm test -- src/features/board/TicketCard.test.tsx src/features/board/BoardPage.test.tsx`

Expected: FAIL because card hierarchy props and rendering are absent.

- [ ] **Step 3: Implement the visual hierarchy rows and data flow**

Memoize `buildTicketHierarchyIndex(tickets ?? [])` in `BoardPage`. Pass the index into each `BoardColumn`, pass the matching entry into each resting `TicketCard`, and pass the active ticket's entry to the drag overlay card. In `TicketCard`, render a `GitBranch` row above the title for children and a `Network` row after badges for parents, using only existing text/token classes. Keep icons `aria-hidden`, keep complete titles in the DOM while applying one-line visual truncation, and retain the card's existing button/drag behavior.

- [ ] **Step 4: Run the focused card and board tests**

Run: `cd web && npm test -- src/features/board/TicketCard.test.tsx src/features/board/BoardPage.test.tsx`

Expected: PASS.

### Task 3: Add same-drawer parent navigation

**Files:**
- Modify: `web/src/features/board/BoardPage.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.tsx`
- Modify: `web/src/features/tickets/TicketDetailPanel.tsx`
- Modify: `web/src/features/tickets/TicketDrawer.test.tsx`

- [ ] **Step 1: Write the failing drawer navigation test**

Render `TicketDrawer` with a resolved parent summary, assert the visible `Parent ticket` section and parent title, activate its button, and verify the router search parameter changes to the parent's ticket ID while the existing child section remains available.

- [ ] **Step 2: Run the drawer test to verify it fails**

Run: `cd web && npm test -- src/features/tickets/TicketDrawer.test.tsx`

Expected: FAIL because the drawer has no parent relationship control.

- [ ] **Step 3: Implement parent navigation**

Derive the selected ticket's resolved parent from the board hierarchy index and pass it into `TicketDrawer` and `TicketDetailPanel`. Render a semantic section with a native button that shows the full parent title, uses the existing router search-parameter navigation, and has an explicit token-based focus ring.

- [ ] **Step 4: Run the drawer test**

Run: `cd web && npm test -- src/features/tickets/TicketDrawer.test.tsx`

Expected: PASS.

### Task 4: Verify the complete frontend change

**Files:**
- Review all files above.

- [ ] **Step 1: Run frontend tests**

Run: `make web-test`

Expected: all Vitest suites pass.

- [ ] **Step 2: Run the production build**

Run: `cd web && npm run build`

Expected: TypeScript and Vite build succeed.

- [ ] **Step 3: Audit the resulting interaction for WCAG 2.1 AA concerns**

Check that relationship meaning is visible in text, icons are decorative, truncated text remains complete for assistive technology, card and drawer controls retain keyboard activation and visible focus, and no relationship element becomes an interactive child of the draggable card.

- [ ] **Step 4: Review and commit**

Inspect the focused diff, address any correctness issues, stage only ticket-owned files, and commit with a clear frontend feature message. Do not stage `.agent/context.md`.
