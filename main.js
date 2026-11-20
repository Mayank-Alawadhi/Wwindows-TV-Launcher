const heroRoot = document.querySelector('#hero');
const allRoot = document.querySelector('#all-sites');
const settingsView = document.querySelector('#settings');
const clock = document.querySelector('#clock');
const bootScreen = document.querySelector('#boot-screen');
const { invoke } = window.__TAURI__.core;
const defaultShortcuts = { up:'ArrowUp',down:'ArrowDown',left:'ArrowLeft',right:'ArrowRight',select:'Enter',home:'Escape',fullscreen:'F11',settings:'S',remote:'Q',power:'P' };
const shortcutNames = { up:'Move up',down:'Move down',left:'Move left',right:'Move right',select:'Select / open',home:'Home / back',fullscreen:'Toggle fullscreen',settings:'Open settings',remote:'Open phone remote',power:'Power menu' };
let sites = [];
let settings = { heroMode: 'most-used', remotePort: 8080, startFullscreen: true, showBootAnimation: true, shortBootAnimation: false, bootSound: false, shortcuts:{...defaultShortcuts} };
let focusId = '';
let editingIndex = -1;
let cursorTimer;
let bootState = 'loading';
let bootSkipRequested = false;
let bootRevealTimer;
let bootFinishTimer;
let recordingShortcut = null;
const tileById = new Map();
const colorCache = new Map();
const $ = (selector) => document.querySelector(selector);

