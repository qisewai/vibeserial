<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import HexByteEditor from "./components/HexByteEditor.vue";

const HEX_MAX_BYTES = 4096;

const isBusy = ref(false);
const errorText = ref("");
const infoText = ref("等待操作");
const isTauriRuntime = "__TAURI_INTERNALS__" in window;

const ports = ref<string[]>([]);
const connected = ref(false);

// 串口参数
const sessionId = "session-main";
const endpoint = ref("loopback");
const baudRate = ref(115200);
const dataBits = ref(8);
const stopBits = ref(1);
const parity = ref("none");
const flowControl = ref("none");

// 发送区：左 ASCII，右 HEX（自定义字节编辑控件）
const sendAsciiEditor = ref("Hello\r\n");
const sendBytes = ref<number[]>(asciiToBytes(sendAsciiEditor.value));

// 接收区：左 ASCII，右 HEX（自定义字节编辑控件）
const receiveAsciiEditor = ref("");
const receiveBytes = ref<number[]>([]);

let syncingSend = false;
let syncingReceive = false;

watch(
  () => sendBytes.value,
  (bytes) => {
    if (syncingSend) {
      return;
    }
    syncingSend = true;
    sendAsciiEditor.value = bytesToAsciiDisplay(bytes);
    syncingSend = false;
  },
  { deep: true },
);

watch(
  () => receiveBytes.value,
  (bytes) => {
    if (syncingReceive) {
      return;
    }
    syncingReceive = true;
    receiveAsciiEditor.value = bytesToAsciiDisplay(bytes);
    syncingReceive = false;
  },
  { deep: true },
);

async function runAction(action: () => Promise<void>) {
  errorText.value = "";
  isBusy.value = true;
  try {
    if (!isTauriRuntime) {
      throw new Error("当前是浏览器模式，请使用 `./start.sh` 启动桌面端。");
    }
    await action();
  } catch (err) {
    errorText.value = String(err);
  } finally {
    isBusy.value = false;
  }
}

async function loadPorts() {
  await runAction(async () => {
    ports.value = await invoke<string[]>("serial_list_ports");
    infoText.value = `已加载 ${ports.value.length} 个端口`;
  });
}

onMounted(() => {
  loadPorts();
});

async function openSession() {
  await runAction(async () => {
    await invoke("serial_open", {
      req: {
        sessionId,
        endpoint: endpoint.value,
        baudRate: Number(baudRate.value),
        dataBits: Number(dataBits.value),
        stopBits: Number(stopBits.value),
        parity: parity.value,
        flowControl: flowControl.value,
      },
    });

    connected.value = true;
    infoText.value = "串口已打开";
  });
}

async function closeSession() {
  await runAction(async () => {
    await invoke("serial_close", { sessionId });
    connected.value = false;
    infoText.value = "串口已关闭";
  });
}

async function sendData() {
  await runAction(async () => {
    if (sendBytes.value.length === 0) {
      throw new Error("发送区为空，请先输入数据");
    }

    await invoke("serial_send", {
      req: {
        sessionId,
        hexPayload: toHexPayload(sendBytes.value),
      },
    });
    infoText.value = `发送 ${sendBytes.value.length} 字节成功`;
  });
}

async function receiveData() {
  await runAction(async () => {
    const hex = await invoke<string>("serial_receive", {
      sessionId,
    });

    const bytes = parseHexPayloadString(hex);
    syncingReceive = true;
    receiveBytes.value = bytes;
    receiveAsciiEditor.value = bytesToAsciiDisplay(bytes);
    syncingReceive = false;

    infoText.value = `读取 ${bytes.length} 字节成功`;
  });
}

async function pushReceiveToInbound() {
  await runAction(async () => {
    if (receiveBytes.value.length === 0) {
      throw new Error("接收区为空，无法写入缓冲");
    }

    await invoke("serial_mock_push_inbound", {
      sessionId,
      hexPayload: toHexPayload(receiveBytes.value),
    });
    infoText.value = `已将接收区 ${receiveBytes.value.length} 字节写入缓冲`;
  });
}

