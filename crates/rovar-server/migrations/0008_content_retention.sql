-- Keep idempotency receipts after historical payloads expire.
ALTER TABLE revisions ALTER COLUMN blob DROP NOT NULL;
ALTER TABLE media ADD COLUMN last_used bigint NOT NULL DEFAULT (EXTRACT(EPOCH FROM now())::bigint);
CREATE INDEX revisions_retention ON revisions(modified) WHERE blob IS NOT NULL;
CREATE INDEX revisions_blob ON revisions(blob) WHERE blob IS NOT NULL;
CREATE INDEX revisions_media ON revisions USING gin(media) WHERE blob IS NOT NULL;
CREATE INDEX media_blob ON media(blob);
CREATE INDEX media_retention ON media(last_used);
