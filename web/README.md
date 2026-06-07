# Coppice Web

React / Vite SPA for the Coppice agent workspace (M02).

## Stack

- React 19 + TypeScript + Vite
- Tailwind CSS (theme wired to `src/styles/tokens.css`)
- React Query, React Router, dnd-kit, React Hook Form + Zod, react-markdown

## Design

See [DESIGN.md](./DESIGN.md) for the coppice-forest aesthetic direction. Design tokens live in `src/styles/tokens.css`.

## Development

```bash
npm install
npm run dev      # http://localhost:5173 — proxies /api → server
npm run build
npm test
```

Set `VITE_API_URL` to override the API proxy target (default `http://localhost:8080`).
