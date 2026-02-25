<script setup lang="ts">
import { computed, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    modelValue: number[];
    maxBytes?: number;
    bytesPerRow?: number;
    disabled?: boolean;
  }>(),
  {
    maxBytes: 4096,
    bytesPerRow: 16,
    disabled: false,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", value: number[]): void;
}>();

// 当前字节数据（本地副本，用于 WinHex 风格编辑）
const bytes = ref<number[]>([]);
// 光标位置：按半字节计数（0=第0字节高4位，1=第0字节低4位）
const cursorNibble = ref(0);
// 组件容器引用，用于键盘焦点控制
const editorRef = ref<HTMLDivElement | null>(null);

let syncingFromParent = false;

const displayByteCount = computed(() => {
  const base = Math.max(bytes.value.length + 1, 1);
  return Math.min(base, props.maxBytes);
});

const nibblePerRow = computed(() => props.bytesPerRow * 2);
const displayNibbleCount = computed(() => displayByteCount.value * 2);

const rows = computed(() => {
  const out: Array<{ offset: string; byteIndexes: number[] }> = [];

  for (let start = 0; start < displayByteCount.value; start += props.bytesPerRow) {
    const byteIndexes: number[] = [];
    for (let i = start; i < Math.min(start + props.bytesPerRow, displayByteCount.value); i++) {
      byteIndexes.push(i);
    }

    out.push({
      offset: start.toString(16).toUpperCase().padStart(6, "0"),
      byteIndexes,
    });
  }

  return out;
});

watch(
  () => props.modelValue,
  (value) => {
    if (syncingFromParent) {
      return;
    }

    bytes.value = clampBytes(value, props.maxBytes);
    clampCursor();
  },
  { immediate: true, deep: true },
);

function clampBytes(input: number[], maxBytes: number): number[] {
  return input
    .slice(0, maxBytes)
    .map((v) => Math.max(0, Math.min(255, v | 0)));
}

function clampCursor() {
  const maxNibble = Math.max(displayNibbleCount.value - 1, 0);
  if (cursorNibble.value < 0) {
    cursorNibble.value = 0;
  }
  if (cursorNibble.value > maxNibble) {
    cursorNibble.value = maxNibble;
  }
}

function emitValue() {
  syncingFromParent = true;
  emit("update:modelValue", [...bytes.value]);
  syncingFromParent = false;
}

function ensureByteExists(byteIndex: number): boolean {
  if (byteIndex < 0 || byteIndex >= props.maxBytes) {
    return false;
  }

  while (bytes.value.length <= byteIndex) {
    bytes.value.push(0);
  }

  return true;
}

function setNibble(byteIndex: number, lowNibble: boolean, value: number): boolean {
  if (!ensureByteExists(byteIndex)) {
    return false;
  }

  const current = bytes.value[byteIndex];
  const next = lowNibble
    ? (current & 0xf0) | (value & 0x0f)
    : ((value & 0x0f) << 4) | (current & 0x0f);

  bytes.value[byteIndex] = next;
  return true;
}

function clearNibble(byteIndex: number, lowNibble: boolean) {
  if (byteIndex < 0 || byteIndex >= bytes.value.length) {
    return;
  }

  const current = bytes.value[byteIndex];
  bytes.value[byteIndex] = lowNibble ? (current & 0xf0) : (current & 0x0f);
}

function removeByte(byteIndex: number): boolean {
  if (byteIndex < 0 || byteIndex >= bytes.value.length) {
    return false;
  }
  bytes.value.splice(byteIndex, 1);
  return true;
}

function moveCursor(delta: number) {
  cursorNibble.value += delta;
  clampCursor();
}

function setCursor(byteIndex: number, nibble: 0 | 1) {
  cursorNibble.value = byteIndex * 2 + nibble;
  clampCursor();
}

function nibbleText(byteIndex: number, nibble: 0 | 1): string {
  const byte = bytes.value[byteIndex];
  if (byte === undefined) {
    return "0";
  }

  const value = nibble === 0 ? (byte >> 4) & 0x0f : byte & 0x0f;
  return value.toString(16).toUpperCase();
}

function nibbleClass(byteIndex: number, nibble: 0 | 1): string {
  const nibbleIndex = byteIndex * 2 + nibble;
  const classes: string[] = [];

  if (nibbleIndex === cursorNibble.value) {
    classes.push("active");
  }
  if (bytes.value[byteIndex] === undefined) {
    classes.push("empty");
  }

  return classes.join(" ");
}

function asciiRow(byteIndexes: number[]): string {
  return byteIndexes
    .map((idx) => {
      const byte = bytes.value[idx];
      if (byte === undefined) {
        return "·";
      }
      if (byte >= 0x20 && byte <= 0x7e) {
        return String.fromCharCode(byte);
      }
      return ".";
    })
    .join("");
}

function focusEditor() {
  editorRef.value?.focus();
}

function onNibbleClick(byteIndex: number, nibble: 0 | 1) {
  if (props.disabled) {
    return;
  }

  setCursor(byteIndex, nibble);
  focusEditor();
}