function clockTick() {
  const now = new Date(); clock.textContent = new Intl.DateTimeFormat(undefined, { hour: 'numeric', minute: '2-digit' }).format(now);
  const hour = now.getHours(); $('#greeting').textContent = hour < 12 ? 'Good morning' : hour < 18 ? 'Good afternoon' : 'Good evening';
}
function displayKey(key) { return ({ArrowUp:'↑',ArrowDown:'↓',ArrowLeft:'←',ArrowRight:'→',Enter:'Enter',Escape:'Esc',Backspace:'Backspace',Space:'Space'})[key] || key; }
function refreshShortcutHints() {
  $('#hint-move').textContent=`${displayKey(settings.shortcuts.up)} ${displayKey(settings.shortcuts.down)} ${displayKey(settings.shortcuts.left)} ${displayKey(settings.shortcuts.right)}  MOVE`;
  $('#hint-select').textContent=`${displayKey(settings.shortcuts.select)}  OPEN`;
  $('#hint-home').textContent=`${displayKey(settings.shortcuts.home)}  HOME`;
  $('#hint-settings').textContent=`${displayKey(settings.shortcuts.settings)}  SETTINGS`;
  $('#hint-remote').textContent=`${displayKey(settings.shortcuts.remote)}  REMOTE`;
  $('#hint-fullscreen').textContent=`${displayKey(settings.shortcuts.fullscreen)}  FULLSCREEN`;
  $('#hint-power').textContent=`${displayKey(settings.shortcuts.power)}  POWER`;
}
function normalizeKey(event) {
  if (['Control','Alt','Shift','Meta'].includes(event.key)) return null;
  const key = event.key === ' ' ? 'Space' : event.key.length === 1 ? event.key.toUpperCase() : event.key;
  const parts=[]; if(event.ctrlKey)parts.push('Ctrl');if(event.altKey)parts.push('Alt');if(event.shiftKey)parts.push('Shift');if(event.metaKey)parts.push('Meta');parts.push(key);return parts.join('+');
}
function shortcutAction(event) { const key=normalizeKey(event);return key?Object.keys(settings.shortcuts).find(action=>settings.shortcuts[action]===key):null; }
function revealHome() {
  document.body.classList.add('home-visible', 'tiles-arriving');
  bootScreen.classList.add('leaving');
  setTimeout(() => document.body.classList.remove('tiles-arriving'), settings.shortBootAnimation ? 550 : 850);
}
function finishBoot(skip = false) {
  if (bootState !== 'running') return;
  bootState = 'finished'; clearTimeout(bootRevealTimer); clearTimeout(bootFinishTimer);
  if (skip) revealHome();
  bootScreen.remove();
}
function startBoot() {
  const shortcut = new URLSearchParams(location.search).get('shortcut');
  const cameFromSite = new URLSearchParams(location.search).has('from');
  if (!settings.showBootAnimation || cameFromSite || shortcut || bootSkipRequested) {
    bootState = 'finished'; bootScreen.remove(); document.body.classList.add('home-visible');
    if (shortcut === 's'||shortcut==='settings') openSettings(); else if (shortcut === 'q'||shortcut==='remote') showRemote(); else if(shortcut==='power')showPowerDialog();
    return;
  }
  bootState = 'running';
  const duration = settings.shortBootAnimation ? 1500 : 8000;
  bootScreen.classList.add('playing');
  if (settings.shortBootAnimation) bootScreen.classList.add('short');
  if (settings.bootSound) {
    fetch('/assets/boot-sound.mp3', { method: 'HEAD' }).then((response) => {
      if (response.ok && bootState === 'running') { const audio = new Audio('/assets/boot-sound.mp3'); audio.volume = 0.45; return audio.play(); }
    }).catch(() => {});
  }
  bootRevealTimer = setTimeout(revealHome, duration - (settings.shortBootAnimation ? 300 : 700));
  bootFinishTimer = setTimeout(() => finishBoot(), duration);
}
function skipBoot(event) {
  if (bootState !== 'running' && bootState !== 'loading') return;
  event.preventDefault(); event.stopImmediatePropagation();
  if (bootState === 'loading') { bootSkipRequested = true; return; }
  finishBoot(true);
}
document.addEventListener('keydown', skipBoot, true);
bootScreen.addEventListener('click', skipBoot, true);
function save() { return invoke('save_launcher_data', { data: { sites, settings } }).catch((e) => toast(String(e))); }
function toast(message) { const el = $('#toast'); el.textContent = message; el.classList.add('show'); setTimeout(() => el.classList.remove('show'), 2800); }
function siteLogo(site) { return site.logo || `https://www.google.com/s2/favicons?domain_url=${encodeURIComponent(site.url)}&sz=128`; }
function focusedColor() {
  const siteId = focusId.slice(focusId.indexOf(':') + 1);
  const site = sites.find((item) => item.id === siteId) || sites[0];
  const brandColors = { netflix:'#e50914',youtube:'#ff2020','prime video':'#00a8e0','disney+ hotstar':'#1769d2',hulu:'#1ce783',plex:'#e5a00d' };
  const color = colorCache.get(site?.id) || brandColors[site?.name?.toLowerCase()] || site?.background || '#7465ce';
  document.documentElement.style.setProperty('--focus-color', color);
  const image=tileById.get(focusId)?.querySelector('img');
  if(site&&!colorCache.has(site.id)&&image?.src.startsWith('data:')){
    const probe=new Image();probe.onload=()=>{try{const canvas=document.createElement('canvas');canvas.width=canvas.height=8;const ctx=canvas.getContext('2d',{willReadFrequently:true});ctx.drawImage(probe,0,0,8,8);const pixels=ctx.getImageData(0,0,8,8).data;let r=0,g=0,b=0,n=0;for(let i=0;i<pixels.length;i+=4){if(pixels[i+3]<40)continue;r+=pixels[i];g+=pixels[i+1];b+=pixels[i+2];n++;}if(n){const picked=`rgb(${Math.round(r/n)},${Math.round(g/n)},${Math.round(b/n)})`;colorCache.set(site.id,picked);if(focusId.includes(site.id))document.documentElement.style.setProperty('--focus-color',picked);}}catch{}};probe.src=image.src;
  }
}
function makeTile(site, id, size = 'normal') {
  const button = document.createElement('button');
  button.type = 'button'; button.className = `tile ${size}`; button.dataset.id = id;
  button.setAttribute('aria-label', site.name); button.style.setProperty('--tile-bg', site.background || '#fff');
  const image = document.createElement('img'); image.alt = ''; image.src = siteLogo(site); image.onerror = () => { image.onerror = null; image.src = `https://www.google.com/s2/favicons?domain=${new URL(site.url).hostname}&sz=128`; };
  button.append(image);
  button.addEventListener('focus', () => { document.querySelectorAll('.tile.selected').forEach((tile)=>tile.classList.remove('selected'));button.classList.add('selected');focusId = id; focusedColor(); });
  button.addEventListener('click', () => { focusId = id; if (id === 'settings') openSettings(); else launch(site); });
  tileById.set(id, button); return button;
}
function heroSites() {
  let candidates = [...sites];
  if (settings.heroMode === 'pinned-only') candidates = candidates.filter((site) => site.pinned);
  else if (candidates.some((site) => site.launchCount || site.lastOpened)) candidates.sort((a,b) => (b.launchCount || 0) - (a.launchCount || 0) || String(b.lastOpened || '').localeCompare(String(a.lastOpened || '')));
  return candidates.slice(0, 6);
}
function render() {
  tileById.clear();
  heroRoot.replaceChildren(...heroSites().map((site, i) => {
    const id = `hero:${site.id || i}`; const wrap = document.createElement('div'); wrap.className = 'tile-wrap';
    wrap.append(makeTile(site, id, 'hero-tile')); const label = document.createElement('span'); label.className='tile-label'; label.textContent=site.name; wrap.append(label); return wrap;
  }));
  const all = sites.map((site, i) => {const wrap=document.createElement('div');wrap.className='tile-wrap';wrap.style.animationDelay=`${Math.min(.65,Math.floor(i/6)*.12+.1)}s`;wrap.append(makeTile(site,`site:${site.id || i}`));const label=document.createElement('span');label.className='tile-label';label.textContent=site.name;wrap.append(label);return wrap;});
  const gear = { name: 'Settings', url: '', background: '#272a37', logo: 'data:image/svg+xml,' + encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 120 120"><text x="60" y="88" text-anchor="middle" font-size="78">⚙</text></svg>') };
  const gearWrap=document.createElement('div');gearWrap.className='tile-wrap';gearWrap.append(makeTile(gear,'settings'));const gearLabel=document.createElement('span');gearLabel.className='tile-label';gearLabel.textContent='Settings';gearWrap.append(gearLabel);all.push(gearWrap);
  allRoot.replaceChildren(...all); tileById.clear();
  [...heroRoot.querySelectorAll('.tile'), ...allRoot.querySelectorAll('.tile')].forEach((node) => tileById.set(node.dataset.id,node));
  const target = focusId==='power'?$('#power-button'):(tileById.get(focusId) || tileById.values().next().value); focusId = target?.dataset.id || (target?.id==='power-button'?'power':''); if(target?.classList.contains('tile'))target.classList.add('selected'); target?.focus({ preventScroll: true }); focusedColor();
}
function launch(site) {
  if (!site) return;
  site.launchCount = (site.launchCount || 0) + 1; site.lastOpened = new Date().toISOString(); save();
  location.href = site.url;
}
function moveFocus(key) {
  const buttons = [...document.querySelectorAll('#hero .tile,#all-sites .tile'),$('#power-button')];
  const current = focusId==='power'?$('#power-button'):tileById.get(focusId); if (!buttons.length) return;
  if (!current || !buttons.includes(current)) { buttons[0].focus(); return; }
  const dir = ({ ArrowLeft:[-1,0], ArrowRight:[1,0], ArrowUp:[0,-1], ArrowDown:[0,1] })[key];
  const r = current.getBoundingClientRect(); const cx=r.left+r.width/2, cy=r.top+r.height/2;
  const candidates = buttons.filter((b)=>b!==current).map((b)=>{const q=b.getBoundingClientRect();const x=q.left+q.width/2-cx,y=q.top+q.height/2-cy;const primary=x*dir[0]+y*dir[1];const cross=Math.abs(x*dir[1]-y*dir[0]);return {b,primary,cross};}).filter((x)=>x.primary>0);
  candidates.sort((a,b)=>(a.primary+a.cross*2)-(b.primary+b.cross*2));
  (candidates[0]?.b || current).focus();
}
function goHome() { if (settingsView.hidden === false) { closeSettings(); return; } location.href = 'http://tauri.localhost/index.html'; }
function openSettings() { settingsView.hidden=false; populateSettings(); setTimeout(()=>$('#close-settings').focus(),20); }
function closeSettings() { settingsView.hidden=true; tileById.get('settings')?.focus() || tileById.get(focusId)?.focus(); }
function renderShortcutSettings() {
  const root=$('#shortcut-list');root.replaceChildren(...Object.entries(shortcutNames).map(([action,name])=>{
    const row=document.createElement('div');row.className='shortcut-row';const label=document.createElement('span');label.className='shortcut-name';label.textContent=name;
    const button=document.createElement('button');button.type='button';button.className='quiet shortcut-key';button.dataset.shortcut=action;button.textContent=recordingShortcut===action?'Press a key…':(settings.shortcuts[action]||defaultShortcuts[action]).split('+').map(displayKey).join(' + ');
    if(recordingShortcut===action)button.classList.add('recording');
    button.addEventListener('click',()=>{recordingShortcut=recordingShortcut===action?null:action;renderShortcutSettings();document.querySelector(`[data-shortcut="${action}"]`)?.focus();});row.append(label,button);return row;
  }));
}
function moveSettingsFocus(action) {
  const items=[...settingsView.querySelectorAll('button,select,input')].filter(el=>!el.disabled&&el.getClientRects().length);
  const current=document.activeElement;if(!items.length||!items.includes(current))return;
  const direction=({up:[0,-1],down:[0,1],left:[-1,0],right:[1,0]})[action];const r=current.getBoundingClientRect(),cx=r.left+r.width/2,cy=r.top+r.height/2;
  const next=items.filter(item=>item!==current).map(item=>{const b=item.getBoundingClientRect(),x=b.left+b.width/2-cx,y=b.top+b.height/2-cy,primary=x*direction[0]+y*direction[1],cross=Math.abs(x*direction[1]-y*direction[0]);return{item,primary,cross,score:primary+cross*2};}).filter(c=>c.primary>0).sort((a,b)=>a.score-b.score)[0];
  next?.item.focus();
}
function populateSettings() {
  $('#hero-mode').value=settings.heroMode; $('#remote-port').value=settings.remotePort; $('#fullscreen-setting').checked=!!settings.startFullscreen;
  $('#boot-animation-setting').checked=!!settings.showBootAnimation; $('#short-boot-setting').checked=!!settings.shortBootAnimation; $('#boot-sound-setting').checked=!!settings.bootSound;
  const root=$('#site-list'); root.replaceChildren(...sites.map((site,index)=>{
    const row=document.createElement('article'); row.className='site-row'; const img=document.createElement('img');img.src=siteLogo(site);img.alt='';
    const info=document.createElement('div');info.className='site-info';const name=document.createElement('strong');name.textContent=site.name;const url=document.createElement('span');url.textContent=site.url;info.append(name,url);
    const actions=document.createElement('div');actions.className='row-actions';
    const button=(label,fn,cls='quiet')=>{const b=document.createElement('button');b.className=cls;b.type='button';b.textContent=label;b.addEventListener('click',fn);actions.append(b);return b;};
    button('Edit',()=>editSite(index)); button(site.pinned?'Unpin':'Pin',()=>{site.pinned=!site.pinned;save();render();populateSettings();});
    button('↑',()=>moveSite(index,-1));button('↓',()=>moveSite(index,1));button('Delete',()=>{sites.splice(index,1);save();render();populateSettings();});
    row.append(img,info,actions);return row;
  }));
  renderShortcutSettings();
  if(!settingsView.hidden)$('#add-site').focus();
}
function moveSite(index,delta) { const next=index+delta;if(next<0||next>=sites.length)return;[sites[index],sites[next]]=[sites[next],sites[index]];save();render();populateSettings(); }
function editSite(index=-1) {
  editingIndex=index; const site=index>=0?sites[index]:null; $('#form-title').textContent=site?'Edit website':'Add website';$('#site-name').value=site?.name||'';$('#site-url').value=site?.url||'';$('#site-background').value=site?.background||'#ffffff';$('#preview-name').textContent=site?.name||'Website name';$('#preview-logo').src=siteLogo(site||{url:'https://example.com'});$('#form-error').textContent='';$('#site-logo').value='';$('#editor').showModal();$('#site-name').focus();
}
async function readLogo(file) {
  if(!file)return null;const ext=file.name.split('.').pop().toLowerCase();if(!['png','jpg','jpeg','svg','webp'].includes(ext))throw Error('Choose PNG, JPG, SVG, or WebP.');
  const dataUrl=await new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result);reader.onerror=reject;reader.readAsDataURL(file);});
  const path=await invoke('save_logo',{dataUrl,extension:ext});$('#preview-logo').src=dataUrl;return path;
}
function normalizeUrl(value) { let url=value.trim();if(!/^https?:\/\//i.test(url))url=`https://${url}`;const parsed=new URL(url);if(!['http:','https:'].includes(parsed.protocol))throw Error('Use a website URL starting with http:// or https://');return parsed.href; }
async function showRemote() {
  $('#remote-message').textContent='Scan the QR code with a phone on the same Wi-Fi network.';
  $('#remote-address').textContent='Starting local remote…';$('#remote-qr').replaceChildren();$('#remote-dialog').showModal();
  try { const info=await invoke('get_remote_info',{port:settings.remotePort});$('#remote-qr').innerHTML=info.qrSvg;$('#remote-address').textContent=info.url.replace(`?t=${new URL(info.url).searchParams.get('t')}`, '');$('#remote-code').textContent=info.pairingCode;$('#remote-message').textContent=`Scan to pair automatically, or open ${new URL(info.url).host} and enter the code. Server port ${info.port}.`;if(settings.remotePort!==info.port){settings.remotePort=info.port;$('#remote-port').value=info.port;save();} }
  catch(error){$('#remote-address').textContent=String(error);}
}
function showPowerDialog() { $('#power-dialog').showModal();setTimeout(()=>$('#exit-app').focus(),10); }
async function toggleFullscreen() {
  try { const appWindow=window.__TAURI__.window.getCurrentWindow();await appWindow.setFullscreen(!(await appWindow.isFullscreen())); }
  catch(error) { toast(`Could not toggle fullscreen: ${error}`); }
}
function acceptShortcut(event) {
  if(!recordingShortcut)return false;
  event.preventDefault();event.stopImmediatePropagation();
  if(event.key==='Escape'&&recordingShortcut!=='home'){recordingShortcut=null;renderShortcutSettings();return true;}
  const key=normalizeKey(event);if(!key)return true;
  const conflict=Object.entries(settings.shortcuts).find(([action,value])=>action!==recordingShortcut&&value===key);
  if(conflict){toast(`${key} is already assigned to ${shortcutNames[conflict[0]]}.`);return true;}
  const action=recordingShortcut;settings.shortcuts[action]=key;recordingShortcut=null;save();renderShortcutSettings();refreshShortcutHints();document.querySelector(`[data-shortcut="${action}"]`)?.focus();return true;
}
async function updateSettingInputs() { const oldPort=settings.remotePort;settings.heroMode=$('#hero-mode').value;settings.remotePort=Math.max(1024,Math.min(65535,Number($('#remote-port').value)||8080));settings.startFullscreen=$('#fullscreen-setting').checked;settings.showBootAnimation=$('#boot-animation-setting').checked;settings.shortBootAnimation=$('#short-boot-setting').checked;settings.bootSound=$('#boot-sound-setting').checked;await save();if(oldPort!==settings.remotePort){try{settings.remotePort=await invoke('configure_remote',{port:settings.remotePort});$('#remote-port').value=settings.remotePort;await save();}catch(error){toast(String(error));}}render();if(!settingsView.hidden)$('#hero-mode').focus(); }

document.addEventListener('keydown',(event)=>{
  if(acceptShortcut(event))return;
  const action=shortcutAction(event);
  const dialogs=['#editor','#remote-dialog','#power-dialog','#restart-confirm-dialog'].map($).filter(dialog=>dialog.open);
  if(dialogs.length){if(event.key==='Escape'||action==='home'){event.preventDefault();dialogs.at(-1).close();}else if(action==='select'&&document.activeElement?.tagName==='BUTTON'){event.preventDefault();document.activeElement.click();}return;}
  if(document.activeElement?.matches('input:not([type=checkbox]),textarea,[contenteditable=true]'))return;
  if(settingsView.hidden===false&&event.key==='Backspace'){event.preventDefault();closeSettings();return;}
  if(!action)return;
  const directions={up:'ArrowUp',down:'ArrowDown',left:'ArrowLeft',right:'ArrowRight'};
  if(directions[action]){event.preventDefault();settingsView.hidden?moveFocus(directions[action]):moveSettingsFocus(action);return;}
  if(action==='select'){
    event.preventDefault();
    if(!settingsView.hidden){const active=document.activeElement;if(active?.matches('button,select,input[type=checkbox]'))active.click();return;}
    if(focusId==='settings')openSettings();else{const id=focusId.split(':')[1];const index=Number(id);launch(sites.find((site,i)=>String(site.id||i)===id)||sites[index]);}return;
  }
  event.preventDefault();
  if(action==='home'){if(!settingsView.hidden)closeSettings();return;}
  if(action==='fullscreen'){toggleFullscreen();return;}
  if(action==='settings'){openSettings();return;}
  if(action==='remote'){showRemote();return;}
  if(action==='power'||focusId==='power')showPowerDialog();
});

$('#power-button').addEventListener('focus',()=>{document.querySelectorAll('.tile.selected').forEach((tile)=>tile.classList.remove('selected'));focusId='power';});
$('#close-settings').addEventListener('click',closeSettings);$('#add-site').addEventListener('click',()=>editSite());$('#cancel-editor').addEventListener('click',()=>$('#editor').close());$('#cancel-editor-2').addEventListener('click',()=>$('#editor').close());$('#show-remote').addEventListener('click',showRemote);$('#power-button').addEventListener('click',showPowerDialog);
$('#forget-phones').addEventListener('click',async()=>{try{await invoke('forget_paired_phones');toast('All paired phones forgotten. Scan again to reconnect.');}catch(error){toast(String(error));}});
$('#reset-shortcuts').addEventListener('click',()=>{settings.shortcuts={...defaultShortcuts};recordingShortcut=null;save();renderShortcutSettings();refreshShortcutHints();toast('Default shortcuts restored');});
$('#exit-app').addEventListener('click',()=>invoke('exit_launcher'));
$('#restart-pc').addEventListener('click',()=>{$('#power-dialog').close();$('#restart-confirm-dialog').showModal();setTimeout(()=>$('#confirm-restart').focus(),10);});
$('#confirm-restart').addEventListener('click',async()=>{try{await invoke('restart_windows');$('#restart-confirm-dialog').close();}catch(error){$('#restart-confirm-dialog').close();toast(`Could not restart Windows: ${error}`);}});
$('#hero-mode').addEventListener('change',updateSettingInputs);$('#remote-port').addEventListener('change',updateSettingInputs);$('#fullscreen-setting').addEventListener('change',updateSettingInputs);
$('#boot-animation-setting').addEventListener('change',updateSettingInputs);$('#short-boot-setting').addEventListener('change',updateSettingInputs);$('#boot-sound-setting').addEventListener('change',updateSettingInputs);
$('#reset-stats').addEventListener('click',()=>{for(const site of sites){site.launchCount=0;site.lastOpened=null;}save();render();populateSettings();toast('Usage statistics reset');});
$('#site-name').addEventListener('input',()=>$('#preview-name').textContent=$('#site-name').value||'Website name');$('#site-background').addEventListener('input',(e)=>$('.preview').style.setProperty('--tile-bg',e.target.value));$('#site-logo').addEventListener('change',e=>readLogo(e.target.files[0]).catch(err=>$('#form-error').textContent=err.message));
$('#drop-zone').addEventListener('dragover',e=>{e.preventDefault();e.currentTarget.classList.add('dragging');});$('#drop-zone').addEventListener('dragleave',e=>e.currentTarget.classList.remove('dragging'));$('#drop-zone').addEventListener('drop',e=>{e.preventDefault();e.currentTarget.classList.remove('dragging');readLogo(e.dataTransfer.files[0]).catch(err=>$('#form-error').textContent=err.message);});
$('#site-form').addEventListener('submit',async(e)=>{e.preventDefault();try{const name=$('#site-name').value.trim();if(!name)throw Error('Please enter a name.');const url=normalizeUrl($('#site-url').value);const file=$('#site-logo').files[0];let icon=editingIndex>=0?sites[editingIndex].icon:'';let logo=editingIndex>=0?sites[editingIndex].logo:'';if(file){icon=await readLogo(file);logo=$('#preview-logo').src;}if(!icon){icon=`https://www.google.com/s2/favicons?domain=${new URL(url).hostname}&sz=128`;logo=icon;}
  const site={...(editingIndex>=0?sites[editingIndex]:{}),id:editingIndex>=0?sites[editingIndex].id:`site-${Date.now()}`,name,url,icon,logo,background:$('#site-background').value,pinned:editingIndex>=0?sites[editingIndex].pinned:false,launchCount:editingIndex>=0?sites[editingIndex].launchCount:0,lastOpened:editingIndex>=0?sites[editingIndex].lastOpened:null};
  if(editingIndex>=0)sites[editingIndex]=site;else sites.push(site);await save();$('#editor').close();render();populateSettings();
}catch(err){$('#form-error').textContent=err.message;}});

function resetCursor(){document.body.classList.remove('cursor-hidden');clearTimeout(cursorTimer);cursorTimer=setTimeout(()=>document.body.classList.add('cursor-hidden'),2500);}document.addEventListener('mousemove',resetCursor);resetCursor();
clockTick();setInterval(clockTick,30000);
invoke('load_launcher_data').then(data=>{sites=(data.sites||[]).map((s,i)=>({...s,id:s.id||`site-${i}`}));settings={...settings,...data.settings,shortcuts:{...defaultShortcuts,...data.settings?.shortcuts}};refreshShortcutHints();render();startBoot();}).catch(err=>{toast(`Could not load launcher data: ${err}`);console.error(err);refreshShortcutHints();startBoot();});

window.__TAURI__?.event?.listen('remote-status',event=>{const count=Number(event.payload)||0;$('#phone-status').textContent=count?`Phone${count===1?'':'s'} connected · ${count}`:'Phone not connected';$('#phone-status').classList.toggle('connected',count>0);});

// In site pages, the document-start hook intercepts Backspace before the site's handlers.
if(location.hostname!=='tauri.localhost'&&location.protocol!=='tauri:'){document.body.style.display='none';}
