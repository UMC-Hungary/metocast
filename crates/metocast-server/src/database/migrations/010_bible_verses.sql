ALTER TABLE events
  ADD COLUMN textus_verses JSONB NOT NULL DEFAULT '[]'::jsonb,
  ADD COLUMN leckio_verses JSONB NOT NULL DEFAULT '[]'::jsonb;
