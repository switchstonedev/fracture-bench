use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Lightweight HTTP/1.1 client for REST-based vector databases (Qdrant, Milvus).
/// Avoids external HTTP dependencies and operates directly over Tokio's async TCP primitives.
#[derive(Debug, Clone)]
pub struct SimpleHttpClient {
    pub host: String,
    pub port: u16,
    pub timeout: Duration,
}

impl SimpleHttpClient {
    /// Parses a URL such as "http://127.0.0.1:6333" or "127.0.0.1:6333".
    pub fn new(endpoint: &str) -> anyhow::Result<Self> {
        let clean = endpoint
            .trim_start_matches("http://")
            .trim_start_matches("https://");

        let parts: Vec<&str> = clean.split(':').collect();
        let (host, port) = if parts.len() == 2 {
            let port: u16 = parts[1].trim_end_matches('/').parse()?;
            (parts[0].to_string(), port)
        } else {
            (parts[0].trim_end_matches('/').to_string(), 80)
        };

        Ok(Self {
            host,
            port,
            timeout: Duration::from_secs(5),
        })
    }

    /// Sends an HTTP request and returns the status code and JSON response value.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> anyhow::Result<(u16, serde_json::Value)> {
        let addr = format!("{}:{}", self.host, self.port);
        let mut stream = tokio::time::timeout(self.timeout, TcpStream::connect(&addr))
            .await
            .map_err(|_| anyhow::anyhow!("Connection timeout to {}", addr))?
            .map_err(|e| anyhow::anyhow!("Failed to connect to {}: {} (Is the database running?)", addr, e))?;

        let body_bytes = if let Some(b) = body {
            serde_json::to_vec(b)?
        } else {
            Vec::new()
        };

        let request_header = format!(
            "{} {} HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            method,
            path,
            self.host,
            self.port,
            body_bytes.len()
        );

        stream.write_all(request_header.as_bytes()).await?;
        if !body_bytes.is_empty() {
            stream.write_all(&body_bytes).await?;
        }
        stream.flush().await?;

        // Read full response
        let mut response_buf = Vec::with_capacity(4096);
        tokio::time::timeout(self.timeout, stream.read_to_end(&mut response_buf))
            .await
            .map_err(|_| anyhow::anyhow!("Read timeout from {}", addr))??;

        // Parse status line and separate headers from body
        let header_end = response_buf
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .unwrap_or(0);

        let header_str = String::from_utf8_lossy(&response_buf[..header_end]);
        let mut status_code = 200u16;

        if let Some(status_line) = header_str.lines().next() {
            let tokens: Vec<&str> = status_line.split_whitespace().collect();
            if tokens.len() >= 2 {
                if let Ok(code) = tokens[1].parse::<u16>() {
                    status_code = code;
                }
            }
        }

        let body_slice = if header_end + 4 <= response_buf.len() {
            &response_buf[header_end + 4..]
        } else {
            &[]
        };

        let json_val: serde_json::Value = if !body_slice.is_empty() {
            serde_json::from_slice(body_slice).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        };

        Ok((status_code, json_val))
    }
}

