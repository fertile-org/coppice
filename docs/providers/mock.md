# Mock provider

**ID:** `mock`  
**Status:** Implemented (M03)  
**Stream backend:** `ScriptedStream`

Default provider for CI, integration tests, and `deploy/docker-compose.yml`. Returns canned JSON from `fixtures/agent-responses/` (e.g. `done.json`, `blocked.json`).

## Config

```toml
[agent]
default_provider = "mock"
```

No extra provider section required.

## Behavior

- Emits scripted terminal lines during `run` for Live Console testing.
- Parses result contract from fixture files, not from LLM output.
- Set `MOCK_AGENT_RESPONSE=blocked` to test blocked outcomes locally.

## When to use

Automated tests, Docker Compose, and any environment without real CLI tools or API keys.
