use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, io::{Read, Write}, net::{TcpListener, TcpStream, UdpSocket}, path::PathBuf, process::Command, sync::{atomic::{AtomicUsize, Ordering}, Mutex}, thread, time::{Duration, SystemTime, UNIX_EPOCH}};
use tauri::{webview::{PageLoadEvent, WebviewWindowBuilder}, Emitter, Manager, WebviewUrl};

static POPUP_ID: AtomicUsize = AtomicUsize::new(1);
static REMOTE_GENERATION: AtomicUsize = AtomicUsize::new(0);
#[cfg(windows)]
#[link(name = "user32")]
extern "system" { fn keybd_event(vk: u8, scan: u8, flags: u32, extra: usize); }

#[derive(Default)]
struct RemoteControl { token: Mutex<String>, pairing_code: Mutex<String>, code_expires: Mutex<u64>, shortcuts: Mutex<HashMap<String, String>>, trusted: Mutex<HashMap<String,u64>>, attempts: Mutex<HashMap<String,(u8,u64)>>, port: AtomicUsize, connected: AtomicUsize }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteInfo { url: String, qr_svg: String, pairing_code: String, port: u16 }

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Site {
    #[serde(default)] id: String,
    name: String,
    url: String,
    icon: String,
    #[serde(default)] background: String,
    #[serde(default)] pinned: bool,
    #[serde(default)] launch_count: u64,
    #[serde(default)] last_opened: Option<String>,
    #[serde(skip)] logo: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
struct Settings {
    hero_mode: String,
    remote_port: u16,
    start_fullscreen: bool,
    show_boot_animation: bool,
    short_boot_animation: bool,
    boot_sound: bool,
    #[serde(default = "default_shortcuts")]
    shortcuts: HashMap<String, String>,
}
fn default_shortcuts() -> HashMap<String, String> {
    [
        ("up", "ArrowUp"), ("down", "ArrowDown"), ("left", "ArrowLeft"), ("right", "ArrowRight"),
        ("select", "Enter"), ("home", "Escape"), ("fullscreen", "F11"), ("settings", "S"),
        ("remote", "Q"), ("power", "P"),
    ].into_iter().map(|(action,key)| (action.to_string(), key.to_string())).collect()
}
impl Default for Settings {
    fn default() -> Self { Self { hero_mode: "most-used".into(), remote_port: 8080, start_fullscreen: true, show_boot_animation: true, short_boot_animation: false, boot_sound: false, shortcuts: default_shortcuts() } }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherData { sites: Vec<Site>, settings: Settings }

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("logos")).map_err(|e| e.to_string())?;
    Ok(dir)
}
fn trusted_path(app:&tauri::AppHandle)->Result<PathBuf,String>{Ok(data_dir(app)?.join("trusted-phones.json"))}

