-- Everyone keeps score on their own from the first login; a household is for
-- sharing. Give every user without one a household named after them.
DO $$
DECLARE
    u RECORD;
    h INTEGER;
BEGIN
    FOR u IN SELECT id, COALESCE(name, email, subject) AS label FROM users WHERE household_id IS NULL LOOP
        INSERT INTO households (name) VALUES (u.label) RETURNING id INTO h;
        UPDATE users SET household_id = h WHERE id = u.id;
    END LOOP;
END $$;
