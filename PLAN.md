# Plan: koryto, a calorie and weight log for a household

One Rust binary (`koryto`) that is an HTTP API, a web UI, a small CLI and an
MCP server over PostgreSQL. It records meals, weigh-ins and sport for the two
people of a household, keeps a shared table of well-known foods so the same
dish always gets the same number, and derives what each person actually burns
from their intake and weight trend. Logging happens mostly through chat: Open
WebUI and Claude Code call the MCP tools, the web UI is for looking at charts
and fixing things.

It is the sibling of `../support-ui` and copies its bones deliberately: the
crate layout, auth (OIDC in-app against authentik plus scoped bearer tokens
with the delegate pattern), the Makefile, the test harness, the frontend stack
and the chart palette. Where this plan says "as support-ui", read that code
and copy it, adapting names; do not invent a second way.

This document is written to be executed unattended from a sandbox. Every step
ends with a "done when" that can be checked with `make` targets, and the things
that can only be checked on the host (authentik, apache, a browser, Open WebUI)
are collected in the host checklist at the end. The runner must not claim
those; it lists them as outstanding.

## Decisions already made

- **The MCP is the primary input surface.** Nobody keeps a food log through
  forms. "Two eggs and rye toast" in a chat, the model estimates or finds the
  food, calls `log_meal`, done. Tool descriptions carry the house rules so the
  model behaves without a system prompt.
- **Households, not per-record grants.** Every user belongs to one household;
  everyone in it sees and may edit everyone's entries. Every entry records
  both its owner and who created it. No invite flow: membership is a CLI
  action. No per-field privacy in this plan.
- **Logging for each other is a first-class feature.** One cooked dinner is
  one message that logs an entry for both people, each with their own
  portion count.
- **Two time facts per entry: the instant and the accounting day.** People
  travel; a dinner in New York is the next day on the Polish clock. Each entry
  stores the instant (`TIMESTAMPTZ`), the IANA zone it was logged under, and a
  materialised `day` computed from instant, zone and the user's day boundary
  (default 04:00: a 01:00 kebab belongs to the evening before). Every chart
  and summary queries `day` and never touches zones. The zone comes from an
  effective-dated per-user location history ("I'm in New York now" is one
  tool call), with a per-entry override.
- **Exercise never subtracts from intake.** Burned-kcal estimates are bad and
  eating them back is worse. Sport is logged for its own sake (kind, minutes,
  optional kcal). The headline number is intake against an **adaptive
  expenditure** derived from the weight trend and intake over rolling weeks,
  seeded by Mifflin-St Jeor until there is enough data.
- **kcal and protein only.** Both are integers. Weight is integer grams,
  duration integer minutes, portions a `NUMERIC(6,2)`. No floats in the
  domain; display converts.
- **Provenance on every number.** A meal has a `source`: `estimate` (the model
  guessed), `manual` (a person typed it), `label` (read off packaging) or
  `food` (a saved food times portions). Descriptions are kept forever so an
  estimate can be redone.
- **Foods are household-scoped**, a named number with a portion description,
  aliases and optional protein. No recipe composition.
- **Voiding through MCP is allowed** with the `edit` scope. This is a
  departure from support-ui's "MCP never deletes": mislogged meals are
  constant, this is not billing data, and "undo that" has to work from chat.
  Void is a soft delete (`voided_at`); nothing is ever hard-deleted.
