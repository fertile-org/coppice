# Mock

This connector is for CI and automated tests only. It is not an end-user connector.

Built-in connector for CI, automated tests, and default Docker Compose. No real CLI or API keys. Returns canned results from `fixtures/agent-responses/` (for example `done.json`, `blocked.json`).

Desktop installers and `make release-tar` are built without the `mock-provider` feature, so this connector is not compiled, not listed, and not packaged with those builds. Opt in for local `cargo run` with `--features mock-provider` (`make server` does).

**Connector id:** `mock`

## Use it

No install or login. With the `mock-provider` feature (Compose, `make server`, CI), creating an agent that omits a connector uses `mock`.

Optional: `MOCK_AGENT_RESPONSE=blocked` to exercise blocked outcomes.

## Behavior notes

- Emits scripted terminal lines for Live Console testing
- Parses the result contract from fixtures, not from an LLM
- Use real connectors ([cursor](cursor.md), [opencode](opencode.md), …) when you want live models
