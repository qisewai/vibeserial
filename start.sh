#!/usr/bin/env bash
set -euo pipefail

# 脚本根目录（仓库根目录）
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# 前端目录（这里管理 Node 依赖）
UI_DIR="$ROOT_DIR/apps/ui"
# 桌面目录（在这里执行 Tauri CLI，避免项目识别失败）
DESKTOP_DIR="$ROOT_DIR/apps/desktop"
TAURI_CONF="$ROOT_DIR/apps/desktop/src-tauri/tauri.conf.json"

require_cmd() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "错误：未找到命令 '$cmd'，请先安装后重试。"
    exit 1
  fi
}

require_cmd npm
require_cmd cargo

if [[ ! -d "$UI_DIR" ]]; then
  echo "错误：目录不存在 $UI_DIR"
  exit 1
fi

if [[ ! -d "$DESKTOP_DIR" ]]; then
  echo "错误：目录不存在 $DESKTOP_DIR"
  exit 1
fi

if [[ ! -f "$TAURI_CONF" ]]; then
  echo "错误：未找到 Tauri 配置文件 $TAURI_CONF"
  exit 1
fi

cd "$UI_DIR"

# 首次启动时自动安装依赖；已安装则跳过
if [[ ! -d "node_modules" ]]; then
  echo "[vibeserial] 检测到未安装前端依赖，开始安装..."
  if [[ -f "package-lock.json" ]]; then
    npm ci
  else
    npm install
  fi
fi

echo "[vibeserial] 启动 Tauri 开发模式..."
cd "$DESKTOP_DIR"
"$UI_DIR/node_modules/.bin/tauri" dev -c src-tauri/tauri.conf.json "$@"