function copyReceiveToSend() {
  errorText.value = "";

  syncingSend = true;
  sendBytes.value = [...receiveBytes.value];
  sendAsciiEditor.value = bytesToAsciiDisplay(sendBytes.value);
  syncingSend = false;

  infoText.value = "已复制接收区到发送区";
}

function onSendAsciiInput() {
  if (syncingSend) {
    return;
  }

  try {
    errorText.value = "";
    const bytes = asciiToBytes(sendAsciiEditor.value);
    if (bytes.length > HEX_MAX_BYTES) {
      throw new Error(`发送区超过上限 ${HEX_MAX_BYTES} 字节`);
    }

    syncingSend = true;
    sendBytes.value = bytes;
    syncingSend = false;
  } catch (err) {
    errorText.value = String(err);
  }
}

function onReceiveAsciiInput() {
  if (syncingReceive) {
    return;
  }

  try {
    errorText.value = "";
    const bytes = asciiToBytes(receiveAsciiEditor.value);
    if (bytes.length > HEX_MAX_BYTES) {
      throw new Error(`接收区超过上限 ${HEX_MAX_BYTES} 字节`);
    }

    syncingReceive = true;
    receiveBytes.value = bytes;
    syncingReceive = false;
  } catch (err) {
    errorText.value = String(err);
  }
}

function clearSendEditors() {
  sendAsciiEditor.value = "";
  sendBytes.value = [];
}

function clearReceiveEditors() {
  receiveAsciiEditor.value = "";
  receiveBytes.value = [];
}

function asciiToBytes(text: string): number[] {
  const bytes: number[] = [];
  for (const ch of text) {
    const code = ch.codePointAt(0) ?? 0;
    if (code > 0x7f) {
      throw new Error(`ASCII 仅支持 0x00-0x7F，发现字符: ${ch}`);
    }
    bytes.push(code);
  }
  return bytes;
}

function bytesToAsciiDisplay(bytes: number[]): string {
  let out = "";
  for (const b of bytes) {
    if (b === 0x0a) {
      out += "\n";
      continue;
    }
    if (b === 0x0d) {
      out += "\r";
      continue;
    }
    if (b === 0x09) {
      out += "\t";
      continue;
    }
    if (b >= 0x20 && b <= 0x7e) {
      out += String.fromCharCode(b);
    } else {
      out += ".";
    }
  }
  return out;
}

function parseHexPayloadString(text: string): number[] {
  const out: number[] = [];
  for (const token of text.split(/\s+/).filter((v) => v.length > 0)) {
    if (!/^[0-9a-fA-F]{2}$/.test(token)) {
      continue;
    }
    out.push(parseInt(token, 16));
    if (out.length >= HEX_MAX_BYTES) {
      break;
    }
  }
  return out;
}

function toHexPayload(bytes: number[]): string {
  return bytes
    .map((b) => b.toString(16).toUpperCase().padStart(2, "0"))
    .join(" ");
}
</script>

