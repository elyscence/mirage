# mirage

A Minecraft Java Edition honeypot that masquerades as a real server to collect data about scanners and bots crawling the internet.

Mirage listens on port 25565, speaks the Minecraft status protocol, and logs every connection — IP address, client version, and whether the client bothered to send a ping.

## What it does

- Accepts Minecraft handshake and status requests
- Responds with a configurable fake server status
- Handles ping/pong
- Logs all connections to a SQLite database

## Why

Thousands of bots scan the internet for Minecraft servers every day. Mirage sits quietly and watches them, collecting data about who they are, what protocol versions they use, and how they behave. Connections with `next_state=2` (login attempts) are logged separately — those are the interesting ones.

## Stack

- **Rust** — because you want it fast and you want it correct
- **Tokio** — async runtime, handles thousands of concurrent connections
- **sqlx + SQLite** — lightweight, zero-ops database for connection logs

## Configuration

```toml
[server]
host = "0.0.0.0"
port = 25565

[motd]
version = "1.21"
max_players = 20
description = "mirage"
```

## Running

```bash
cargo run --release
```

## What the data looks like

| field | description |
|---|---|
| `ip` | source IP address |
| `protocol` | client protocol version — identifies which scanner |
| `address` | server address the scanner used in handshake |
| `next_state` | 1 = status check, 2 = login attempt |
| `reached_ping` | whether the client sent a ping after status |
| `timestamp` | UTC time of connection |

## Notes

Mirage does not implement the full Minecraft protocol. It speaks just enough to look like a real server to a scanner. No authentication, no game state, no chunks.
