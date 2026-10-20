-- Search index over the decks in the watched slide folders: title, hymn number and slide text.
-- Rows are refreshed by ppt::sync_index (by mtime and size) and go with their folder.
CREATE EXTENSION IF NOT EXISTS unaccent;

-- Accent- and case-insensitive words, no stemming: "hala" finds "hála", prefixes find "hálát".
CREATE TEXT SEARCH CONFIGURATION metocast_search (COPY = simple);
ALTER TEXT SEARCH CONFIGURATION metocast_search
    ALTER MAPPING FOR hword, hword_part, word WITH unaccent, simple;

CREATE TABLE ppt_decks (
    path        TEXT PRIMARY KEY,
    folder_id   UUID NOT NULL REFERENCES ppt_folders(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    title       TEXT NOT NULL,
    number      INTEGER,
    body        TEXT NOT NULL DEFAULT '',
    modified_at TIMESTAMPTZ NOT NULL,
    size        BIGINT NOT NULL,
    -- Why `body` is empty: the deck could not be read. Cleared when the file parses.
    parse_error TEXT,
    search      TSVECTOR GENERATED ALWAYS AS (
        setweight(to_tsvector('metocast_search'::regconfig, title), 'A') ||
        setweight(to_tsvector('metocast_search'::regconfig, body), 'B')
    ) STORED
);

CREATE INDEX ppt_decks_search ON ppt_decks USING GIN (search);
CREATE INDEX ppt_decks_folder ON ppt_decks (folder_id);
