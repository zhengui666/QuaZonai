# Third-Party Notices

QuaZonai depends on third-party packages. Their source code is not vendored or relicensed by this repository. Each package remains available under its upstream license.

## Authoritative dependency inputs

Use these committed manifests to produce a release-specific license inventory and SBOM:

- `backend/pyproject.toml`
- `frontend/package.json` and `frontend/package-lock.json`

The current repository does not commit a backend `uv.lock`; do not treat a nonexistent lockfile as release evidence.

## Direct dependency inventory

| Component | Direct dependencies |
| --- | --- |
| Backend runtime | Alembic, cryptography, FastAPI, HTTPX, idna, packaging, psycopg, Pydantic, PyOTP, python-multipart, SQLAlchemy, uv, Uvicorn |
| Backend optional research/runtime groups | Optuna, PyArrow, openai-codex, MCP, PyJWT |
| Frontend | React, React DOM, React Router DOM, Radix Themes, TanStack Query/Table/Virtual, XYFlow React, Phosphor Icons, ECharts, Lightweight Charts, Geist fonts |
| Development and test tooling | Ruff, mypy, pytest, Playwright, Vitest, ESLint, TypeScript, Vite, Testing Library, jsdom |

## Release requirement

Before publishing a GHCR image or a release, generate and archive an exact dependency-license report and SBOM from the dependency graph resolved by the release build. Review package license texts and notices for that resolved graph; this document is an entry point, not a substitute for the upstream license terms.
