# Installing on Windows

There are two ways: a **ready-made installer** (nothing else needed) or **building from source** (for development).

## A) Ready-made installer (recommended)

1. Open [Releases](https://github.com/ItsCr4zyyy/music-box/releases) and download the latest `music-box_…_x64-setup.exe`.
2. Run it. Windows SmartScreen may warn about an "unknown publisher" (the installer isn't signed) →
   **More info → Run anyway**.
3. It installs for your user only into `%LOCALAPPDATA%\Programs\music-box` – no administrator rights needed.
4. Start **music-box** from the Start menu.
5. On first start **Windows Firewall** asks about network access → tick **Private networks** and click
   **Allow access**. Otherwise phones can't reach the web.
6. A QR code appears on the screen. Scan it with your phone and **sign up – the first account becomes the administrator**.

Music: the `music` folder next to the app (`%LOCALAPPDATA%\Programs\music-box\music`). The exact path is
shown in **Admin → System**.

Data (database, downloaded songs, yt-dlp): `%APPDATA%\music-box`.

## B) Building from source

You need (once):

| What | Why | Where |
|---|---|---|
| **Node.js 20+** | the web and the display (Svelte) | <https://nodejs.org> (LTS) |
| **Rust** | the core and the app | <https://rustup.rs> → `rustup-init.exe`, default options |
| **Visual Studio Build Tools** | linker for Rust | rustup-init offers it; otherwise "Desktop development with C++" |
| **WebView2** | the display window | already part of Windows 10/11; otherwise <https://developer.microsoft.com/microsoft-edge/webview2> |
| **Git** | getting the source | <https://git-scm.com> |

> Node.js alone is not enough – the core is written in Rust, so Rust is needed to build it.
> If you only want to use music-box, download the ready-made installer (A).

Optional: **ffmpeg** in PATH – converts YouTube formats other than M4A (`winget install ffmpeg`).

```powershell
git clone https://github.com/ItsCr4zyyy/music-box.git
cd music-box

# the web (page for guests)
cd web
npm install
npm run build
cd ..

# display + server (opens a window; the first start compiles for a while)
cd box
npm install
npm run tauri dev
```

Your own installer: `cd box && npm run tauri build` → output in `target\release\bundle\` (`nsis\…-setup.exe`, `msi\…`).

### Server only, without a window

```powershell
cargo run --release --bin musicbox-headless
```

The web then runs at `http://<computer-ip>:8787/`.

## Troubleshooting

- **The phone can't connect** – the firewall (see above), or the phone and the computer are on different
  networks (guest Wi-Fi). Check the address in **Admin → System**.
- **No sound** – pick the right device in **Admin → System → Audio output**.
- **YouTube doesn't work** – **Admin → System → Update yt-dlp** (YouTube changes often).
- **Port 8787 is taken** – start with another port: `set MUSICBOX_PORT=8888` (cmd) or
  `$env:MUSICBOX_PORT=8888` (PowerShell), then run the app.
