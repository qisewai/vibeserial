use serde::{Deserialize, Serialize};
use serial_core::{FlowControl, MemoryTransport, Parity, SerialOpenConfig, SerialSessionManager};
use std::sync::Mutex;

struct AppState {
    /// 串口会话管理器，统一管理全部会话。
    session_manager: Mutex<SerialSessionManager>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OpenSessionRequest {
    /// 会话唯一标识。
    session_id: String,
    /// 串口端点（示例：COM3 / /dev/ttyUSB0 / loopback）。
    endpoint: String,
    /// 波特率。
    baud_rate: u32,
    /// 数据位（可选，默认 8）。
    data_bits: Option<u8>,
    /// 停止位（可选，默认 1）。
    stop_bits: Option<u8>,
    /// 校验位（可选：none/odd/even）。
    parity: Option<String>,
    /// 流控（可选：none/software/hardware）。
    flow_control: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SendRequest {
    /// 会话唯一标识。
    session_id: String,
    /// 十六进制字符串（支持空格分隔）。
    hex_payload: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionStateDto {
    /// 会话标识。
    session_id: String,
    /// 连接端点。
    endpoint: String,
    /// 波特率。
    baud_rate: u32,
    /// 数据位。
    data_bits: u8,
    /// 停止位。
    stop_bits: u8,
    /// 校验位文本。
    parity: String,
    /// 流控文本。
    flow_control: String,
    /// 是否在线。
    connected: bool,
    /// 重连次数。
    reconnect_count: u32,
}

#[tauri::command]
fn serial_list_ports() -> Vec<String> {
    // 当前先返回模拟端口，后续替换成真实端口枚举。
    vec![
        "COM1".to_string(),
        "COM2".to_string(),
        "/dev/ttyUSB0".to_string(),
        "/dev/ttyS0".to_string(),
        "loopback".to_string(),
    ]
}

#[tauri::command]
fn serial_open(state: tauri::State<'_, AppState>, req: OpenSessionRequest) -> Result<(), String> {
    let mut manager = state
        .session_manager
        .lock()
        .map_err(|_| "session manager lock poisoned".to_string())?;

    let config = SerialOpenConfig {
        endpoint: req.endpoint,
        baud_rate: req.baud_rate,
        data_bits: req.data_bits.unwrap_or(8),
        stop_bits: req.stop_bits.unwrap_or(1),
        parity: parse_parity(req.parity.as_deref())?,
        flow_control: parse_flow_control(req.flow_control.as_deref())?,
        ..SerialOpenConfig::default()
    };

    manager
        .open_session(req.session_id, config, Box::new(MemoryTransport::new()))
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn serial_close(state: tauri::State<'_, AppState>, session_id: String) -> Result<(), String> {
    let mut manager = state
        .session_manager
        .lock()
        .map_err(|_| "session manager lock poisoned".to_string())?;
    manager
        .close_session(&session_id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn serial_send(state: tauri::State<'_, AppState>, req: SendRequest) -> Result<(), String> {
    let payload = parse_hex_payload(&req.hex_payload)?;

    let mut manager = state
        .session_manager
        .lock()
        .map_err(|_| "session manager lock poisoned".to_string())?;

    manager
        .send(&req.session_id, &payload)
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn serial_receive(state: tauri::State<'_, AppState>, session_id: String) -> Result<String, String> {
    let mut manager = state
        .session_manager
        .lock()
        .map_err(|_| "session manager lock poisoned".to_string())?;

    let data = manager.receive(&session_id).map_err(|e| e.to_string())?;
    Ok(to_hex_string(&data))
}

#[tauri::command]
fn serial_mock_push_inbound(
    state: tauri::State<'_, AppState>,
    session_id: String,
    hex_payload: String,
) -> Result<(), String> {
    let data = parse_hex_payload(&hex_payload)?;

    let mut manager = state
        .session_manager
        .lock()
        .map_err(|_| "session manager lock poisoned".to_string())?;

    let transport = manager
        .transport_mut::<MemoryTransport>(&session_id)
        .ok_or_else(|| "session not found or transport mismatch".to_string())?;
    transport.push_inbound(&data);

    Ok(())
}

#[tauri::command]
fn serial_list_sessions(state: tauri::State<'_, AppState>) -> Result<Vec<SessionStateDto>, String> {
    let manager = state
        .session_manager
        .lock()
        .map_err(|_| "session manager lock poisoned".to_string())?;

    let sessions = manager
        .list_sessions()
        .into_iter()
        .map(|s| SessionStateDto {
            session_id: s.session_id,
            endpoint: s.config.endpoint,
            baud_rate: s.config.baud_rate,
            data_bits: s.config.data_bits,
            stop_bits: s.config.stop_bits,
            parity: parity_to_string(s.config.parity),
            flow_control: flow_control_to_string(s.config.flow_control),
            connected: s.connected,
            reconnect_count: s.reconnect_count,
        })
        .collect();

    Ok(sessions)
}

fn parse_hex_payload(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    for token in input
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|s| !s.is_empty())
    {
        let normalized = token
            .trim()
            .trim_start_matches("0x")
            .trim_start_matches("0X");
        if normalized.is_empty() {
            continue;
        }

        let byte = u8::from_str_radix(normalized, 16)
            .map_err(|_| format!("invalid hex token: {token}"))?;
        out.push(byte);
    }

    if out.is_empty() {
        return Err("payload is empty".to_string());
    }

    Ok(out)
}

fn to_hex_string(data: &[u8]) -> String {
    data.iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

fn parse_parity(input: Option<&str>) -> Result<Parity, String> {
    match input.unwrap_or("none").to_ascii_lowercase().as_str() {
        "none" => Ok(Parity::None),
        "odd" => Ok(Parity::Odd),
        "even" => Ok(Parity::Even),
        other => Err(format!("invalid parity: {other}")),
    }
}

fn parse_flow_control(input: Option<&str>) -> Result<FlowControl, String> {
    match input.unwrap_or("none").to_ascii_lowercase().as_str() {
        "none" => Ok(FlowControl::None),
        "software" => Ok(FlowControl::Software),
        "hardware" => Ok(FlowControl::Hardware),
        other => Err(format!("invalid flow control: {other}")),
    }
}

fn parity_to_string(parity: Parity) -> String {
    match parity {
        Parity::None => "none".to_string(),
        Parity::Odd => "odd".to_string(),
        Parity::Even => "even".to_string(),
    }
}

fn flow_control_to_string(mode: FlowControl) -> String {
    match mode {
        FlowControl::None => "none".to_string(),
        FlowControl::Software => "software".to_string(),
        FlowControl::Hardware => "hardware".to_string(),
    }
}

fn main() {
    let state = AppState {
        session_manager: Mutex::new(SerialSessionManager::new()),
    };

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            serial_list_ports,
            serial_open,
            serial_close,
            serial_send,
            serial_receive,
            serial_mock_push_inbound,
            serial_list_sessions
        ])
        .run(tauri::generate_context!())
        .expect("failed to run tauri app");
}
