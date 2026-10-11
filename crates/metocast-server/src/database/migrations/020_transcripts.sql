-- Live Hungarian → English transcript sessions and the English caption tracks uploaded to YouTube.
CREATE TABLE IF NOT EXISTS transcript_sessions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id    UUID        NOT NULL REFERENCES events(id) ON DELETE CASCADE,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    ended_at    TIMESTAMPTZ,
    status      TEXT        NOT NULL DEFAULT 'running', -- running|stopped|stoppedOffline|error
    -- Last `seq` posted to YouTube's caption ingestion URL; it must keep rising across restarts.
    caption_seq BIGINT      NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_transcript_sessions_event ON transcript_sessions (event_id);

CREATE TABLE IF NOT EXISTS transcript_segments (
    session_id UUID        NOT NULL REFERENCES transcript_sessions(id) ON DELETE CASCADE,
    seq        INT         NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    ended_at   TIMESTAMPTZ NOT NULL,
    text       TEXT        NOT NULL,
    PRIMARY KEY (session_id, seq)
);

CREATE TABLE IF NOT EXISTS caption_tracks (
    recording_id       UUID        NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
    language           TEXT        NOT NULL DEFAULT 'en',
    video_id           TEXT,
    youtube_caption_id TEXT,
    vtt                TEXT        NOT NULL,
    offset_ms          BIGINT      NOT NULL DEFAULT 0,
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (recording_id, language)
);
