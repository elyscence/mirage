CREATE TABLE IF NOT EXISTS scans (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    ip          TEXT NOT NULL,
    timestamp   TEXT NOT NULL,
    protocol    INTEGER,
    address     TEXT,
    port        INTEGER,
    next_state  INTEGER,
    reached_ping BOOLEAN DEFAULT FALSE
)
