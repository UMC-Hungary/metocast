-- The transcript library's AI clean-up pass: its state on the session, and the cleaned text of
-- each line next to the raw one.
ALTER TABLE transcript_sessions
    ADD COLUMN IF NOT EXISTS cleanup_status TEXT NOT NULL DEFAULT 'none', -- none|running|succeeded|failed
    ADD COLUMN IF NOT EXISTS cleanup_error  TEXT;

ALTER TABLE transcript_segments ADD COLUMN IF NOT EXISTS clean_text TEXT;
