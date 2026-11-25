# Store listing: Audio Lock

## Product name

Audio Lock

## Short description

Stop apps changing your mic level and Windows switching your default audio device.

## Description

Your microphone level keeps changing by itself. Zoom, Teams, Discord and your browser adjust it mid-call, even with their own auto-adjust settings turned off. Then you dock your laptop or plug in an HDMI monitor and Windows quietly switches your sound output or microphone to something you did not choose. Windows has no setting to stop either.

Audio Lock fixes both. Pick the level you want for a microphone and Audio Lock holds it: the moment an app changes it, it is put straight back. Pin your default playback and recording devices, including the communications defaults, and if Windows or an app switches them, Audio Lock switches them back.

An activity log shows exactly what changed, when, and what it was restored to, so you can see which events are happening and how often. Need to change something on purpose? Pause locking from the tray icon, make your change, then resume.

Audio Lock copes with real desks. Unplug a locked microphone and it is remembered; plug it back in and the lock is re-applied automatically. Close the window and it keeps working quietly from the system tray. Optionally start it with Windows so you never think about it again.

It is small, runs without administrator rights, and does nothing else: no per-app mixer, no effects, no driver changes. No accounts and no telemetry.

## Product features

- Lock the level of any microphone; changes by apps are reverted immediately
- Choose the exact level to hold with a simple percentage slider
- Pin the default playback device so docks and monitors cannot hijack it
- Pin the default recording device
- Pin the communications defaults for playback and recording separately
- Activity log of what changed, when, and what it was restored to
- Pause and resume locking from the system tray when a change is intended
- Keeps running in the tray when you close the window
- Locked microphones are re-locked automatically when they reconnect
- Optional start with Windows, hidden in the tray
- Keyboard shortcuts: Ctrl+P to pause, Ctrl+1/2/3 to switch tabs
- No administrator rights, no accounts, no telemetry
- Automatic background updates (can be switched off)

## Search terms

- microphone volume lock
- stop mic auto adjust
- default audio device
- lock default device
- mic level
- prevent audio switching
- sound device manager

## Category and subcategory

Utilities & tools > System utilities (alternatively: Multimedia design > Audio)

## Privacy

Stored locally in `%LOCALAPPDATA%\AudioLock\settings.json`: which microphones are locked and at what level, which default devices are pinned, and the auto-update preference. The activity log lives in memory only and is cleared on exit. If "Start with Windows" is enabled, one value is added to the current user's Run registry key.

Network: apart from a feedback note you choose to send (below), the only network call is a check against this project's GitHub Releases for a newer version, about once a day, and only when "Automatically update" is on. No audio is recorded or read, nothing else is sent anywhere, there is no telemetry and no account.

Feedback: the "What would make this better?" link in the window lets you send the developer a short note. Only the note, the app's name and version, and the platform are sent, to the developer's server at feedback.leatham.net, and only when you press Send. No name, email address or device identifier is sent, and the server does not keep IP addresses. Notes are used only to improve the app and are never shared or sold.

## Screenshot ideas

1. Microphones tab with a mic ticked and the "Locked at 70%" slider, header showing "Active".
2. Default devices tab with playback and recording devices pinned in the dropdowns.
3. Activity tab showing a few entries such as "Microphone: level changed to 100%, restored to 70%" and "Default playback changed to HDMI, restored to Speakers".
4. The tray menu open with "Pause locking" next to the tray icon, with the window behind it.
