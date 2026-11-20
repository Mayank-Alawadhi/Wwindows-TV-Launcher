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

The home screen includes a Most used shelf, a six-column All sites grid, and keyboard navigation. Press **S** or choose **Settings** to add and edit sites, upload logos, pin sites, reorder them, or reset usage counts. Use **F11** to toggle fullscreen.

## Phone remote

1. Start the launcher and press **Q**, or open Settings and choose **Connect your phone**.
2. Connect the phone and PC to the same non-guest Wi-Fi network. Scan the QR code to pair, or type the displayed address into the phone browser and enter the six-digit code. The pairing code expires after ten minutes.
3. If Windows Firewall asks, allow Windows TV Launcher on **Private networks**. If the page does not load, check that both devices use the same Wi-Fi and turn off router client isolation or guest-network isolation.
4. The mobile page offers Keypad, Touchpad and Keyboard tabs. Add it to the phone home screen from the browser menu for shortcut access. No internet connection is used by the remote page itself.

The local HTTP and WebSocket server listens on port **8080** by default. Change **Remote server port** in Settings to choose another port; if the requested port is busy, the launcher uses the next available port and displays the actual address in the connect dialog. Keep the firewall rule limited to Private networks.

Settings also lets you remap the launcher keys for directional movement, select, home/back, fullscreen, settings, remote, and the power menu. The power menu can close the launcher or restart Windows; restarting requires a second confirmation. The home screen shows the current time above a smaller time-of-day greeting.

On first run, default sites and icons are copied from the bundled `sites.json` and `icons` folder. At runtime, the persistent configuration and uploaded logos are stored in `%LOCALAPPDATA%\app.windowstv.launcher\` (`sites.json` and `logos`). Settings changes are saved there and survive app updates and reinstalls. You can edit that user-data `sites.json` manually while the app is closed. The bundled root `sites.json` is the template for new profiles. Single-instance behavior is enabled.

## Startup screen

The launch animation uses the bundled `assets/boot-logo.png`; it does not fetch the logo at runtime. The Settings screen can disable the animation or use its 1.5 second version. An optional sound is off by default; to enable it, add `assets/boot-sound.mp3` before building. The Windows `.ico` installer icon is generated from the same logo mark and includes a 256×256 image.

## DRM note

Windows WebView2 does not provide Widevine DRM. Services that require Widevine (including some Netflix, Prime Video, and Disney+ playback) may refuse protected playback inside this app. WebView2 can load the sites, retain cookies, and play content supported by its available media technologies.
