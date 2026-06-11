-- Message attachments for group chat
CREATE TABLE IF NOT EXISTS message_attachments (
    id            TEXT PRIMARY KEY,
    group_id      TEXT NOT NULL,
    message_id    TEXT,
    uploader_id   TEXT NOT NULL,
    filename      TEXT NOT NULL,
    content_type  TEXT NOT NULL,
    size          INTEGER NOT NULL,
    data          BLOB NOT NULL,
    created_at    INTEGER NOT NULL,
    FOREIGN KEY (group_id) REFERENCES groups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_message_attachments_group_id ON message_attachments(group_id);
CREATE INDEX IF NOT EXISTS idx_message_attachments_message_id ON message_attachments(message_id);
