# sdkwork-database-config Specs

Machine contract: `component.spec.json`.

This crate owns canonical `SDKWORK_DATABASE_*` parsing, workspace PostgreSQL
identity validation, same-named schema normalization, and role-based client-local
SQLite selection. Application and module names identify ownership only; they do
not create database environment prefixes, database names, or schemas.

Resolution is role-based (ENVIRONMENT_SPEC §7.2): the server role
(`load_from_env`) resolves the workspace PostgreSQL profile and ignores
`SDKWORK_DATABASE_SQLITE_URL`; the client-local role
(`load_client_local_from_env`) resolves the SQLite URL exclusively and is not
vetoed by the server `SDKWORK_DATABASE_ENGINE` marker. Both roles coexist in one
process.
