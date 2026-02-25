use std::fmt::{Display, Formatter};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayConfig {
    /// 网关主机名或 IP。
    pub host: String,
    /// 网关端口。
    pub port: u16,
    /// 连接超时毫秒。
    pub connect_timeout_ms: u64,
    /// 读取超时毫秒。
    pub read_timeout_ms: u64,
    /// 写入超时毫秒。
    pub write_timeout_ms: u64,
    /// 是否自动重连。
    pub auto_reconnect: bool,
}

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 9000,
            connect_timeout_ms: 1_000,
            read_timeout_ms: 1_000,
            write_timeout_ms: 1_000,
            auto_reconnect: true,
        }
    }
}

#[derive(Debug)]
pub enum GatewayError {
    Resolve(String),
    Connect(String),
    Io(String),
    NotConnected,
}

impl Display for GatewayError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            GatewayError::Resolve(msg) => write!(f, "resolve error: {msg}"),
            GatewayError::Connect(msg) => write!(f, "connect error: {msg}"),
            GatewayError::Io(msg) => write!(f, "io error: {msg}"),
            GatewayError::NotConnected => write!(f, "not connected"),
        }
    }
}

impl std::error::Error for GatewayError {}

pub type GatewayResult<T> = Result<T, GatewayError>;

pub struct TcpGatewayClient {
    /// 网关连接配置。
    config: GatewayConfig,
    /// 当前 TCP 连接。
    stream: Option<TcpStream>,
    /// 累计重连次数。
    reconnect_count: u32,
}

impl TcpGatewayClient {
    pub fn new(config: GatewayConfig) -> Self {
        Self {
            config,
            stream: None,
            reconnect_count: 0,
        }
    }

    pub fn connect(&mut self) -> GatewayResult<()> {
        let addr = resolve_addr(&self.config.host, self.config.port)?;
        let stream = TcpStream::connect_timeout(
            &addr,
            Duration::from_millis(self.config.connect_timeout_ms),
        )
        .map_err(|e| GatewayError::Connect(e.to_string()))?;

        stream
            .set_read_timeout(Some(Duration::from_millis(self.config.read_timeout_ms)))
            .map_err(|e| GatewayError::Io(e.to_string()))?;
        stream
            .set_write_timeout(Some(Duration::from_millis(self.config.write_timeout_ms)))
            .map_err(|e| GatewayError::Io(e.to_string()))?;

        self.stream = Some(stream);
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self.stream = None;
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    pub fn reconnect_count(&self) -> u32 {
        self.reconnect_count
    }

    pub fn send(&mut self, data: &[u8]) -> GatewayResult<()> {
        self.ensure_connected()?;
        let stream = self.stream.as_mut().ok_or(GatewayError::NotConnected)?;

        stream
            .write_all(data)
            .map_err(|e| GatewayError::Io(e.to_string()))?;
        stream.flush().map_err(|e| GatewayError::Io(e.to_string()))
    }

    pub fn receive_once(&mut self, max_len: usize) -> GatewayResult<Vec<u8>> {
        self.ensure_connected()?;
        let stream = self.stream.as_mut().ok_or(GatewayError::NotConnected)?;

        let mut buf = vec![0u8; max_len.max(1)];
        let size = stream
            .read(&mut buf)
            .map_err(|e| GatewayError::Io(e.to_string()))?;
        buf.truncate(size);
        Ok(buf)
    }

    fn ensure_connected(&mut self) -> GatewayResult<()> {
        if self.stream.is_some() {
            return Ok(());
        }

        if !self.config.auto_reconnect {
            return Err(GatewayError::NotConnected);
        }

        self.connect()?;
        self.reconnect_count += 1;
        Ok(())
    }
}

fn resolve_addr(host: &str, port: u16) -> GatewayResult<SocketAddr> {
    let mut addrs = (host, port)
        .to_socket_addrs()
        .map_err(|e| GatewayError::Resolve(e.to_string()))?;

    addrs
        .next()
        .ok_or_else(|| GatewayError::Resolve("empty addr result".to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_addr_should_work_for_ip() {
        let addr = resolve_addr("127.0.0.1", 9000).unwrap();
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert_eq!(addr.port(), 9000);
    }

    #[test]
    fn send_should_fail_when_not_connected_and_no_reconnect() {
        let mut client = TcpGatewayClient::new(GatewayConfig {
            auto_reconnect: false,
            ..GatewayConfig::default()
        });

        let err = client.send(b"ping").unwrap_err();
        assert!(matches!(err, GatewayError::NotConnected));
    }
}
