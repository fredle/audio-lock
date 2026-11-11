# Audio Lock

Stops apps changing your mic level and Windows switching your default audio device.

Zoom, Teams, Discord and browsers keep adjusting the microphone level even with
their own auto-adjust settings off, and Windows switches the default audio device
whenever a dock or HDMI monitor connects. Windows has no setting for either.
Audio Lock holds both in place.

## Features

- **Lock a microphone's level.** Tick a mic and pick a level. Any change by an
  app is reverted immediately (it listens to Windows' endpoint volume callback
  rather than polling).
- **Pin default devices.** Pin the default playback and recording devices and
  the communications defaults. If Windows or an app switches, Audio Lock
  switches back.
- **Activity log.** Shows what changed, when, and what it was restored to.
  Repeated identical events are folded into one row.
- **Pause from the tray.** Closing the window keeps Audio Lock running in the
  tray. Right-click the tray icon to pause or resume locking (also `Ctrl+P` in
  the window) when you intend to change something.
- **Devices that come and go.** A locked mic that is unplugged is remembered and
  locked again when it returns.
- Optional **Start with Windows** (starts hidden in the tray).

Keyboard: `Ctrl+P` pause/resume, `Ctrl+1/2/3` switch tabs.

Not included: per-app volume mixer, hotkeys/profiles for switching devices,
audio effects, or driver/enhancement settings.

## Install

- **Microsoft Store**: search for "Audio Lock".
- **GitHub Releases**: download the latest `Setup.exe` from
  <https://github.com/fredle/audio-lock/releases>. It installs per-user, no
  administrator rights needed, and updates itself (toggle in the app footer).

## Where data lives

`%LOCALAPPDATA%\AudioLock\settings.json` holds your locked microphones and
levels, pinned devices, and the auto-update preference. The activity log is kept
in memory only and is cleared when the app exits. "Start with Windows" adds one
value (`AudioLock`) under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`.

## Privacy

No telemetry, no accounts, no admin rights. The only network traffic is the
update check against this project's GitHub Releases (about once a day, and only
if "Automatically update" is on). Audio is never recorded or read; the app only
reads and sets volume levels and default-device assignments.

## Build

```
cargo build --release
cargo test --release
```

Rust 2024, egui/eframe. Windows only. Setting the default device uses the
undocumented `IPolicyConfig` COM interface, which is what other tools use for
this since Windows exposes no public API.
