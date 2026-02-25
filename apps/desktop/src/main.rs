use gateway_client::{GatewayConfig, TcpGatewayClient};
use project_store::{ProjectSnapshot, ProjectStore, SessionProfile};
use protocol_core::{build_modbus_read_holding_registers, parse_modbus_rtu};
use serial_core::{MemoryTransport, SerialOpenConfig, SerialSessionManager};
use std::env;
use std::path::PathBuf;
use task_engine::{FailurePolicy, TaskAction, TaskEngine, TaskIo, TaskSpec, Trigger};

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("list-ports") => {
            for port in list_ports() {
                println!("{port}");
            }
            Ok(())
        }
        Some("demo-session") => demo_session(),
        Some("modbus-demo") => modbus_demo(),
        Some("run-task") => run_task_demo(),
        Some("save-project") => save_project_demo(),
        Some("gateway-ping") => gateway_ping(&args),
        _ => {
            print_help();
            Ok(())
        }
    }
}

fn print_help() {
    println!("vibeserial desktop demo");
    println!("commands:");
    println!("  list-ports");
    println!("  demo-session");
    println!("  modbus-demo");
    println!("  run-task");
    println!("  save-project");
    println!("  gateway-ping <host> <port>");
}

fn list_ports() -> Vec<String> {
    vec![
        "COM1".to_string(),
        "COM2".to_string(),
        "/dev/ttyUSB0".to_string(),
        "/dev/ttyS0".to_string(),
    ]
}

fn demo_session() -> Result<(), String> {
    let mut manager = SerialSessionManager::new();
    let config = SerialOpenConfig {
        endpoint: "loopback".to_string(),
        ..SerialOpenConfig::default()
    };

    manager
        .open_session("demo", config, Box::new(MemoryTransport::new()))
        .map_err(|e| e.to_string())?;

    manager
        .send("demo", b"hello device")
        .map_err(|e| e.to_string())?;

    let transport = manager
        .transport_mut::<MemoryTransport>("demo")
        .ok_or_else(|| "transport type mismatch".to_string())?;
    transport.push_inbound(b"hello host");

    let rx = manager.receive("demo").map_err(|e| e.to_string())?;
    println!("received: {}", String::from_utf8_lossy(&rx));
    Ok(())
}

fn modbus_demo() -> Result<(), String> {
    let packet = build_modbus_read_holding_registers(1, 0x0000, 2);
    let parsed = parse_modbus_rtu(&packet).map_err(|e| e.to_string())?;

    println!(
        "slave={} func={} data={:02X?}",
        parsed.slave, parsed.function, parsed.data
    );
    Ok(())
}

struct SessionIo {
    /// 串口会话管理器。
    manager: SerialSessionManager,
    /// 任务绑定会话 ID。
    session_id: String,
    /// 最近接收缓存。
    latest_rx: Vec<u8>,
}

impl TaskIo for SessionIo {
    fn send(&mut self, data: &[u8]) -> Result<(), String> {
        self.manager
            .send(&self.session_id, data)
            .map_err(|e| e.to_string())
    }

    fn latest_receive(&self) -> &[u8] {
        &self.latest_rx
    }
}

fn run_task_demo() -> Result<(), String> {
    let mut manager = SerialSessionManager::new();
    manager
        .open_session(
            "task-demo",
            SerialOpenConfig::default(),
            Box::new(MemoryTransport::new()),
        )
        .map_err(|e| e.to_string())?;

    let transport = manager
        .transport_mut::<MemoryTransport>("task-demo")
        .ok_or_else(|| "transport type mismatch".to_string())?;
    transport.push_inbound(&[0x10, 0x20, 0x30]);

    let latest_rx = manager.receive("task-demo").map_err(|e| e.to_string())?;
    let mut io = SessionIo {
        manager,
        session_id: "task-demo".to_string(),
        latest_rx,
    };

    let spec = TaskSpec {
        task_id: "task-1".to_string(),
        name: "quick-check".to_string(),
        trigger: Trigger::Manual,
        failure_policy: FailurePolicy::StopOnFail,
        repeat: 1,
        actions: vec![
            TaskAction::SendHex(vec![0xAA, 0xBB]),
            TaskAction::AssertContains(vec![0x20]),
        ],
    };

    let report = TaskEngine::run(&mut io, &spec);
    println!(
        "task success={} actions={}",
        report.success, report.executed_actions
    );
    for log in report.logs {
        println!("  {log}");
    }

    Ok(())
}

fn save_project_demo() -> Result<(), String> {
    let mut root = PathBuf::from(".vibeserial");
    root.push("projects");

    let store = ProjectStore::new(&root);
    let mut snapshot = ProjectSnapshot::new("demo-project", "串口调试示例项目");
    snapshot.sessions.push(SessionProfile {
        session_id: "session-1".to_string(),
        endpoint: "/dev/ttyUSB0".to_string(),
        baud_rate: 115_200,
    });
    snapshot.task_ids.push("task-1".to_string());
    snapshot.log_files.push("logs/demo.log".to_string());
    snapshot.touch();

    let path = store.save(&snapshot).map_err(|e| e.to_string())?;
    println!("saved snapshot: {}", path.display());
    Ok(())
}

fn gateway_ping(args: &[String]) -> Result<(), String> {
    let host = args.get(2).ok_or_else(|| "missing host".to_string())?;
    let port = args
        .get(3)
        .ok_or_else(|| "missing port".to_string())?
        .parse::<u16>()
        .map_err(|_| "invalid port".to_string())?;

    let mut client = TcpGatewayClient::new(GatewayConfig {
        host: host.clone(),
        port,
        ..GatewayConfig::default()
    });

    client.connect().map_err(|e| e.to_string())?;
    client.send(b"ping").map_err(|e| e.to_string())?;

    match client.receive_once(64) {
        Ok(data) => {
            println!("gateway replied: {:02X?}", data);
            Ok(())
        }
        Err(err) => {
            println!("gateway read timeout or error: {err}");
            Ok(())
        }
    }
}
