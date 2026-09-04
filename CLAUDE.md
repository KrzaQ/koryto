# CLAUDE.md

Guidance for Claude Code when working in this repository.

## What this is

`koryto` logs meals, weigh-ins and sport for the people of a household and
derives each person's energy expenditure from their intake and weight trend.
One Rust binary provides an HTTP API, serves the Vue web UI, offers a CLI, and
exposes an MCP endpoint that is the main way data gets in. Data lives in
PostgreSQL. `PLAN.md` is the design; `../support-ui` is the sibling project
this one copies its bones from.

## Layout

- `src/main.rs` — clap entry point: `serve | migrate | token | household | user | recompute-days`
- `src/config.rs` — `KORYTO_*` environment parsing, dev-auth guard
- `src/db/` — `Db` (Postgres via sqlx, runtime queries) and row types
- `src/domain/` — day computation, weight trend, expenditure, Mifflin-St Jeor, durations
- `src/http/` — axum router, auth (OIDC session cookie + bearer tokens), handlers, OpenAPI
- `src/mcp/` — rmcp server and tools
- `src/cli/` — the terminal commands
- `migrations/` — sqlx migrations, applied by `koryto migrate` and at `serve` startup
- `frontend/` — Vue 3 + Vite + TypeScript + Pinia + Tailwind 4 + vue-echarts; embedded into the binary from `frontend/dist`
- `scripts/` — `test-db.sh` (throwaway Postgres for tests), `scratch-pg.sh` (host-side scratch server)
- `packaging/` — apache vhost
- `docs/deploy.md` — the host-side runbook

## Rules that matter

- Integers in the domain: kcal, protein grams, weight grams, minutes. No
  floats. Portions are `NUMERIC(6,2)`.
- Every meal, weigh-in and activity stores an instant, the IANA zone it was
  logged under, and a materialised `day`. The day is `(instant in zone −
  day_boundary).date()`, default boundary 04:00. Charts and summaries query
  `day` and never touch zones. `day_override` marks a hand-set day that
  recomputes leave alone.
- The zone for a user at an instant comes from `user_locations`, the row with
  the greatest `valid_from <= instant`. Every user has a `-infinity` row.
- Expenditure on a day is a base plus that day's logged sport kcal. The base
  is derived from intake, sport and the weight trend over a rolling window
  (so habitual sport is not counted twice), seeded by Mifflin-St Jeor times
  a non-sport activity factor (default 1.20). Sport kcal is never subtracted
  from intake; it raises the day's burn, and the budget is burn minus intake.
- Households: everyone gets one of their own at first login; sharing is
  `household add-member EMAIL --to EMAIL`, which moves a person (entries
  follow their owner, foods move or fork). Everyone in one sees and may edit
  everyone's entries. Every person-scoped read and write checks the
  household. Entries record `user_id` (whose) and `created_by` (who logged it).
- Nothing is hard-deleted; void sets `voided_at`. MCP may void with `edit`.
- Token scopes: `read`, `write` (log, add foods, set location), `edit`
  (change, void, targets, foods, profile), `delegate` (acts as the user named
  in `X-Koryto-User`). A delegate token has no user; a personal token has one.
  A delegate token acts only for someone who logged in through the browser
  in the last 30 days; `household remove-member` cuts access at once.
- No CORS headers; the frontend is same-origin. `/api/*` never redirects on 401.
- Tests must not need the network or an existing database. DB-backed tests use
  `scripts/test-db.sh`, which refuses a `TEST_DATABASE_URL` whose database
  name does not end in `_test`.
- After changing an API shape, regenerate `frontend/src/api/schema.d.ts`
  (`make types` with `make dev-backend` running) and commit it.
- Charts use the palette in `frontend/src/lib/palette.ts` in fixed slot
  order; never a dual-axis chart.
- The landing page `/` is the dashboard (today, yesterday, weight, the
  week); `/d/:day` is the log for one day. Numbers over sentences: card
  notes are terse, separated by `·`, not prose.

## Commands

```bash
make                  # list targets
make test             # cargo test + vitest, no database needed
make test-db          # database suite against a real Postgres
make dev-backend      # KORYTO_AUTH=dev on http://localhost:8000
make dev-frontend     # Vite with /api proxied to the dev backend
make build            # format, lint, frontend bundle, release binary
```

Commits: imperative subject with a module prefix (`server:`, `frontend:`,
`mcp:`, `docker:`, `plan:`), body explains why, one concern per commit,
never any AI attribution trailers.
