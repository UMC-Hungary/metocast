-- Add duration_seconds to untracked_recordings so it can be displayed
-- and carried over when the recording is assigned to an event.
ALTER TABLE untracked_recordings
    ADD COLUMN duration_seconds DOUBLE PRECISION NOT NULL DEFAULT 0;