- **PostgreSQL 18 as a compose sidecar, one app container**, published on
  `127.0.0.1:13384` (app) and `127.0.0.1:13385` (db, for DataGrip), fronted by
  the house apache vhost at `https://koryto.int.krzaq.cc`. support-ui holds
  13382/13383; the scratch Postgres for development is on `<host>:15434`
  (support-ui's is 15433).
- **Auth is OIDC in-app against authentik**, group `koryto`, exactly as
  support-ui. Tokens: `read`, `write` (log entries, add foods, set location),
  `edit` (change and void entries, targets, foods, profile), `delegate` (a
  gateway token, Open WebUI, that names the acting user in `X-Koryto-User`).
  A non-delegate token belongs to a user and acts as that user; a delegate
  token belongs to nobody and must carry the header.
- **Frontend is the house stack**: Vue 3, Vite 7, TypeScript, Pinia,
  vue-router, Tailwind 4, vitest, Reka UI primitives, vue-echarts with the
  palette from `support-ui/frontend/src/lib/palette.ts` copied verbatim.
- **Commits follow `~/.claude/skills/commit`**: imperative subject, module
  prefix (`server:`, `frontend:`, `mcp:`, `docker:`, `plan:`), body says why,
  no AI trailers of any kind, stage files by name, one commit per concern.
  **The Makefile follows `~/.claude/skills/makefile`.** Charts follow the
  `dataviz` skill (invoke it with the Skill tool; it is not a file under
  `~/.claude/skills`).

## Sandbox contract

The sandbox is limes (`~/code/misc/limes`): the host's `/usr` mirrored
read-only, an empty `$HOME`, the rust toolchain and npm cache mounted in via
`.limes.local.toml`, outbound network. Expected inside: `cargo`, `node`/`npm`,
`psql`. `initdb` cannot run as uid 0, and although docker (the limes daemon)
works, the `postgres` image cannot drop privileges under the sandbox's
single-uid mapping and never comes up, so **the database for tests is
`TEST_DATABASE_URL`**, which
`.limes.local.toml` exports as `postgres://koryto:scratch@<host>:15434/koryto_test`.
That database lives in the scratch Postgres that `scripts/scratch-pg.sh up`
starts on the host (it creates both `koryto_scratch` for development and
`koryto_test` for tests). If `TEST_DATABASE_URL` is set but unreachable, the
DB suite must fail, not skip: the host forgot to start the scratch server and
the runner reports that as a gap, not as green.

`../support-ui` is mounted read-only into the sandbox. Read it freely; copy
code from it; never depend on it at build time. Prefer crates.io over git
dependencies; if a registry is unreachable, stop and report, do not vendor.

Consequences, each a design requirement below:

- Database access goes through a `Db`/`Repo` type whose SQL is exercised by
  integration tests against a real Postgres via `scripts/test-db.sh` (already
  written: tries `TEST_DATABASE_URL`, then docker, then an `initdb` cluster
  when not root, else prints `SKIPPED` and exits 0). The `TEST_DATABASE_URL`
  path refuses a database whose name does not end in `_test`; each test
  creates and drops its own `<name>_<random>` database next to it, as
  `support-ui/src/db/test_db.rs` does. Everything above the repository (day
  computation, trend, expenditure, Mifflin, auth middleware, MCP schemas) is
  unit-tested with in-memory fakes so `cargo test` alone is meaningful.
- `koryto serve` has a `KORYTO_AUTH=dev` mode that logs everyone in as a fixed
  user in a fixed household, with the same loopback-only guard as support-ui.
- `make build` must succeed from a clean clone with no database and no
  environment variables set.

## Repository layout after step 0

```
Cargo.toml              crate "koryto", edition 2024, one binary
src/
  main.rs               clap: serve | migrate | token | household | user | recompute-days
  config.rs             KORYTO_* env parsing, dev guard
  db/                   Db (sqlx runtime queries), row types, test_db.rs
  domain/               day, trend, expenditure, mifflin, token, when
  http/                 axum router, auth, oidc, handlers, OpenAPI (utoipa), static
  mcp/                  rmcp server and tools
  cli/                  migrate, token, household, user, recompute_days
migrations/             sqlx migrations, 0001_... upward
frontend/               Vue app (step 5)
scripts/                test-db.sh, scratch-pg.sh (already written)
packaging/httpd/koryto.int.krzaq.cc
Dockerfile, docker-compose.yml, .env.example
Makefile, Makefile.local.example
PLAN.md, CLAUDE.md, README.md
```

Configuration is environment only, prefix `KORYTO_`:

| Variable | Meaning |
|---|---|
| `KORYTO_DATABASE_URL` | `postgres://koryto:pw@db/koryto` in compose; the scratch URL in the sandbox |
| `KORYTO_BIND` | default `0.0.0.0:8000` |
| `KORYTO_PUBLIC_URL` | `https://koryto.int.krzaq.cc`; source of the OIDC redirect URI, never derived from headers |
| `KORYTO_SECRET` | 32+ bytes, signs the session cookie |
| `KORYTO_AUTH` | `oidc` (default) or `dev` |
| `KORYTO_OIDC_ISSUER`, `KORYTO_OIDC_CLIENT_ID`, `KORYTO_OIDC_CLIENT_SECRET` | from authentik |
| `KORYTO_OIDC_GROUP` | default `koryto`; login requires membership |
| `KORYTO_TIMEZONE` | house zone, default `Europe/Warsaw`; the first location row of a new user |
| `KORYTO_AUTO_MIGRATE` | default `1`; `serve` applies migrations at startup |

## Schema

Migrations are plain SQL under `migrations/`, applied by `koryto migrate` and
by `koryto serve` at startup. Never edited after they are committed.

```sql
households
  id            INTEGER PK GENERATED ALWAYS AS IDENTITY
  name          TEXT NOT NULL
  created_at    TIMESTAMPTZ NOT NULL DEFAULT now()

users
  id            INTEGER PK IDENTITY
  subject       TEXT NOT NULL UNIQUE              -- OIDC sub
  email         TEXT UNIQUE                       -- what X-Koryto-User names
  name          TEXT
  household_id  INTEGER NULL FK households        -- NULL: logged in, not yet placed; API returns 403 "no household"
  day_boundary_minutes INTEGER NOT NULL DEFAULT 240  -- 04:00
  height_mm     INTEGER NULL
  born_on       DATE NULL
  sex           TEXT NULL CHECK (sex IN ('female','male'))   -- only for Mifflin-St Jeor
  activity_factor NUMERIC(3,2) NOT NULL DEFAULT 1.40           -- Mifflin multiplier
  created_at, last_login_at TIMESTAMPTZ

user_locations                                    -- effective-dated zone
  id            INTEGER PK IDENTITY
  user_id       INTEGER NOT NULL FK users
  valid_from    TIMESTAMPTZ NOT NULL
  timezone      TEXT NOT NULL                     -- IANA name, validated with chrono-tz
  UNIQUE (user_id, valid_from)

targets                                           -- effective-dated per user
  id            INTEGER PK IDENTITY
  user_id       INTEGER NOT NULL FK users
  valid_from    DATE NOT NULL
  kcal          INTEGER NOT NULL CHECK (kcal > 0)
  protein_g     INTEGER NULL CHECK (protein_g > 0)
  weight_g      INTEGER NULL CHECK (weight_g > 0)       -- goal weight
  UNIQUE (user_id, valid_from)

foods                                             -- household-scoped named numbers
  id            INTEGER PK IDENTITY
  household_id  INTEGER NOT NULL FK households
  name          TEXT NOT NULL
  aliases       TEXT[] NOT NULL DEFAULT '{}'
  portion       TEXT NOT NULL                     -- "1 bowl (350 g)", "1 slice"
  kcal          INTEGER NOT NULL CHECK (kcal >= 0)      -- per portion
  protein_g     INTEGER NULL CHECK (protein_g >= 0)
  created_by    INTEGER NOT NULL FK users
  created_at, updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
  archived_at   TIMESTAMPTZ NULL
  UNIQUE (household_id, lower(name)) WHERE archived_at IS NULL   -- partial unique index

meals
  id            INTEGER PK IDENTITY
  user_id       INTEGER NOT NULL FK users         -- whose intake
  eaten_at      TIMESTAMPTZ NOT NULL
  timezone      TEXT NOT NULL                     -- zone the day was computed in
  day           DATE NOT NULL                     -- materialised accounting day
  day_override  BOOLEAN NOT NULL DEFAULT FALSE    -- TRUE: day was set by hand, recompute leaves it
  description   TEXT NOT NULL
  kcal          INTEGER NOT NULL CHECK (kcal >= 0)
  protein_g     INTEGER NULL CHECK (protein_g >= 0)
  source        TEXT NOT NULL CHECK (source IN ('estimate','manual','label','food'))
  food_id       INTEGER NULL FK foods
  portions      NUMERIC(6,2) NULL CHECK (portions > 0)   -- with food_id; kcal = round(food.kcal * portions) at write
  created_by    INTEGER NOT NULL FK users
  created_via   TEXT NOT NULL CHECK (created_via IN ('web','mcp','cli'))
  created_at, updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
  voided_at     TIMESTAMPTZ NULL
  INDEX (user_id, day) WHERE voided_at IS NULL

weights
  id            INTEGER PK IDENTITY
  user_id       INTEGER NOT NULL FK users
  measured_at   TIMESTAMPTZ NOT NULL
  timezone, day, day_override                     -- as meals
  weight_g      INTEGER NOT NULL CHECK (weight_g BETWEEN 20000 AND 400000)
  created_by, created_via, created_at, updated_at, voided_at   -- as meals
  INDEX (user_id, day) WHERE voided_at IS NULL

activities
  id            INTEGER PK IDENTITY
  user_id       INTEGER NOT NULL FK users
  started_at    TIMESTAMPTZ NOT NULL
  timezone, day, day_override                     -- as meals
  kind          TEXT NOT NULL                     -- free text, lower-cased: "run", "gym", "cycling"
  minutes       INTEGER NOT NULL CHECK (minutes > 0)
  kcal          INTEGER NULL CHECK (kcal >= 0)    -- informational only
  note          TEXT NOT NULL DEFAULT ''
  created_by, created_via, created_at, updated_at, voided_at   -- as meals
  INDEX (user_id, day) WHERE voided_at IS NULL

api_tokens
  id            INTEGER PK IDENTITY
  name          TEXT NOT NULL
  token_hash    CHAR(64) NOT NULL UNIQUE          -- sha256 of the secret, prefix `ko_`
  scopes        TEXT[] NOT NULL                   -- subset of {read,write,edit,delegate}
  user_id       INTEGER NULL FK users             -- the person a personal token acts as
  created_by    INTEGER NOT NULL FK users
  created_at, last_used_at, revoked_at TIMESTAMPTZ
  CHECK ((user_id IS NULL) = ('delegate' = ANY(scopes)))
```

Meals, weights and activities are three tables, not one polymorphic table:
their columns differ, their charts differ, and a `UNION` for "the day" is
cheap. The shared time columns are handled by one Rust helper, not by
inheritance.

### Domain rules

- **Zone for a user at instant T** = the `user_locations` row with the
  greatest `valid_from <= T`. Every user gets a first row at first login
  (`valid_from = '-infinity'`, zone `KORYTO_TIMEZONE`), so resolution never
  fails. An explicit `timezone` on a write wins over the history.
- **Day** = `(T in zone − day_boundary_minutes).date()`. Computed in Rust at
  write time and stored. Changing a location row or the boundary recomputes
  `day` for that user's entries whose `day_override` is false, in the same
  transaction (`recompute_days(user)`, also exposed as `koryto recompute-days`).
- **A day is logged** when it has at least one non-voided meal. Unlogged days
  are gaps, never zeros, and are excluded from averages and from the
  expenditure window.
- **Day totals**: `kcal = Σ meals.kcal`, `protein_g = Σ protein_g` over meals
  that have it, plus `meals_without_protein` so the UI can say the protein
  figure is partial. `sport_minutes = Σ activities.minutes`.
- **Target for day D** = the `targets` row with the greatest `valid_from <= D`,
  or none. **Balance** = `kcal − target.kcal`, only on logged days with a
  target.
- **Day's weight** = the earliest non-voided reading of that day (morning
  weight is the convention; a second reading after breakfast is noise).
