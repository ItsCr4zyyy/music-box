# Installing on Linux

This guide is for **Debian / Ubuntu / Linux Mint / Raspberry Pi OS** (other distributions have packages with similar names).

## A) Ready-made package (recommended, x86-64)

From [Releases](https://github.com/ItsCr4zyyy/music-box/releases) download the latest:

- **`.deb`** (Debian, Ubuntu, Mint):

  ```bash
  sudo apt install ./music-box_*_amd64.deb
  music-box
  ```

- **`.AppImage`** (any distribution, no installation):

  ```bash
  chmod +x music-box_*_amd64.AppImage
  ./music-box_*_amd64.AppImage
  ```

Then scan the QR code on the screen and **sign up – the first account becomes the administrator**.

Music:
- AppImage → the `music` folder next to the `.AppImage` file,
- `.deb` → `~/Music/music-box` (`/usr/bin` isn't writable).

The exact path is shown in **Admin → System**. Data (database, downloaded songs, yt-dlp): `~/.local/share/music-box`.

> There is no ready-made package for Raspberry Pi (ARM) yet – build from source (B).

## B) Building from source

### 1. System packages

```bash
sudo apt update
sudo apt install -y build-essential curl git pkg-config libssl-dev \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libasound2-dev
# optional – converting YouTube formats:
sudo apt install -y ffmpeg
```

### 2. Node.js 20+ and Rust

> Node.js alone is not enough – the core is written in Rust, so Rust is needed to build it.

```bash
# Node.js (via NodeSource; or use nvm)
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo -E bash -
sudo apt install -y nodejs

# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
```

### 3. Build and run

```bash
git clone https://github.com/ItsCr4zyyy/music-box.git
cd music-box

cd web && npm install && npm run build && cd ..
cd box && npm install

npm run tauri dev        # development (window with the display)
npm run tauri build      # packages into target/release/bundle/ (deb, AppImage)
```

## C) Without a display (server + sound)

Handy for a mini PC or a Raspberry Pi without a screen – people control music-box only from their phones.
You only need `build-essential pkg-config libssl-dev libasound2-dev` (no webkit), Node.js and Rust.

```bash
cd web && npm install && npm run build && cd ..
cargo build --release --bin musicbox-headless
./target/release/musicbox-headless
```

### Start on boot (systemd user service)

`~/.config/systemd/user/music-box.service`:

```ini
[Unit]
Description=music-box
After=network-online.target sound.target

[Service]
WorkingDirectory=%h/music-box
ExecStart=%h/music-box/target/release/musicbox-headless
Restart=on-failure

[Install]
WantedBy=default.target
```

```bash
systemctl --user daemon-reload
systemctl --user enable --now music-box
sudo loginctl enable-linger $USER   # keeps running without a login session
journalctl --user -u music-box -f   # log
```

## The display as a kiosk

The app switches to fullscreen by itself (**Admin → Display → Fullscreen**). To start it automatically
after login, add `music-box` to the *startup applications* of your desktop (GNOME, KDE, LXDE…).

## Troubleshooting

- **No sound** – pick a device in **Admin → System → Audio output**. With PipeWire / PulseAudio, `pipewire`,
  `pulse` or "System default" usually works best. Your user must be in the `audio` group
  (`sudo usermod -aG audio $USER`, then log out and back in).
- **The phone can't connect** – the firewall (`sudo ufw allow 8787/tcp`) or a different network.
- **YouTube doesn't work** – **Admin → System → Update yt-dlp**.
- **Another port** – `MUSICBOX_PORT=8888 music-box`.
