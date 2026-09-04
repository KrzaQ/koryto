# koryto

Calorie and weight log for a household: one Rust binary that is an HTTP API,
a web UI, a CLI and an MCP server, over PostgreSQL. Meals, weigh-ins and
sport for two people, a shared table of well-known foods, and an expenditure
estimate derived from the weight trend. Logging happens mostly through chat
(Open WebUI, Claude Code) via MCP; the web UI is for charts and corrections.

`PLAN.md` is the design and the build sequence. `make` lists the targets;
`make dev-backend` and `make dev-frontend` run the two halves for
development; `make test` is sandbox-safe and `make test-db` runs the database
suite against a real Postgres.
