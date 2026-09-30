ALTER TABLE sessions ADD COLUMN id text NOT NULL DEFAULT gen_random_uuid()::text;
ALTER TABLE sessions ADD COLUMN created_at bigint NOT NULL DEFAULT extract(epoch FROM now())::bigint;
CREATE UNIQUE INDEX sessions_id ON sessions(id);
CREATE INDEX sessions_user ON sessions(user_id);
