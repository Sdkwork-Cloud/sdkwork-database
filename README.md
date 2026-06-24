# sdkwork-database

Unified database connection pool management library for all SDKWork applications.

## Overview

`sdkwork-database` provides standardized database connection pool configuration and management for SDKWork services. It supports:

- **SQLite** and **PostgreSQL** databases via sqlx
- **Standalone mode**: each service has its own database
- **Integrated mode**: all services share one database with table prefixes
- **Unified configuration**: environment variables and TOML files
- **Repository pattern**: unified CRUD operations
- **Health checks**: database connection monitoring
- **Migrations**: database schema management
- **Best practices**: WAL mode, busy_timeout, foreign_keys for SQLite

## Crates

| Crate | Description |
|-------|-------------|
| [`sdkwork-database-config`](crates/sdkwork-database-config) | Configuration types and parsing |
| [`sdkwork-database-sqlx`](crates/sdkwork-database-sqlx) | sqlx-based connection pool implementation |
| [`sdkwork-database-repository`](crates/sdkwork-database-repository) | Repository pattern abstraction |
| [`sdkwork-database-spi`](crates/sdkwork-database-spi) | Database lifecycle SPI traits, manifest parsing, and module registry |
| [`sdkwork-database-lifecycle`](crates/sdkwork-database-lifecycle) | Migrate/seed orchestration and ops history tables |
| [`sdkwork-database-drift`](crates/sdkwork-database-drift) | Schema drift reports and introspection |
| [`sdkwork-database-cli`](crates/sdkwork-database-cli) | `sdkwork-db` lifecycle CLI |

## Database Lifecycle Standard

Application database bootstrap, migration, seed, and drift standards are defined in:

- L0: `../sdkwork-specs/DATABASE_FRAMEWORK_SPEC.md`
- L1: [`specs/DATABASE_FRAMEWORK_STANDARD.md`](specs/DATABASE_FRAMEWORK_STANDARD.md)

Application roots `MUST` place lifecycle assets under `database/` and register a `DatabaseModule` from `sdkwork-database-spi` at bootstrap time.

## Quick Start

### Add Dependency

```toml
[dependencies]
sdkwork-database-config = { path = "../sdkwork-database/crates/sdkwork-database-config" }
sdkwork-database-sqlx = { path = "../sdkwork-database/crates/sdkwork-database-sqlx" }
sdkwork-database-repository = { path = "../sdkwork-database/crates/sdkwork-database-repository" }
```

### Define Entity

```rust
use sdkwork_database_repository::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: i64,
    name: String,
    email: String,
    status: String,
}

impl_entity!(User, "users", id, [id, name, email, status]);
```

### Create Repository

```rust
impl_repository!(User);

// UserRepository is now available with all CRUD methods
let repo = UserRepository::new(pool);

// Insert
let user = User { id: 1, name: "Alice".into(), email: "alice@example.com".into(), status: "active".into() };
repo.insert(&user).await?;

// Find
let found = repo.find_by_id(1).await?;

// Query
let query = Query::eq("status", serde_json::Value::String("active".into()));
let active_users = repo.find_all(&query).await?;

// Paginate
let (users, total) = repo.find_paginated(&query, 1, 10).await?;

// Update
user.name = "Alice Updated".into();
repo.update(&user).await?;

// Delete
repo.delete(1).await?;
```

### Health Check

```rust
use sdkwork_database_repository::health::HealthChecker;

let checker = HealthChecker::new(pool);
let result = checker.check().await?;

if result.status == HealthStatus::Healthy {
    println!("Database is healthy");
}
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `SDKWORK_{SERVICE}_DATABASE_URL` | Database connection URL | Required |
| `SDKWORK_{SERVICE}_DATABASE_ENGINE` | Database engine (sqlite/postgres) | Auto-detect |
| `SDKWORK_{SERVICE}_DATABASE_MODE` | Deployment mode (standalone/integrated) | standalone |
| `SDKWORK_{SERVICE}_DATABASE_TABLE_PREFIX` | Table prefix for integrated mode | `{service}_` |
| `SDKWORK_{SERVICE}_DATABASE_MAX_CONNECTIONS` | Maximum connections | 16 |
| `SDKWORK_{SERVICE}_DATABASE_MIN_CONNECTIONS` | Minimum connections | 1 |
| `SDKWORK_{SERVICE}_DATABASE_ACQUIRE_TIMEOUT` | Acquire timeout (seconds) | 10 |
| `SDKWORK_{SERVICE}_DATABASE_IDLE_TIMEOUT` | Idle timeout (seconds) | 300 |
| `SDKWORK_{SERVICE}_DATABASE_MAX_LIFETIME` | Max lifetime (seconds) | 1800 |

### TOML Configuration

```toml
[database]
engine = "sqlite"
url = "sqlite:data/app.db"
mode = "standalone"
max_connections = 8

[database.sqlite]
journal_mode = "wal"
busy_timeout_secs = 5
foreign_keys = true

[database.postgres]
ssl_mode = "prefer"
application_name = "my-service"
```

## Modules

### Entity

Define database entities with the `impl_entity!` macro:

```rust
impl_entity!(User, "users", id, [id, name, email]);
```

### Repository

Generate CRUD operations with the `impl_repository!` macro:

```rust
impl_repository!(User);
// Generates UserRepository with insert, find_by_id, find_all, update, delete, count, etc.
```

### Query

Build queries with the Query builder:

```rust
let query = Query::new()
    .and_eq("status", Value::String("active".into()))
    .gt("age", Value::Number(18.into()))
    .order_by("created_at", false)
    .limit(10);
```

### Types

Common types for database operations:

| Type | Description |
|------|-------------|
| `AutoTimestamp` | Automatic created_at/updated_at timestamps |
| `SoftDelete` | Soft deletion with deleted_at/deleted_by |
| `Versioned` | Optimistic locking with version field |
| `Pagination` | Pagination parameters |
| `PaginatedResponse` | Paginated response with metadata |
| `QueryFilter` | Query filter conditions |

### Health

Database health checking:

```rust
let checker = HealthChecker::new(pool);
let result = checker.check().await?;
```

### Migration (legacy)

The repository-layer `MigrationManager` has been **removed**. Application roots MUST use the
`database/` module layout, `LifecycleOrchestrator`, and `sdkwork-db` CLI. The
`define_migrations!` macro remains only for legacy inline examples. See
`specs/DATABASE_FRAMEWORK_STANDARD.md`.

```rust
use sdkwork_database_lifecycle::LifecycleOrchestrator;
use sdkwork_database_spi::DefaultDatabaseModule;

let module = Arc::new(DefaultDatabaseModule::from_app_root(".")?);
let orchestrator = LifecycleOrchestrator::new(pool, module);
orchestrator.migrate().await?;
```

## Building and Testing

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace

# Check formatting
cargo fmt --all --check

# Run linter
cargo clippy --workspace --tests -- -D warnings
```

## License

MIT OR Apache-2.0

## Documentation Canon

- [docs/README.md](docs/README.md)
- [docs/product/prd/PRD.md](docs/product/prd/PRD.md)
- [docs/architecture/tech/TECH_ARCHITECTURE.md](docs/architecture/tech/TECH_ARCHITECTURE.md)

