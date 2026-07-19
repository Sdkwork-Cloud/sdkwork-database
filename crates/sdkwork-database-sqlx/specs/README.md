# sdkwork-database-sqlx Specs

Machine contract: `component.spec.json`.

The crate owns the canonical SQLx pool factory and the opt-in process-shared pool registry. Runtime processes enable the registry before database bootstrap; embedded modules reuse the installed handle and fail closed on identity or driver mismatch.