function onEditorKeydown(event: KeyboardEvent) {
  if (props.disabled) {
    return;
  }

  if (event.ctrlKey || event.metaKey || event.altKey) {
    return;
  }

  const key = event.key;
  const lower = key.toLowerCase();

  if (/^[0-9a-f]$/.test(lower)) {
    event.preventDefault();

    const value = parseInt(lower, 16);
    const byteIndex = Math.floor(cursorNibble.value / 2);
    const lowNibble = cursorNibble.value % 2 === 1;

    if (!setNibble(byteIndex, lowNibble, value)) {
      return;
    }

    emitValue();
    moveCursor(1);
    return;
  }

  switch (key) {
    case "ArrowLeft":
      event.preventDefault();
      moveCursor(-1);
      return;
    case "ArrowRight":
      event.preventDefault();
      moveCursor(1);
      return;
    case "ArrowUp":
      event.preventDefault();
      moveCursor(-nibblePerRow.value);
      return;
    case "ArrowDown":
      event.preventDefault();
      moveCursor(nibblePerRow.value);
      return;
    case "Home": {
      event.preventDefault();
      const rowStart = Math.floor(cursorNibble.value / nibblePerRow.value) * nibblePerRow.value;
      cursorNibble.value = rowStart;
      clampCursor();
      return;
    }
    case "End": {
      event.preventDefault();
      const rowStart = Math.floor(cursorNibble.value / nibblePerRow.value) * nibblePerRow.value;
      cursorNibble.value = rowStart + nibblePerRow.value - 1;
      clampCursor();
      return;
    }
    case " ":
      event.preventDefault();
      moveCursor(2);
      return;
    case "Enter":
      event.preventDefault();
      moveCursor(nibblePerRow.value);
      return;
    case "Backspace": {
      event.preventDefault();
      const byteIndex = Math.floor(cursorNibble.value / 2);
      const target = byteIndex - 1;
      if (!removeByte(target)) {
        return;
      }
      emitValue();
      cursorNibble.value = Math.max(0, target * 2);
      clampCursor();
      return;
    }
    case "Delete": {
      event.preventDefault();
      const byteIndex = Math.floor(cursorNibble.value / 2);
      if (!removeByte(byteIndex)) {
        return;
      }
      emitValue();
      cursorNibble.value = byteIndex * 2;
      clampCursor();
      return;
    }
    case "Tab":
      return;
    default:
      if (key.length === 1) {
        // 关键点：非法字符直接禁止输入。
        event.preventDefault();
      }
  }
}

function onEditorPaste(event: ClipboardEvent) {
  if (props.disabled) {
    return;
  }

  event.preventDefault();
  const raw = event.clipboardData?.getData("text") ?? "";
  const digits = raw.toUpperCase().replace(/[^0-9A-F]/g, "");
  if (!digits) {
    return;
  }

  let nibbleIndex = cursorNibble.value;
  for (const ch of digits) {
    if (nibbleIndex >= props.maxBytes * 2) {
      break;
    }

    const value = parseInt(ch, 16);
    const byteIndex = Math.floor(nibbleIndex / 2);
    const lowNibble = nibbleIndex % 2 === 1;

    if (!setNibble(byteIndex, lowNibble, value)) {
      break;
    }

    nibbleIndex += 1;
  }

  cursorNibble.value = nibbleIndex;
  clampCursor();
  emitValue();
}
</script>

<template>
  <div
    ref="editorRef"
    class="hex-editor"
    tabindex="0"
    @keydown="onEditorKeydown"
    @paste="onEditorPaste"
  >
    <div class="head row">
      <span class="offset">Offset</span>
      <span class="hex">Hex</span>
      <span class="ascii">ASCII</span>
    </div>

    <div v-for="row in rows" :key="row.offset" class="row">
      <span class="offset">{{ row.offset }}</span>
      <div class="hex">
        <span v-for="idx in row.byteIndexes" :key="idx" class="byte">
          <span
            :class="['nibble', nibbleClass(idx, 0)]"
            @mousedown.prevent
            @click="onNibbleClick(idx, 0)"
            >{{ nibbleText(idx, 0) }}</span
          >
          <span
            :class="['nibble', nibbleClass(idx, 1)]"
            @mousedown.prevent
            @click="onNibbleClick(idx, 1)"
            >{{ nibbleText(idx, 1) }}</span
          >
        </span>
      </div>
      <span class="ascii">{{ asciiRow(row.byteIndexes) }}</span>
    </div>
  </div>
</template>

<style scoped>
.hex-editor {
  border: 1px solid #a0a0a0;
  border-radius: 0;
  background: #ffffff;
  padding: 8px;
  min-height: 220px;
  overflow: auto;
  outline: none;
  font-family: "JetBrains Mono", "Fira Code", monospace;
  font-size: 12px;
}

.hex-editor:focus {
  border-color: #0060c0;
}

.row {
  display: grid;
  grid-template-columns: 72px 1fr 140px;
  align-items: center;
  gap: 10px;
  min-height: 24px;
}

.head {
  padding-bottom: 6px;
  margin-bottom: 6px;
  border-bottom: 1px dashed #b7c3d1;
  color: #5a6878;
}

.offset {
  color: #5a6878;
}

.hex {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.byte {
  display: inline-flex;
  min-width: 18px;
}

.nibble {
  display: inline-flex;
  width: 9px;
  justify-content: center;
  border-radius: 2px;
  cursor: text;
  user-select: none;
}

.nibble.empty {
  color: #a0aec0;
}

.nibble.active {
  color: #fff;
  background: #0060c0;
}

.ascii {
  color: #364556;
}
</style>
