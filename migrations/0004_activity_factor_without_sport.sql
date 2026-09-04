-- Sport kcal is now added to the day's expenditure, so the activity factor
-- covers everything but sport: a desk or standing job, walks, chores. The old
-- default of 1.40 (light training included) would count a swimmer's sessions
-- twice; sedentary is the new default, and rows still on the old default move
-- with it. A factor someone set by hand stays.
ALTER TABLE users ALTER COLUMN activity_factor SET DEFAULT 1.20;
UPDATE users SET activity_factor = 1.20 WHERE activity_factor = 1.40;
