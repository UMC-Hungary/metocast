ALTER TABLE recordings
    ADD COLUMN IF NOT EXISTS uploadable         BOOLEAN NOT NULL DEFAULT false,
    ADD COLUMN IF NOT EXISTS custom_description TEXT;

CREATE TABLE IF NOT EXISTS recording_uploads (
    recording_id      UUID        NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    platform          TEXT        NOT NULL,
    state             TEXT        NOT NULL DEFAULT 'pending',
    progress_bytes    BIGINT      NOT NULL DEFAULT 0,
    total_bytes       BIGINT      NOT NULL DEFAULT 0,
    upload_uri        TEXT,
    upload_session_id TEXT,
    visibility        TEXT        NOT NULL DEFAULT 'private',
    video_id          TEXT,
    video_url         TEXT,
    error             TEXT,
    started_at        TIMESTAMPTZ,
    completed_at      TIMESTAMPTZ,
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (recording_id, platform)
);

CREATE INDEX IF NOT EXISTS idx_recording_uploads_pending
    ON recording_uploads (state)
    WHERE state IN ('pending', 'uploading', 'paused');
