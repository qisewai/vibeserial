use std::any::Any;
use std::collections::{HashMap, VecDeque};
use std::fmt::{Display, Formatter};

/// 文本编码模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingMode {
    Ascii,
    Utf8,
    Hex,
}

/// 串口校验位。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parity {
    None,
    Odd,
    Even,
}

/// 流控模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowControl {
    None,
    Software,
    Hardware,
}

/// 串口会话打开参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialOpenConfig {
    /// 连接目标（例如 COM3、/dev/ttyUSB0、tcp://127.0.0.1:9000）。
    pub endpoint: String,
    /// 波特率。
    pub baud_rate: u32,
    /// 数据位。
    pub data_bits: u8,
    /// 停止位。
    pub stop_bits: u8,
    /// 校验位。
    pub parity: Parity,
    /// 流控。
    pub flow_control: FlowControl,
    /// 展示编码。
    pub encoding: EncodingMode,
    /// 是否自动重连。
    pub auto_reconnect: bool,
}

impl Default for SerialOpenConfig {
    fn default() -> Self {
        Self {
            endpoint: "loopback".to_string(),
            baud_rate: 115_200,
            data_bits: 8,
            stop_bits: 1,
            parity: Parity::None,
            flow_control: FlowControl::None,
            encoding: EncodingMode::Utf8,
            auto_reconnect: true,
        }
    }
}

/// 会话概要信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionInfo {
    /// 会话唯一标识。
    pub session_id: String,
    /// 会话对应配置。
    pub config: SerialOpenConfig,
    /// 当前是否已连接。
    pub connected: bool,
    /// 重连次数。
    pub reconnect_count: u32,
}

#[derive(Debug)]
pub enum SerialError {
    AlreadyOpen(String),
    NotFound(String),
    NotOpen(String),
    Io(String),
}

impl Display for SerialError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SerialError::AlreadyOpen(id) => write!(f, "session already open: {id}"),
            SerialError::NotFound(id) => write!(f, "session not found: {id}"),
            SerialError::NotOpen(id) => write!(f, "session not open: {id}"),
            SerialError::Io(msg) => write!(f, "io error: {msg}"),
        }
    }
}

impl std::error::Error for SerialError {}

pub type SerialResult<T> = Result<T, SerialError>;

/// 传输抽象，后续可替换为真实串口实现。
pub trait Transport: Send {
    fn open(&mut self, config: &SerialOpenConfig) -> SerialResult<()>;
    fn close(&mut self) -> SerialResult<()>;
    fn send(&mut self, payload: &[u8]) -> SerialResult<()>;
    fn receive(&mut self) -> SerialResult<Vec<u8>>;
    fn is_open(&self) -> bool;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// 内存传输，用于测试与离线联调。
#[derive(Default)]
pub struct MemoryTransport {
    /// 当前是否打开。
    opened: bool,
    /// 模拟接收缓冲。
    inbound: VecDeque<u8>,
    /// 模拟发送缓冲。
    outbound: Vec<u8>,
}

impl MemoryTransport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_inbound(&mut self, data: &[u8]) {
        self.inbound.extend(data.iter().copied());
    }

    pub fn take_outbound(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.outbound)
    }
}

impl Transport for MemoryTransport {
    fn open(&mut self, _config: &SerialOpenConfig) -> SerialResult<()> {
        self.opened = true;
        Ok(())
    }

    fn close(&mut self) -> SerialResult<()> {
        self.opened = false;
        Ok(())
    }

    fn send(&mut self, payload: &[u8]) -> SerialResult<()> {
        if !self.opened {
            return Err(SerialError::Io("transport closed".to_string()));
        }
        self.outbound.extend_from_slice(payload);
        Ok(())
    }

    fn receive(&mut self) -> SerialResult<Vec<u8>> {
        if !self.opened {
            return Err(SerialError::Io("transport closed".to_string()));
        }

        let len = self.inbound.len();
        let mut out = Vec::with_capacity(len);
        out.extend(self.inbound.drain(..));
        Ok(out)
    }

    fn is_open(&self) -> bool {
        self.opened
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

struct Session {
    /// 会话参数。
    config: SerialOpenConfig,
    /// 底层传输实例。
    transport: Box<dyn Transport>,
    /// 累计重连次数。
    reconnect_count: u32,
}

/// 串口会话管理器。
#[derive(Default)]
pub struct SerialSessionManager {
    /// 全部会话集合，键为 session_id。
    sessions: HashMap<String, Session>,
}

impl SerialSessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_session(
        &mut self,
        session_id: impl Into<String>,
        config: SerialOpenConfig,
        mut transport: Box<dyn Transport>,
    ) -> SerialResult<()> {
        let session_id = session_id.into();
        if self.sessions.contains_key(&session_id) {
            return Err(SerialError::AlreadyOpen(session_id));
        }

        transport.open(&config)?;
        self.sessions.insert(
            session_id,
            Session {
                config,
                transport,
                reconnect_count: 0,
            },
        );

        Ok(())
    }

    pub fn close_session(&mut self, session_id: &str) -> SerialResult<()> {
        let mut session = self
            .sessions
            .remove(session_id)
            .ok_or_else(|| SerialError::NotFound(session_id.to_string()))?;
        session.transport.close()?;
        Ok(())
    }

    pub fn send(&mut self, session_id: &str, payload: &[u8]) -> SerialResult<()> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SerialError::NotFound(session_id.to_string()))?;

        if !session.transport.is_open() {
            return Err(SerialError::NotOpen(session_id.to_string()));
        }
        session.transport.send(payload)
    }

    pub fn receive(&mut self, session_id: &str) -> SerialResult<Vec<u8>> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| SerialError::NotFound(session_id.to_string()))?;

        if !session.transport.is_open() {
            if session.config.auto_reconnect {
                session.transport.open(&session.config)?;
                session.reconnect_count += 1;
            } else {
                return Err(SerialError::NotOpen(session_id.to_string()));
            }
        }

        session.transport.receive()
    }

    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions
            .iter()
            .map(|(session_id, session)| SessionInfo {
                session_id: session_id.clone(),
                config: session.config.clone(),
                connected: session.transport.is_open(),
                reconnect_count: session.reconnect_count,
            })
            .collect()
    }

    pub fn transport_mut<T: 'static>(&mut self, session_id: &str) -> Option<&mut T> {
        self.sessions
            .get_mut(session_id)
            .and_then(|s| s.transport.as_any_mut().downcast_mut::<T>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_transport_send_and_receive() {
        let mut transport = MemoryTransport::new();
        let config = SerialOpenConfig::default();

        transport.open(&config).unwrap();
        transport.send(b"abc").unwrap();
        transport.push_inbound(b"xyz");

        assert_eq!(transport.take_outbound(), b"abc");
        assert_eq!(transport.receive().unwrap(), b"xyz");
    }

    #[test]
    fn session_manager_open_send_receive() {
        let mut manager = SerialSessionManager::new();
        let config = SerialOpenConfig::default();

        manager
            .open_session("s1", config, Box::new(MemoryTransport::new()))
            .unwrap();

        manager.send("s1", b"hello").unwrap();
        manager
            .transport_mut::<MemoryTransport>("s1")
            .unwrap()
            .push_inbound(b"world");

        let data = manager.receive("s1").unwrap();
        assert_eq!(data, b"world");
    }
}
