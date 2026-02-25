<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import HexByteEditor from "./components/HexByteEditor.vue";

type SessionState = {
  sessionId: string;
  endpoint: string;
  baudRate: number;
  dataBits: number;
  stopBits: number;
  parity: string;
  flowControl: string;
  connected: boolean;
  reconnectCount: number;
};

const HEX_MAX_BYTES = 4096;

const isBusy = ref(false);
const errorText = ref("");
const infoText = ref("等待操作");
const isTauriRuntime = "__TAURI_INTERNALS__" in window;

const ports = ref<string[]>([]);
const sessions = ref<SessionState[]>([]);

// 串口参数
const sessionId = ref("session-main");
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

const activeSession = computed(() =>
  sessions.value.find((s) => s.sessionId === sessionId.value),
);

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

async function refreshSessions() {
  await runAction(async () => {
    sessions.value = await invoke<SessionState[]>("serial_list_sessions");
    infoText.value = `当前会话数 ${sessions.value.length}`;
  });
}

async function openSession() {
  await runAction(async () => {
    await invoke("serial_open", {
      req: {
        sessionId: sessionId.value,
        endpoint: endpoint.value,
        baudRate: Number(baudRate.value),
        dataBits: Number(dataBits.value),
        stopBits: Number(stopBits.value),
        parity: parity.value,
        flowControl: flowControl.value,
      },
    });

    infoText.value = "串口已打开";
    await refreshSessions();
  });
}

async function closeSession() {
  await runAction(async () => {
    await invoke("serial_close", { sessionId: sessionId.value });
    infoText.value = "串口已关闭";
    await refreshSessions();
  });
}

async function sendData() {
  await runAction(async () => {
    if (sendBytes.value.length === 0) {
      throw new Error("发送区为空，请先输入数据");
    }

    await invoke("serial_send", {
      req: {
        sessionId: sessionId.value,
        hexPayload: toHexPayload(sendBytes.value),
      },
    });
    infoText.value = `发送 ${sendBytes.value.length} 字节成功`;
  });
}

