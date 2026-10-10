-- An event's Order of Service: the songs, Bible readings and blocks of the service, in order.
-- A Bible step points at one of the event's own readings (textus or leckio), so a
-- service never depends on a network lookup while it is live.
CREATE TABLE event_order_items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_id UUID NOT NULL REFERENCES events(id) ON DELETE CASCADE,
  position INT NOT NULL,
  kind TEXT NOT NULL CHECK (kind IN ('song', 'bible', 'block')),
  title TEXT NOT NULL DEFAULT '',
  deck_path TEXT,
  bible_type TEXT CHECK (bible_type IN ('textus', 'leckio')),
  notes TEXT NOT NULL DEFAULT '',
  UNIQUE (event_id, position)
);
