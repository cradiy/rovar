CREATE TABLE sync_cursors (
    space_id TEXT PRIMARY KEY REFERENCES spaces(id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL DEFAULT 0 CHECK(sequence >= 0)
);

ALTER TABLE objects ADD COLUMN change_sequence BIGINT NOT NULL DEFAULT 0;
WITH numbered AS (
    SELECT space_id, id, row_number() OVER (PARTITION BY space_id ORDER BY created,id) AS sequence
    FROM objects WHERE revision > 0
)
UPDATE objects o SET change_sequence=n.sequence FROM numbered n
WHERE o.space_id=n.space_id AND o.id=n.id;
INSERT INTO sync_cursors(space_id,sequence)
SELECT s.id,coalesce(max(o.change_sequence),0) FROM spaces s
LEFT JOIN objects o ON o.space_id=s.id GROUP BY s.id;
CREATE INDEX objects_changes ON objects(space_id,change_sequence) WHERE revision > 0;