- **Weight trend** = exponential moving average over day weights in day
  order, α = 0.1, seeded with the first reading, updated only on days with a
  reading (gaps do not decay it). Returned alongside the raw points.
- **Expenditure (adaptive)** over a trailing window of 28 days ending at D,
  for a user: let the logged days in the window be `n`, `intake` their mean
  kcal, `Δtrend_g` the trend weight at the last weigh-in in the window minus
  the trend at the first, `span` the days between those two weigh-ins. Then
  `expenditure = intake − Δtrend_g × 7.7 / span` (7 700 kcal per kg, stated
  as a constant with a comment). Reported only when `n ≥ 14` and `span ≥ 10`;
  otherwise the **seed**: Mifflin-St Jeor with the latest trend weight (or
  raw weight, or nothing) times `activity_factor`, and the response says
  which (`basis: "adaptive" | "seed" | "none"`). Mifflin: `10 × kg + 6.25 × cm
  − 5 × age + 5` (male) or `− 161` (female); needs `height_mm`, `born_on`,
  `sex` and a weight, otherwise `none`. All of this is a pure function over a
  slice of `(day, kcal, weight_g)` rows, unit-tested with synthetic series
  including the flight-day case (a 30-hour day with five meals does not move
  the number by more than the maths says).
- **Portions and foods**: a meal logged against a food stores
  `kcal = round(food.kcal × portions)` and `protein_g` likewise at write time,
  with `source = 'food'`. Editing the food later does not touch past meals.
