# Coppice Web

React / Vite SPA for the Coppice agent workspace (M02).

## Stack

- React 19 + TypeScript + Vite
- Tailwind CSS (theme wired to `src/styles/tokens.css`)
- React Query, React Router, dnd-kit, React Hook Form + Zod, react-markdown

## Design

See [docs/web/DESIGN.md](../docs/web/DESIGN.md) for the coppice-forest aesthetic direction. Design tokens live in `src/styles/tokens.css`.

## Development

```bash
yarn install
yarn dev      # http://localhost:5001 — proxies /api → server
yarn build
yarn test
```

Or from the repo root: `make web-dev` (installs deps, then starts Vite against the host API on `:5000`).

Set `VITE_API_URL` to override the API proxy target (default `http://localhost:5000`).
