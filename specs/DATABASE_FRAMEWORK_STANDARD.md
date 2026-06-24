# SDKWork Database Framework Standard (L1)

- Version: 1.0
- Scope: executable profile for `sdkwork-database` lifecycle SPI, orchestrator, drift engine, CLI, and validation tooling
- Parent standard: `../../sdkwork-specs/DATABASE_FRAMEWORK_SPEC.md`
- Related: `../../sdkwork-specs/DATABASE_SPEC.md`, `../../sdkwork-specs/PNPM_SCRIPT_SPEC.md`, `../../sdkwork-specs/ENVIRONMENT_SPEC.md`

This document is the L1 executable profile for the database lifecycle framework.

## 1. Crate Map

| Crate | Status | Responsibility |
| --- | --- | --- |
| `sdkwork-database-spi` | active v1 | SPI traits, manifest parsing, `DefaultDatabaseModule`, registry |
| `sdkwork-database-history` | active v1 | Ops history tables, checksum queries, migration/seed recording |
| `sdkwork-database-contract` | active v1 | `schema.yaml` parsing: profiles/field_sets, constraints, indexes, expected tables |
| `sdkwork-database-lifecycle` | active v1 | Migrate/seed/bootstrap orchestration (uses history crate) |
| `sdkwork-database-drift` | active v1 | Drift reports: migrations, checksum, table/column/type/nullability/index/constraint diffs |
| `sdkwork-database-ops` | active v1 | Ops read models; drift report caching with `refresh` flag |
| `sdkwork-database-ops-http` | active v1 | Axum router for ops database endpoints; requires `DatabaseOpsAuth` |
| `sdkwork-database-cli` | active v1 | `sdkwork-db` binary |
| `sdkwork-database-config` | active | Database env/TOML config |
| `sdkwork-database-sqlx` | active | Connection pool |
| `sdkwork-database-repository` | active | Repository layer (legacy inline migration macro only) |

Ops HTTP auth:

- Applications `MUST` pass a `DatabaseOpsAuth` implementation to `DatabaseOpsHttpState`.
- Reference helper: `BearerTokenOpsAuth::from_env("SDKWORK_ACCESS_TOKEN")`.
- Unauthenticated requests `MUST` receive `401 Unauthorized`.

## 2. CLI

Binary: `sdkwork-db`

```bash
cargo run -p sdkwork-database-cli -- --app-root . validate
cargo run -p sdkwork-database-cli -- --app-root . init
cargo run -p sdkwork-database-cli -- --app-root . plan
cargo run -p sdkwork-database-cli -- --app-root . migrate
cargo run -p sdkwork-database-cli -- --app-root . seed --locale zh-CN --profile standard
cargo run -p sdkwork-database-cli -- --app-root . bootstrap --locale zh-CN --profile standard
cargo run -p sdkwork-database-cli -- --app-root . status
cargo run -p sdkwork-database-cli -- --app-root . drift
cargo run -p sdkwork-database-cli -- --app-root . drift-check
```

Requires `SDKWORK_{SERVICE}_DATABASE_URL` or manifest `serviceCode` via `-s`.

Recommended application `package.json` wiring:

```json
{
  "scripts": {
    "db:validate": "node ../sdkwork-specs/tools/check-database-framework-standard.mjs --root .",
    "db:plan": "cargo run --manifest-path ../sdkwork-database/Cargo.toml -p sdkwork-database-cli -- --app-root . plan",
    "db:init": "cargo run --manifest-path ../sdkwork-database/Cargo.toml -p sdkwork-database-cli -- --app-root . init",
    "db:migrate": "cargo run --manifest-path ../sdkwork-database/Cargo.toml -p sdkwork-database-cli -- --app-root . migrate",
    "db:seed": "cargo run --manifest-path ../sdkwork-database/Cargo.toml -p sdkwork-database-cli -- --app-root . seed",
    "db:status": "cargo run --manifest-path ../sdkwork-database/Cargo.toml -p sdkwork-database-cli -- --app-root . status",
    "db:drift": "cargo run --manifest-path ../sdkwork-database/Cargo.toml -p sdkwork-database-cli -- --app-root . drift",
    "db:drift:check": "cargo run --manifest-path ../sdkwork-database/Cargo.toml -p sdkwork-database-cli -- --app-root . drift-check"
  }
}
```

Layout validation for copied templates:

```bash
node ../sdkwork-specs/tools/check-database-framework-standard.mjs --layout module --root database
```

## 3. History Tables

| Table | Purpose |
| --- | --- |
| `ops_schema_migration_history` | Applied migration version/checksum |
| `ops_seed_history` | Applied seed id/locale/profile/checksum |
| `ops_database_installation_state` | Module install summary |

Created automatically by `LifecycleOrchestrator::init()` or before migrate/seed.

## 4. SPI Bootstrap

```rust
use std::sync::Arc;

use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::DefaultDatabaseModule;
use sdkwork_database_spi::{LocaleTag, SeedProfile};

let module = Arc::new(DefaultDatabaseModule::from_app_root(".")?);
let orchestrator = LifecycleOrchestrator::new(pool, module);
orchestrator.migrate().await?;
orchestrator.seed(&LocaleTag::zh_cn(), &SeedProfile::standard()).await?;
```

## 5. Verification

Framework repository:

```bash
cargo test -p sdkwork-database-spi
cargo test -p sdkwork-database-lifecycle --test migrate_seed_smoke
cargo test -p sdkwork-database-drift --test drift_smoke
cargo test -p sdkwork-database-drift --test constraint_drift
cargo test -p sdkwork-database-drift --test nullability_drift
cargo test -p sdkwork-database-ops --test ops_smoke
cargo test -p sdkwork-database-lifecycle --test registry_orchestrator
cargo test -p sdkwork-database-lifecycle --test checksum_immutability
cargo test -p sdkwork-database-contract --test forum_schema
cargo fmt --all --check
```

Standards repository:

```bash
node tools/check-database-framework-standard.test.mjs
node tools/check-database-framework-standard.mjs --layout module --root templates/database
```

## 6. Roadmap

1. Down migration rollback execution in lifecycle CLI
2. Java lifecycle adapter
3. IAM-integrated ops auth (private bootstrap bearer interim via unified `SDKWORK_ACCESS_TOKEN`)
