# Windows TV Launcher

A keyboard-first Windows streaming launcher built with Tauri v2, Rust, and plain HTML/CSS/JavaScript. It uses the installed Microsoft Edge WebView2 runtime and stores browser data persistently in the app's WebView2 data folder.

## Install requirements

1. Install **Node.js LTS** from [nodejs.org](https://nodejs.org/).
2. Install **Rust** with rustup from [rustup.rs](https://rustup.rs/) and keep the MSVC toolchain selected.
3. Install **Microsoft C++ Build Tools** (Visual Studio Build Tools 2022) with **Desktop development with C++** and the Windows 10/11 SDK.
4. Install **WebView2 Runtime** if it is not already present: [Microsoft Evergreen Bootstrapper](https://developer.microsoft.com/microsoft-edge/webview2/).
5. Open PowerShell in this project folder and run `npm install`.

## Run in development

```powershell
npm run tauri dev
```

## Build the Windows installer

```powershell
npm run tauri build
```

The NSIS installer is written under `src-tauri/target/release/bundle/nsis/`.

For a downloadable Windows x64 installer, see [GitHub Releases](https://github.com/Mayank-Alawadhi/Wwindows-TV-Launcher/releases). Pushing a `v*` version tag builds and attaches an x64 NSIS installer automatically; the initial release uses `v1.0.0`.

## Customize services

Edit `sites.json` and add an icon file under `icons/`. Each entry has `name`, `url`, and `icon` fields. On first run, the launcher copies these defaults beside the executable. In an installed copy, edit `sites.json` beside `Windows TV Launcher.exe` and add the icon under its `icons` folder, then restart the launcher. The home grid reads this file on startup.

The launcher can return to the home grid with Backspace or Alt+Left and toggles fullscreen with F11. The Backspace handler ignores editable fields. Single-instance behavior is enabled.

## DRM note

Windows WebView2 does not currently expose Widevine DRM. Services that require Widevine (including Netflix, Prime Video, or Disney+ playback in some regions/titles) may refuse protected playback inside this app. WebView2 can still load the sites, retain cookies, and support services that use compatible playback technology. The app cannot add Widevine support through a Tauri setting.
