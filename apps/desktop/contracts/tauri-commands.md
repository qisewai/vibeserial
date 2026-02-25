# Tauri Commands 契约草案

> 当前仓库先实现 Rust 核心库与 CLI 入口，后续在 `src-tauri` 中按本契约挂接命令。

## 当前已实现命令（2026-02-24）

- `serial_list_ports`
- `serial_open`
- `serial_close`
- `serial_send`
- `serial_receive`
- `serial_mock_push_inbound`
- `serial_list_sessions`

## 串口会话

### `serial_list_ports() -> PortInfo[]`
返回系统串口端口列表。

### `serial_open(session_id, config) -> Result<()>`
打开会话并绑定配置。

### `serial_close(session_id) -> Result<()>`
关闭会话。

### `serial_send(session_id, packet) -> Result<()>`
发送数据。

### `session_subscribe(session_id) -> EventStreamKey`
订阅接收事件流。

## 自动化任务

### `task_run(task_id) -> TaskRunId`
启动任务。

### `task_stop(task_run_id) -> Result<()>`
停止任务。

## 远程网关

### `gateway_connect(config) -> Result<ConnId>`
连接 TCP 网关。

### `gateway_disconnect(conn_id) -> Result<()>`
断开网关连接。

## 更新

### `update_check() -> UpdateInfo`
检查 GitHub Releases 更新。

### `update_install(version) -> Result<()>`
下载并安装更新。