- **Ordering** everywhere: `(day, eaten_at | measured_at | started_at, id)`.
- **Units on the wire**: kcal integer, protein grams integer, weight a
  decimal string in kilograms (`"82.4"`, two decimals max), portions a decimal
  string, minutes integer, durations also accepted as `45m`, `1h`, `1h30`,
  `1:30` (reuse support-ui's hours grammar, renamed). Instants are RFC 3339
  in and UTC out; a wall-clock instant without an offset is read on the
  user's zone at that moment (support-ui's `domain/when.rs`, generalised to a
  zone resolver).

## API

JSON under `/api`. Errors are `{"error": {"code": "...", "message": "..."}}`.
OpenAPI at `/api/openapi.json` via utoipa; the frontend generates its types
from it. No CORS headers; same-origin only. `/api/*` never redirects on 401.
Every read that concerns a person takes `?user=ID`, defaulting to the
principal's user, and is refused with 403 when that user is not in the
principal's household. Every write that creates an entry takes `user_id` the
same way.

```
GET    /api/health                              unauthenticated
GET    /api/auth/login   GET /api/auth/callback   POST /api/auth/logout
GET    /api/me                                  user, household members, current zone, today (as a day), target in force

PATCH  /api/users/{id}/profile                  day_boundary_minutes, height_mm, born_on, sex, activity_factor, name
GET    /api/users/{id}/locations                 POST ... {valid_from?, timezone}   PATCH/DELETE .../{loc_id}   (never the -infinity row)
GET    /api/users/{id}/targets                   POST ... {valid_from, kcal, protein_g?, weight_g?}   PATCH/DELETE .../{target_id}

GET    /api/foods?q=                            household's foods, archived excluded unless ?include_archived=1
POST   /api/foods      PATCH /api/foods/{id}     POST /api/foods/{id}/archive   POST .../unarchive

GET    /api/day?user=&date=YYYY-MM-DD           meals, weights, activities, totals, target, balance
GET    /api/days?user=&from=&to=                one row per day: totals, target, balance, weight, trend, logged
GET    /api/meals?user=&from=&to=               flat lists, for tables and search
GET    /api/weights?...   GET /api/activities?...

POST   /api/meals         {user_ids: [..], eaten_at?, timezone?, day?, description, kcal?, protein_g?, source?, food_id?, portions?}
                          one entry per user_id; kcal required unless food_id; returns the created rows
PATCH  /api/meals/{id}    any column above; setting `day` sets day_override
POST   /api/meals/{id}/void   {reason?}         DELETE is not routed
POST   /api/weights       {user_id, measured_at?, timezone?, weight_kg}       PATCH, POST .../void as meals
POST   /api/activities    {user_id, started_at?, timezone?, kind, minutes, kcal?, note?}   PATCH, POST .../void

GET    /api/stats/weight?user=&from=&to=        raw points and trend
GET    /api/stats/expenditure?user=&from=&to=   the adaptive number per day with its basis
GET    /api/stats/weekly?user=&from=&to=        per ISO week: mean intake, balance, sport minutes, logged days

GET    /api/tokens   POST /api/tokens {name, scopes, user_id?}  DELETE /api/tokens/{id}   session only
POST   /mcp                                     streamable HTTP, bearer only
```

