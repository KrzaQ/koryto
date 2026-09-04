-- Initial schema. See PLAN.md for the reasoning behind every table.

CREATE TABLE households (
    id          INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    name        TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- A person. household_id is NULL between first login and the CLI placing
-- them in a household; until then they can log in but see nothing. The
-- profile columns (height, birth date, sex, activity factor) only feed the
-- Mifflin-St Jeor seed for the expenditure estimate.
CREATE TABLE users (
    id                    INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    subject               TEXT NOT NULL UNIQUE,
    email                 TEXT,
    name                  TEXT,
    household_id          INTEGER REFERENCES households(id),
    day_boundary_minutes  INTEGER NOT NULL DEFAULT 240
                          CHECK (day_boundary_minutes BETWEEN 0 AND 1439),
    height_mm             INTEGER CHECK (height_mm > 0),
    born_on               DATE,
    sex                   TEXT CHECK (sex IN ('female', 'male')),
    activity_factor       NUMERIC(3,2) NOT NULL DEFAULT 1.40
                          CHECK (activity_factor BETWEEN 1.00 AND 2.50),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_login_at         TIMESTAMPTZ
);

-- Effective-dated zone per person: the zone at instant T is the row with the
-- greatest valid_from <= T. Every user gets an origin row at first login
-- (valid_from far in the past, the house zone) so resolution never fails.
CREATE TABLE user_locations (
    id          INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    user_id     INTEGER NOT NULL REFERENCES users(id),
    valid_from  TIMESTAMPTZ NOT NULL,
    timezone    TEXT NOT NULL,
    UNIQUE (user_id, valid_from)
);

-- Effective-dated goals per person: the row with the greatest valid_from <= day.
CREATE TABLE targets (
    id          INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    user_id     INTEGER NOT NULL REFERENCES users(id),
    valid_from  DATE NOT NULL,
    kcal        INTEGER NOT NULL CHECK (kcal > 0),
    protein_g   INTEGER CHECK (protein_g > 0),
    weight_g    INTEGER CHECK (weight_g > 0),
    UNIQUE (user_id, valid_from)
);

-- Well-known meals, shared by the household so the same dish always gets the
-- same number. A food is a named number per portion; no composition.
CREATE TABLE foods (
    id            INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    household_id  INTEGER NOT NULL REFERENCES households(id),
    name          TEXT NOT NULL,
    aliases       TEXT[] NOT NULL DEFAULT '{}',
    portion       TEXT NOT NULL,
    kcal          INTEGER NOT NULL CHECK (kcal >= 0),
    protein_g     INTEGER CHECK (protein_g >= 0),
    created_by    INTEGER NOT NULL REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived_at   TIMESTAMPTZ
);
CREATE UNIQUE INDEX foods_household_name ON foods (household_id, lower(name))
    WHERE archived_at IS NULL;

-- Every entry stores the instant, the zone the day was computed in, and the
-- materialised accounting day. day_override marks a hand-set day that a
-- recompute leaves alone. Nothing is deleted; voided_at hides a row.
CREATE TABLE meals (
    id            INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    user_id       INTEGER NOT NULL REFERENCES users(id),
    eaten_at      TIMESTAMPTZ NOT NULL,
    timezone      TEXT NOT NULL,
    day           DATE NOT NULL,
    day_override  BOOLEAN NOT NULL DEFAULT FALSE,
    description   TEXT NOT NULL,
    kcal          INTEGER NOT NULL CHECK (kcal >= 0),
    protein_g     INTEGER CHECK (protein_g >= 0),
    source        TEXT NOT NULL CHECK (source IN ('estimate', 'manual', 'label', 'food')),
    food_id       INTEGER REFERENCES foods(id),
    portions      NUMERIC(6,2) CHECK (portions > 0),
    created_by    INTEGER NOT NULL REFERENCES users(id),
    created_via   TEXT NOT NULL CHECK (created_via IN ('web', 'mcp', 'cli')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    voided_at     TIMESTAMPTZ
);
CREATE INDEX meals_user_day ON meals (user_id, day) WHERE voided_at IS NULL;

CREATE TABLE weights (
    id            INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    user_id       INTEGER NOT NULL REFERENCES users(id),
    measured_at   TIMESTAMPTZ NOT NULL,
    timezone      TEXT NOT NULL,
    day           DATE NOT NULL,
    day_override  BOOLEAN NOT NULL DEFAULT FALSE,
    weight_g      INTEGER NOT NULL CHECK (weight_g BETWEEN 20000 AND 400000),
    created_by    INTEGER NOT NULL REFERENCES users(id),
    created_via   TEXT NOT NULL CHECK (created_via IN ('web', 'mcp', 'cli')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    voided_at     TIMESTAMPTZ
);
CREATE INDEX weights_user_day ON weights (user_id, day) WHERE voided_at IS NULL;

-- Sport, logged for its own sake. kcal is informational and never enters the
-- balance.
CREATE TABLE activities (
    id            INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    user_id       INTEGER NOT NULL REFERENCES users(id),
    started_at    TIMESTAMPTZ NOT NULL,
    timezone      TEXT NOT NULL,
    day           DATE NOT NULL,
    day_override  BOOLEAN NOT NULL DEFAULT FALSE,
    kind          TEXT NOT NULL,
    minutes       INTEGER NOT NULL CHECK (minutes > 0),
    kcal          INTEGER CHECK (kcal >= 0),
    note          TEXT NOT NULL DEFAULT '',
    created_by    INTEGER NOT NULL REFERENCES users(id),
    created_via   TEXT NOT NULL CHECK (created_via IN ('web', 'mcp', 'cli')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    voided_at     TIMESTAMPTZ
);
CREATE INDEX activities_user_day ON activities (user_id, day) WHERE voided_at IS NULL;

-- A personal token acts as its user; a delegate token has no user and names
-- the acting person in X-Koryto-User on every request.
CREATE TABLE api_tokens (
    id            INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    name          TEXT NOT NULL,
    token_hash    CHAR(64) NOT NULL UNIQUE,
    scopes        TEXT[] NOT NULL,
    user_id       INTEGER REFERENCES users(id),
    created_by    INTEGER NOT NULL REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at  TIMESTAMPTZ,
    revoked_at    TIMESTAMPTZ,
    CHECK ((user_id IS NULL) = ('delegate' = ANY (scopes)))
);
