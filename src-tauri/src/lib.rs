use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{fs, net::UdpSocket, path::PathBuf, sync::{atomic::{AtomicUsize, Ordering}, Mutex}, thread, time::Duration};
use tauri::{webview::WebviewWindowBuilder, Manager, WebviewUrl};
use tiny_http::{Header, Response, Server};

static POPUP_ID: AtomicUsize = AtomicUsize::new(1);
static REMOTE_GENERATION: AtomicUsize = AtomicUsize::new(0);

#[derive(Default)]
struct RemoteControl { queue: Mutex<Vec<String>>, token: Mutex<String> }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteInfo { url: String, qr_svg: String }

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
}
impl Default for Settings {
    fn default() -> Self { Self { hero_mode: "most-used".into(), remote_port: 47831, start_fullscreen: true, show_boot_animation: true, short_boot_animation: false, boot_sound: false } }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherData { sites: Vec<Site>, settings: Settings }

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(dir.join("logos")).map_err(|e| e.to_string())?;
    Ok(dir)
}

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
fn save_launcher_data(app: tauri::AppHandle, data: LauncherData) -> Result<(), String> {
    let dir = data_dir(&app)?;
    let mut clean = data;
    for site in &mut clean.sites {
        if site.name.trim().is_empty() { return Err("Site name cannot be empty".into()); }
        let parsed = url::Url::parse(&site.url).map_err(|_| format!("Invalid URL: {}", site.url))?;
        if !["http", "https"].contains(&parsed.scheme()) { return Err("Only HTTP and HTTPS URLs are supported".into()); }
        site.name = site.name.trim().to_string();
    }
    fs::write(dir.join("sites.json"), serde_json::to_vec_pretty(&clean).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
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
    UdpSocket::bind("0.0.0.0:0").and_then(|socket| { socket.connect("192.0.2.1:80")?; socket.local_addr() }).map(|addr| addr.ip().to_string()).unwrap_or_else(|_| "127.0.0.1".into())
}

fn start_remote(app: tauri::AppHandle, port: u16) -> Result<(), String> {
    if !(1024..=65535).contains(&port) { return Err("Remote port must be between 1024 and 65535".into()); }
    let generation = REMOTE_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let token = format!("{}{:x}", uuid_v4(), generation);
    *app.state::<RemoteControl>().token.lock().map_err(|e| e.to_string())? = token.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(250));
        if REMOTE_GENERATION.load(Ordering::SeqCst) != generation { return; }
        let server = match Server::http(format!("0.0.0.0:{port}")) { Ok(server) => server, Err(error) => { eprintln!("Remote server could not start: {error}"); return; } };
        while REMOTE_GENERATION.load(Ordering::SeqCst) == generation {
            let request = match server.recv_timeout(Duration::from_millis(150)) { Ok(Some(request)) => request, Ok(None) => continue, Err(_) => break };
            let path = request.url().to_string();
            let (route, query) = path.split_once('?').unwrap_or((&path, ""));
            let params: std::collections::HashMap<_, _> = query.split('&').filter_map(|part| part.split_once('=')).collect();
            let authorized = params.get("token").map(|value| *value == token).unwrap_or(false);
            let response = if route == "/" && authorized {
                let html = format!(r#"<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1"><title>TV Remote</title><style>*{{box-sizing:border-box}}body{{margin:0;min-height:100vh;background:#11131c;color:#fff;font:600 20px system-ui;display:grid;place-items:center}}main{{width:min(420px,92vw);text-align:center}}h1{{font-size:24px}}.pad{{display:grid;grid-template-columns:repeat(3,1fr);gap:14px;margin-top:28px}}button{{height:78px;border:1px solid #ffffff30;border-radius:20px;color:white;background:#ffffff15;font-size:27px;touch-action:manipulation}}button:active{{background:#aaa0ff;color:#111}}.blank{{visibility:hidden}}small{{color:#bbb;font-size:13px}}</style><main><h1>Windows TV Launcher</h1><small>Connected to your TV launcher</small><div class="pad"><button class="blank"></button><button onclick="send('ArrowUp')">▲</button><button class="blank"></button><button onclick="send('ArrowLeft')">◀</button><button onclick="send('Enter')">OK</button><button onclick="send('ArrowRight')">▶</button><button onclick="send('Escape')">⌂</button><button onclick="send('ArrowDown')">▼</button><button onclick="send('s')">⚙</button></div><p><small>Arrow buttons move · OK selects · Home goes back</small></p></main><script>const token='{token}';function send(key){{fetch('/command?token='+token+'&key='+encodeURIComponent(key)).catch(()=>alert('Remote disconnected'));}}document.addEventListener('keydown',e=>{{if(['ArrowUp','ArrowDown','ArrowLeft','ArrowRight','Enter','Escape','Backspace'].includes(e.key)){{e.preventDefault();send(e.key==='Backspace'?'Escape':e.key)}}}})</script>"#);
                Response::from_string(html).with_header(Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap())
            } else if route == "/command" && authorized {
                if let Some(key) = params.get("key") { if ["ArrowUp","ArrowDown","ArrowLeft","ArrowRight","Enter","Escape","Backspace","s","q"].contains(key) { if let Some(remote) = app.try_state::<RemoteControl>() { if let Ok(mut queue)=remote.queue.lock() { queue.push(if *key=="Backspace" {"Escape".into()} else {(*key).into()}); } } } }
                Response::from_string("ok")
            } else { Response::from_string("Unauthorized or unknown route").with_status_code(403) };
            let _ = request.respond(response);
        }
    });
    Ok(())
}

