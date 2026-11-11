# Audio Lock

Stops apps changing your mic level and Windows switching your default audio device.

## Problem

Zoom, Teams, Discord and browsers keep adjusting the microphone level even with their own auto-adjust settings off, and Windows switches the default audio device whenever a dock or HDMI monitor connects. Windows has no setting for either; the usual answers are settings walkthroughs that often do not work, or a tiny single-purpose mic tool.

## Features (1.0)

- Lock the level of each chosen microphone; any change is reverted immediately via an endpoint volume callback
- Pin the default playback and recording devices, including the communications default, and restore them when Windows or an app switches
- Activity log naming what changed, when, and what it was restored to
- Tray toggle to pause locking, for when a change is intended
- Handles devices that disappear and return, re-applying the lock on reconnect

## Non-goals

- No per-app volume mixer
- No device switching hotkeys or profiles
- No audio effects, equaliser or noise suppression
- No changes to driver or enhancement settings

## Evidence of demand

- https://community.zoom.com/t5/Meetings/Zoom-auto-adjusts-Microphone-even-thought-that-setting-is-turned/m-p/95218
- https://neowin.net/forum/topic/1449405-windows-11-is-constantly-adjusting-my-microphone-volume-for-me-how-to-fix-this
- https://answers.microsoft.com/en-us/windows/forum/all/windows-11-input-device-volume-automatically/39c85225-5239-4bab-9642-b65437f64dd5
- https://windowsreport.com/microphone-automatically-adjusting-volume/
