CREATE TABLE server_info (id boolean PRIMARY KEY DEFAULT true CHECK (id), server_id text NOT NULL);
CREATE TABLE users (
    id text PRIMARY KEY,
    username text UNIQUE NOT NULL,
    password_hash text NOT NULL
);
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    user_id text NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at bigint NOT NULL
);
CREATE INDEX sessions_expiry ON sessions(expires_at);
CREATE TABLE spaces (
    id text PRIMARY KEY,
    name text NOT NULL,
    kind text NOT NULL CHECK(kind IN ('personal','team')),
    personal_owner text UNIQUE REFERENCES users(id),
    CHECK((kind='personal') = (personal_owner IS NOT NULL))
);
CREATE TABLE memberships (
    space_id text NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    user_id text NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role text NOT NULL CHECK(role IN ('owner','member')),
    PRIMARY KEY(space_id,user_id)
);
CREATE INDEX memberships_user ON memberships(user_id);
CREATE TABLE invitations (
    token_hash bytea PRIMARY KEY,
    space_id text NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    expires_at bigint NOT NULL,
    used_by text REFERENCES users(id)
);
CREATE TABLE objects (
    space_id text NOT NULL REFERENCES spaces(id),
    id text NOT NULL,
    kind text NOT NULL CHECK (kind IN ('document', 'component')),
    title text NOT NULL,
    revision bigint NOT NULL DEFAULT 0,
    created bigint NOT NULL,
    modified bigint NOT NULL,
    deleted boolean NOT NULL DEFAULT false,
    PRIMARY KEY(space_id, id)
);
CREATE TABLE revisions (
    space_id text NOT NULL,
    object_id text NOT NULL,
    revision bigint NOT NULL,
    request_id text NOT NULL,
    request_hash bytea NOT NULL,
    blob text NOT NULL,
    PRIMARY KEY(space_id, object_id, revision),
    UNIQUE(space_id, object_id, request_id),
    FOREIGN KEY(space_id, object_id) REFERENCES objects(space_id, id) ON DELETE CASCADE
);
