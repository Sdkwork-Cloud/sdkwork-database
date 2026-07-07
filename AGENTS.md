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

Build scripts, dev runners, and `pnpm clean` must follow `CODE_STYLE_SPEC.md` §7 (Build Source Integrity And Self-Healing). Git-tracked build-critical source files must be verified before builds and self-healed from git when missing; `clean` must not delete them.

## Build, Test, and Verification

Run commands from this directory unless a command explicitly targets another path.

- `cargo fmt --all --check`: verify Rust formatting across workspace crates.
- `cargo test --workspace`: run workspace Rust tests.
- `cargo clippy --workspace --tests -- -D warnings`: lint Rust tests and crates with warnings denied.
- `cargo build --workspace`: build all workspace crates.

Run the narrowest relevant check first, then broader verification when API contracts, SDK generation, persistence, security, or cross-package boundaries change.

## Agent Execution Rules

Use the convention dictionary instead of broad context loading. Keep changes scoped to the owning module, package, crate, or app root. Record the exact verification commands and important outputs before reporting completion.

## App SDK Consumer Imports



Application, feature, shell, and service packages `MUST` consume HTTP SDKs through scoped composed consumer packages, not generator transport package names.



- App API clients: `@sdkwork/<application-code>-app-sdk`

- Backend API clients (`backend-admin` only): `@sdkwork/<application-code>-backend-sdk`

- Federated Claw Router domain surfaces: `@sdkwork/clawrouter-app-sdk/domains` and `@sdkwork/clawrouter-backend-sdk/domains`

- Open/domain API clients: `@sdkwork/<domain>-sdk`



Canonical examples (IAM):



```typescript

import { createClient, type SdkworkAppClient } from '@sdkwork/iam-app-sdk';

import type { SdkworkBackendClient } from '@sdkwork/iam-backend-sdk'; // backend-admin only

import { createClient as createClawRouterDomainsClient } from '@sdkwork/clawrouter-app-sdk/domains';

```



Forbidden in application `apps/`, `packages/`, bootstrap, services, UI, contract tests, and composed SDK `src/**` outside generator ownership:



- `sdkwork-*-app-sdk-generated-typescript`, `sdkwork-*-backend-sdk-generated-typescript`, and other generator transport names as consumer imports

- `@sdkwork/commerce-app-sdk`, `@sdkwork/commerce-backend-sdk`, `@sdkwork/clawrouter-*-domain-transport-sdk`

- filesystem paths containing `domain-transport-typescript`, `domain-transport-sdk`, or sibling `*-typescript/generated` hops from composed `src/**`

- deep imports into `generated/server-openapi/src/*` from consumers when a composed facade exists



Allowed:



- Composed facade entry imports such as `@sdkwork/iam-app-sdk`, `@sdkwork/knowledgebase-app-sdk`, and `@sdkwork/clawrouter-app-sdk/domains`

- Composed re-exports that import only from `../generated/**` within the same `*-sdk-typescript` family root

- Generated transport ownership inside `sdks/**/generated/**` only



Each SDK family `MUST` expose the composed TypeScript facade at `sdks/<sdk-family>/<sdk-family>-typescript/src/index.ts` (and optional subpath exports such as `./domains`) with `package.json#name` equal to the scoped consumer package.



Before completing SDK integration or frontend service work, run:



```bash

node <sdkwork-specs>/tools/check-app-sdk-consumer-imports.mjs --workspace <workspace-root>

```



Authority: `APP_SDK_INTEGRATION_SPEC.md` section 9, `SDK_SPEC.md` package naming table, `SDK_WORKSPACE_GENERATION_SPEC.md` composed facade rules.

## HTTP API Response Envelope

All L2+ SDKWork-owned custom HTTP contracts, including `app-api`, `backend-api`, and SDKWork-owned business `open-api`, `MUST` follow `API_SPEC.md` section 4.5, section 14, and section 15:

