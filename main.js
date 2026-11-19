const tileRoot = document.querySelector('#tiles');
const home = document.querySelector('#home');
const clock = document.querySelector('#clock');
const hint = document.querySelector('#hint');
const { invoke } = window.__TAURI__.core;
let sites = [];
let selected = 0;
let cursorTimer;
let hintTimer;

function updateClock() {
  clock.textContent = new Intl.DateTimeFormat(undefined, { weekday: 'long', hour: 'numeric', minute: '2-digit' }).format(new Date());
}

function render() {
  tileRoot.replaceChildren(...sites.map((site, index) => {
    const button = document.createElement('button');
    button.className = `tile${index === selected ? ' selected' : ''}`;
    button.type = 'button';
    button.setAttribute('aria-label', site.name);
    button.setAttribute('aria-pressed', String(index === selected));
    const image = document.createElement('img');
    image.src = site.icon;
    image.alt = '';
    button.append(image);
    return button;
  }));
}

function openSelected() {
  const site = sites[selected];
  if (site) location.href = site.url;
}

function move(dx, dy) {
  const columns = innerWidth < 780 ? 3 : 4;
  const rows = Math.ceil(sites.length / columns);
  const row = Math.floor(selected / columns);
  const col = selected % columns;
  let nextRow = row;
  let nextCol = col;
  if (dx) nextCol = (col + dx + columns) % columns;
  if (dy) nextRow = (row + dy + rows) % rows;
  let candidate = nextRow * columns + nextCol;
  if (candidate >= sites.length) candidate = Math.min(sites.length - 1, candidate - columns);
  selected = Math.max(0, candidate);
  render();
}

document.addEventListener('keydown', event => {
  if (event.key === 'ArrowLeft') { event.preventDefault(); move(-1, 0); }
  else if (event.key === 'ArrowRight') { event.preventDefault(); move(1, 0); }
  else if (event.key === 'ArrowUp') { event.preventDefault(); move(0, -1); }
  else if (event.key === 'ArrowDown') { event.preventDefault(); move(0, 1); }
  else if (event.key === 'Enter') { event.preventDefault(); openSelected(); }
});

function resetCursorTimer() {
  document.body.classList.remove('cursor-hidden');
  clearTimeout(cursorTimer);
  cursorTimer = setTimeout(() => document.body.classList.add('cursor-hidden'), 2500);
}
document.addEventListener('mousemove', resetCursorTimer);
resetCursorTimer();
updateClock();
setInterval(updateClock, 30_000);

invoke('load_sites').then(items => {
  sites = items.filter(site => site.name && /^https:\/\//i.test(site.url) && site.icon);
  render();
}).catch(error => {
  tileRoot.textContent = 'Could not load sites.json';
  console.error(error);
});

// The same webview is navigated back to this local page by the Rust document-start handler.
if (location.protocol === 'tauri:' || ['tauri.localhost', '127.0.0.1', 'localhost'].includes(location.hostname)) {
  home.hidden = false;
  home.classList.remove('away');
  hint.classList.remove('visible');
} else {
  home.hidden = true;
  hintTimer = setTimeout(() => hint.classList.add('visible'), 2500);
  setTimeout(() => hint.classList.remove('visible'), 6500);
}
