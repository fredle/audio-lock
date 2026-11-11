//! Pure, testable logic: what is locked/pinned, when a change needs
//! reverting, the activity log and display formatting. No Windows calls here.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Volume differences smaller than this are treated as "unchanged" (the
/// endpoint reports 32-bit floats that round-trip through driver steps).
pub const TOLERANCE: f32 = 0.004;

/// Maximum number of activity log rows kept in memory.
pub const LOG_CAPACITY: usize = 300;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct LockedMic {
    pub id: String,
    pub name: String,
    /// Target master volume, 0.0..=1.0.
    pub level: f32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Pin {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Pins {
    #[serde(default)]
    pub playback: Option<Pin>,
    #[serde(default)]
    pub playback_comms: Option<Pin>,
    #[serde(default)]
    pub record: Option<Pin>,
    #[serde(default)]
    pub record_comms: Option<Pin>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Playback,
    Record,
}

/// One of the four pinnable defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Playback,
    PlaybackComms,
    Record,
    RecordComms,
}

impl Slot {
    pub const ALL: [Slot; 4] = [
        Slot::Playback,
        Slot::PlaybackComms,
        Slot::Record,
        Slot::RecordComms,
    ];

    pub fn flow(self) -> Flow {
        match self {
            Slot::Playback | Slot::PlaybackComms => Flow::Playback,
            Slot::Record | Slot::RecordComms => Flow::Record,
        }
    }

    pub fn is_comms(self) -> bool {
        matches!(self, Slot::PlaybackComms | Slot::RecordComms)
    }

    pub fn label(self) -> &'static str {
        match self {
            Slot::Playback => "Default playback device",
            Slot::PlaybackComms => "Default communications playback",
            Slot::Record => "Default recording device",
            Slot::RecordComms => "Default communications recording",
        }
    }

    /// Short lowercase phrase used in log messages.
    pub fn short(self) -> &'static str {
        match self {
            Slot::Playback => "default playback",
            Slot::PlaybackComms => "communications playback",
            Slot::Record => "default recording",
            Slot::RecordComms => "communications recording",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Slot::Playback => 0,
            Slot::PlaybackComms => 1,
            Slot::Record => 2,
            Slot::RecordComms => 3,
        }
    }
}

impl Pins {
    pub fn get(&self, slot: Slot) -> Option<&Pin> {
        match slot {
            Slot::Playback => self.playback.as_ref(),
            Slot::PlaybackComms => self.playback_comms.as_ref(),
            Slot::Record => self.record.as_ref(),
            Slot::RecordComms => self.record_comms.as_ref(),
        }
    }

    pub fn set(&mut self, slot: Slot, pin: Option<Pin>) {
        match slot {
            Slot::Playback => self.playback = pin,
            Slot::PlaybackComms => self.playback_comms = pin,
            Slot::Record => self.record = pin,
            Slot::RecordComms => self.record_comms = pin,
        }
    }
}

/// True when `current` has drifted from `target` enough to revert.
pub fn volume_differs(current: f32, target: f32) -> bool {
    (current - target).abs() > TOLERANCE
}

/// Level as a whole percentage for display.
pub fn percent(level: f32) -> u32 {
    (level.clamp(0.0, 1.0) * 100.0).round() as u32
}

#[derive(Debug, PartialEq, Eq)]
pub enum PinAction {
    /// Default already matches the pin.
    Keep,
    /// Default differs and the pinned device is available: switch back.
    Restore,
    /// Default differs but the pinned device is not connected: wait.
    Unavailable,
}

