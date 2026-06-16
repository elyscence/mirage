use std::net::SocketAddr;

use anyhow::Result;
use serde::Deserialize;
use sqlx::{Sqlite, SqlitePool, migrate::MigrateDatabase};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tracing::{info, warn};

#[derive(Deserialize)]
struct Config {
    server: ServerConfig,
    database: DatabaseConfig,
    motd: MotdConfig,
}

#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    port: u16,
}

#[derive(Deserialize)]
struct DatabaseConfig {
    url: String,
}

#[derive(Deserialize)]
struct MotdConfig {
    version: String,
    protocol: u16,
    max_players: u16,
    description: String,
}

struct ConnectionData {
    protocol: i32,
    ip: String,
    port: u32,
    timestamp: String,
    next_state: i32,
    reached_ping: bool,
    username: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let config: Config = toml::from_str(&std::fs::read_to_string("config.toml")?)?;
    let listener =
        TcpListener::bind(format!("{}:{}", config.server.host, config.server.port)).await?;

    let json = format!(
        r#"{{"version":{{"name":"{}","protocol":{}}},"players":{{"max":{},"online":0,"sample":[]}},"description":{{"text":"{}"}}}}"#,
        config.motd.version, config.motd.protocol, config.motd.max_players, config.motd.description
    );

    if !Sqlite::database_exists(&config.database.url).await? {
        info!("Creating database: {}", &config.database.url);
        match Sqlite::create_database(&config.database.url).await {
            Ok(_) => info!("Successfully created db"),
            Err(err) => {
                warn!("An error occured while trying to create db: {}", err);
                std::process::exit(1);
            }
        }
    }

    let pool = SqlitePool::connect(&config.database.url).await?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to connect to database");

    loop {
        let (mut socket, addr) = listener.accept().await?;

        let pool_clone = pool.clone();
        let json_clone = json.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(&mut socket, &json_clone, &pool_clone, addr).await {
                warn!("ошибка соединения: {}", e);
            }
        });
    }
}

async fn handle_connection(
    socket: &mut TcpStream,
    json: &str,
    pool: &SqlitePool,
    addr: SocketAddr,
) -> Result<()> {
    let mut reached_ping = false;
    let mut username: Option<String> = None;

    let _length = read_varint(socket).await?;
    let _id = read_varint(socket).await?;
    let protocol = read_varint(socket).await?;
    let address_length = read_varint(socket).await?;

    let mut addr_buf = vec![0u8; address_length as usize];
    socket.read_exact(&mut addr_buf).await?;
    let _address = String::from_utf8_lossy(&addr_buf);

    let mut port_buf = [0u8; 2];
    socket.read_exact(&mut port_buf).await?;
    let port = u16::from_be_bytes(port_buf);

    let next_state = read_varint(socket).await?;

    match next_state {
        1 => {
            let _length = read_varint(socket).await?;
            let id = read_varint(socket).await?;

            if id == 0 {
                handle_status(socket, json).await?;
                let _length = read_varint(socket).await?;
                let id = read_varint(socket).await?;
                let mut ping_buf = [0u8; 8];
                socket.read_exact(&mut ping_buf).await?;
                if id == 1 {
                    reached_ping = handle_ping(socket, ping_buf).await?;
                };
            }
        }
        2 => {
            let _length = read_varint(socket).await?;
            let _id = read_varint(socket).await?;
            let username_length = read_varint(socket).await?;
            let mut username_buf = vec![0u8; username_length as usize];
            socket.read_exact(&mut username_buf).await?;
            username = Some(String::from_utf8_lossy(&username_buf).to_string());
        }
        _ => info!("something other"),
    };

    info!("next state: {next_state}");

    let connection_data = ConnectionData {
        protocol,
        ip: addr.ip().to_string(),
        port: port as u32,
        timestamp: chrono::Utc::now().to_rfc3339(),
        next_state,
        reached_ping,
        username,
    };

    add_connection(pool, connection_data).await?;

    Ok(())
}

async fn handle_status(socket: &mut TcpStream, json: &str) -> Result<()> {
    let json_len = json.len() as i32;
    let packet_length = varint_size(1) + varint_size(json_len) + json_len;
    write_varint(socket, packet_length).await?;
    write_varint(socket, 0).await?;
    write_varint(socket, json_len).await?;
    socket.write_all(json.as_bytes()).await?;
    info!("отправил статус клиенту");
    Ok(())
}

async fn handle_ping(socket: &mut TcpStream, ping_buf: [u8; 8]) -> Result<bool> {
    write_varint(socket, 9).await?;
    write_varint(socket, 1).await?;
    socket.write_all(&ping_buf).await?;
    info!("отправил ответ пинга");
    Ok(true)
}

async fn read_varint(socket: &mut TcpStream) -> Result<i32> {
    let mut buf = [0u8; 1];
    let mut ans = 0;
    for i in 0..5 {
        socket.read_exact(&mut buf).await?;
        ans |= ((buf[0] & 0x7F) as i32) << (7 * i);
        if buf[0] & 0x80 == 0 {
            break;
        }
    }

    Ok(ans)
}

async fn write_varint(socket: &mut TcpStream, mut value: i32) -> Result<()> {
    let mut buf = [0];
    loop {
        buf[0] = (value & 0x7F) as u8;
        value = (value >> 7) & (i32::MAX >> 6);
        if value != 0 {
            buf[0] |= 0x80;
        }
        socket.write_all(&buf).await?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

fn varint_size(mut value: i32) -> i32 {
    let mut varint = 0;
    loop {
        if value / 128 != 0 {
            varint += 1;
            value /= 128;
        } else {
            varint += 1;
            break;
        }
    }
    varint
}

async fn add_connection(pool: &SqlitePool, connection: ConnectionData) -> Result<()> {
    sqlx::query!(
        "INSERT INTO scans (protocol, ip, port, timestamp, next_state, reached_ping, username) VALUES (?, ?, ?, ?, ?, ?, ?)",
        connection.protocol,
        connection.ip,
        connection.port,
        connection.timestamp,
        connection.next_state,
        connection.reached_ping,
        connection.username,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_varint_size() {
        assert_eq!(varint_size(0), 1);
        assert_eq!(varint_size(127), 1);
        assert_eq!(varint_size(128), 2);
        assert_eq!(varint_size(300), 2);
        assert_eq!(varint_size(16383), 2);
        assert_eq!(varint_size(16384), 3);
    }
}
