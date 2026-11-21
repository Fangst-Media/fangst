-- Categories created before any job uses them, as download clients do when they are set up.
-- Jobs keep their category as text, so a category in use needs no row here.
CREATE TABLE categories (
    name TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL
);
