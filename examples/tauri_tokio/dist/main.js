const statusEl = document.getElementById('status');
const counterEl = document.getElementById('counter');
const messageEl = document.getElementById('message');

function setStatus(text) {
  statusEl.textContent = text;
}

function getTauriGlobal() {
  return window.__TAURI__ || window.__TAURI_INTERNALS__ || null;
}

async function bindEvents(tauri) {
  await tauri.event.listen('hot_state', (event) => {
    const payload = event.payload || {};
    counterEl.textContent = String(payload.counter ?? '-');
    messageEl.textContent = payload.message ?? 'No message';
    setStatus('Live');
  });

  await tauri.event.listen('hot_reload', () => {
    setStatus('Reloaded logic');
  });

  setStatus('Listening…');
}

function waitForTauri(attempt = 0) {
  const tauri = getTauriGlobal();
  if (tauri && tauri.event && tauri.event.listen) {
    bindEvents(tauri).catch((err) => {
      setStatus('Listener error');
      messageEl.textContent = String(err);
    });
    return;
  }

  if (attempt >= 50) {
    setStatus('Tauri API not found');
    messageEl.textContent = 'Check that this page is running inside Tauri v2.';
    return;
  }

  setStatus('Booting…');
  setTimeout(() => waitForTauri(attempt + 1), 200);
}

waitForTauri();