fn seed_data(app: &tauri::AppHandle, dir: &PathBuf) -> Result<(), String> {
    let file = dir.join("sites.json");
    if file.exists() { return Ok(()); }
    let resource = app.path().resource_dir().ok();
    let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().map(PathBuf::from);
    let source = resource.filter(|p| p.join("sites.json").exists()).or(fallback).ok_or("Bundled sites.json is missing")?;
    let mut sites: Vec<Site> = serde_json::from_slice(&fs::read(source.join("sites.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let icons = source.join("icons");
    for site in &mut sites {
        if let Some(name) = site.icon.strip_prefix("icons/") {
            let target = dir.join("logos").join(name);
            if !target.exists() { let _ = fs::copy(icons.join(name), &target); }
            site.icon = format!("logos/{name}");
        }
        site.background = "#ffffff".into();
    }
    for (index, site) in sites.iter_mut().enumerate() { site.id = format!("default-{index}"); }
    let defaults = LauncherData { sites, settings: Settings::default() };
    fs::write(file, serde_json::to_vec_pretty(&defaults).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn hydrate_sites(dir: &PathBuf, sites: &mut [Site]) {
    for site in sites {
        if site.icon.starts_with("data:") { site.logo = site.icon.clone(); continue; }
        if let Ok(bytes) = fs::read(dir.join(&site.icon)) {
            let mime = match PathBuf::from(&site.icon).extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
                "png" => "image/png", "jpg" | "jpeg" => "image/jpeg", "webp" => "image/webp", _ => "image/svg+xml"
            };
            site.logo = format!("data:{mime};base64,{}", STANDARD.encode(bytes));
        }
    }
}

#[tauri::command]
fn load_launcher_data(app: tauri::AppHandle) -> Result<LauncherData, String> {
    let dir = data_dir(&app)?; seed_data(&app, &dir)?;
    let mut data: LauncherData = serde_json::from_slice(&fs::read(dir.join("sites.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    hydrate_sites(&dir, &mut data.sites); Ok(data)
}

#[tauri::command]
fn save_launcher_data(app: tauri::AppHandle, data: LauncherData, remote: tauri::State<'_, RemoteControl>) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let mut clean = data;
    for site in &mut clean.sites {
        if site.name.trim().is_empty() { return Err("Site name cannot be empty".into()); }
        let parsed = url::Url::parse(&site.url).map_err(|_| format!("Invalid URL: {}", site.url))?;
        if !["http", "https"].contains(&parsed.scheme()) { return Err("Only HTTP and HTTPS URLs are supported".into()); }
        site.name = site.name.trim().to_string();
    }
    fs::write(dir.join("sites.json"), serde_json::to_vec_pretty(&clean).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    *remote.shortcuts.lock().map_err(|e| e.to_string())? = clean.settings.shortcuts.clone();
    Ok(())
}

#[tauri::command]
fn save_logo(app: tauri::AppHandle, data_url: String, extension: String) -> Result<String, String> {
    match extension.to_ascii_lowercase().as_str() { "png" | "jpg" | "jpeg" | "webp" | "svg" => (), _ => return Err("Unsupported logo format".into()) };
    let encoded = data_url.split_once(',').map(|(_, b)| b).ok_or("Invalid logo data")?;
    let bytes = STANDARD.decode(encoded).map_err(|e| e.to_string())?;
    if bytes.len() > 5_000_000 { return Err("Logo must be smaller than 5 MB".into()); }
    let name = format!("{}.{}", uuid_v4(), extension.to_ascii_lowercase());
    let relative = format!("logos/{name}");
    fs::write(data_dir(&app)?.join(&relative), bytes).map_err(|e| e.to_string())?;
    Ok(relative)
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

fn local_ip() -> String {
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let script="Get-NetIPConfiguration | Where-Object { $_.IPv4DefaultGateway -and $_.NetAdapter.Status -eq 'Up' -and $_.NetAdapter.InterfaceDescription -notmatch 'VPN|Virtual|Hyper-V|WSL|Docker|VMware|VirtualBox|TAP|TUN|WireGuard|Loopback' } | ForEach-Object { $_.IPv4Address.IPAddress } | Select-Object -First 1";
        if let Ok(out)=Command::new("powershell.exe").args(["-NoProfile","-NonInteractive","-Command",script]).creation_flags(CREATE_NO_WINDOW).output(){let ip=String::from_utf8_lossy(&out.stdout).trim().to_string();if !ip.is_empty(){return ip;}}
    }
    UdpSocket::bind("0.0.0.0:0").and_then(|socket| { socket.connect("192.0.2.1:80")?; socket.local_addr() }).map(|addr| addr.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".into())
}
fn http(stream: &mut TcpStream, status: &str, mime: &str, body: &str) { let _=write!(stream,"HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{body}",body.len()); }
fn page() -> &'static str { r#"<!doctype html><html><head><meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no"><link rel="manifest" href="/manifest.webmanifest"><title>TV Remote</title><style>*{box-sizing:border-box;-webkit-user-select:none;user-select:none}body{margin:0;background:#10121a;color:white;font:600 18px system-ui;touch-action:manipulation}main{max-width:520px;margin:auto;padding:18px}h1{font-size:22px}small,#status{color:#aeb1c2}.tabs{position:fixed;bottom:0;left:0;right:0;display:flex;background:#20222c;padding:8px}.tabs button{flex:1}.tab{display:none;padding-bottom:90px}.tab.on{display:block}button,input,textarea{font:inherit;color:white;background:#282a36;border:1px solid #ffffff25;border-radius:16px;padding:16px}button{min-height:58px;touch-action:manipulation}.pad{display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin-top:25px}.blank{visibility:hidden}.touch{height:65vh;border:1px solid #ffffff30;border-radius:24px;background:#191b25;touch-action:none}.touch span{display:grid;place-items:center;height:100%;color:#888}.keyrow{display:flex;gap:8px}textarea{width:100%;min-height:150px;user-select:text;-webkit-user-select:text;touch-action:auto}.pair{padding:28px 0}.hide{display:none!important}</style></head><body><main><h1>Windows TV Launcher</h1><div id="status">Pair your phone</div><section id="pair" class="pair"><p>Enter the 6-digit code shown on your TV.</p><input id="code" inputmode="numeric" maxlength="6" placeholder="000000"><button id="pairbtn">Pair phone</button></section><div id="remote" class="hide"><section id="keypad" class="tab on"><div class="pad"><i class="blank"></i><button data-a="up">▲</button><i class="blank"></i><button data-a="left">◀</button><button data-a="select">OK</button><button data-a="right">▶</button><button data-a="home">⌂ Back</button><button data-a="down">▼</button><button data-a="settings">⚙</button><button data-a="fullscreen">Fullscreen</button><button data-a="volume-up">Vol +</button><button data-a="mute">Mute</button><button data-a="volume-down">Vol −</button></div></section><section id="touchpad" class="tab"><label><input id="gestures" type="checkbox"> Gesture mode</label><input id="sensitivity" type="range" min="0.5" max="3" step=".1" value="1.4"><div class="touch" id="surface"><span>Touchpad · drag, tap, or swipe</span></div></section><section id="keyboard" class="tab"><textarea id="typing" placeholder="Tap here to type or use voice typing"></textarea><div class="keyrow"><button id="send">Send / Enter</button><button id="backspace">⌫</button><button id="paste">Paste</button></div></section></div></main><nav class="tabs hide" id="tabs"><button data-tab="keypad">Keypad</button><button data-tab="touchpad">Touchpad</button><button data-tab="keyboard">Keyboard</button></nav><script>let ws,retry;const status=document.querySelector('#status');let token=localStorage.remoteToken;async function pair(code){try{let r=await fetch('/pair?code='+encodeURIComponent(code));if(!r.ok)throw 0;let d=await r.json();token=d.token;localStorage.remoteToken=token;localStorage.remoteExpires=Date.now()+30*86400000;document.querySelector('#pair').classList.add('hide');document.querySelector('#remote').classList.remove('hide');document.querySelector('#tabs').classList.remove('hide');connect()}catch{status.textContent='Pairing failed. Check the code and try again.'}}document.querySelector('#pairbtn').onclick=()=>pair(document.querySelector('#code').value);const master=new URLSearchParams(location.search).get('t');if(master){fetch('/pair?t='+encodeURIComponent(master)).then(r=>r.ok?r.json():Promise.reject()).then(d=>{token=d.token;localStorage.remoteToken=token;localStorage.remoteExpires=Date.now()+30*86400000;history.replaceState({},'', '/');document.querySelector('#pair').classList.add('hide');document.querySelector('#remote').classList.remove('hide');document.querySelector('#tabs').classList.remove('hide');connect()}).catch(()=>status.textContent='This pairing link has expired. Enter the code below.')}else if(token&&+(localStorage.remoteExpires||0)>Date.now()){document.querySelector('#pair').classList.add('hide');document.querySelector('#remote').classList.remove('hide');document.querySelector('#tabs').classList.remove('hide');connect()}function connect(){status.textContent='Reconnecting…';ws=new WebSocket('ws://'+location.host+'/ws?token='+encodeURIComponent(token||''));ws.onopen=()=>{status.textContent='Connected';clearTimeout(retry)};ws.onclose=()=>{status.textContent='Reconnecting…';retry=setTimeout(connect,1500)};ws.onerror=()=>ws.close()}function send(o){if(ws?.readyState===1)ws.send(JSON.stringify(o))}document.querySelectorAll('[data-tab]').forEach(b=>b.onclick=()=>{document.querySelectorAll('.tab').forEach(x=>x.classList.remove('on'));document.querySelector('#'+b.dataset.tab).classList.add('on')});function haptic(){navigator.vibrate?.(12)}document.querySelectorAll('[data-a]').forEach(b=>{let timer;b.onpointerdown=()=>{haptic();send({type:'action',action:b.dataset.a});timer=setInterval(()=>send({type:'action',action:b.dataset.a}),180)};['pointerup','pointerleave','pointercancel'].forEach(e=>b.addEventListener(e,()=>clearInterval(timer)))});const area=document.querySelector('#surface');let start;area.onpointerdown=e=>{if(e.pointerType==='touch')return;start={x:e.clientX,y:e.clientY};area.setPointerCapture(e.pointerId)};area.onpointerup=e=>{if(e.pointerType==='touch'||!start)return;send({type:Math.hypot(e.clientX-start.x,e.clientY-start.y)<12?'click':'pointer',dx:e.clientX-start.x,dy:e.clientY-start.y,sensitivity:+document.querySelector('#sensitivity').value});start=null};let touchStart=null,touchCount=0,moved=false,longPress=false,longTimer;const distance=(a,b)=>Math.hypot(a.clientX-b.clientX,a.clientY-b.clientY);area.addEventListener('touchstart',e=>{e.preventDefault();touchCount=e.touches.length;moved=false;longPress=false;clearTimeout(longTimer);if(touchCount===1){let t=e.touches[0],rect=area.getBoundingClientRect();touchStart={x:t.clientX,y:t.clientY,localY:t.clientY-rect.top,t:t.clientY,lastX:t.clientX,lastY:t.clientY};if(document.querySelector('#gestures').checked)longTimer=setTimeout(()=>{longPress=true;send({type:'action',action:'home'})},700)}else if(touchCount===2)touchStart={x:distance(e.touches[0],e.touches[1]),lastX:(e.touches[0].clientX+e.touches[1].clientX)/2,lastY:(e.touches[0].clientY+e.touches[1].clientY)/2};},{passive:false});area.addEventListener('touchmove',e=>{e.preventDefault();if(!touchStart)return;const gest=document.querySelector('#gestures').checked;if(touchCount===1){let t=e.touches[0],dx=t.clientX-touchStart.lastX,dy=t.clientY-touchStart.lastY;if(Math.abs(t.clientX-touchStart.x)+Math.abs(t.clientY-touchStart.y)>10)moved=true;if(moved){clearTimeout(longTimer);if(gest&&touchStart.localY<36&&dy>35){send({type:'action',action:'home'});touchStart.localY=999}else if(!gest)send({type:'pointer',dx,dy,sensitivity:+document.querySelector('#sensitivity').value})}touchStart.lastX=t.clientX;touchStart.lastY=t.clientY}else if(touchCount===2&&e.touches.length===2){let d=distance(e.touches[0],e.touches[1]),cx=(e.touches[0].clientX+e.touches[1].clientX)/2,cy=(e.touches[0].clientY+e.touches[1].clientY)/2;if(Math.abs(d-touchStart.x)>8){moved=true;send({type:'zoom',delta:(touchStart.x-d)*1.5});touchStart.x=d}else{let dy=cy-touchStart.lastY,dx=cx-touchStart.lastX;if(Math.abs(dx)+Math.abs(dy)>5){moved=true;send({type:'scroll',dx,dy})}}touchStart.lastX=cx;touchStart.lastY=cy}},{passive:false});area.addEventListener('touchend',e=>{e.preventDefault();clearTimeout(longTimer);if(touchStart&&!moved&&!longPress){if(touchCount===2)send({type:'rightclick'});else send({type:document.querySelector('#gestures').checked?'action':'click',action:'select'})}else if(touchStart&&moved&&touchCount===1&&!longPress&&document.querySelector('#gestures').checked){let dx=e.changedTouches[0].clientX-touchStart.x,dy=e.changedTouches[0].clientY-touchStart.y;send({type:'action',action:Math.abs(dx)>Math.abs(dy)?(dx>0?'right':'left'):(dy>0?'down':'up')})}touchStart=null;touchCount=0;},{passive:false});let typed='';document.querySelector('#typing').oninput=e=>{let value=e.target.value;if(value.length>typed.length)send({type:'text',text:value.slice(typed.length)});else if(value.length<typed.length)send({type:'raw-key',key:'Backspace'});typed=value};document.querySelector('#send').onclick=()=>send({type:'action',action:'select'});document.querySelector('#backspace').onclick=()=>send({type:'raw-key',key:'Backspace'});document.querySelector('#paste').onclick=async()=>{try{document.querySelector('#typing').value=await navigator.clipboard.readText();document.querySelector('#typing').dispatchEvent(new Event('input'))}catch{}}</script></body></html>"# }

fn run_ws(mut stream: TcpStream, app: tauri::AppHandle, token: String) {
    let mut ws=match tungstenite::accept(&mut stream){Ok(ws)=>ws,Err(_)=>return};
    let remote=app.state::<RemoteControl>();remote.connected.fetch_add(1,Ordering::SeqCst);
    let _=app.emit("remote-status",remote.connected.load(Ordering::SeqCst));
    while let Ok(msg)=ws.read(){if !msg.is_text(){continue} let Ok(v)=serde_json::from_str::<serde_json::Value>(msg.to_text().unwrap_or("")) else{continue};
        let typ=v["type"].as_str().unwrap_or("");
        let js=if typ=="action"||typ=="click" { let action=if typ=="click"{"click"}else{v["action"].as_str().unwrap_or("")};
            #[cfg(windows)] if let Some(vk)=match action{"volume-up"=>Some(0xAF),"volume-down"=>Some(0xAE),"mute"=>Some(0xAD),_=>None}{unsafe{keybd_event(vk,0,0,0);keybd_event(vk,0,2,0);}continue;}
            if action=="click" {"(()=>{let x=window.__remoteX||innerWidth/2,y=window.__remoteY||innerHeight/2;document.elementFromPoint(x,y)?.dispatchEvent(new MouseEvent('click',{bubbles:true,clientX:x,clientY:y}))})()".into()} else {
            let a=match action{"up"=>"up","down"=>"down","left"=>"left","right"=>"right","select"=>"select","home"=>"home","settings"=>"settings","fullscreen"=>"fullscreen","remote"=>"remote","power"=>"power",_=>""}; let mapping=remote.shortcuts.lock().ok().and_then(|m|m.get(a).cloned()).unwrap_or_else(||default_shortcuts().get(a).cloned().unwrap_or_default()); let parts:Vec<&str>=mapping.split('+').collect();let k=parts.last().copied().unwrap_or("");let k=serde_json::to_string(if k=="Space"{" "}else{k}).unwrap_or_default();format!("document.dispatchEvent(new KeyboardEvent('keydown',{{key:{k},ctrlKey:{},altKey:{},shiftKey:{},metaKey:{},bubbles:true,cancelable:true}}))",parts.contains(&"Ctrl"),parts.contains(&"Alt"),parts.contains(&"Shift"),parts.contains(&"Meta"))}
        } else if typ=="raw-key" {let k=serde_json::to_string(v["key"].as_str().unwrap_or("Backspace")).unwrap_or_default();format!("document.activeElement?.dispatchEvent(new KeyboardEvent('keydown',{{key:{k},bubbles:true,cancelable:true}}))")
        } else if typ=="text" {let t=serde_json::to_string(v["text"].as_str().unwrap_or("")).unwrap_or_default();format!("(()=>{{let e=document.activeElement;if(e&&('value'in e)){{let s=e.selectionStart??e.value.length,z=e.selectionEnd??s;e.setRangeText({t},s,z,'end');e.dispatchEvent(new InputEvent('input',{{bubbles:true,inputType:'insertText',data:{t}}}))}}else if(e?.isContentEditable){{document.execCommand('insertText',false,{t})}}}})()")
        } else if typ=="rightclick" {"(()=>{let x=window.__remoteX||innerWidth/2,y=window.__remoteY||innerHeight/2,e=document.elementFromPoint(x,y);e?.dispatchEvent(new MouseEvent('contextmenu',{bubbles:true,button:2,clientX:x,clientY:y}))})()".into()
        } else if typ=="zoom" {let delta=v["delta"].as_f64().unwrap_or(0.0);format!("document.dispatchEvent(new WheelEvent('wheel',{{bubbles:true,ctrlKey:true,deltaY:{delta}}}))")
        } else if typ=="pointer" {let dx=v["dx"].as_f64().unwrap_or(0.0);let dy=v["dy"].as_f64().unwrap_or(0.0);let s=v["sensitivity"].as_f64().unwrap_or(1.0);format!("(()=>{{window.__remoteX=Math.max(0,Math.min(innerWidth,(window.__remoteX||innerWidth/2)+{dx}*{s}));window.__remoteY=Math.max(0,Math.min(innerHeight,(window.__remoteY||innerHeight/2)+{dy}*{s}));let x=window.__remoteX,y=window.__remoteY;let e=document.elementFromPoint(x,y);e?.dispatchEvent(new MouseEvent('mousemove',{{bubbles:true,clientX:x,clientY:y}}));document.documentElement.style.cursor='default';clearTimeout(window.__rcTimer);window.__rcTimer=setTimeout(()=>document.documentElement.style.cursor='none',3000)}})()")
        } else if typ=="scroll" {let dy=v["dy"].as_f64().unwrap_or(0.0);let dx=v["dx"].as_f64().unwrap_or(0.0);format!("window.scrollBy({{top:{dy},left:{dx},behavior:'smooth'}})")}
        else {String::new()};
        if !js.is_empty(){if let Some(w)=app.get_webview_window("main"){let _=w.eval(&js);}}
    }
    remote.connected.fetch_sub(1,Ordering::SeqCst);let _=app.emit("remote-status",remote.connected.load(Ordering::SeqCst));let _=token;
}
fn start_remote(app: tauri::AppHandle, requested: u16) -> Result<u16, String> {
    let base=if requested>=1024{requested}else{8080};let mut bound=None;
    for p in base..=u16::MAX {if let Ok(l)=TcpListener::bind(("0.0.0.0",p)){bound=Some((l,p));break}}
    let (listener,port)=bound.ok_or("Could not find a free remote port")?;
    listener.set_nonblocking(true).map_err(|e|e.to_string())?;
    let generation=REMOTE_GENERATION.fetch_add(1,Ordering::SeqCst)+1;let token=uuid_v4();let code=format!("{:06}",uuid::Uuid::new_v4().as_u128()%1_000_000);
    {let r=app.state::<RemoteControl>();*r.token.lock().map_err(|e|e.to_string())?=token.clone();*r.pairing_code.lock().map_err(|e|e.to_string())?=code;*r.code_expires.lock().map_err(|e|e.to_string())?=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()+600;r.port.store(port as usize,Ordering::SeqCst);}
    thread::spawn(move||while REMOTE_GENERATION.load(Ordering::SeqCst)==generation {
        match listener.accept() {
            Ok((mut stream,_))=>{
                let mut peek=[0u8;2048];let n=stream.peek(&mut peek).unwrap_or(0);let head=String::from_utf8_lossy(&peek[..n]);
                if head.to_ascii_lowercase().contains("upgrade: websocket") {
                    let target=head.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("");let bearer=target.split("token=").nth(1).unwrap_or("").split('&').next().unwrap_or("");
                    let valid=app.state::<RemoteControl>().trusted.lock().map(|tokens|tokens.get(bearer).is_some_and(|expiry|*expiry>SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs())).unwrap_or(false);
                    if valid{let client_app=app.clone();let client_token=bearer.to_string();thread::spawn(move||run_ws(stream,client_app,client_token));}else{let _=stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n");}
                } else {
                    let mut buf=[0u8;8192];let n=stream.read(&mut buf).unwrap_or(0);let req=String::from_utf8_lossy(&buf[..n]);let target=req.lines().next().unwrap_or("").split_whitespace().nth(1).unwrap_or("/");let route=target.split('?').next().unwrap_or("/");
                    match route {
                        "/"=>http(&mut stream,"200 OK","text/html; charset=utf-8",page()),
                        "/manifest.webmanifest"=>http(&mut stream,"200 OK","application/manifest+json",r##"{"name":"Windows TV Remote","short_name":"TV Remote","start_url":"/","display":"standalone","background_color":"#10121a","theme_color":"#10121a","icons":[{"src":"/icon.svg","sizes":"any","type":"image/svg+xml"}]}"##),
                        "/icon.svg"=>http(&mut stream,"200 OK","image/svg+xml",r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect width="128" height="128" rx="28" fill="#161824"/><path d="M18 30h92v60H18z" fill="#8673ff"/><path d="m54 45 31 15-31 16z" fill="white"/><path d="M40 103h48" stroke="white" stroke-width="7" stroke-linecap="round"/></svg>"##),
                        "/pair"=>{
                            let query=target.split_once('?').map(|x|x.1).unwrap_or("");let args:HashMap<_,_>=query.split('&').filter_map(|x|x.split_once('=')).collect();let remote=app.state::<RemoteControl>();let master=remote.token.lock().map(|x|x.clone()).unwrap_or_default();let code=remote.pairing_code.lock().map(|x|x.clone()).unwrap_or_default();let expiry=*remote.code_expires.lock().unwrap_or_else(|e|e.into_inner());let now=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();let ip=stream.peer_addr().map(|x|x.ip().to_string()).unwrap_or_default();
                            let lock_until=remote.attempts.lock().ok().and_then(|a|a.get(&ip).map(|x|x.1)).unwrap_or(0);
                            let qr_ok=args.get("t").map(|x|x.to_string()==master).unwrap_or(false);let code_ok=args.get("code").map(|x|x.to_string()==code).unwrap_or(false)&&now<expiry&&lock_until<=now;
                            if qr_ok||code_ok {
                                let phone_token=uuid_v4();let trusted_until=now+30*24*60*60;
                                if let Ok(mut trusted)=remote.trusted.lock(){trusted.insert(phone_token.clone(),trusted_until);if let Ok(path)=trusted_path(&app){let _=fs::write(path,serde_json::to_vec(&*trusted).unwrap_or_default());}}
                                if let Ok(mut attempts)=remote.attempts.lock(){attempts.remove(&ip);}
                                http(&mut stream,"200 OK","application/json",&format!("{{\"token\":\"{phone_token}\",\"expires\":{trusted_until}}}"));
                            } else {
                                if args.contains_key("code")&&lock_until<=now {if let Ok(mut attempts)=remote.attempts.lock(){let entry=attempts.entry(ip).or_insert((0,0));entry.0=entry.0.saturating_add(1);if entry.0>=5{entry.0=0;entry.1=now+60;}}}
                                http(&mut stream,"403 Forbidden","application/json","{}");
                            }
                        },
                        _=>http(&mut stream,"404 Not Found","text/plain","Not found")
                    }
                }
            },
            Err(ref e) if e.kind()==std::io::ErrorKind::WouldBlock=>thread::sleep(Duration::from_millis(25)),
            Err(_)=>break
        }
    });
    Ok(port)
}

#[tauri::command]
fn configure_remote(app: tauri::AppHandle, port: u16) -> Result<u16, String> { start_remote(app, port) }

#[tauri::command]
fn get_remote_info(app: tauri::AppHandle, _port: u16) -> Result<RemoteInfo, String> {
    let r=app.state::<RemoteControl>();let token=r.token.lock().map_err(|e|e.to_string())?.clone();let port=r.port.load(Ordering::SeqCst) as u16;let code=r.pairing_code.lock().map_err(|e|e.to_string())?.clone();
    let url=format!("http://{}:{port}/?t={token}",local_ip());let qr_svg=qrcode::QrCode::new(url.as_bytes()).map_err(|e|e.to_string())?.render::<qrcode::render::svg::Color>().min_dimensions(220,220).build();
    Ok(RemoteInfo{url,qr_svg,pairing_code:code,port})
}

#[tauri::command]
fn forget_paired_phones(app:tauri::AppHandle,remote:tauri::State<'_,RemoteControl>)->Result<(),String>{remote.trusted.lock().map_err(|e|e.to_string())?.clear();let path=trusted_path(&app)?;if path.exists(){fs::remove_file(path).map_err(|e|e.to_string())?;}Ok(())}

#[tauri::command]
fn exit_launcher(app: tauri::AppHandle) { app.exit(0); }

#[tauri::command]
fn restart_windows() -> Result<(), String> {
    #[cfg(windows)]
    { std::process::Command::new("shutdown").args(["/r", "/t", "5", "/c", "Restart requested by Windows TV Launcher"]).spawn().map(|_| ()).map_err(|e| e.to_string()) }
    #[cfg(not(windows))]
    { Err("Computer restart is available only on Windows".into()) }
}

const INPUT_SCRIPT: &str = r#"
(() => {
  let cursorTimer;
  const resetCursor = () => { document.documentElement.style.cursor='default'; clearTimeout(cursorTimer); cursorTimer=setTimeout(()=>document.documentElement.style.cursor='none',2500); };
  window.addEventListener('mousemove',resetCursor,true); resetCursor();
  const typing = el => !!el && (el.isContentEditable || ['INPUT','TEXTAREA','SELECT'].includes(el.tagName) || !!el.closest?.('[contenteditable="true"]'));
  const isLauncher = () => location.protocol === 'tauri:' || ['tauri.localhost','127.0.0.1','localhost'].includes(location.hostname);
  window.__launcherShortcuts=window.__launcherShortcuts||{up:'ArrowUp',down:'ArrowDown',left:'ArrowLeft',right:'ArrowRight',select:'Enter',home:'Escape',fullscreen:'F11',settings:'S',remote:'Q',power:'P'};
  const keyName=e=>e.key===' '?'Space':e.key.length===1?e.key.toUpperCase():e.key;
  const shortcutFor=e=>{if(['Control','Alt','Shift','Meta'].includes(e.key))return null;const parts=[];if(e.ctrlKey)parts.push('Ctrl');if(e.altKey)parts.push('Alt');if(e.shiftKey)parts.push('Shift');if(e.metaKey)parts.push('Meta');parts.push(keyName(e));return parts.join('+');};
  const actionFor=e=>{const key=shortcutFor(e);return key?Object.keys(window.__launcherShortcuts).find(action=>window.__launcherShortcuts[action]===key):null;};
  window.addEventListener('keydown',e=>{
    if(e.key==='F11' && document.getElementById('boot-screen')) return;
    if(isLauncher() || typing(e.target))return;
    const action=actionFor(e);
    if(action==='fullscreen'){e.preventDefault();e.stopImmediatePropagation();const w=window.__TAURI__?.window?.getCurrentWindow?.();if(w)w.isFullscreen().then(v=>w.setFullscreen(!v));return;}
    if(action==='home'||e.key==='Backspace'||(e.altKey&&e.key==='ArrowLeft')){e.preventDefault();e.stopImmediatePropagation();location.href='http://tauri.localhost/index.html?from=site';return;}
    if(['settings','remote','power'].includes(action)){e.preventDefault();e.stopImmediatePropagation();location.href='http://tauri.localhost/index.html?from=site&shortcut='+action;}
  },true);
})();
"#;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| { if let Some(w)=app.get_webview_window("main"){let _=w.set_focus();} }))
        .invoke_handler(tauri::generate_handler![load_launcher_data, save_launcher_data, save_logo, configure_remote, get_remote_info, forget_paired_phones, exit_launcher, restart_windows])
        .setup(|app| {
            app.manage(RemoteControl::default());
            if let Ok(path)=trusted_path(app.handle()){if let Ok(bytes)=fs::read(path){if let Ok(map)=serde_json::from_slice::<HashMap<String,u64>>(&bytes){if let Ok(mut trusted)=app.state::<RemoteControl>().trusted.lock(){*trusted=map;}}}}
            let data=load_launcher_data(app.handle().clone()).ok();
            if let Some(data)=data.as_ref() {
                if let Ok(mut shortcuts)=app.state::<RemoteControl>().shortcuts.lock() { *shortcuts=data.settings.shortcuts.clone(); }
                let _=start_remote(app.handle().clone(),data.settings.remote_port);
            }
            let app_handle=app.handle().clone(); let fullscreen=data.map(|d|d.settings.start_fullscreen).unwrap_or(true);
            let window=WebviewWindowBuilder::new(app,"main",WebviewUrl::App("index.html".into()))
                .title("Windows TV Launcher").fullscreen(fullscreen).decorations(false).background_color(tauri::window::Color(0,0,0,255)).initialization_script(INPUT_SCRIPT)
                .on_page_load({
                    let settings_handle=app_handle.clone();
                    move |window,payload| {
                        if matches!(payload.event(),PageLoadEvent::Started|PageLoadEvent::Finished) && ["http","https"].contains(&payload.url().scheme()) {
                            let json={let remote=settings_handle.state::<RemoteControl>();remote.shortcuts.lock().ok().and_then(|shortcuts|serde_json::to_string(&*shortcuts).ok())};
                            if let Some(json)=json { let _=window.eval(&format!("window.__launcherShortcuts={json};")); }
                        }
                    }
                })
                .on_new_window(move |url,_| { let label=format!("auth-popup-{}",POPUP_ID.fetch_add(1,Ordering::Relaxed));
                    match WebviewWindowBuilder::new(&app_handle,label,WebviewUrl::External(url)).title("Sign in").inner_size(900.0,700.0).build(){Ok(window)=>tauri::webview::NewWindowResponse::Create{window},Err(_)=>tauri::webview::NewWindowResponse::Deny} })
                .build()?; let _=window.set_focus(); Ok(())
        })
        .run(tauri::generate_context!()).expect("error while running Windows TV Launcher");
}
