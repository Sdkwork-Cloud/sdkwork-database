# sdkwork-database-lifecycle Specs

Machine contract: `component.spec.json`.

This crate owns lifecycle orchestration over an injected `DatabasePool`. It uses
history ports for durable state and the SQLx pool schema identity for PostgreSQL
introspection; it does not re-resolve database identity from module environment.
`init()` inserts only a missing installation state and never downgrades
`schema_current` or `seeded` state established by later lifecycle phases.

The ignored `postgres_restart_state` integration test requires
`SDKWORK_DATABASE_TEST_POSTGRES_URL` with schema create/drop permission. It
covers concurrent init, baseline anchor authority, migrate/seed state, manifest
version changes, and a reconstructed pool/orchestrator restart.
