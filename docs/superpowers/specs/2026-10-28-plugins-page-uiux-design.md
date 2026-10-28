# Plugins Page UI/UX — Design

**Status:** approved (2026-10-28)

## Goal

Make **Plugins → Installed / Marketplace / Custom** three clear task areas. Remove the large inline guide, make marketplace discovery useful without manual source setup, and give entries recognizable visual identities without changing plugin installation, enablement, settings, or agent-access rules.

## Decisions

| Topic | Decision |
|---|---|
| Page structure | Three tabs: **Installed**, **Marketplace**, **Custom**. |
| Installed | Shows every installed plugin, whether it came from a marketplace, git, or a directory. No installation forms. |
| Guide | Remove the inline **How plugins work** panel and its local-storage state. Link **Plugin guide ↗** to `https://getcoppice.app/docs/plugins`. Do not duplicate the guide in a modal. |
| Marketplace | Search and source/category filters above a responsive card grid. Source management moves into a dialog. |
| Custom | Contains the existing git-install and plugin-directory controls. Link to `https://getcoppice.app/docs/plugins#create-a-plugin` for the required format. |
| Default external source | Seed only Anthropic's official directory: `https://github.com/anthropics/claude-plugins-official.git`, `main`, source name `claude-plugins-official`. |
| Trust | The Anthropic source and its entries remain **Community / not reviewed by Coppice**. Do not present them as Coppice-reviewed. |
| Icons | Use locally bundled brand icons for recognized entries, including the GitHub logo for `github`; use a puzzle icon for all others. No runtime third-party icon requests. |
| Catalog size | Reveal 24 matching entries initially and 24 more per **Load more**. Reset to 24 whenever query, source, or category changes. |
| Plugin behavior | Existing install/update/provenance, disabled initial state, settings, test, and agent-access behavior stays unchanged. |

## Rejected alternatives

- **Two tabs plus a custom-install dialog:** fewer tabs, but hides git and directory management behind another interaction.
- **One continuous page:** simple initially, but becomes long again as the catalog and installed list grow.
- **Official plus Anthropic community sources:** unnecessarily increases the default third-party trust surface and catalog noise.
- **A small Coppice-curated external catalog:** requires per-entry review and continuing maintenance; not needed for this pass.

## Components and navigation

### `PluginsPage`

- Extend tab state to `installed | marketplace | custom`.
- Keep plugin cards and marketplace-install completion notices under **Installed**.
- Move the existing git-install and plugin-directory sections together into a focused `CustomPluginsSection` component without changing their business rules.
- When an external installed card chooses **Install from git**, prefill the URL, switch to **Custom**, then scroll to and focus the git URL field after the panel renders.
- Keep accessible tab/tabpanel relationships and preserve loading, session, and admin guards.

### `MarketplaceSection`

- Put search and filters first, followed by entries. Remove the large sources section from the page flow.
- Open a controlled Radix **Manage sources** dialog containing the existing source list and add/refresh/remove controls. The dialog is scrollable on short screens and uses the existing overlay behavior.
- Entry cards use a two-column desktop grid and one column on narrow screens. Each has an icon, name, version/source metadata, trust badge, description, category, and the existing Install / Installed / Update / unavailable state.
- Filtering remains client-side over the entries returned by the existing API; no API pagination or query changes are introduced.
- Icons are decorative (`aria-hidden`) because the adjacent plugin name supplies the accessible label.

### `MarketplaceSourcesDialog`

- List Coppice and external sources with their current refresh status and trust label.
- Adding a source registers it and immediately refreshes it, so the user does not need a separate first **Refresh** action.
- Refresh and removal keep their existing API semantics. The built-in Coppice source cannot be removed; the seeded Anthropic source is a normal removable git source.

## Default-source lifecycle and data flow

1. A one-time migration inserts `claude-plugins-official` as a git marketplace source pointing at the verified Anthropic repository. If that name already exists, the migration preserves the existing row rather than overwriting it.
2. Because insertion happens only in the migration, removing the default source is permanent across restarts. Existing source-removal behavior continues to keep installed plugins while clearing their update provenance.
3. When **Marketplace** opens, each source with `refreshedAt === null` is refreshed through the existing refresh endpoint. Attempt each source at most once per page mount so query invalidation or a failure does not create a refresh loop.
4. A successful refresh invalidates sources and entries through the existing TanStack Query flow. The embedded Coppice catalog remains visible while external sources load.
5. Manual **Retry** / **Refresh** remains available in the dialog. No network work is added to server startup and no default-source identifier is hardcoded in the web client.
6. Adding a custom source follows the same register-then-refresh flow. A shared per-mount attempt guard covers automatic and add-triggered first fetches so they cannot duplicate a request. If registration succeeds but fetching fails, the source stays registered and the dialog explains that distinction.

The existing entry API and schemas remain unchanged. Unsupported entry kinds continue to use their existing `installable: false` behavior; broader compatibility filtering is out of scope.

## Loading, errors, and empty states

- Initial marketplace loading keeps the existing page-level loading state.
- First-fetch progress is non-blocking and identifies that external catalogs are being loaded.
- A failed source does not hide entries from healthy sources. Show a concise warning with access to **Manage sources**, where the detailed source error and Retry action live.
- A failed custom-source fetch after successful registration says the source was added but its catalog could not be fetched.
- Empty-filter results retain the existing message. **Load more** appears only when additional matching entries exist.
- External documentation links open in a new tab with `rel="noreferrer"`.

## Documentation

- Update `docs/plugins.md` and the English website plugin guide for the three tabs, the sources dialog, first-fetch behavior, and the bundled Anthropic source.
- Apply corresponding navigation/source corrections to localized website plugin pages without unrelated content changes.
- The website remains the authoritative explanation of what plugins are, how to install them, and their required folder/manifest format.

## Testing and verification

### Web

- Three-tab separation and installed cards from all origins.
- External-card **Install from git** switches to **Custom**, prefills, and focuses the form.
- Header and Custom documentation links; the old inline guide is absent.
- Source-dialog open/add/refresh/remove flows, including add-success/fetch-failure.
- Automatic first fetch occurs once per never-fetched source and is not triggered for fetched sources.
- Entry icons, generic fallback, 24-entry increments, and reset after every filter change.
- Healthy entries remain usable when another source fails.

### Server

- Migration seeds the expected Anthropic URL/ref exactly once and preserves a pre-existing source with the same name.
- Removing the seeded source is not undone by built-in-source startup seeding.
- Existing marketplace install/update/source tests remain green.

### Manual and CI

- Verify the real Anthropic catalog in the default Docker Compose stack; automated tests do not use live network access.
- Check desktop and narrow layouts, keyboard/dialog navigation, loading, empty, and source-failure states.
- Run `make web-test`, targeted server tests during development, and final `make test` plus `cargo clippy --workspace -- -D warnings`.

## Out of scope

- Additional default sources or a hosted registry.
- GitHub search, server-side catalog search, or API pagination.
- Changes to plugin format support, MCP proxying, enablement, credentials, or agent assignment.
- A redesign of installed plugin-card internals.
