-- Sport kcal from a rate rather than a guess. A kind carries its MET, the
-- multiple of resting metabolism the activity costs, and a session's burn is
-- (MET - 1) x weight x hours: the person's base already covers the resting
-- part of those hours, so only the excess is the session's own.
--
-- The table is reference data, not household data: what swimming costs is
-- physiology. Editing a MET does not touch entries already logged, exactly as
-- editing a food leaves past meals alone.
CREATE TABLE activity_kinds (
    id            INTEGER PRIMARY KEY GENERATED ALWAYS AS IDENTITY,
    name          TEXT NOT NULL,
    aliases       TEXT[] NOT NULL DEFAULT '{}',
    met           NUMERIC(4,2) NOT NULL CHECK (met >= 1.00 AND met <= 25.00),
    note          TEXT NOT NULL DEFAULT '',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    archived_at   TIMESTAMPTZ
);
CREATE UNIQUE INDEX activity_kinds_name ON activity_kinds (lower(name)) WHERE archived_at IS NULL;

-- Where an entry's kcal came from: what the person said, or the rate.
ALTER TABLE activities
    ADD COLUMN source TEXT NOT NULL DEFAULT 'manual' CHECK (source IN ('manual', 'met')),
    ADD COLUMN activity_kind_id INTEGER REFERENCES activity_kinds(id);

-- Compendium of Physical Activities values, rounded to the useful digit.
INSERT INTO activity_kinds (name, aliases, met, note) VALUES
    ('walk',        '{"spacer","walking"}',           3.5,  'about 5 km/h, flat'),
    ('brisk walk',  '{"marsz","fast walk"}',          5.0,  'about 6.5 km/h'),
    ('hike',        '{"hiking","wycieczka"}',         6.0,  'cross-country, some climb'),
    ('run',         '{"bieg","jogging","bieganie"}',  9.0,  'about 9 km/h'),
    ('cycling',     '{"bike","rower","cycle"}',       6.8,  'about 16-19 km/h'),
    ('swim',        '{"swimming","plywanie","basen"}',6.0,  'freestyle, moderate'),
    ('gym',         '{"weights","silownia","lifting"}',5.0, 'free weights, vigorous'),
    ('circuit',     '{"crossfit","hiit","obwod"}',    8.0,  'circuit training, vigorous'),
    ('rowing',      '{"row","erg","wioslarz"}',       7.0,  'moderate effort'),
    ('elliptical',  '{"orbitrek","cross trainer"}',   5.0,  'moderate effort'),
    ('stairs',      '{"schody","stairmaster"}',       8.0,  'climbing steadily'),
    ('yoga',        '{"joga","pilates"}',             3.0,  'hatha, gentle'),
    ('stretching',  '{"rozciaganie","mobility"}',     2.3,  'light'),
    ('football',    '{"soccer","pilka","pilka nozna"}',7.0, 'casual game'),
    ('basketball',  '{"kosz","koszykowka"}',          6.5,  'casual game'),
    ('volleyball',  '{"siatkowka","siatka"}',         4.0,  'casual game'),
    ('tennis',      '{"tenis"}',                      7.3,  'singles'),
    ('squash',      '{"skwosz"}',                     7.3,  'casual'),
    ('badminton',   '{"kometka"}',                    5.5,  'casual'),
    ('climbing',    '{"bouldering","wspinaczka"}',    8.0,  'rock or wall'),
    ('skiing',      '{"narty","snowboard"}',          7.0,  'downhill, moderate'),
    ('skating',     '{"lyzwy","rolki","rollerblade"}',7.0,  'moderate effort'),
    ('dancing',     '{"taniec","dance"}',             5.0,  'general'),
    ('housework',   '{"sprzatanie","cleaning"}',      3.0,  'general tidying'),
    ('gardening',   '{"ogrod","garden"}',             3.8,  'general'),
    ('sex',         '{"seks"}',                       2.8,  'general effort');
