# sdkwork-database-drift Specs

`component.spec.json` declares the reusable schema drift detector, its database
contract input, and its SQLx-backed live schema dependency. Application-owned
schema contracts and drift policies remain under each application's
`database/` directory.

PostgreSQL introspection uses the effective schema identity of the injected
pool. The ignored `postgres_schema_authority` integration test requires
`SDKWORK_DATABASE_TEST_POSTGRES_URL` with schema create/drop permission and
proves that post-connect environment changes cannot redirect drift checks.