async function receiveData() {
  await runAction(async () => {
    const hex = await invoke<string>("serial_receive", {
      sessionId: sessionId.value,
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
      sessionId: sessionId.value,
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
  <main class="page">
    <section class="hero">
      <h1>VibeSerial 串口收发</h1>
      <p>左侧 ASCII，右侧 HEX（自定义字节控件，固定 `00 00 00` 格式）。</p>
      <div class="status-row">
        <span class="chip">状态: {{ infoText }}</span>
        <span v-if="activeSession" class="chip ok"
          >当前会话: {{ activeSession.sessionId }}</span
        >
      </div>
      <p v-if="errorText" class="error">{{ errorText }}</p>
    </section>

    <section class="layout">
      <article class="panel">
        <h2>串口参数</h2>
        <div class="row">
          <button :disabled="isBusy" @click="loadPorts">枚举串口</button>
          <button :disabled="isBusy" @click="refreshSessions">刷新会话</button>
        </div>

        <label>会话 ID</label>
        <input v-model="sessionId" />

        <label>串口端点</label>
        <input v-model="endpoint" list="port-list" />
        <datalist id="port-list">
          <option v-for="port in ports" :key="port" :value="port" />
        </datalist>

        <label>波特率</label>
        <input v-model.number="baudRate" type="number" min="1200" step="1200" />

        <div class="row split">
          <div>
            <label>数据位</label>
            <select v-model.number="dataBits">
              <option :value="5">5</option>
              <option :value="6">6</option>
              <option :value="7">7</option>
              <option :value="8">8</option>
            </select>
          </div>
          <div>
            <label>停止位</label>
            <select v-model.number="stopBits">
              <option :value="1">1</option>
              <option :value="2">2</option>
            </select>
          </div>
          <div>
            <label>校验位</label>
            <select v-model="parity">
              <option value="none">None</option>
              <option value="odd">Odd</option>
              <option value="even">Even</option>
            </select>
          </div>
          <div>
            <label>流控</label>
            <select v-model="flowControl">
              <option value="none">None</option>
              <option value="software">Software</option>
              <option value="hardware">Hardware</option>
            </select>
          </div>
        </div>

        <div class="row">
          <button :disabled="isBusy" @click="openSession">打开串口</button>
          <button :disabled="isBusy" @click="closeSession">关闭串口</button>
        </div>

        <ul class="list">
          <li v-for="s in sessions" :key="s.sessionId">
            <b>{{ s.sessionId }}</b>
            <span>{{ s.endpoint }}</span>
            <span>{{ s.connected ? "已连接" : "未连接" }}</span>
            <span>{{ s.baudRate }}/{{ s.dataBits }}/{{ s.stopBits }}</span>
            <span>{{ s.parity }} / {{ s.flowControl }}</span>
          </li>
        </ul>
      </article>

      <article class="panel editor-panel">
        <h2>发送区</h2>
        <div class="row">
          <button :disabled="isBusy" @click="clearSendEditors">清空</button>
          <button :disabled="isBusy" @click="sendData">发送</button>
        </div>

        <div class="editors-grid">
          <div>
            <label>ASCII（可编辑）</label>
            <textarea
              v-model="sendAsciiEditor"
              class="editor ascii"
              rows="12"
              @input="onSendAsciiInput"
              placeholder="在这里输入 ASCII，右侧 HEX 会同步"
            />
          </div>
          <div>
            <label>HEX（字节控件）</label>
            <HexByteEditor v-model="sendBytes" :max-bytes="HEX_MAX_BYTES" :disabled="isBusy" />
            <p class="hint">字节数: {{ sendBytes.length }}/{{ HEX_MAX_BYTES }}</p>
          </div>
        </div>
      </article>

      <article class="panel editor-panel">
        <h2>接收区</h2>
        <div class="row">
          <button :disabled="isBusy" @click="receiveData">读取串口</button>
          <button :disabled="isBusy" @click="clearReceiveEditors">清空</button>
        </div>

        <div class="row">
          <button :disabled="isBusy" @click="pushReceiveToInbound">将接收区写入缓冲</button>
          <button :disabled="isBusy" @click="copyReceiveToSend">复制到发送区</button>
        </div>

        <div class="editors-grid">
          <div>
            <label>ASCII（可编辑）</label>
            <textarea
              v-model="receiveAsciiEditor"
              class="editor ascii"
              rows="12"
              @input="onReceiveAsciiInput"
              placeholder="左侧编辑 ASCII，右侧 HEX 同步"
            />
          </div>
          <div>
            <label>HEX（字节控件）</label>
            <HexByteEditor
              v-model="receiveBytes"
              :max-bytes="HEX_MAX_BYTES"
              :disabled="isBusy"
            />
            <p class="hint">字节数: {{ receiveBytes.length }}/{{ HEX_MAX_BYTES }}</p>
          </div>
        </div>
      </article>
    </section>
  </main>
</template>

<style scoped>
:global(body) {
  margin: 0;
  min-height: 100vh;
  background:
    radial-gradient(circle at 10% 10%, #f6efe3 0%, transparent 40%),
    radial-gradient(circle at 90% 15%, #d8f0e4 0%, transparent 35%),
    linear-gradient(145deg, #f7fafc 0%, #e8eef6 70%);
  color: #17202a;
  font-family: "Noto Serif SC", "Source Han Serif SC", serif;
}

.page {
  max-width: 1260px;
  margin: 0 auto;
  padding: 24px 20px 36px;
}

.hero h1 {
  margin: 0 0 8px;
  font-size: clamp(26px, 4vw, 40px);
}

.hero p {
  margin: 4px 0;
}

.status-row {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
  margin-top: 12px;
}

.chip {
  background: rgba(255, 255, 255, 0.72);
  border: 1px solid #a0b2c6;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 13px;
}

.chip.ok {
  border-color: #2f855a;
  color: #22543d;
}

.error {
  color: #c53030;
  font-weight: 600;
}

.layout {
  margin-top: 18px;
  display: grid;
  grid-template-columns: 360px 1fr;
  gap: 14px;
}

.panel {
  background: rgba(255, 255, 255, 0.78);
  backdrop-filter: blur(8px);
  border: 1px solid rgba(62, 92, 118, 0.25);
  border-radius: 16px;
  padding: 14px;
  box-shadow: 0 12px 22px rgba(24, 39, 75, 0.08);
}

.editor-panel {
  min-width: 0;
}

h2 {
  margin: 0 0 10px;
  font-size: 20px;
}

label {
  display: block;
  font-size: 13px;
  margin: 8px 0 6px;
}

input,
select,
textarea {
  width: 100%;
  box-sizing: border-box;
  border: 1px solid #9fb0c3;
  border-radius: 10px;
  padding: 8px 10px;
  font-size: 14px;
  background: rgba(255, 255, 255, 0.92);
}

.editor {
  line-height: 1.45;
  white-space: pre;
}

.editor.ascii {
  font-family: "Noto Sans Mono CJK SC", "JetBrains Mono", monospace;
}

.row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 8px;
  align-items: center;
}

.split {
  display: grid;
  grid-template-columns: repeat(2, minmax(120px, 1fr));
  gap: 8px;
}

.editors-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-top: 10px;
}

button {
  border: none;
  border-radius: 10px;
  padding: 8px 12px;
  background: linear-gradient(135deg, #2458b6, #1f3f75);
  color: #fff;
  cursor: pointer;
  transition: transform 120ms ease;
}

button:hover {
  transform: translateY(-1px);
}

button:disabled {
  opacity: 0.6;
  cursor: not-allowed;
}

.list {
  margin: 10px 0 0;
  padding-left: 18px;
}

.list li {
  margin: 4px 0;
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
  font-size: 12px;
}

.hint {
  margin: 6px 0 0;
  font-size: 12px;
  color: #4a5568;
}

@media (max-width: 1120px) {
  .layout {
    grid-template-columns: 1fr;
  }

  .editors-grid {
    grid-template-columns: 1fr;
  }
}
</style>
