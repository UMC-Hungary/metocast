-- ============================================================
-- 017_event_computed_title.sql
-- The composed "date · reference · title · — speaker" line the editor
-- previews. Stored rather than derived: it is what gets published to
-- YouTube, so it must not drift with the viewer's UI locale.
-- ============================================================

ALTER TABLE events ADD COLUMN IF NOT EXISTS computed_title TEXT NOT NULL DEFAULT '';
