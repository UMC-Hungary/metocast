-- ============================================================
-- 019_media_pipeline.sql
-- Camera clip log (the server-clock anchor for audio/video sync) and render
-- metadata that survives an untracked recording being assigned to an event.
-- ============================================================

-- One row per clip the Blackmagic camera recorded while Metocast watched it.
-- started_at/stopped_at are the server's clock, which the RØDECaster recordings
-- share; the camera's own clock may be wrong.
CREATE TABLE IF NOT EXISTS camera_clips (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id        UUID REFERENCES events(id) ON DELETE SET NULL,
    clip_name       TEXT,
    start_timecode  TEXT,
    started_at      TIMESTAMPTZ NOT NULL,
    stopped_at      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_camera_clips_clip_name ON camera_clips (clip_name);
CREATE INDEX IF NOT EXISTS idx_camera_clips_started_at ON camera_clips (started_at DESC);

ALTER TABLE untracked_recordings
    ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}'::jsonb;
