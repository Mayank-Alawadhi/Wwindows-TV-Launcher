const heroRoot = document.querySelector('#hero');
const allRoot = document.querySelector('#all-sites');
const settingsView = document.querySelector('#settings');
const clock = document.querySelector('#clock');
const bootScreen = document.querySelector('#boot-screen');
const { invoke } = window.__TAURI__.core;
let sites = [];
let settings = { heroMode: 'most-used', remotePort: 47831, startFullscreen: true, showBootAnimation: true, shortBootAnimation: false, bootSound: false };
let focusId = '';
let editingIndex = -1;
let cursorTimer;
let bootState = 'loading';
let bootSkipRequested = false;
let bootRevealTimer;
let bootFinishTimer;
const tileById = new Map();
const colorCache = new Map();
const $ = (selector) => document.querySelector(selector);

function clockTick() { clock.textContent = new Intl.DateTimeFormat(undefined, { weekday: 'long', hour: 'numeric', minute: '2-digit' }).format(new Date()); }
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
    if (shortcut === 's') openSettings(); else if (shortcut === 'q') showRemote();
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
  const target = tileById.get(focusId) || tileById.values().next().value; focusId = target?.dataset.id || ''; target?.classList.add('selected'); target?.focus({ preventScroll: true }); focusedColor();
}
function launch(site) {
  if (!site) return;
  site.launchCount = (site.launchCount || 0) + 1; site.lastOpened = new Date().toISOString(); save();
  location.href = site.url;
}
function moveFocus(key) {
  const buttons = [...document.querySelectorAll('#hero .tile,#all-sites .tile')];
  const current = tileById.get(focusId); if (!buttons.length) return;
  if (!current || !buttons.includes(current)) { buttons[0].focus(); return; }
  const dir = ({ ArrowLeft:[-1,0], ArrowRight:[1,0], ArrowUp:[0,-1], ArrowDown:[0,1] })[key];
  const r = current.getBoundingClientRect(); const cx=r.left+r.width/2, cy=r.top+r.height/2;
  const candidates = buttons.filter((b)=>b!==current).map((b)=>{const q=b.getBoundingClientRect();const x=q.left+q.width/2-cx,y=q.top+q.height/2-cy;const primary=x*dir[0]+y*dir[1];const cross=Math.abs(x*dir[1]-y*dir[0]);return {b,primary,cross};}).filter((x)=>x.primary>0);
  candidates.sort((a,b)=>a.cross-b.cross || a.primary-b.primary);
  (candidates[0]?.b || buttons[0]).focus();
}
function goHome() { if (settingsView.hidden === false) { closeSettings(); return; } location.href = 'http://tauri.localhost/index.html'; }
function openSettings() { settingsView.hidden=false; populateSettings(); setTimeout(()=>$('#close-settings').focus(),20); }
function closeSettings() { settingsView.hidden=true; tileById.get('settings')?.focus() || tileById.get(focusId)?.focus(); }
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
  try { const info=await invoke('get_remote_info',{port:settings.remotePort});$('#remote-qr').innerHTML=info.qrSvg;$('#remote-address').textContent=info.url; }
  catch(error){$('#remote-address').textContent=String(error);}
}
function updateSettingInputs() { const oldPort=settings.remotePort;settings.heroMode=$('#hero-mode').value;settings.remotePort=Math.max(1024,Math.min(65535,Number($('#remote-port').value)||47831));settings.startFullscreen=$('#fullscreen-setting').checked;settings.showBootAnimation=$('#boot-animation-setting').checked;settings.shortBootAnimation=$('#short-boot-setting').checked;settings.bootSound=$('#boot-sound-setting').checked;save();if(oldPort!==settings.remotePort)invoke('configure_remote',{port:settings.remotePort}).catch(toast);render();if(!settingsView.hidden)$('#hero-mode').focus(); }

document.addEventListener('keydown',(event)=>{
  const editorOpen=$('#editor').open, remoteOpen=$('#remote-dialog').open;
  if(editorOpen||remoteOpen){if(event.key==='Escape'){event.preventDefault();if(editorOpen)$('#editor').close();if(remoteOpen)$('#remote-dialog').close();}return;}
  if(settingsView.hidden===false){
    if(event.key==='Escape'||event.key==='Backspace'){event.preventDefault();closeSettings();}
    else if(['ArrowUp','ArrowDown','ArrowLeft','ArrowRight'].includes(event.key)){const items=[...settingsView.querySelectorAll('button,select,input')].filter(el=>!el.disabled);const i=items.indexOf(document.activeElement);items[(i+(event.key==='ArrowLeft'||event.key==='ArrowUp'?-1:1)+items.length)%items.length]?.focus();event.preventDefault();}
    else if(event.key==='Enter'&&document.activeElement?.tagName==='BUTTON'){document.activeElement.click();event.preventDefault();}
    return;
  }
  if(['ArrowLeft','ArrowRight','ArrowUp','ArrowDown'].includes(event.key)){event.preventDefault();moveFocus(event.key);}
  else if(event.key==='Enter'){event.preventDefault();if(focusId==='settings')openSettings();else{const index=Number((focusId.split(':')[1]));launch(sites.find((s,i)=>String(s.id||i)===String(focusId.split(':')[1]))||sites[index]);}}
  else if(event.key==='s'||event.key==='S'){event.preventDefault();openSettings();}
  else if(event.key==='q'||event.key==='Q'){event.preventDefault();showRemote();}
  else if(event.key==='Escape'||event.key==='Backspace'){event.preventDefault();}
});

$('#close-settings').addEventListener('click',closeSettings);$('#add-site').addEventListener('click',()=>editSite());$('#cancel-editor').addEventListener('click',()=>$('#editor').close());$('#cancel-editor-2').addEventListener('click',()=>$('#editor').close());$('#show-remote').addEventListener('click',showRemote);
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
invoke('load_launcher_data').then(data=>{sites=(data.sites||[]).map((s,i)=>({...s,id:s.id||`site-${i}`}));settings={...settings,...data.settings};render();startBoot();}).catch(err=>{toast(`Could not load launcher data: ${err}`);console.error(err);startBoot();});

// In site pages, the document-start hook intercepts Backspace before the site's handlers.
if(location.hostname!=='tauri.localhost'&&location.protocol!=='tauri:'){document.body.style.display='none';}
