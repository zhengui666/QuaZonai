# Open-source research and specification

Research date: 2026-09-17. Scope: first-use documentation, contribution flow, code navigation and maintainable delivery. Sources below are upstream repositories and their own documentation, not rankings or third-party summaries. Upstream pages evolve; these observations describe the pages inspected on this date. None of their performance, maturity, platform support or trading capabilities is attributed to QuaZonai.

## Comparative research

### uv: explain the tool before explaining the repository

The [README](https://github.com/astral-sh/uv#readme) starts with a precise purpose, highlights, installation and short examples, then routes readers to documentation and contribution. The [contribution guide](https://github.com/astral-sh/uv/blob/main/CONTRIBUTING.md) separates finding work, environment setup, tests, generated files and crate structure. This lets users evaluate usefulness before learning implementation details, while contributors can find exact checks.

**Apply:** QuaZonai's opening should explain evidence-backed research and target-only delivery, show an existing real screenshot, and offer the credential-free preview before a Rust toolchain setup. Move contributor setup and check selection to CONTRIBUTING; link rather than repeat it in every document.

**Do not copy:** uv's speed claims, installation binaries, platform support or large maintenance toolchain. QuaZonai has no comparable benchmark or binary release acceptance. The existing Make, Cargo, npm and lychee checks already cover the needed entrypoints.

### Polars: separate learning, reference and contribution

The [README](https://github.com/pola-rs/polars#readme) connects the product description to examples, language bindings and documentation. The [contribution guide](https://docs.pola.rs/development/contributing/) distinguishes bug reporting, accepted work, environment setup, testing, documentation and release flow. It also discloses specialized setup requirements rather than assuming every test runs in a minimal environment.

**Apply:** route readers by task: preview, real setup, command/API reference, architecture, contribution and acceptance evidence. Each check entry must state prerequisites and what it cannot prove; database and native-runtime tests must remain recognizable as separate environments.

**Do not copy:** the multi-language installation matrix, cloud product navigation or broad documentation website. QuaZonai's current audience can use a compact README plus its existing authoritative Markdown documents. A second API reference would drift from generated Rust contracts.

### NautilusTrader: make correctness and scope visible

The [README](https://github.com/nautechsystems/nautilus_trader#readme) exposes architecture, installation, examples and support boundaries. Its [contribution guide](https://github.com/nautechsystems/nautilus_trader/blob/develop/CONTRIBUTING.md) asks contributors to coordinate substantial work, check for competing implementations, validate changes locally and take responsibility for AI-assisted submissions. Generated artifacts are updated through their generator.

**Apply:** use real source locations, check active work before edits, explain target-only ownership, keep generated OpenAPI/TypeScript downstream of Rust, and make submitters responsible for evidence and readable changes. Link the existing compatibility matrix instead of declaring an upstream RC stable.

**Do not copy:** broker execution responsibilities, CLA, upstream release policy, or its larger language/hook toolchain. QuaZonai preserves its existing AGPL license and does not gain trading authority by reusing a backtest engine. Repository cleanup does not authorize deleting user data.

### Temporal: distinguish a quick try from a full environment

The [README](https://github.com/temporalio/temporal#readme) gives a short local entry before deeper samples and server-development material. The [contribution guide](https://github.com/temporalio/temporal/blob/main/CONTRIBUTING.md) distinguishes unit, integration and functional tests, along with their runtime dependencies and shutdown procedures.

**Apply:** keep the preview's expected URL, supported interaction, in-memory lifetime and Ctrl+C shutdown next to its command. Map checks to changed boundaries in CONTRIBUTING; reuse existing database/OCI/browser CI instead of calling a unit run complete acceptance.

**Do not copy:** Temporal's workflow engine, distributed service topology, or development database mode. QuaZonai already uses PostgreSQL/PGMQ and explicit domain transitions. A synthetic browser preview must not become a shortcut around production evidence or authorization.

### Zed: make source navigation and platform limits discoverable

The [README](https://github.com/zed-industries/zed#readme) is concise and routes users toward downloads, documentation and development. Its [development documentation](https://zed.dev/docs/development) separates platform setup, debugging and contribution topics rather than embedding every build detail in the front page.

**Apply:** provide a source map and one concrete request trace from browser/CLI through server/domain/Store to native computation and result publication. State the actual backend platform and give contributors a narrow first validation target.

**Do not copy:** unsupported native platform instructions, editor-extension infrastructure, download badges or large-team processes. The current project owner and GitHub workflow remain the maintenance mechanism.

## Decisions for QuaZonai

| Reader need | Deliverable | Observable acceptance |
| --- | --- | --- |
| Understand the project without historical context | Chinese README with short English status, target-only purpose, real screenshot and quickstart | A fresh reader can identify purpose, preview command and current limits; screenshot provenance stays explicit |
| Make a first contribution | CONTRIBUTING with setup, code conventions, change/check mapping, support and PR process | Commands exist; readers can choose checks without needing production credentials |
| Find the owner of behavior | Corrected DESIGN map and `docs/architecture.md` navigation | Every linked source exists; actual Cargo graph respects the declared direct workspace dependencies |
| Maintain one workflow | `.opensdlc/project.md`, `review.md`, `operations.md`, task record and eval suite | Stable anchors and links pass; existing native configuration remains in place; no copied domain state machine |
| Keep code conventions enforceable | Rust architecture regression and existing format/type/build checks | Actual metadata passes; a deliberately forbidden dependency fails; Rust EditorConfig agrees with rustfmt |
| Accept and transfer this work | GitHub PR, exact-Head CI and explicit clean Codex review | All applicable runs succeed, review threads are resolved, merge uses expected Head and main is reread |

DESIGN remains the product authority. New workflow prose uses English under the OpenSDLC default; existing Chinese documents retain Chinese. New contributor documentation can be read independently, but links detailed contracts rather than translating a second copy. Review policy distinguishes ordinary maintenance from product release, and records the observed lack of branch protection without claiming native enforcement exists.

## Architecture scope

The seven existing Rust packages already separate wire contracts, pure domain rules, persistence, native integration and executable entrypoints. Keep these boundaries. Correct the obsolete DESIGN tree (`frontend/`, hypothetical `deploy/` and documentation subtrees), document current ownership, and guard direct workspace dependencies using native Cargo metadata. The check covers normal and build dependencies; test-only helpers may cross these boundaries and remain subject to behavior review. It is an architectural regression check, not a sandbox or a transitive dependency scanner.

No product protocol, database migration, model, authorization rule or trading behavior changes. Remove no user data or other worktree content. No gratuitous service split, README-only feature claim, duplicate roadmap, new license or prefilled release/incident report is needed.

## Verification and failure actions

- `make check-architecture`: inspect Cargo's actual package metadata; reject forbidden dependency direction and unnamed workspace packages. Fix the actual ownership mistake, or revise DESIGN with the owner's architectural decision before changing the check.
- `make check-docs`: detect broken local file/heading links and invalid CLI help; fix the source link or command, without excluding failures.
- `make check-web` and the preview browser scenario: detect generated drift, type/behavior/build failures and a broken quickstart. Fix the original source, regenerate and rerun.
- Existing full GitHub workflows: preserve Rust, PostgreSQL/PGMQ, OCI, browser and CodeQL checks. A failed or missing run blocks this task's merge; investigate and retry only where justified.
- Fresh-context agent cases and independent Codex review: distinguish navigation correctness from actual model behavior. Findings return to the author. No hidden reasoning, credentials or fabricated approvals enter the records.
