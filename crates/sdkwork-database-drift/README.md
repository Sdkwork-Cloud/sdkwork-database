# sdkwork-database-drift

Domain: database
Capability: schema-drift-detection
Package type: Rust validation/tooling crate
Status: active

## Public API

The crate exports `DriftEngine`, `DriftReport`, `DriftSummary`, and `DriftDiff`.
It compares portable SDKWork database contracts with live SQLite or PostgreSQL
schema metadata.

## Predicate Matching

Partial-index predicates are tokenized without changing quoted literal content.
The matcher removes redundant grouping and recognizes database-rendered forms
that select the same rows, including PostgreSQL `ANY (ARRAY[...])` output for a
contract `IN (...)` predicate. Unsupported SQL remains comparable as a stable
token sequence and is never accepted through a broad text replacement.

## Security And Operations

The detector is read-only with respect to application schemas. Drift ignore
policy must not be used to hide parser or introspection defects. Reports must
mask connection secrets through the owning database configuration layer.

## Verification

```powershell
cargo test -p sdkwork-database-drift
cargo clippy -p sdkwork-database-drift --all-targets -- -D warnings
```
