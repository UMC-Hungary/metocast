CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE events (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  title TEXT NOT NULL,
  date_time TIMESTAMPTZ NOT NULL,
  speaker TEXT NOT NULL DEFAULT '',
  description TEXT NOT NULL DEFAULT '',
  textus TEXT NOT NULL DEFAULT '',
  leckio TEXT NOT NULL DEFAULT '',
  textus_translation TEXT NOT NULL DEFAULT 'UF',
  leckio_translation TEXT NOT NULL DEFAULT 'UF',
  youtube_privacy_status TEXT NOT NULL DEFAULT 'private',
  auto_upload_enabled BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE recordings (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  file_path TEXT NOT NULL,
  file_name TEXT NOT NULL,
  file_size BIGINT NOT NULL DEFAULT 0,
  duration_seconds DOUBLE PRECISION NOT NULL DEFAULT 0,
  detected_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  whitelisted BOOLEAN NOT NULL DEFAULT false,
  uploaded BOOLEAN NOT NULL DEFAULT false,
  uploaded_at TIMESTAMPTZ,
  video_id TEXT,
  video_url TEXT,
  custom_title TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE event_activities (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  activity_type TEXT NOT NULL,
  message TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE OR REPLACE FUNCTION notify_event_change() RETURNS trigger AS $$
BEGIN
  PERFORM pg_notify('event_changes', json_build_object(
    'operation', TG_OP, 'record', row_to_json(NEW)
  )::text);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER events_notify
  AFTER INSERT OR UPDATE ON events
  FOR EACH ROW EXECUTE FUNCTION notify_event_change();

CREATE OR REPLACE FUNCTION notify_recording_change() RETURNS trigger AS $$
BEGIN
  PERFORM pg_notify('recording_changes', json_build_object(
    'operation', TG_OP, 'record', row_to_json(NEW)
  )::text);
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER recordings_notify
  AFTER INSERT ON recordings
  FOR EACH ROW EXECUTE FUNCTION notify_recording_change();
