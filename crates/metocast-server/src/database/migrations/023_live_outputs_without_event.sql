-- A live transcript can run with no event, like an OBS recording that lands in
-- `untracked_recordings`. The session keeps its segments; an event can be named later.
ALTER TABLE transcript_sessions ALTER COLUMN event_id DROP NOT NULL;
