# IS Fleet — Desktop (Tauri)

A thin native desktop shell that opens the deployed IS Fleet web app
(`https://is-fleet-frontend.vercel.app`) in its own window — Viber-style:

- Real Windows installer (`.exe` / `.msi`)
- System tray icon (left-click to open, right-click for **Open / Quit**)
- **Close hides to tray** — the app keeps running in the background so the
  socket stays connected and new messages keep arriving
- Native OS notifications (raised by the web page via the Notification API)

There is **no local frontend build** — the shell just loads the hosted site,
so it auto-updates whenever you deploy to Vercel. To point it at a local dev
server instead, change the URL in `src-tauri/tauri.conf.json`.

---

## Prerequisites (one-time)

1. **Node.js** 18+ (you already have it).
2. **Rust** toolchain — https://rustup.rs (`rustup` installs `cargo`).
3. **WebView2** runtime — preinstalled on Windows 11; on Windows 10 install the
   "Evergreen" runtime from Microsoft if missing.
4. **MSVC build tools** — Visual Studio Build Tools with the
   "Desktop development with C++" workload (Rust needs the MSVC linker on Windows).

## Setup

```bash
cd is-fleet-desktop
npm install
```

### Generate the app icons (required before first run/build)

Tauri needs generated icon files. Point it at the IS Fleet logo:

```bash
npm run icon ../is-fleet-frontend/public/IS_logo.png
```

This creates `src-tauri/icons/` (`.ico`, `.png`, `.icns`). Commit them.

## Run in dev

```bash
npm run dev
```

Opens a native window loading the site, with the tray and close-to-tray
behaviour active. First run compiles Rust (a few minutes); later runs are fast.

## Build the installer

```bash
npm run build
```

Output (Windows):

```
src-tauri/target/release/bundle/nsis/IS Fleet_0.1.0_x64-setup.exe
src-tauri/target/release/bundle/msi/IS Fleet_0.1.0_x64_en-US.msi
```

Ship either one to users.

---

## Configuration

- **Which URL it loads** — `src-tauri/tauri.conf.json` → `build.frontendDist`
  and `build.devUrl`. Set both to `http://localhost:3000` to develop against a
  local Next.js server, or to a staging URL as needed.
- **App identity / version** — `identifier`, `version`, `productName` in
  `tauri.conf.json`.

## Roadmap / notes

- **Notifications**: the web app's own notifications (Notification API + socket)
  surface through WebView2. For a tighter Viber feel — unread **badge on the
  taskbar/tray icon**, "flash on new message", start-on-login — wire the web
  app's socket events to the Tauri side (`tauri-plugin-notification` +
  `set_overlay_icon`) as a phase 2.
- **Auto-update**: add `tauri-plugin-updater` when you want in-app updates of
  the shell itself (rarely needed, since the web content already auto-updates).
- **Code signing**: unsigned installers show a SmartScreen warning. For
  distribution, sign with an Authenticode certificate (config under
  `bundle.windows`).
- If the Rust build ever complains about a Tauri API mismatch, regenerate a
  clean baseline with `npm create tauri-app@latest` and re-apply the
  `tauri.conf.json` URL + the tray/close-to-tray code from `src-tauri/src/lib.rs`.
```
