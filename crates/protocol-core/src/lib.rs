use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumAlgorithm {
    Xor8,
    Crc16Modbus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameRule {
    /// 帧头字节序列。
    pub header: Vec<u8>,
    /// 帧尾字节序列。
    pub footer: Vec<u8>,
    /// 固定帧长度（包含头尾和校验位），`None` 表示按 footer 截断。
    pub fixed_length: Option<usize>,
    /// 校验算法。
    pub checksum: Option<ChecksumAlgorithm>,
}

impl Default for FrameRule {
    fn default() -> Self {
        Self {
            header: Vec::new(),
            footer: Vec::new(),
            fixed_length: None,
            checksum: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFrame {
    /// 原始帧数据。
    pub raw: Vec<u8>,
    /// 纯载荷数据（去头尾、去校验）。
    pub payload: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    FrameTooShort,
    ChecksumMismatch,
    InvalidFrame,
    InvalidModbus,
}

impl Display for ProtocolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ProtocolError::FrameTooShort => write!(f, "frame too short"),
            ProtocolError::ChecksumMismatch => write!(f, "checksum mismatch"),
            ProtocolError::InvalidFrame => write!(f, "invalid frame"),
            ProtocolError::InvalidModbus => write!(f, "invalid modbus rtu frame"),
        }
    }
}

impl std::error::Error for ProtocolError {}

pub type ProtocolResult<T> = Result<T, ProtocolError>;

pub fn xor8(data: &[u8]) -> u8 {
    data.iter().fold(0, |acc, b| acc ^ b)
}

pub fn crc16_modbus(data: &[u8]) -> u16 {
    let mut crc = 0xFFFF_u16;
    for byte in data {
        crc ^= *byte as u16;
        for _ in 0..8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

pub fn build_frame(payload: &[u8], rule: &FrameRule) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(&rule.header);
    frame.extend_from_slice(payload);

    if let Some(alg) = rule.checksum {
        match alg {
            ChecksumAlgorithm::Xor8 => frame.push(xor8(payload)),
            ChecksumAlgorithm::Crc16Modbus => {
                let crc = crc16_modbus(payload);
                frame.push((crc & 0xFF) as u8);
                frame.push((crc >> 8) as u8);
            }
        }
    }

    frame.extend_from_slice(&rule.footer);
    frame
}

pub fn split_frames(stream: &[u8], rule: &FrameRule) -> ProtocolResult<Vec<ParsedFrame>> {
    let chunks = if let Some(size) = rule.fixed_length {
        split_by_fixed_length(stream, size)
    } else {
        split_by_footer(stream, &rule.footer)
    };

    let mut out = Vec::new();
    for raw in chunks {
        let parsed = parse_single_frame(&raw, rule)?;
        out.push(parsed);
    }
    Ok(out)
}

fn split_by_fixed_length(stream: &[u8], size: usize) -> Vec<Vec<u8>> {
    if size == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut start = 0;
    while start + size <= stream.len() {
        out.push(stream[start..start + size].to_vec());
        start += size;
    }
    out
}

fn split_by_footer(stream: &[u8], footer: &[u8]) -> Vec<Vec<u8>> {
    if footer.is_empty() {
        return vec![stream.to_vec()];
    }

    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i + footer.len() <= stream.len() {
        if &stream[i..i + footer.len()] == footer {
            let end = i + footer.len();
            out.push(stream[start..end].to_vec());
            start = end;
            i = end;
            continue;
        }
        i += 1;
    }

    out
}

fn parse_single_frame(raw: &[u8], rule: &FrameRule) -> ProtocolResult<ParsedFrame> {
    if raw.len() < rule.header.len() + rule.footer.len() {
        return Err(ProtocolError::FrameTooShort);
    }

    if !rule.header.is_empty() && !raw.starts_with(&rule.header) {
        return Err(ProtocolError::InvalidFrame);
    }

    if !rule.footer.is_empty() && !raw.ends_with(&rule.footer) {
        return Err(ProtocolError::InvalidFrame);
    }

    let start = rule.header.len();
    let end = raw.len() - rule.footer.len();
    if start > end {
        return Err(ProtocolError::InvalidFrame);
    }

    let mut payload_with_checksum = raw[start..end].to_vec();
    let payload = if let Some(alg) = rule.checksum {
        match alg {
            ChecksumAlgorithm::Xor8 => {
                if payload_with_checksum.len() < 1 {
                    return Err(ProtocolError::FrameTooShort);
                }
                let checksum = payload_with_checksum
                    .pop()
                    .ok_or(ProtocolError::FrameTooShort)?;
                let payload = payload_with_checksum;
                if xor8(&payload) != checksum {
                    return Err(ProtocolError::ChecksumMismatch);
                }
                payload
            }
            ChecksumAlgorithm::Crc16Modbus => {
                if payload_with_checksum.len() < 2 {
                    return Err(ProtocolError::FrameTooShort);
                }
                let hi = payload_with_checksum
                    .pop()
                    .ok_or(ProtocolError::FrameTooShort)?;
                let lo = payload_with_checksum
                    .pop()
                    .ok_or(ProtocolError::FrameTooShort)?;
                let payload = payload_with_checksum;
                let expected = ((hi as u16) << 8) | lo as u16;
                if crc16_modbus(&payload) != expected {
                    return Err(ProtocolError::ChecksumMismatch);
                }
                payload
            }
        }
    } else {
        payload_with_checksum
    };

    Ok(ParsedFrame {
        raw: raw.to_vec(),
        payload,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModbusFunction {
    ReadHoldingRegisters = 0x03,
    WriteSingleRegister = 0x06,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModbusRtuPacket {
    /// 从站地址。
    pub slave: u8,
    /// 功能码。
    pub function: u8,
    /// 功能负载（不含 CRC）。
    pub data: Vec<u8>,
}

pub fn parse_modbus_rtu(frame: &[u8]) -> ProtocolResult<ModbusRtuPacket> {
    if frame.len() < 4 {
        return Err(ProtocolError::InvalidModbus);
    }

    let body_len = frame.len() - 2;
    let body = &frame[..body_len];
    let crc = ((frame[frame.len() - 1] as u16) << 8) | frame[frame.len() - 2] as u16;
    if crc16_modbus(body) != crc {
        return Err(ProtocolError::ChecksumMismatch);
    }

    Ok(ModbusRtuPacket {
        slave: body[0],
        function: body[1],
        data: body[2..].to_vec(),
    })
}

pub fn build_modbus_read_holding_registers(slave: u8, start_addr: u16, quantity: u16) -> Vec<u8> {
    let mut body = Vec::with_capacity(6);
    body.push(slave);
    body.push(ModbusFunction::ReadHoldingRegisters as u8);
    body.push((start_addr >> 8) as u8);
    body.push((start_addr & 0xFF) as u8);
    body.push((quantity >> 8) as u8);
    body.push((quantity & 0xFF) as u8);

    let crc = crc16_modbus(&body);
    body.push((crc & 0xFF) as u8);
    body.push((crc >> 8) as u8);
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_known_vector() {
        let data = b"123456789";
        assert_eq!(crc16_modbus(data), 0x4B37);
    }

    #[test]
    fn xor_checksum_works() {
        assert_eq!(xor8(&[0x01, 0x02, 0x03]), 0x00);
    }

    #[test]
    fn build_and_split_frame_with_xor() {
        let rule = FrameRule {
            header: vec![0xAA],
            footer: vec![0x55],
            fixed_length: None,
            checksum: Some(ChecksumAlgorithm::Xor8),
        };

        let frame = build_frame(&[0x10, 0x20], &rule);
        let parsed = split_frames(&frame, &rule).unwrap();

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].payload, vec![0x10, 0x20]);
    }

    #[test]
    fn parse_modbus_packet() {
        let req = build_modbus_read_holding_registers(1, 0x000A, 2);
        let pkt = parse_modbus_rtu(&req).unwrap();

        assert_eq!(pkt.slave, 1);
        assert_eq!(pkt.function, ModbusFunction::ReadHoldingRegisters as u8);
        assert_eq!(pkt.data, vec![0x00, 0x0A, 0x00, 0x02]);
    }
}
