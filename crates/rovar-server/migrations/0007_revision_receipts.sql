ALTER TABLE revisions ADD COLUMN modified bigint;
-- Published schemas did not retain historical timestamps. Preserve the last
-- known timestamp for existing rows; new revisions store their exact commit time.
UPDATE revisions r SET modified = o.modified
FROM objects o WHERE r.space_id = o.space_id AND r.object_id = o.id;
ALTER TABLE revisions ALTER COLUMN modified SET NOT NULL;
