CREATE TABLE event_bible_references (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  type TEXT NOT NULL CHECK (type IN ('textus', 'leckio')),
  reference TEXT NOT NULL DEFAULT '',
  translation TEXT NOT NULL DEFAULT 'UF',
  verses JSONB NOT NULL DEFAULT '[]'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (event_id, type)
);

-- Migrate existing non-empty references
INSERT INTO event_bible_references (event_id, type, reference, translation, verses)
SELECT id, 'textus', textus, textus_translation, textus_verses
FROM events
WHERE textus != '' OR jsonb_array_length(textus_verses) > 0;

INSERT INTO event_bible_references (event_id, type, reference, translation, verses)
SELECT id, 'leckio', leckio, leckio_translation, leckio_verses
FROM events
WHERE leckio != '' OR jsonb_array_length(leckio_verses) > 0;

ALTER TABLE events
  DROP COLUMN textus,
  DROP COLUMN leckio,
  DROP COLUMN textus_translation,
  DROP COLUMN leckio_translation,
  DROP COLUMN textus_verses,
  DROP COLUMN leckio_verses;
