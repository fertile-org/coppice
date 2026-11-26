# Third-party notices

Coppice skills are written and shipped by Coppice. Agents do not load these repositories at runtime, and users do not install them. The notes below are the files a skill comment points at. The Coppice text is a short adaptation, not a copy of the file.

## google-labs-code/design.md

- Repository: https://github.com/google-labs-code/design.md
- File: `docs/spec.md`
- Commit: `db977b3c1d1f309269f43d5d17ae828d54bd6ae3`
- License: Apache-2.0 (`LICENSE` in that repository)
- Used by: `server/builtin-plugins/design/skills/design-ui-options/SKILL.md`, `server/builtin-plugins/design/skills/design-system/SKILL.md`
- The text was modified. Coppice keeps the idea of a root `DESIGN.md` with markdown plus YAML tokens, and writes its own steps for Coppice tools.

## anthropics/skills

- Repository: https://github.com/anthropics/skills
- File: `skills/frontend-design/SKILL.md`
- Commit: `41bbe19d1a1a7eaab5e7bb9050a417e5c6cffc8f`
- License: Apache-2.0 (`skills/frontend-design/LICENSE.txt`)
- Used by: `server/builtin-plugins/design/skills/design-ui-options/SKILL.md`
- The text was modified. Coppice keeps the idea that options should differ in layout and that copy should say what an action does, and drops the rest.

## pbakaus/impeccable

- Repository: https://github.com/pbakaus/impeccable
- File: `.agent/skills/impeccable/reference/audit.md`
- Commit: `c475ffcec4045688323eb69b1e6a4f6c4655031c`
- License: Apache-2.0 (`LICENSE` in that repository)
- Used by: `server/builtin-plugins/design/skills/design-ui-options/SKILL.md`
- The text was modified. Coppice keeps the contrast ratio, the 44 pixel tap size, and the empty, loading, and error checks, and drops the scoring rubric.
