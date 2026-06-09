# OpenCode session view

Frontend module for rendering OpenCode live sessions.

**Upstream:** [anomalyco/opencode](https://github.com/anomalyco/opencode) @ `b4a641921559031b53ac9dd782652e3def036d42`

**Ported from:** `packages/tui/src/routes/session/index.tsx` (upstream moved from `packages/opencode/src/cli/cmd/tui/routes/session/index.tsx`)

| Coppice file | Upstream component |
|--------------|-------------------|
| `tools/ToolShell.tsx` | `InlineTool` / `BlockTool` (shared border, title, status) |
| `tools/Bash.tsx` | `Shell` (`bash` tool) |
| `tools/Read.tsx` | `Read` |
| `tools/Write.tsx` | `Write` |
| `tools/Edit.tsx` | `Edit` |
| `tools/Grep.tsx` | `Grep` |
| `tools/Glob.tsx` | `Glob` |
| `tools/List.tsx` | Generic `ls` tool (no dedicated upstream component) |
| `tools/WebFetch.tsx` | `WebFetch` |
| `tools/Task.tsx` | `Task` |
| `tools/Skill.tsx` | `Skill` |
| `tools/Question.tsx` | `Question` |
| `tools/TodoWrite.tsx` | `TodoWrite` |
| `tools/ApplyPatch.tsx` | `ApplyPatch` |
| `parts/TextPart.tsx` | `TextPart` |
| `parts/ReasoningPart.tsx` | `ReasoningPart` |
| `parts/ToolPart.tsx` | `ToolPart` router |
| `sync/reduce-event.ts` | OpenCode TUI event reducer semantics |

## Pin upstream commit

```bash
git ls-remote https://github.com/anomalyco/opencode.git refs/heads/dev
```

## Coppice deltas

- Solid.js / OpenTUI → React + Tailwind (no `@opentui/solid`)
- `Shell` upstream component exported as `Bash.tsx` for the `bash` tool key
- `List.tsx` covers the `ls` tool (falls through to `GenericTool` upstream)