### Auth

As support-ui, renamed: `openidconnect` 4.x with PKCE, redirect URI
`{KORYTO_PUBLIC_URL}/api/auth/callback`, group check against
`KORYTO_OIDC_GROUP`, user row upserted by `sub`, private cookie from
`KORYTO_SECRET`. `Principal` is `Session(User) | Token(ApiToken, User) |
Delegate(ApiToken, User)`; after resolution every principal has a user, and
handlers never see the difference except for `require_session` on token
management. Bearer resolution lives in `http::auth::bearer` and is shared by
the API extractor and the MCP middleware. A delegate token without
`X-Koryto-User`, or naming an email that has never logged in, is 403 with a
message that says so. A user with `household_id IS NULL` is 403 on everything
but `/api/me` and logout; the frontend shows a "not in a household yet" page.

## MCP

`/mcp` using `rmcp` with the streamable HTTP transport, bearer only. The server
instructions (the text a client shows the model) state the house rules: what
a day is, that estimates should be searched in foods first, that logging for
several people is one call, that void is the undo, and that weights are in
kilograms. Tools:

| Tool | Scope | Notes |
|---|---|---|
| `whoami` | read | acting user, household members (id, name), current zone, today, target in force, expenditure basis |
| `get_day(date?, user?)` | read | meals, weights, activities, totals, target, balance; `date` defaults to today on the user's clock |
| `get_summary(from, to, user?)` | read | per-day rows plus averages, trend weight, expenditure with basis, logged-day count |
| `search_foods(query)` | read | name and alias match, ranked; empty query lists the most used |
| `add_food(name, portion, kcal, protein_g?, aliases?, confirmed)` | write | refuses a duplicate name; suggests the existing one |
| `log_meal(description, kcal?, protein_g?, source?, food?, portions?, eaten_at?, for_users?, confirmed)` | write | `food` by name or alias; `for_users` by name or email, default the caller; one entry per person; returns the rows and the day's running total |
| `log_weight(weight_kg, measured_at?, for_user?)` | write | no confirmation: the number is the user's own |
| `log_activity(kind, minutes, kcal?, note?, started_at?, for_user?)` | write | as above |
| `set_location(timezone, from?)` | write | "I'm in New York"; `from` defaults to now; recomputes days |
| `update_meal(id, ...)`, `update_weight(id, ...)`, `update_activity(id, ...)` | edit | any column; `day` sets the override |
| `void_entry(kind, id, reason?)` | edit | the undo; kind is `meal`, `weight` or `activity` |
| `set_target(kcal, protein_g?, weight_kg?, from?)` | edit | new effective-dated row, `from` defaults to today |
| `update_food(id, ...)`, `archive_food(id)` | edit | |

