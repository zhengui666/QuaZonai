# Third-Party Notices

QuaZonai depends on third-party packages. Their source code is not vendored or relicensed by this repository. Each package remains available under its upstream license.

## Authoritative dependency inputs

Use these committed manifests and lockfiles to produce a release-specific license inventory and SBOM:

- `backend/pyproject.toml` and `backend/uv.lock`
- `frontend/package.json` and `frontend/pnpm-lock.yaml`

## Direct dependency inventory

| Component | Direct dependencies |
| --- | --- |
| Backend | FastAPI, Uvicorn, SQLAlchemy, Pydantic, Alembic, psycopg, PyYAML, jsonschema, DuckDB, PyArrow, HTTPX, cryptography, LangGraph, LangGraph PostgreSQL checkpoint, LangGraph SQLite checkpoint |
| Frontend | React, React DOM, TanStack Query, TanStack Router, Radix Dialog, ECharts, i18next, react-i18next, Zod, Vite, TypeScript, openapi-typescript |
| Development and test tooling | Ruff, mypy, pytest, Playwright, Vitest, Storybook, ESLint, Prettier, Testing Library, axe-core, Tailwind CSS, MSW, js-yaml |

## Release requirement

Before publishing a GHCR image or a release, generate and archive an exact dependency-license report and SBOM from the committed lockfiles. Review package license texts and notices for that resolved graph; this document is an entry point, not a substitute for the upstream license terms.
