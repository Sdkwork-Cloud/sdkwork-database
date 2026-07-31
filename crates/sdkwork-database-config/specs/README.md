# sdkwork-database-config Specs

Machine contract: `component.spec.json`.

This crate owns canonical `SDKWORK_DATABASE_*` parsing, workspace PostgreSQL
identity validation, same-named schema normalization, and client-local SQLite
selection. Application and module names identify ownership only; they do not
create database environment prefixes, database names, or schemas.
