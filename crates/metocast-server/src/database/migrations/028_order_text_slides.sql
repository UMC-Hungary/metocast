-- Text: an Order of Service step shown as one slide of its own, such as the welcome: the
-- title, and its notes as optional paragraphs, one per line.
ALTER TABLE event_order_items DROP CONSTRAINT event_order_items_kind_check;
ALTER TABLE event_order_items ADD CONSTRAINT event_order_items_kind_check
  CHECK (kind IN ('song', 'bible', 'block', 'announcements', 'text'));