`confirmed` is required on `log_meal` when the number is an estimate (no
`food`, `source` absent or `estimate`) and on `add_food`; the descriptions
say to show the person the description, kcal and protein first and pass
`confirmed=true` only after a yes. A logged food and a weight are the user's
own words and need no confirmation. Every tool that takes `for_users` or
`user` refuses a person outside the caller's household. Tool schemas and
scope enforcement are tested with `rmcp`'s in-process client against the
`Db` fake, per tool. The Open WebUI token is `read,write,edit,delegate`; the
Claude Code token is a personal `read,write,edit`.

## Frontend

`frontend/` on the house stack, embedded by rust-embed from `frontend/dist`
with the same `build.rs` fallback as support-ui. Types from
`/api/openapi.json` via `openapi-typescript`, checked in, regenerated by `make
types`. The nav carries a **person switcher** (household members, defaults to
me) and every view reads `?user=` from it. The nav also has the light/dark
toggle and the clock choice (my current zone or the browser's), as support-ui.

Views for the first release:

- **Day** (default, `/d/2026-09-04`): the meals of the day as a list with
  inline edit and an add row (description, kcal, protein, a food picker that
  fills the numbers, portions), weigh-ins, activities, and a header with
  kcal against target as a bar, protein, balance, sport minutes. Voided
  entries are hidden behind a "show voided" toggle. Day arrows and a date
  picker; a badge shows the zone the day was computed in when it differs
  from the house zone.
- **Trends** (`/trends`): step 6.
- **Foods** (`/foods`): searchable table, add, edit, archive, "used N times".
- **Profile** (`/profile`): targets timeline with add, locations timeline
  with add, day boundary, height, birth date, sex, activity factor. Editable
  for any household member (the switcher applies).
- **Tokens** (`/tokens`): create (secret shown once, personal or delegate),
  revoke.
- **Login** and the "not in a household yet" page.

Vitest covers the weight and portion input formatting, the day header maths
and a jsdom smoke render of each view against fixture data.

## CLI

All subcommands read `KORYTO_DATABASE_URL` and talk to the database directly.

- `koryto serve`
- `koryto migrate` (and `--status`)
- `koryto token create NAME --scopes read,write[,edit][,delegate] [--user EMAIL]` / `list` / `revoke ID`
- `koryto household create NAME` / `add-member HOUSEHOLD EMAIL` / `list`
- `koryto user list`
- `koryto recompute-days [--user EMAIL]`

No logging CLI: the MCP and the web UI are the input surfaces.

## Docker and packaging

As support-ui minus typst and the data directory: two-stage build (`node:24`
frontend, `rust:1.96` binary) into `debian:bookworm-slim`, non-root,
`HEALTHCHECK` on `/api/health`. `docker-compose.yml` with services `koryto`
(`127.0.0.1:13384:8000`, `env_file: .env`, `TZ` and `KORYTO_TIMEZONE`
`Europe/Warsaw`) and `db` (`postgres:18`, `./data/postgres:/var/lib/postgresql`,
`127.0.0.1:13385:5432`, `pg_isready` healthcheck). `data/` gitignored.
`.env.example` lists every `KORYTO_*` variable and `POSTGRES_PASSWORD`.
`packaging/httpd/koryto.int.krzaq.cc` copies the support vhost with the port
changed and the comments kept honest.

## Makefile

House style, same targets as support-ui without `tools`:

```
install format lint test test-db build build-frontend
dev-backend dev-frontend types docker-build docker-run clean
```

`test-db` runs `TEST_DATABASE_URL=$(TEST_DATABASE_URL) ./scripts/test-db.sh`.
`dev-backend` is `KORYTO_AUTH=dev KORYTO_PUBLIC_URL=http://localhost:8000
KORYTO_BIND=127.0.0.1:8000 cargo run -- serve`. `Makefile.local.example`
documents `deploy` and `push`.

## Work sequence

Each step is independently shippable and ends in a commit or a few. "Done
when" lists what the sandbox verifies; anything else goes on the host
checklist.

### 0. Scaffold

`git init`, Cargo crate with `serve` printing "not implemented", the frontend
skeleton (copy support-ui's `frontend/` config files, `package.json` renamed,
empty views), Makefile per the skill, `.gitignore` (`/target/`, `/data/`,
`/frontend/node_modules/`, `/frontend/dist/`, `.env`, `Makefile.local`,
`.limes.local.toml`), `README.md` pointing at this plan, `CLAUDE.md` in the
shape of support-ui's for the new layout and rules. `scripts/` is already
there; make sure both scripts are executable.

Done when: `make build` and `make test` pass on a clean clone with nothing
configured.

### 1. Database layer and migrations

Migration 0001 as specified. `Db` with every query the rest of the plan needs,
written once and shared by the API, CLI and MCP. `test_db.rs` copied from
support-ui. DB-backed tests: migrate a fresh database, create a household with
two users, a food, entries for both people across a zone change, and assert
the day computation, the partial unique index on foods, the token check
constraint, the household scoping of reads, and the recompute after a
location change (overridden days untouched).

Done when: `cargo test` passes without a database; `make test-db` passes in
the sandbox against `TEST_DATABASE_URL` (a failure, not a skip, if the URL is
set); `koryto migrate --status` runs.

### 2. Domain and CLI

`domain/`: zone resolution and day, the duration grammar, the trend, the
expenditure with its seed, and the token helpers. `koryto token`, `household`,
`user`, `recompute-days`. Unit tests cover every rule under "Domain rules":
the 04:00 boundary on both sides, a DST transition, a westbound and an
eastbound zone change, the EMA with gaps, expenditure below and above the
data threshold, Mifflin for both sexes and with missing profile fields, the
food-times-portions rounding.

Done when: `cargo test` and `make test-db` pass.

### 3. HTTP API and auth

Router, error envelope, OpenAPI, principal middleware with personal and
delegate tokens, OIDC flow (copy support-ui's `oidc.rs` and its wiremock
test), dev mode with its guard, tokens, all handlers above. Handler tests run
against the in-memory `Db` fake, including household scoping (a user from
another household gets 403 on every person-scoped route), the
no-household 403, and multi-user meal creation.

Done when: `cargo test` passes; `make dev-backend` starts with no database
and serves `/api/health` and `/api/openapi.json`; a `read` token gets 403 on a
POST; a delegate token without the header gets 403 with the explanatory
message; no auth gets 401 and never a redirect on `/api/*`.

### 4. MCP

Comes before the frontend because it is how the data gets in. All tools
above, the server instructions text, schema tests with the in-process client,
scope tests per tool, a test that `log_meal` with `for_users` naming both
people creates two rows with separate portions, and one that `void_entry`
hides an entry from `get_day` and the running total.

Done when: `cargo test` passes; a `read` token sees the write tools listed and
gets a clean error calling one; `confirmed=false` on an estimated meal returns
the preview rather than writing.

### 5. Frontend, first release

Day, Foods, Profile, Tokens, Login and the no-household page, the person
switcher, Pinia stores, generated types, the food picker and the
weight/portion inputs with tests. Bundle embedded and served.

Done when: `make build` passes including `vue-tsc` and vitest; `make
dev-backend` serves `index.html` at `/` and `/d/2026-09-04` after `npm run
build`; no `console.error` in a jsdom render of each view.

### 6. Charts

The **Trends** view with a range picker, per person, following the dataviz
skill and the palette in fixed slots (weight raw = slot 0, trend = slot 0
darker line, intake = slot 1, target = neutral, expenditure = slot 2,
protein = slot 3). Charts, top to bottom:

1. Weight: raw points and the trend line, goal weight as a reference line.
2. Daily intake bars against the target line, with a 7-day mean line;
   unlogged days are gaps, not zero bars.
3. Expenditure line with the intake 7-day mean on the same axis, the seed
   period drawn dashed.
4. Weekly balance bars (intake minus expenditure, then minus target as a
   second view) and sport minutes as a small bar row under them.
5. A logging calendar heatmap (days logged, meals per day).

Never a dual-axis chart. `/api/stats/*` handlers get tests.

Done when: `make build` passes and each stats endpoint has a handler test.

### 7. Docker, packaging, host runbook

Dockerfile, compose, `.env.example`, apache vhost, `Makefile.local.example`,
and `docs/deploy.md` turned from the host checklist below with exact commands.
`docker compose config` if docker is reachable, else note it as unverified.

Done when: files exist, `make build` still passes, `docs/deploy.md` can be
followed top to bottom without this plan.

### Later, deliberately not in this plan

- Barcode and Open Food Facts lookup.
- Recipe composition (foods made of foods).
- Per-user privacy of weight from the household.
- A `koryto log` CLI.
- Photos on meals.
- Importing a history from another tracker.
- Estimating exercise kcal from anything but what the user typed.

## Host checklist (manual, after step 7)

1. `scripts/scratch-pg.sh up` on the host for development and sandbox tests
   (already needed from step 1; the sandbox's `TEST_DATABASE_URL` points at
   it).
2. authentik: OAuth2/OpenID provider (authorization code, confidential,
   redirect URI `https://koryto.int.krzaq.cc/api/auth/callback`, scopes
   `openid email profile`), application `koryto`, group `koryto` with both
   of you in it. Issuer, client id and secret into `.env`.
3. DNS: `koryto.int.krzaq.cc` in the internal zone; the wildcard certificate
   covers it.
4. Apache: install `packaging/httpd/koryto.int.krzaq.cc`, `vhost add`,
   `configtest`, reload.
5. `cp .env.example .env`, fill it, `make deploy`. Migrations run on first
   start.
6. Both of you log in once through the browser. Then `docker compose exec
   koryto koryto household create home` and `add-member home <email>` twice.
   Set each profile (height, birth date, sex, activity factor) and a first
   target in the UI.
7. Tokens: `openwebui` with `read,write,edit,delegate`; `claude-code` personal
   with `read,write,edit`. Register `https://koryto.int.krzaq.cc/mcp` in
   Open WebUI with the `X-Koryto-User` header set from the acting user, as
   support's `X-Support-User` is, and in Claude Code.
8. Backups: add `docker compose exec -T db pg_dump -U koryto koryto` to the
   existing job. DataGrip at `127.0.0.1:13385`.
9. Acceptance: log a meal for both people from Open WebUI as each of you,
   log a weight, set a location to `America/New_York`, log a late dinner, and
   confirm in the Day view that it landed on the American day and the earlier
   entries did not move.
