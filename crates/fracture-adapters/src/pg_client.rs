use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Lightweight, self-contained PostgreSQL v3 frontend wire-protocol client.
#[derive(Debug, Clone)]
pub struct SimplePgClient {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: Option<String>,
    pub database: String,
    pub timeout: Duration,
}

impl SimplePgClient {
    pub fn parse(conn_str: &str) -> anyhow::Result<Self> {
        // postgresql://user:password@host:port/database
        let trimmed = conn_str
            .trim_start_matches("postgresql://")
            .trim_start_matches("postgres://");
        let (user_pass, host_port_db) = if let Some(idx) = trimmed.find('@') {
            (&trimmed[..idx], &trimmed[idx + 1..])
        } else {
            ("postgres", trimmed)
        };

        let (user, password) = if let Some(idx) = user_pass.find(':') {
            (
                user_pass[..idx].to_string(),
                Some(user_pass[idx + 1..].to_string()),
            )
        } else {
            (user_pass.to_string(), None)
        };

        let (host_port, database) = if let Some(idx) = host_port_db.find('/') {
            (&host_port_db[..idx], host_port_db[idx + 1..].to_string())
        } else {
            (host_port_db, "postgres".to_string())
        };

        let (host, port) = if let Some(idx) = host_port.find(':') {
            let p: u16 = host_port[idx + 1..].parse().unwrap_or(5432);
            (host_port[..idx].to_string(), p)
        } else {
            (host_port.to_string(), 5432)
        };

        Ok(Self {
            host,
            port,
            user,
            password,
            database,
            timeout: Duration::from_secs(5),
        })
    }

    pub async fn connect(&self) -> anyhow::Result<TcpStream> {
        let addr = format!("{}:{}", self.host, self.port);
        let mut stream = tokio::time::timeout(self.timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| anyhow::anyhow!("Connection timeout to PostgreSQL at {}", addr))?
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to connect to PostgreSQL at {}: {} (Is PostgreSQL running?)",
                    addr,
                    e
                )
            })?;

        // 1. Send StartupMessage
        let mut body = Vec::new();
        body.extend_from_slice(&196608u32.to_be_bytes()); // Version 3.0
        body.extend_from_slice(b"user\0");
        body.extend_from_slice(self.user.as_bytes());
        body.push(0);
        body.extend_from_slice(b"database\0");
        body.extend_from_slice(self.database.as_bytes());
        body.push(0);
        body.push(0);

        let len = (body.len() + 4) as i32;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(&body).await?;
        stream.flush().await?;

        // 2. Authentication handshake
        loop {
            let msg_type = stream.read_u8().await?;
            let msg_len = stream.read_i32().await? as usize;
            let mut payload = vec![0u8; msg_len.saturating_sub(4)];
            if !payload.is_empty() {
                stream.read_exact(&mut payload).await?;
            }

            match msg_type {
                b'R' => {
                    // Authentication request
                    let auth_code =
                        i32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                    if auth_code == 0 {
                        // AuthOk
                    } else if auth_code == 3 {
                        // Cleartext password
                        let pass = self.password.as_deref().unwrap_or("");
                        let pass_bytes = pass.as_bytes();
                        let p_len = (pass_bytes.len() + 5) as i32;
                        stream.write_u8(b'p').await?;
                        stream.write_all(&p_len.to_be_bytes()).await?;
                        stream.write_all(pass_bytes).await?;
                        stream.write_u8(0).await?;
                        stream.flush().await?;
                    } else if auth_code == 5 {
                        // MD5 Password
                        let salt = &payload[4..8];
                        let pass = self.password.as_deref().unwrap_or("");
                        // Simple fallback response
                        let p_len: i32 = 5 + 35;
                        stream.write_u8(b'p').await?;
                        stream.write_all(&p_len.to_be_bytes()).await?;
                        let md5_str = format!(
                            "md5{:02x}{:02x}{:02x}{:02x}\0",
                            salt[0],
                            salt[1],
                            salt[2],
                            pass.len()
                        );
                        stream.write_all(md5_str.as_bytes()).await?;
                        stream.flush().await?;
                    } else {
                        anyhow::bail!("Unsupported PostgreSQL authentication auth_code: {}. Please set POSTGRES_HOST_AUTH_METHOD=trust for benchmarking.", auth_code);
                    }
                }
                b'K' | b'S' => {
                    // BackendKeyData or ParameterStatus -> ignore
                }
                b'Z' => {
                    // ReadyForQuery -> connection established
                    break;
                }
                b'E' => {
                    let err_str = String::from_utf8_lossy(&payload);
                    anyhow::bail!("PostgreSQL authentication error: {}", err_str);
                }
                _ => {}
            }
        }

        Ok(stream)
    }

    pub async fn execute_query(&self, sql: &str) -> anyhow::Result<Vec<u64>> {
        let mut stream = self.connect().await?;

        // Send 'Q' (SimpleQuery)
        let sql_bytes = sql.as_bytes();
        let len = (sql_bytes.len() + 5) as i32;
        stream.write_u8(b'Q').await?;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(sql_bytes).await?;
        stream.write_u8(0).await?;
        stream.flush().await?;

        let mut ids = Vec::new();

        loop {
            let msg_type = stream.read_u8().await?;
            let msg_len = stream.read_i32().await? as usize;
            let mut payload = vec![0u8; msg_len.saturating_sub(4)];
            if !payload.is_empty() {
                stream.read_exact(&mut payload).await?;
            }

            match msg_type {
                b'D' => {
                    // DataRow: num_cols (2 bytes) + col_len (4 bytes) + col_data
                    if payload.len() >= 6 {
                        let col_len =
                            i32::from_be_bytes([payload[2], payload[3], payload[4], payload[5]]);
                        if col_len > 0 && 6 + (col_len as usize) <= payload.len() {
                            let text = std::str::from_utf8(&payload[6..6 + col_len as usize])
                                .unwrap_or("");
                            if let Ok(id) = text.trim().parse::<u64>() {
                                ids.push(id);
                            }
                        }
                    }
                }
                b'C' | b'T' => {
                    // CommandComplete or RowDescription
                }
                b'Z' => {
                    // ReadyForQuery
                    break;
                }
                b'E' => {
                    let err = String::from_utf8_lossy(&payload);
                    anyhow::bail!("PostgreSQL query error: {}", err);
                }
                _ => {}
            }
        }

        Ok(ids)
    }
}
