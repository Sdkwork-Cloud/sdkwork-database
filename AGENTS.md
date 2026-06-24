# Repository Guidelines

<!-- SDKWORK-AGENTS-GENERATED: v1 -->

## SDKWORK Soul

Read `../sdkwork-specs/SOUL.md` before executing tasks in this root. Follow specs before memory, dictionary before context, stop on ambiguity, and evidence before completion.

## SDKWORK Standards

Canonical SDKWORK specs path from this root:

- `../sdkwork-specs/README.md`
- `../sdkwork-specs/SOUL.md`
- `../sdkwork-specs/AGENTS_SPEC.md`
- `../sdkwork-specs/CODE_STYLE_SPEC.md`
- `../sdkwork-specs/NAMING_SPEC.md`

Do not copy root standard text into this repository. If these relative paths do not resolve, stop and report the broken workspace layout.

## Project Purpose

`sdkwork-database` is the unified database framework for all SDKWork applications. It provides connection pool management, lifecycle SPI, and standardized bootstrap/migration/seed/drift orchestration.

**Key Design Goals:**
- Unified configuration format across all services
- Standard application `database/` lifecycle assets via `DATABASE_FRAMEWORK_SPEC.md`
- Lifecycle SPI in `sdkwork-database-spi` with `DefaultDatabaseModule`
- Support for both standalone and integrated deployment modes
- SQLite and PostgreSQL support via sqlx
- Table prefix mechanism for integrated deployments
- Environment variable and TOML configuration

## Local Dictionary Structure

- `AGENTS.md`: local agent entrypoint and relative SDKWORK spec index.
- `CLAUDE.md`: Claude Code compatibility shim that points to `AGENTS.md` and must not duplicate rules.
- `GEMINI.md`: Gemini CLI compatibility shim that points to `AGENTS.md` and must not duplicate rules.
- `CODEX.md`: Codex compatibility shim that points to `AGENTS.md` and must not duplicate rules.
- `.sdkwork/`: reserved local dictionary folder; create only for local skills, plugins, manifests, or AI workspace metadata.
- `crates/`: Rust crates:
  - `sdkwork-database-config`: configuration types and parsing
  - `sdkwork-database-sqlx`: sqlx-based connection pool implementation
- `configs/`: example configuration files
- `docs/`: documentation
- `tests/`: integration tests

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

## Spec Resolution Order

1. Read this `AGENTS.md` and any nearer component-level `AGENTS.md`.
2. Read local `.sdkwork/README.md`, `.sdkwork/skills/`, and `.sdkwork/plugins/` when relevant.
3. Read `../sdkwork-specs/README.md` and the task-specific root specs.
4. Inspect implementation files only after the relevant dictionary entries are clear.

## Required Specs By Task Type

- Agent/workflow changes: `../sdkwork-specs/SOUL.md`, `../sdkwork-specs/AGENTS_SPEC.md`, `../sdkwork-specs/SDKWORK_WORKSPACE_SPEC.md`.
- Any code change: `../sdkwork-specs/CODE_STYLE_SPEC.md`, `../sdkwork-specs/NAMING_SPEC.md`, plus only the touched language/framework spec.
- Rust code: `../sdkwork-specs/RUST_CODE_SPEC.md`.
- API, SDK, database, runtime, security, and deployment changes must follow the task matrix in `../sdkwork-specs/README.md`.

## Code Style Rules

Read `../sdkwork-specs/CODE_STYLE_SPEC.md` and `../sdkwork-specs/NAMING_SPEC.md` before code changes.

For Rust, keep `src/lib.rs` limited to module declarations, re-exports, light docs, and wiring; move handlers, services, repositories, DTOs, SQL, provider clients, and tests into focused modules.

## Build, Test, and Verification

Run commands from this directory unless a command explicitly targets another path.

- `cargo fmt --all --check`: verify Rust formatting across workspace crates.
- `cargo test --workspace`: run workspace Rust tests.
- `cargo clippy --workspace --tests -- -D warnings`: lint Rust tests and crates with warnings denied.
- `cargo build --workspace`: build all workspace crates.

Run the narrowest relevant check first, then broader verification when API contracts, SDK generation, persistence, security, or cross-package boundaries change.

## Agent Execution Rules

Use the convention dictionary instead of broad context loading. Keep changes scoped to the owning module, package, crate, or app root. Record the exact verification commands and important outputs before reporting completion.

## Human Review Rules

Request human review before breaking SDKWORK standards, changing public naming, altering security/auth behavior, changing database migrations or production deployment config, deleting data/files, or changing generated SDK ownership. Surface unresolved spec paths, app identity conflicts, component ownership conflicts, and API authority ambiguity instead of guessing.
