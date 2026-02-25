import { createApp } from "vue";
import App from "./App.vue";

const app = createApp(App);

app.config.errorHandler = (err) => {
  renderBootError(err);
};

window.addEventListener("error", (event) => {
  renderBootError(event.error ?? event.message);
});

window.addEventListener("unhandledrejection", (event) => {
  renderBootError(event.reason);
});

try {
  app.mount("#app");
} catch (err) {
  renderBootError(err);
}

function renderBootError(err: unknown) {
  const root = document.getElementById("app");
  if (!root) {
    return;
  }

  const message = err instanceof Error ? `${err.name}: ${err.message}` : String(err);
  root.innerHTML = `
    <div style="padding:16px;font-family:monospace;white-space:pre-wrap;color:#b00020;background:#fff3f3;border:1px solid #f0bcbc;border-radius:8px;">
前端运行异常，请把下面错误发给开发者：
${escapeHtml(message)}
    </div>
  `;
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/\"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
