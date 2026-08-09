# Mock

Built-in connector for CI, automated tests, and default Docker Compose. No real CLI or API keys. Returns canned results from `fixtures/agent-responses/` (for example `done.json`, `blocked.json`).

**Connector id:** `mock`

## Use it

Default in Compose (`AGENT_DEFAULT_PROVIDER` / `default_connector` = `mock`). No install or login.

```toml
[agent]
default_connector = "mock"
```

Optional: `MOCK_AGENT_RESPONSE=blocked` to exercise blocked outcomes.

## Behavior notes

- Emits scripted terminal lines for Live Console testing
- Parses the result contract from fixtures, not from an LLM
- Use real connectors ([cursor](cursor.md), [opencode](opencode.md), …) when you want live models
