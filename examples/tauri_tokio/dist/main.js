const $ = (id) => document.getElementById(id);
const invoke = window.__TAURI__?.core?.invoke;
let savedSource = '';
let lastLogs = '';
let lastReplies = '';
let lastError = '';
function editorChanged() {
  $('editor-state').textContent = $('source').value === savedSource ? 'Saved' : 'Unsaved';
  $('line-numbers').textContent = $('source').value.split('\n').map((_, i) => i + 1).join('\n');
}
function edit(text) { $('source').value = text; $('source').scrollTop = 0; editorChanged(); }
async function action(fn) {
  try { await fn(); } catch (error) { $('editor-message').textContent = String(error); }
}
async function read() {
  savedSource = await invoke('read_source'); edit(savedSource);
  $('editor-message').textContent = 'Try changing LABEL, ACCENT or STEP.';
}
async function save() {
  const source = $('source').value;
  await invoke('save_source', { source }); savedSource = source; editorChanged();
  $('editor-message').textContent = 'Saved. Building in the background.';
}
function preset(label, accent, step) {
  let text = $('source').value.replace(/const LABEL: &str = "[^"]*";/, `const LABEL: &str = "${label}";`)
    .replace(/const ACCENT: &str = "[^"]*";/, `const ACCENT: &str = "${accent}";`)
    .replace(/const STEP: u64 = \d+;/, `const STEP: u64 = ${step};`)
    .replace(/\ncompile_error!\("GUI sample: intentional build error"\);\n?/g, '\n');
  edit(text); $('editor-message').textContent = 'Preset applied to the editor. Select Save & reload to run it.';
}
function list(element, values, render) {
  element.replaceChildren(...values.map(render));
}
function render(s) {
  $('host-id').textContent = `HOST ${s.host_id}`;
  $('uptime').textContent = `${Math.floor(s.uptime_ms / 1000)}s`;
  $('status').textContent = s.status;
  $('status-dot').style.background = s.building ? '#b28b39' : s.accepting ? 'var(--accent)' : '#bb6553';
  $('counter').textContent = s.worker.counter.toLocaleString('en-US');
  $('logic-label').textContent = s.worker.label || 'Preparing logic';
  $('worker-generation').textContent = `worker / gen ${s.worker.generation || '—'}`;
  $('generation').textContent = s.generation ? `gen ${s.generation}` : '—';
  $('checkpoint').textContent = String(s.checkpoint);
  $('in-flight').textContent = String(s.in_flight);
  $('step').textContent = `+${s.worker.step || '—'} / tick`;
  if (/^#[a-fA-F0-9]{6}$/.test(s.worker.accent)) document.documentElement.style.setProperty('--accent', s.worker.accent);
  $('budget').textContent = `LOADS ${s.budget_used} / ${s.budget_limit}`;
  $('send').disabled = $('send-slow').disabled = !s.accepting;
  $('resume').hidden = s.accepting || !s.generation || s.building;
  $('mode-hint').textContent = s.mode === 'drain'
    ? 'Drain keeps the current worker running. Only new requests use the new code.'
    : 'Restart updates the worker and restores its saved counter.';
  if (document.activeElement !== $('mode')) $('mode').value = s.mode;
  if (s.history.length) {
    const min = Math.min(...s.history), max = Math.max(...s.history);
    $('spark').setAttribute('points', s.history.map((v, i) => `${i * 360 / Math.max(s.history.length - 1, 1)},${48 - (v - min) * 42 / Math.max(max - min, 1)}`).join(' '));
  }
  const logs = JSON.stringify(s.logs);
  if (lastLogs !== logs) { list($('logs'), s.logs, (text) => { const e = document.createElement('li'); e.textContent = text; return e; }); lastLogs = logs; }
  const replies = JSON.stringify(s.replies);
  if (lastReplies !== replies) {
    if (s.replies.length) list($('replies'), s.replies, (reply) => {
      const e = document.createElement('div'); e.className = 'reply';
      const tag = document.createElement('span'); tag.textContent = `GEN ${reply.generation}`;
      const p = document.createElement('p'); p.textContent = reply.text + (reply.stopped ? ' (completed after a stop request)' : '');
      e.append(tag, p); return e;
    });
    lastReplies = replies;
  }
  if (lastError !== s.build_error) {
    $('build-error').textContent = s.build_error;
    $('diagnostics').open = !!s.build_error;
    lastError = s.build_error;
  }
}
async function refresh() {
  try { render(await invoke('get_dashboard')); }
  catch (error) { $('status').textContent = String(error); }
  setTimeout(refresh, 180);
}
$('source').addEventListener('input', editorChanged);
$('source').addEventListener('scroll', () => { $('line-numbers').scrollTop = $('source').scrollTop; });
$('source').addEventListener('keydown', (e) => {
  if (e.key === 'Tab') { e.preventDefault(); const t = e.target; t.setRangeText('    ', t.selectionStart, t.selectionEnd, 'end'); editorChanged(); }
});
document.addEventListener('keydown', (e) => { if ((e.ctrlKey || e.metaKey) && e.key === 's') { e.preventDefault(); action(save); } });
$('read').onclick = () => action(read);
$('save').onclick = () => action(save);
$('rebuild').onclick = () => action(() => invoke('rebuild'));
$('resume').onclick = () => action(() => invoke('resume'));
$('mode').onchange = () => action(() => invoke('set_reload_mode', { mode: $('mode').value }));
$('send').onclick = () => action(() => invoke('send_request', { text: $('message').value, slow: false }));
$('send-slow').onclick = () => action(() => invoke('send_request', { text: $('message').value, slow: true }));
$('preset-v1').onclick = () => preset('Mint / v1', '#16856a', 1);
$('preset-v2').onclick = () => preset('Coral / v2', '#c36743', 5);
$('preset-error').onclick = () => { edit($('source').value + '\ncompile_error!("GUI sample: intentional build error");\n'); $('editor-message').textContent = 'Save to trigger a build error. The current logic will keep running.'; };
$('initial').onclick = () => action(async () => { edit(await invoke('initial_source')); $('editor-message').textContent = 'Initial source loaded into the editor. Save to apply it.'; });
if (invoke) { action(read); refresh(); }
else { $('status').textContent = 'Open this page in the Tauri app'; }
