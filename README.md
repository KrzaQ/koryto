# koryto

Calorie and weight log for a household: one Rust binary that is an HTTP API,
a web UI, a CLI and an MCP server, over PostgreSQL. Meals, weigh-ins and
sport for two people, a shared table of well-known foods, and an expenditure
estimate derived from the weight trend. Logging happens mostly through chat
(Open WebUI, Claude Code) via MCP; the web UI is for charts and corrections.

`PLAN.md` is the design and the build sequence; `docs/deploy.md` is the
host-side runbook. `make` lists the targets;
`make dev-backend` and `make dev-frontend` run the two halves for
development; `make test` is sandbox-safe and `make test-db` runs the database
suite against a real Postgres.

- `koryto serve` runs the API, serves the built Vue frontend and exposes MCP
  at `/mcp`.
- `koryto household`, `koryto user`, `koryto token` and `koryto recompute-days`
  are the admin CLI. Token scopes: `read`; `write` logs entries, adds foods
  and sets locations; `edit` changes and voids entries, targets, foods and
  the profile; `delegate` marks a gateway token (Open WebUI) that names the
  acting user in `X-Koryto-User` on every request.
- The web UI has day, trends, foods, profile and tokens pages, with a person
  chooser for the household.
