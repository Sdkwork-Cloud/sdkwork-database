# MIG-2026-0715 Snowflake Node Lease Fencing

```yaml
id: MIG-2026-0715
owner: sdkwork-platform
status: completed
type: database-and-runtime
scope:
  producers:
    - sdkwork-database-id
    - sdkwork-id-core
  consumers:
    - sdkwork-im-runtime-id
    - sdkwork-notary-embedded-bootstrap
    - sdkwork-account-repository-sqlx
    - sdkwork-iam-database-host
    - sdkwork-intelligence-memory-repository-sqlx
compatibility_window:
  starts_at: 2026-07-15
  ends_at: 2026-10-15
strategy: expand-contract
rollback:
  supported: true
  steps:
    - Stop new binaries before reverting runtime code.
    - Keep BIGINT columns and lease columns; the widening change is forward-safe.
    - Re-enable the previous allocator only for an isolated maintenance window.
rollout:
  - Drain old binaries before enabling the fencing-aware allocator; old binaries do not write lease tokens.
  - Grant the migration role `ALTER` on the registry table, then deploy the new binaries.
verification:
  - cargo test -p sdkwork-id-core
  - cargo test -p sdkwork-database-id
  - cargo test -p sdkwork-im-runtime-id --test runtime_id_contract_test
  - cargo check -p sdkwork-notary-embedded-bootstrap
```

The registry is a shared coordination table, not an ID-generation hot path.
PostgreSQL timestamp, process, and lease-version fields are `BIGINT`; old
registries are widened in place only when metadata inspection detects the old
shape. New leases carry a random token and monotonically increasing fencing
version. Active leases are never reclaimed by identity matching. PostgreSQL
allocation, expiry comparison, takeover, and renewal use `clock_timestamp()`
from the registry database rather than clocks from individual application
hosts, preventing clock skew from creating overlapping ownership. The
`lease_token` compatibility default is removed after expansion so an old
allocator cannot silently create an unfenced empty-token lease.

One operating-system process uses one generator and one lease through
`allocate_process_generator`. Separate processes or Pods still require distinct
nodes from the same authoritative registry. The process cache compares a
non-secret authority fingerprint and fails closed when embedded modules point
at different registry databases. A lease-aware generator rejects IDs at both
wall-clock and monotonic local deadlines and after a fenced heartbeat failure.
Leased generators reject historical timestamp overrides. Small clock rollback
is pinned to the last logical millisecond and advances the sequence; rollback
beyond the configured tolerance fails closed.

Production, staging, unknown lifecycle values, and explicit
cloud/server/container deployment targets fail closed instead of falling back
to node 0. A local unsafe-fallback override is considered only when lifecycle
and deployment settings are absent, and it cannot override an explicit
production-like lifecycle. Release builds with no lifecycle configuration also
fail closed; development and test profiles retain the explicit legacy fallback.
