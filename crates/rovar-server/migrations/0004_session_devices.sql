ALTER TABLE sessions
    ADD COLUMN system TEXT NOT NULL DEFAULT '' CHECK (char_length(system) <= 128),
    ADD COLUMN device_name TEXT NOT NULL DEFAULT '' CHECK (char_length(device_name) <= 128),
    ADD COLUMN client TEXT NOT NULL DEFAULT '' CHECK (char_length(client) <= 128);
