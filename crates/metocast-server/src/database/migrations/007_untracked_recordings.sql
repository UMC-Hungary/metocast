-- ============================================================
-- 007_untracked_recordings.sql
-- Add untracked_recordings table.
-- Note: event_activities already exists from 001_initial.sql.
-- ============================================================

-- Separate table for recordings that have no event assignment.
CREATE TABLE untracked_recordings (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    file_path   TEXT        NOT NULL,
    file_name   TEXT        NOT NULL,
    file_size   BIGINT      NOT NULL DEFAULT 0,
    detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for current-event query performance
CREATE INDEX IF NOT EXISTS idx_event_activities_event_type
    ON event_activities (event_id, activity_type);