pub fn pin_action(current_default: Option<&str>, pin_id: &str, pin_available: bool) -> PinAction {
    if current_default == Some(pin_id) {
        PinAction::Keep
    } else if pin_available {
        PinAction::Restore
    } else {
        PinAction::Unavailable
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogEntry {
    pub time: String,
    pub message: String,
    /// How many times in a row this exact message happened.
    pub count: u32,
}

#[derive(Default, Debug)]
pub struct ActivityLog {
    entries: VecDeque<LogEntry>,
}

impl ActivityLog {
    /// Adds an entry; an identical message directly after the previous one is
    /// folded into it (an app fighting the lock would otherwise flood the log).
    pub fn push(&mut self, time: String, message: String) {
        if let Some(last) = self.entries.back_mut()
            && last.message == message
        {
            last.count += 1;
            last.time = time;
            return;
        }
        if self.entries.len() >= LOG_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(LogEntry {
            time,
            message,
            count: 1,
        });
    }

    /// Newest first.
    pub fn newest_first(&self) -> Vec<LogEntry> {
        self.entries.iter().rev().cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

pub fn format_time(hour: u16, minute: u16, second: u16) -> String {
    format!("{hour:02}:{minute:02}:{second:02}")
}

pub fn log_message_volume(name: &str, from: f32, to: f32) -> String {
    format!(
        "{name}: level changed to {}%, restored to {}%",
        percent(from),
        percent(to)
    )
}

pub fn log_message_default(slot: Slot, switched_to: &str, restored: &str) -> String {
    format!(
        "{} changed to {switched_to}, restored to {restored}",
        capitalise(slot.short())
    )
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Registry value for "start with Windows": quoted exe path plus the flag that
/// keeps the window hidden in the tray.
pub fn autostart_command(exe: &std::path::Path) -> String {
    format!("\"{}\" --minimized", exe.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_within_tolerance_is_unchanged() {
        assert!(!volume_differs(0.50, 0.502));
        assert!(volume_differs(0.50, 0.55));
        assert!(volume_differs(0.0, 1.0));
    }

    #[test]
    fn percent_rounds_and_clamps() {
        assert_eq!(percent(0.495), 50);
        assert_eq!(percent(1.7), 100);
        assert_eq!(percent(-1.0), 0);
    }

    #[test]
    fn pin_actions() {
        assert_eq!(pin_action(Some("a"), "a", true), PinAction::Keep);
        assert_eq!(pin_action(Some("b"), "a", true), PinAction::Restore);
        assert_eq!(pin_action(None, "a", true), PinAction::Restore);
        assert_eq!(pin_action(Some("b"), "a", false), PinAction::Unavailable);
        assert_eq!(pin_action(Some("a"), "a", false), PinAction::Keep);
    }

    #[test]
    fn log_folds_repeats() {
        let mut log = ActivityLog::default();
        log.push("10:00:00".into(), "x".into());
        log.push("10:00:01".into(), "x".into());
        log.push("10:00:02".into(), "y".into());
        let e = log.newest_first();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].message, "y");
        assert_eq!(e[1].count, 2);
        assert_eq!(e[1].time, "10:00:01");
    }

    #[test]
    fn log_is_capped() {
        let mut log = ActivityLog::default();
        for i in 0..(LOG_CAPACITY + 50) {
            log.push("t".into(), format!("m{i}"));
        }
        assert_eq!(log.len(), LOG_CAPACITY);
        assert_eq!(log.newest_first()[0].message, format!("m{}", LOG_CAPACITY + 49));
        log.clear();
        assert_eq!(log.len(), 0);
    }

    #[test]
    fn pins_get_set_roundtrip() {
        let mut pins = Pins::default();
        for slot in Slot::ALL {
            assert!(pins.get(slot).is_none());
            pins.set(
                slot,
                Some(Pin {
                    id: format!("id{}", slot.index()),
                    name: "n".into(),
                }),
            );
            assert_eq!(pins.get(slot).unwrap().id, format!("id{}", slot.index()));
        }
        pins.set(Slot::Record, None);
        assert!(pins.get(Slot::Record).is_none());
    }

    #[test]
    fn slots_map_to_flow_and_comms() {
        assert_eq!(Slot::PlaybackComms.flow(), Flow::Playback);
        assert_eq!(Slot::Record.flow(), Flow::Record);
        assert!(Slot::RecordComms.is_comms());
        assert!(!Slot::Playback.is_comms());
    }

    #[test]
    fn messages_read_well() {
        assert_eq!(
            log_message_volume("Mic", 0.8, 0.5),
            "Mic: level changed to 80%, restored to 50%"
        );
        assert_eq!(
            log_message_default(Slot::Playback, "HDMI", "Speakers"),
            "Default playback changed to HDMI, restored to Speakers"
        );
    }

    #[test]
    fn time_and_autostart_format() {
        assert_eq!(format_time(9, 5, 3), "09:05:03");
        assert_eq!(
            autostart_command(std::path::Path::new(r"C:\a b\x.exe")),
            r#""C:\a b\x.exe" --minimized"#
        );
    }

    #[test]
    fn settings_json_roundtrip_and_old_files() {
        let pins = Pins {
            record: Some(Pin {
                id: "i".into(),
                name: "n".into(),
            }),
            ..Default::default()
        };
        let json = serde_json::to_string(&pins).unwrap();
        assert_eq!(serde_json::from_str::<Pins>(&json).unwrap(), pins);
        assert_eq!(serde_json::from_str::<Pins>("{}").unwrap(), Pins::default());
    }
}
