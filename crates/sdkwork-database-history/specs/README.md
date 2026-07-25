# sdkwork-database-history Specs

Machine contract: `component.spec.json`.

This crate owns database lifecycle history records, installation-state persistence,
and database-backed lifecycle locks. Lifecycle orchestration consumes these ports;
the history crate does not decide lifecycle transitions.
