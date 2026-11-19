/// Plan a plugin build from the five answers (PM agent).
pub const JOB_TYPE_PLAN_PLUGIN_BUILD: &str = "plan_plugin_build";
/// Write plugin files into the build workspace (implementer agent).
pub const JOB_TYPE_BUILD_PLUGIN: &str = "build_plugin";

/// Exact admin copy when no PM/implementer has a ready connector (§10).
pub const NO_AGENT_READY_TO_BUILD: &str =
    "No agent is ready to build. Set one up in Agents first.";

/// Section 9 must / must-not rules injected into build-run context.
pub const BUILD_FORMAT_RULES: &str = r#"# Plugin format rules (must / must not)

**Must**

1. Layout: `.claude-plugin/plugin.json` with `name`, `version`, `description`; skills in `skills/<skill-name>/SKILL.md`; tool servers in `.mcp.json` at the plugin root.
2. Plugin name matches `^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$`, isn't `coppice`, and doesn't clash with an installed plugin name.
3. Keep `<plugin>__<tool>` at or under 50 chars, so Coppice doesn't hash the name. In practice: plugin name ≤ 20 chars, tool names ≤ 28 chars.
4. Every `SKILL.md` starts with YAML frontmatter with non-empty `name` and `description`; the skill folder name equals `name`.
5. Skills refer to tools by the name the MCP server advertises (for example `create_entry`), never `changelog-helper__create_entry` and never `mcp__…`.
6. Tool servers are `stdio` or `http` only.
7. Every secret or per-user value is a `${KEY}` placeholder (upper snake case). No literal keys, tokens, passwords, emails or home paths anywhere in the files.
8. Paths inside the plugin use `${CLAUDE_PLUGIN_ROOT}`, never absolute paths.
9. Include a short `README.md`: what it does, settings it needs, how to install it.
10. Everything stays inside the plugin folder (no symlinks out, no `..` paths).

**Must not**

1. Reference Coppice gateway tools: `ticket_*`, `comment_post`, `result_submit`, `knowledge_*`, `open_repo`, `skill_list`, `skill_load`, `design__*`.
2. Create `commands/`, `agents/` or `hooks/`.
3. Use `"type": "sse"`, or add npm sources to any marketplace file.
4. Mention Coppice-only concepts in skills (tickets, boards, Plan Review).
5. Rely on Coppice-only env names (`COPPICE_*`).
"#;

pub fn is_plugin_build_job(job_type: &str) -> bool {
    job_type == JOB_TYPE_PLAN_PLUGIN_BUILD || job_type == JOB_TYPE_BUILD_PLUGIN
}
