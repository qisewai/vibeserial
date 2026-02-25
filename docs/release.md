# 发布与更新流程（v1）

## 目标

- 三端安装包：Windows MSI、macOS DMG、Linux AppImage
- 更新源：GitHub Releases

## 推荐步骤

1. 在主分支打语义化标签（例如 `v0.1.0`）。
2. CI 按平台构建安装包并产出更新清单文件。
3. 上传安装包和更新清单到对应 Release。
4. 客户端调用 `update_check` 拉取最新版本并提示升级。

## 清单建议字段

- `version`: 版本号
- `pub_date`: 发布时间
- `notes`: 更新说明
- `platforms`: 各平台下载地址和签名信息

## 安全建议

- 首版是局域网明文 TCP，必须在 UI 明示风险。
- 后续版本建议升级为 TLS + Token 或 mTLS。
