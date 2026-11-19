use tauri::{webview::WebviewWindowBuilder, Manager, WebviewUrl};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::atomic::{AtomicUsize, Ordering}};

static POPUP_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Deserialize, Serialize)]
struct Site {
    name: String,
    url: String,
    icon: String,
}

fn install_dir(app: &tauri::AppHandle) -> PathBuf {
    app.path().resolve(".", tauri::path::BaseDirectory::Executable).unwrap_or_else(|_| {
        std::env::current_exe().ok().and_then(|p| p.parent().map(PathBuf::from)).unwrap_or_default()
    })
}

#[tauri::command]
fn load_sites(app: tauri::AppHandle) -> Result<Vec<Site>, String> {
    let root = install_dir(&app);
    let source = root.join("sites.json");
    let text = fs::read_to_string(&source).map_err(|error| format!("{}: {error}", source.display()))?;
    let mut sites: Vec<Site> = serde_json::from_str(&text).map_err(|error| format!("Invalid sites.json: {error}"))?;
    for site in &mut sites {
        let icon_path = root.join(&site.icon);
        if let Ok(bytes) = fs::read(icon_path) {
            let mime = if site.icon.to_ascii_lowercase().ends_with(".png") { "image/png" } else { "image/svg+xml" };
            site.icon = format!("data:{mime};base64,{}", STANDARD.encode(bytes));
        }
    }
    Ok(sites)
}

fn seed_user_config(app: &tauri::AppHandle) {
    let root = install_dir(app);
    let resource = app.path().resource_dir().ok().filter(|path| path.join("sites.json").exists())
        .or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().map(PathBuf::from));
    let site_target = root.join("sites.json");
    if !site_target.exists() {
        if let Some(source) = resource.as_ref().map(|path| path.join("sites.json")) {
            let _ = fs::copy(source, site_target);
        }
    }
    if let Some(source_dir) = resource.map(|path| path.join("icons")) {
        let target_dir = root.join("icons");
        if !target_dir.exists() {
            let _ = fs::create_dir_all(&target_dir);
            if let Ok(entries) = fs::read_dir(source_dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        let _ = fs::copy(entry.path(), target_dir.join(name));
                    }
                }
            }
        }
    }
}

const INPUT_SCRIPT: &str = r#"
(() => {
  if (window.top !== window) return;
  const isLauncher = () => location.protocol === 'tauri:' || location.hostname === 'tauri.localhost' || location.hostname === '127.0.0.1' || location.hostname === 'localhost';
  let cursorTimer;
  const resetCursor = () => {
    if (document.documentElement) document.documentElement.style.cursor = 'default';
    clearTimeout(cursorTimer);
    cursorTimer = setTimeout(() => { if (document.documentElement) document.documentElement.style.cursor = 'none'; }, 2500);
  };
  window.addEventListener('mousemove', resetCursor, true);
  resetCursor();
  const isTyping = (el) => {
    if (!el) return false;
    const tag = (el.tagName || '').toLowerCase();
    return el.isContentEditable || ['input', 'textarea', 'select'].includes(tag) || el.closest('[contenteditable="true"]');
  };
  window.addEventListener('keydown', (event) => {
    if (event.key === 'F11') {
      event.preventDefault();
      event.stopImmediatePropagation();
      const win = window.__TAURI__?.window?.getCurrentWindow?.();
      if (win) win.isFullscreen().then(value => win.setFullscreen(!value));
      return;
    }
    if ((event.key === 'Backspace' && !isTyping(event.target)) || (event.altKey && event.key === 'ArrowLeft')) {
      if (isLauncher()) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      location.href = 'http://tauri.localhost/index.html';
    }
  }, true);
  window.addEventListener('DOMContentLoaded', () => {
    if (isLauncher() || document.getElementById('__launcher-home-hint')) return;
    const label = document.createElement('div');
    label.id = '__launcher-home-hint';
    label.textContent = 'BACKSPACE  HOME';
    Object.assign(label.style, { position: 'fixed', zIndex: '2147483647', right: '24px', bottom: '20px', padding: '8px 12px', borderRadius: '8px', background: 'rgba(14,15,23,.76)', color: '#ddd', font: '11px Segoe UI,sans-serif', letterSpacing: '1px', opacity: '0', pointerEvents: 'none', transition: 'opacity .2s' });
    document.documentElement.appendChild(label);
    setTimeout(() => label.style.opacity = '.8', 2500);
    setTimeout(() => label.style.opacity = '0', 6500);
  });
})();
"#;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
        }))
        .invoke_handler(tauri::generate_handler![load_sites])
        .setup(|app| {
            seed_user_config(app.handle());
            let app_handle = app.handle().clone();
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Windows TV Launcher")
                .fullscreen(true)
                .decorations(false)
                .initialization_script(INPUT_SCRIPT)
                .on_new_window(move |url, _features| {
                    // Keep authentication popups inside the app; regular same-tab links stay in main.
                    let label = format!("auth-popup-{}", POPUP_ID.fetch_add(1, Ordering::Relaxed));
                    match WebviewWindowBuilder::new(&app_handle, label, WebviewUrl::External(url))
                        .title("Sign in")
                        .inner_size(900.0, 700.0)
                        .build()
                    {
                        Ok(window) => tauri::webview::NewWindowResponse::Create { window },
                        Err(_) => tauri::webview::NewWindowResponse::Deny,
                    }
                })
                .build()?;
            let _ = window.set_focus();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Windows TV Launcher");
}
