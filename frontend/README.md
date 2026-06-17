# StonksCollect — frontend

React 19 + Vite 8 + TypeScript SPA (MUI v9, dark-first; ECharts; Vitest +
Playwright) for the StonksCollect fundamental-analysis dashboard.

- **What it does / where things live:** [`src/README.md`](src/README.md) (module
  map) and [`/FEATURES.md`](../FEATURES.md) (full-stack feature catalog).
- **Project overview & setup:** root [`/README.md`](../README.md).
- **Frontend feature details:** [`/docs/features.md`](../docs/features.md).
- **Conventions (TDD, 100% coverage, MUI rules):** [`/CLAUDE.md`](../CLAUDE.md),
  [`/AGENTS.md`](../AGENTS.md), [`/.cursorrules`](../.cursorrules).

## Develop

```bash
npm install
npm run dev        # Vite dev server (proxies /api and /auth to VITE_API_TARGET)
npm run test:run   # Vitest
npm run coverage   # Vitest + coverage (gate: 100%, src/charts/ excluded)
npm run build      # tsc -b + vite build
npm run e2e        # Playwright
```

The dev proxy target is `VITE_API_TARGET` (`frontend/.env`, see `.env.example`).