<template>
  <div class="app">
    <!-- 菜单栏 -->
    <div class="menubar">
      <span class="menu-item">文件</span>
      <span class="menu-item">视图</span>
      <span class="menu-item">帮助</span>
    </div>

    <!-- 工具栏 -->
    <div class="toolbar">
      <button :disabled="isBusy" @click="openSession">打开</button>
      <button :disabled="isBusy" @click="closeSession">关闭</button>
      <span class="toolbar-sep"></span>
      <button :disabled="isBusy" @click="sendData">发送</button>
      <button :disabled="isBusy" @click="receiveData">接收</button>
    </div>

    <!-- 主体：侧边栏 + 内容区 -->
    <div class="main-body">
      <!-- 侧边栏：串口参数 -->
      <div class="sidebar">
        <fieldset>
          <legend>串口参数</legend>

          <label>串口端点</label>
          <input v-model="endpoint" list="port-list" />
          <datalist id="port-list">
            <option v-for="port in ports" :key="port" :value="port" />
          </datalist>

          <label>波特率</label>
          <input v-model.number="baudRate" type="number" min="1200" step="1200" />

          <label>数据位</label>
          <select v-model.number="dataBits">
            <option :value="5">5</option>
            <option :value="6">6</option>
            <option :value="7">7</option>
            <option :value="8">8</option>
          </select>

          <label>停止位</label>
          <select v-model.number="stopBits">
            <option :value="1">1</option>
            <option :value="2">2</option>
          </select>

          <label>校验位</label>
          <select v-model="parity">
            <option value="none">None</option>
            <option value="odd">Odd</option>
            <option value="even">Even</option>
          </select>

          <label>流控</label>
          <select v-model="flowControl">
            <option value="none">None</option>
            <option value="software">Software</option>
            <option value="hardware">Hardware</option>
          </select>
        </fieldset>

      </div>

      <!-- 内容区：接收区(上) + 发送区(下) -->
      <div class="content">
        <!-- 接收区 -->
        <fieldset class="zone zone-recv">
          <legend>接收区</legend>
          <div class="zone-toolbar">
            <button :disabled="isBusy" @click="receiveData">读取</button>
            <button :disabled="isBusy" @click="clearReceiveEditors">清空</button>
            <button :disabled="isBusy" @click="pushReceiveToInbound">写入缓冲</button>
            <button :disabled="isBusy" @click="copyReceiveToSend">复制到发送区</button>
          </div>
          <div class="editors-grid">
            <div class="editor-col">
              <label>ASCII</label>
              <textarea
                v-model="receiveAsciiEditor"
                class="editor ascii"
                @input="onReceiveAsciiInput"
                placeholder="ASCII 显示区"
              />
            </div>
            <div class="editor-col">
              <label>HEX <span class="hint">{{ receiveBytes.length }}/{{ HEX_MAX_BYTES }}</span></label>
              <HexByteEditor
                v-model="receiveBytes"
                :max-bytes="HEX_MAX_BYTES"
                :disabled="isBusy"
              />
            </div>
          </div>
        </fieldset>

        <!-- 发送区 -->
        <fieldset class="zone zone-send">
          <legend>发送区</legend>
          <div class="zone-toolbar">
            <button :disabled="isBusy" @click="sendData">发送</button>
            <button :disabled="isBusy" @click="clearSendEditors">清空</button>
          </div>
          <div class="editors-grid">
            <div class="editor-col">
              <label>ASCII</label>
              <textarea
                v-model="sendAsciiEditor"
                class="editor ascii"
                @input="onSendAsciiInput"
                placeholder="在这里输入 ASCII"
              />
            </div>
            <div class="editor-col">
              <label>HEX <span class="hint">{{ sendBytes.length }}/{{ HEX_MAX_BYTES }}</span></label>
              <HexByteEditor v-model="sendBytes" :max-bytes="HEX_MAX_BYTES" :disabled="isBusy" />
            </div>
          </div>
        </fieldset>
      </div>
    </div>

    <!-- 状态栏 -->
    <div class="statusbar">
      <span class="status-cell">{{ connected ? '已连接' : '未连接' }}</span>
      <span class="status-sep"></span>
      <span class="status-cell">{{ endpoint }} {{ baudRate }},{{ dataBits }},{{ parity === 'none' ? 'N' : parity === 'odd' ? 'O' : 'E' }},{{ stopBits }}</span>
      <span class="status-sep"></span>
      <span class="status-cell">{{ infoText }}</span>
      <span v-if="errorText" class="status-cell status-error">{{ errorText }}</span>
    </div>
  </div>
</template>

<style scoped>
:global(body) {
  margin: 0;
  padding: 0;
  background: #f0f0f0;
  color: #1a1a1a;
  font-family: "Microsoft YaHei", "Segoe UI", "Noto Sans SC", sans-serif;
  font-size: 12px;
  overflow: hidden;
}

.app {
  display: flex;
  flex-direction: column;
  height: 100vh;
  overflow: hidden;
}

/* ---- 菜单栏 ---- */
.menubar {
  display: flex;
  align-items: center;
  height: 24px;
  background: #f0f0f0;
  border-bottom: 1px solid #a0a0a0;
  padding: 0 4px;
  flex-shrink: 0;
}

