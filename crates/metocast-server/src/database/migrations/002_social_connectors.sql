-- Add YouTube scheduling columns to events
ALTER TABLE events ADD COLUMN youtube_broadcast_id TEXT;
ALTER TABLE events ADD COLUMN youtube_stream_url TEXT;
ALTER TABLE events ADD COLUMN youtube_schedule_status TEXT NOT NULL DEFAULT 'not_scheduled';

-- Add Facebook scheduling columns to events
ALTER TABLE events ADD COLUMN facebook_event_id TEXT;
ALTER TABLE events ADD COLUMN facebook_stream_id TEXT;
ALTER TABLE events ADD COLUMN facebook_event_url TEXT;
ALTER TABLE events ADD COLUMN facebook_schedule_status TEXT NOT NULL DEFAULT 'not_scheduled';

-- OAuth token storage (no NOTIFY trigger needed)
CREATE TABLE connector_tokens (
    connector    TEXT PRIMARY KEY,
    access_token TEXT NOT NULL,
    refresh_token TEXT,
    expires_at   TIMESTAMPTZ,
    scope        TEXT,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Replace the existing trigger function to support a skip-flag that prevents
-- scheduling loops when Rust writes external IDs back to the DB.
CREATE OR REPLACE FUNCTION notify_event_change() RETURNS trigger AS $$
BEGIN
  IF current_setting('app.skip_sync_notify', true) = 'true' THEN
    RETURN NEW;
  END IF;
  PERFORM pg_notify('event_changes', json_build_object(
    'operation', TG_OP, 'record', row_to_json(NEW)
  )::text);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;