- **Default classification:** omitted `x-sdkwork-wire-protocol` means SDKWork-owned custom API (`sdkwork-v3`); only operation-level `x-sdkwork-wire-protocol: external` plus `x-sdkwork-external-protocol-id` identifies a third-party compatibility `open-api` operation.
- **Input:** typed request bodies, section 14.1 list/search/command input, `SdkWorkListQuery`, and `q` for free-text search.
- **Success output:** `SdkWorkApiResponse` with `{ "code": 0, "data": <payload>, "traceId": "<server-uuid>" }`.
- **Error output:** HTTP 4xx/5xx `application/problem+json` (`ProblemDetail`) with numeric `code` and `traceId`.
- Success `code` is numeric `int32`; HTTP 2xx JSON bodies `MUST` use `0` only. REST semantics remain on HTTP status (`201`, `202`, etc.).
- Platform error codes are numeric non-zero values per section 15.3 (`40001`, `40101`, `40401`, …).
- Single resource: `data.item`
- Lists: `data.items` + `data.pageInfo` (`PageInfo.mode` is `offset` or `cursor`)
- Commands: `data.accepted` plus optional `resourceId` / `status`
- Async accept (`202`): `data.operationId`, `data.status`, optional `pollUrl`
- Operation patterns: retrieve/list/search/create/update/delete/command/async/bulk semantics follow `API_SPEC.md` section 15.4; create uses `201`, delete uses `204` with no JSON body, and `PUT`/`PATCH` use SDK action `update`.

Vendor compatibility `open-api` routes that mirror upstream tool or provider wire (for example OpenAI `/v1/*`, Anthropic/Claude `/anthropic/v1/*`, Google/Gemini `/google/v1beta/*`, Claude Code, or Codex) `MAY` opt out only when every exempt operation declares operation-level `x-sdkwork-wire-protocol: external` and `x-sdkwork-external-protocol-id` per `API_SPEC.md` section 4.5.2. SDKWork-owned business `open-api` operations `MUST NOT` opt out. Mixed OpenAPI documents are validated per operation; one external operation never exempts SDKWork-owned operations in the same document.

Errors `MUST` use HTTP 4xx/5xx with `application/problem+json` (`ProblemDetail`) including required numeric `code` and `traceId`. Business failures `MUST NOT` use HTTP 2xx with non-zero `code`, string wire codes, `success`, or human `message`.

Forbidden legacy envelopes and fields: `PlusApiResult`, `AppbaseApiResult`, `StoreApiResult`, `SdkWorkResponse`, per-domain `*ApiResult`, wire field `requestId`, bare domain DTOs at the HTTP root, and top-level `{ items, pageInfo, traceId }` without `data`.

Handlers `MUST` serialize success and map errors through `sdkwork-web-framework` response mapping. Generated HTTP SDKs (`--standard-profile sdkwork-v3`) unwrap `data` by default and expose typed numeric `ProblemDetail.code` / `traceId` on errors; use `.raw` when the full envelope is required.

Before completing API contract, SDK generation, or frontend service work, run:

```bash
node <sdkwork-specs>/tools/check-api-operation-patterns.mjs --workspace <workspace-root>
node <sdkwork-specs>/tools/check-api-response-envelope.mjs --workspace <workspace-root>
```

Authority: `sdkwork-specs/API_SPEC.md` section 4.5 and sections 14–16, `SDK_SPEC.md` section 4.2, `FRONTEND_SPEC.md`, `MIGRATION_SPEC.md` section 4.2.

## List And Search Pagination

All L2+ list/search APIs and their backing services, repositories, SDK consumers, and interactive frontend lists `MUST` follow `PAGINATION_SPEC.md`:

- **Input:** standard `SdkWorkListQuery` or query params (`page`/`page_size` or `cursor`/`page_size` per `API_SPEC.md` §14.1); default `page_size` `20`; max `200` unless a documented exception exists.
- **Output:** `SdkWorkApiResponse.data.items` + `data.pageInfo` with `PageInfo.mode` (`offset` or `cursor`) per `API_SPEC.md` §16.
- **Store-level pagination:** push filtering, sorting, and page selection to SQL `LIMIT`/keyset or incrementally maintained indexes — never unbounded collect then `skip`/`take`/`slice` in process memory (`PAGINATION_SPEC.md` §2).
- **SDK and frontend:** interactive lists request one page at a time from the server; no default `listAll*` on P0/P1 paths; no client-side `slice` pagination over full downloads.

Before completing list/search API, repository, SDK list helper, projection read model, or paginated UI work, run:

```bash
node <sdkwork-specs>/tools/check-pagination.mjs --workspace <workspace-root>
```

Authority: `PAGINATION_SPEC.md`, `API_SPEC.md` §14.1/§16, `DATABASE_SPEC.md` §20.5, `WEB_BACKEND_SPEC.md` §12, `SDK_SPEC.md` §4.2/§6, `FRONTEND_SPEC.md`, `APP_SDK_INTEGRATION_SPEC.md` §9.

## Human Review Rules

Request human review before breaking SDKWORK standards, changing public naming, altering security/auth behavior, changing database migrations or production deployment config, deleting data/files, or changing generated SDK ownership. Surface unresolved spec paths, app identity conflicts, component ownership conflicts, and API authority ambiguity instead of guessing.