.menu-item {
  padding: 2px 8px;
  cursor: default;
  user-select: none;
}

.menu-item:hover {
  background: #0060c0;
  color: #fff;
}

/* ---- 工具栏 ---- */
.toolbar {
  display: flex;
  align-items: center;
  gap: 2px;
  height: 26px;
  background: #e8e8e8;
  border-bottom: 1px solid #a0a0a0;
  padding: 0 4px;
  flex-shrink: 0;
}

.toolbar-sep {
  width: 1px;
  height: 16px;
  background: #a0a0a0;
  margin: 0 4px;
}

/* ---- 主体 ---- */
.main-body {
  display: flex;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

/* ---- 侧边栏 ---- */
.sidebar {
  width: 200px;
  flex-shrink: 0;
  border-right: 1px solid #a0a0a0;
  background: #f0f0f0;
  overflow-y: auto;
  padding: 4px;
}

.sidebar fieldset {
  margin: 0 0 4px;
  padding: 4px 6px 6px;
  border: 1px solid #a0a0a0;
}

.sidebar legend {
  font-size: 11px;
  font-weight: 600;
  padding: 0 4px;
}

.sidebar label {
  display: block;
  font-size: 11px;
  margin: 4px 0 2px;
  color: #333;
}

.sidebar input,
.sidebar select {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid #a0a0a0;
  border-radius: 0;
  padding: 2px 4px;
  font-size: 12px;
  background: #fff;
}

/* ---- 内容区 ---- */
.content {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  padding: 4px;
  gap: 4px;
}

/* ---- 区域 (接收/发送) ---- */
.zone {
  display: flex;
  flex-direction: column;
  border: 1px solid #a0a0a0;
  margin: 0;
  padding: 4px 6px 6px;
  min-height: 0;
  overflow: hidden;
}

.zone-recv {
  flex: 3;
}

.zone-send {
  flex: 2;
}

.zone legend {
  font-size: 11px;
  font-weight: 600;
  padding: 0 4px;
}

.zone-toolbar {
  display: flex;
  gap: 4px;
  margin-bottom: 4px;
  flex-shrink: 0;
}

.editors-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px;
  flex: 1;
  min-height: 0;
  overflow: hidden;
}

.editor-col {
  display: flex;
  flex-direction: column;
  min-height: 0;
  overflow: hidden;
}

.editor-col label {
  display: block;
  font-size: 11px;
  margin: 0 0 2px;
  color: #333;
  flex-shrink: 0;
}

.editor-col .editor {
  flex: 1;
  min-height: 0;
  resize: none;
}

.editor-col .hex-editor {
  flex: 1;
  min-height: 0;
}

.editor {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid #a0a0a0;
  border-radius: 0;
  padding: 4px;
  font-size: 12px;
  background: #fff;
  line-height: 1.4;
  white-space: pre;
  overflow: auto;
}

.editor.ascii {
  font-family: "JetBrains Mono", "Consolas", monospace;
}

.hint {
  font-size: 11px;
  color: #666;
  margin-left: 6px;
  font-weight: normal;
}

/* ---- 按钮（全局） ---- */
button {
  border: 1px solid #a0a0a0;
  border-radius: 0;
  padding: 2px 8px;
  background: #e0e0e0;
  color: #1a1a1a;
  font-size: 12px;
  cursor: default;
  white-space: nowrap;
}

button:hover {
  background: #d0d0d0;
}

button:active {
  background: #c0c0c0;
}

button:disabled {
  color: #999;
  background: #e0e0e0;
  cursor: not-allowed;
}

/* ---- 状态栏 ---- */
.statusbar {
  display: flex;
  align-items: center;
  height: 22px;
  background: #f0f0f0;
  border-top: 1px solid #a0a0a0;
  padding: 0 6px;
  flex-shrink: 0;
  font-size: 11px;
  gap: 0;
}

.status-cell {
  padding: 0 8px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.status-sep {
  width: 1px;
  height: 14px;
  background: #a0a0a0;
}

.status-error {
  color: #c00;
  font-weight: 600;
}
</style>
