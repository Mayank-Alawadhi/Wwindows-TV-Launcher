# Windows TV Launcher

A TV-style Windows streaming launcher built with Tauri v2, Rust, and plain HTML/CSS/JavaScript. It uses Microsoft Edge WebView2 for site browsing and keeps the launcher configuration in the current user's local app-data folder. WebView2 cookies and logins persist with the app.

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

For a downloadable Windows x64 installer, see [GitHub Releases](https://github.com/Mayank-Alawadhi/Wwindows-TV-Launcher/releases). Pushing to `main` builds and attaches the Windows x64 NSIS installer to the `v1.0.0` release. The workflow can also be run manually from the GitHub Actions page to retry a failed build.

## Sites, logos, and settings

The home screen includes a Most used shelf, a six-column All sites grid, and keyboard navigation. Press **S** or choose **Settings** to add and edit sites, upload logos, pin sites, reorder them, or reset usage counts. The **Q** key shows the phone remote QR code; the phone and PC must be on the same local network. The remote server port can be changed in Settings. Use **F11** to toggle fullscreen.

On first run, default sites and icons are copied from the bundled `sites.json` and `icons` folder. At runtime, the persistent configuration and uploaded logos are stored in `%LOCALAPPDATA%\app.windowstv.launcher\` (`sites.json` and `logos`). Settings changes are saved there and survive app updates and reinstalls. You can edit that user-data `sites.json` manually while the app is closed. The bundled root `sites.json` is the template for new profiles. Single-instance behavior is enabled.

## DRM note

Windows WebView2 does not provide Widevine DRM. Services that require Widevine (including some Netflix, Prime Video, and Disney+ playback) may refuse protected playback inside this app. WebView2 can load the sites, retain cookies, and play content supported by its available media technologies.