#[tauri::command]
fn configure_remote(app: tauri::AppHandle, port: u16) -> Result<(), String> { start_remote(app, port) }

#[tauri::command]
fn get_remote_info(app: tauri::AppHandle, port: u16) -> Result<RemoteInfo, String> {
    let token = app.state::<RemoteControl>().token.lock().map_err(|e| e.to_string())?.clone();
    if token.is_empty() { return Err("Remote server is starting".into()); }
    let url = format!("http://{}:{port}/?token={token}", local_ip());
    let qr_svg = qrcode::QrCode::new(url.as_bytes()).map_err(|e|e.to_string())?.render::<qrcode::render::svg::Color>().min_dimensions(220,220).build();
    Ok(RemoteInfo { url, qr_svg })
}

#[tauri::command]
fn poll_remote_commands(app: tauri::AppHandle) -> Vec<String> {
    app.state::<RemoteControl>().queue.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default()
}

const INPUT_SCRIPT: &str = r#"
(() => {
  let cursorTimer;
  const resetCursor = () => { document.documentElement.style.cursor='default'; clearTimeout(cursorTimer); cursorTimer=setTimeout(()=>document.documentElement.style.cursor='none',2500); };
  window.addEventListener('mousemove',resetCursor,true); resetCursor();
  const typing = el => !!el && (el.isContentEditable || ['INPUT','TEXTAREA','SELECT'].includes(el.tagName) || !!el.closest?.('[contenteditable="true"]'));
  const isLauncher = () => location.protocol === 'tauri:' || ['tauri.localhost','127.0.0.1','localhost'].includes(location.hostname);
  window.addEventListener('keydown',e=>{
    if(e.key==='F11' && document.getElementById('boot-screen')) return;
    if(e.key==='F11'){e.preventDefault();e.stopImmediatePropagation();const w=window.__TAURI__?.window?.getCurrentWindow?.();if(w)w.isFullscreen().then(v=>w.setFullscreen(!v));return;}
    if(!isLauncher() && !typing(e.target) && ((e.key==='Backspace') || e.key==='Escape' || (e.altKey&&e.key==='ArrowLeft'))){e.preventDefault();e.stopImmediatePropagation();location.href='http://tauri.localhost/index.html?from=site';}
    if(!isLauncher() && !typing(e.target) && (e.key==='s'||e.key==='S'||e.key==='q'||e.key==='Q')){e.preventDefault();e.stopImmediatePropagation();location.href='http://tauri.localhost/index.html?from=site&shortcut='+e.key.toLowerCase();}
  },true);
  setInterval(()=>{const invoke=window.__TAURI__?.core?.invoke;if(invoke)invoke('poll_remote_commands').then(keys=>keys.forEach(key=>document.dispatchEvent(new KeyboardEvent('keydown',{key,bubbles:true,cancelable:true})))).catch(()=>{});},250);
})();
"#;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| { if let Some(w)=app.get_webview_window("main"){let _=w.set_focus();} }))
        .invoke_handler(tauri::generate_handler![load_launcher_data, save_launcher_data, save_logo, configure_remote, get_remote_info, poll_remote_commands])
        .setup(|app| {
            app.manage(RemoteControl::default());
            let data=load_launcher_data(app.handle().clone()).ok();
            if let Some(data)=data.as_ref() { let _=start_remote(app.handle().clone(),data.settings.remote_port); }
            let app_handle=app.handle().clone(); let fullscreen=data.map(|d|d.settings.start_fullscreen).unwrap_or(true);
            let window=WebviewWindowBuilder::new(app,"main",WebviewUrl::App("index.html".into()))
                .title("Windows TV Launcher").fullscreen(fullscreen).decorations(false).background_color(tauri::window::Color(0,0,0,255)).initialization_script(INPUT_SCRIPT)
                .on_new_window(move |url,_| { let label=format!("auth-popup-{}",POPUP_ID.fetch_add(1,Ordering::Relaxed));
                    match WebviewWindowBuilder::new(&app_handle,label,WebviewUrl::External(url)).title("Sign in").inner_size(900.0,700.0).build(){Ok(window)=>tauri::webview::NewWindowResponse::Create{window},Err(_)=>tauri::webview::NewWindowResponse::Deny} })
                .build()?; let _=window.set_focus(); Ok(())
        })
        .run(tauri::generate_context!()).expect("error while running Windows TV Launcher");
}
