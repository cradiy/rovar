CREATE TABLE media (
    space_id TEXT NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    hash TEXT NOT NULL CHECK(hash ~ '^[0-9a-f]{64}$'),
    length BIGINT NOT NULL CHECK(length >= 0 AND length <= 134217728),
    blob TEXT NOT NULL,
    PRIMARY KEY(space_id,hash)
);
ALTER TABLE revisions ADD COLUMN media JSONB NOT NULL DEFAULT '[]';
