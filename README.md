# vibeserial

跨平台串口助手（串口收发精简版）。

## 当前实现

- Rust Workspace 多模块结构
- 串口会话抽象与内存传输实现（可用于后续接入真实串口驱动）
- 桌面入口：Tauri 后端命令与 Vue3 页面已联调
- 功能范围：仅保留串口参数设置与收发（ASCII/HEX 双格式编辑）

## 快速验证

```bash
cargo test
cargo run -p desktop -- list-ports
```

## 启动桌面界面（Tauri）

```bash
cd apps/ui
npm install
npm run tauri:dev
```

说明：
- 前端在 `apps/ui`
- Tauri 后端在 `apps/desktop/src-tauri`
- 当前串口层仍是 `MemoryTransport`，用于先打通桌面命令链路

## 一键启动

```bash
cd /home/ning/work/vibeserial
./start.sh
```

## 后续扩展建议

- 在 `serial-core` 中替换 `MemoryTransport` 为真实串口实现
- 在 `apps/desktop/src-tauri` 加入事件推送（实时数据流）
- 在 `apps/ui` 增加真实串口状态监控与会话标签页
