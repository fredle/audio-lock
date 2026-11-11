//! Core Audio engine: a worker thread that owns all COM objects, watches
//! endpoint volume and default-device changes via callbacks, and reverts them.
//! The UI talks to it through `Engine` (shared state + a wake channel).

// COM method names keep their Windows spelling.
#![allow(non_snake_case)]

use crate::logic::{self, ActivityLog, Flow, LockedMic, Pins, Slot};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::PROPERTYKEY;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_ACTIVE, EDataFlow, ERole, IMMDevice,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    eCapture, eCommunications, eConsole, eMultimedia, eRender,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToStringAlloc;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree, STGM_READ,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::core::{GUID, HRESULT, PCWSTR, implement, interface};
/// Fallback re-check interval; callbacks normally wake the worker sooner.
const POLL_INTERVAL: Duration = Duration::from_secs(3);

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub flow: Flow,
    /// Master volume (capture devices only).
    pub volume: Option<f32>,
}

#[derive(Default, Clone)]
pub struct Config {
    pub locks: Vec<LockedMic>,
    pub pins: Pins,
    pub paused: bool,
}

#[derive(Default)]
pub struct Snapshot {
    /// False until the first scan finished.
    pub ready: bool,
    pub error: Option<String>,
    pub devices: Vec<DeviceInfo>,
    /// Current default device id per `Slot::index()`.
    pub defaults: [Option<String>; 4],
}

#[derive(Default)]
pub struct Shared {
    pub config: Config,
    pub snapshot: Snapshot,
    pub log: ActivityLog,
}

enum Event {
    Wake,
}

/// Handle used by the UI thread.
pub struct Engine {
    shared: Arc<Mutex<Shared>>,
    tx: Sender<Event>,
}

impl Engine {
    pub fn start(config: Config, ctx: eframe::egui::Context) -> Self {
        let shared = Arc::new(Mutex::new(Shared {
            config,
            ..Default::default()
        }));
        let (tx, rx) = mpsc::channel();
        let worker_shared = shared.clone();
        let worker_tx = tx.clone();
        std::thread::Builder::new()
            .name("audio-engine".into())
            .spawn(move || {
                if let Err(e) = run_worker(&worker_shared, worker_tx, rx, &ctx) {
                    let mut s = lock(&worker_shared);
                    s.snapshot.error = Some(format!("Audio engine stopped: {e}"));
                    s.snapshot.ready = true;
                    drop(s);
                    ctx.request_repaint();
                }
            })
            .expect("spawn audio thread");
        Self { shared, tx }
    }

    pub fn state(&self) -> MutexGuard<'_, Shared> {
        lock(&self.shared)
    }

    /// Replace locks/pins and apply them straight away.
    pub fn set_rules(&self, locks: Vec<LockedMic>, pins: Pins) {
        {
            let mut s = self.state();
            s.config.locks = locks;
            s.config.pins = pins;
        }
        let _ = self.tx.send(Event::Wake);
    }

    pub fn set_paused(&self, paused: bool) {
        {
            let mut s = self.state();
            if s.config.paused == paused {
                return;
            }
            s.config.paused = paused;
            let msg = if paused {
                "Locking paused"
            } else {
                "Locking resumed"
            };
            s.log.push(now_string(), msg.to_string());
        }
        let _ = self.tx.send(Event::Wake);
    }

    pub fn paused(&self) -> bool {
        self.state().config.paused
    }
}

fn lock(m: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn now_string() -> String {
    let t = unsafe { GetLocalTime() };
    logic::format_time(t.wHour, t.wMinute, t.wSecond)
}

// ---------------------------------------------------------------- callbacks

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback(Sender<Event>);

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, _data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        let _ = self.0.send(Event::Wake);
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceCallback(Sender<Event>);

impl IMMNotificationClient_Impl for DeviceCallback_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> windows::core::Result<()> {
        let _ = self.0.send(Event::Wake);
        Ok(())
    }
    fn OnDeviceAdded(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.0.send(Event::Wake);
        Ok(())
    }
    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> windows::core::Result<()> {
        let _ = self.0.send(Event::Wake);
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        _flow: EDataFlow,
        _role: ERole,
        _id: &PCWSTR,
    ) -> windows::core::Result<()> {
        let _ = self.0.send(Event::Wake);
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        _id: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        // Fires constantly (including on volume changes); not needed.
        Ok(())
    }
}

// ------------------------------------------------- undocumented IPolicyConfig
// The only way to change the default device. Vtable order must match exactly.

#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: windows::core::IUnknown {
    fn GetMixFormat(&self, device: PCWSTR, format: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, device: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, device: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, device: PCWSTR, a: *mut c_void, b: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(
        &self,
        device: PCWSTR,
        default: i32,
        a: *mut i64,
        b: *mut i64,
    ) -> HRESULT;
    fn SetProcessingPeriod(&self, device: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, device: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, device: PCWSTR, fx: i32, key: *const PROPERTYKEY, v: *mut c_void)
    -> HRESULT;
    fn SetPropertyValue(&self, device: PCWSTR, fx: i32, key: *const PROPERTYKEY, v: *mut c_void)
    -> HRESULT;
    fn SetDefaultEndpoint(&self, device: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, device: PCWSTR, visible: i32) -> HRESULT;
}

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

fn set_default_endpoint(id: &str, roles: &[ERole]) -> windows::core::Result<()> {
    let policy: IPolicyConfig =
        unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)? };
    let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
    for role in roles {
        unsafe { policy.SetDefaultEndpoint(PCWSTR(wide.as_ptr()), *role).ok()? };
    }
    Ok(())
}

// ------------------------------------------------------------------- worker

struct ActiveDevice {
    info: DeviceInfo,
    device: IMMDevice,
}

/// A registered volume callback for one locked microphone.
struct Registration {
    endpoint: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            let _ = self.endpoint.UnregisterControlChangeNotify(&self.callback);
        }
    }
}

fn pwstr_to_string(p: windows::core::PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(p.0 as *const c_void)) };
    s
}

fn device_id(device: &IMMDevice) -> Option<String> {
    unsafe { device.GetId() }.ok().map(pwstr_to_string)
}

fn device_name(device: &IMMDevice) -> String {
    let name = (|| -> windows::core::Result<String> {
        let store = unsafe { device.OpenPropertyStore(STGM_READ)? };
        let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName)? };
        let p = unsafe { PropVariantToStringAlloc(&value)? };
        Ok(pwstr_to_string(p))
    })();
    name.ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "Unknown device".into())
}

fn read_volume(device: &IMMDevice) -> Option<f32> {
    let ep: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None) }.ok()?;
    unsafe { ep.GetMasterVolumeLevelScalar() }.ok()
}

fn enumerate(enumerator: &IMMDeviceEnumerator) -> Vec<ActiveDevice> {
    let mut out = Vec::new();
    for (flow, data_flow) in [(Flow::Playback, eRender), (Flow::Record, eCapture)] {
        let Ok(coll) = (unsafe { enumerator.EnumAudioEndpoints(data_flow, DEVICE_STATE_ACTIVE) })
        else {
            continue;
        };
        let count = unsafe { coll.GetCount() }.unwrap_or(0);
        for i in 0..count {
            let Ok(device) = (unsafe { coll.Item(i) }) else {
                continue;
            };
            let Some(id) = device_id(&device) else {
                continue;
            };
            let volume = if flow == Flow::Record {
                read_volume(&device)
            } else {
                None
            };
            out.push(ActiveDevice {
                info: DeviceInfo {
                    id,
                    name: device_name(&device),
                    flow,
                    volume,
                },
                device,
            });
        }
    }
    out
}

fn default_id(enumerator: &IMMDeviceEnumerator, slot: Slot) -> Option<String> {
    let flow = match slot.flow() {
        Flow::Playback => eRender,
        Flow::Record => eCapture,
    };
    let role = if slot.is_comms() {
        eCommunications
    } else {
        eConsole
    };
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(flow, role) }.ok()?;
    device_id(&device)
}

fn run_worker(
    shared: &Arc<Mutex<Shared>>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    ctx: &eframe::egui::Context,
) -> Result<(), String> {
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|e| e.to_string())?;
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|e| e.to_string())?;
    let notifier: IMMNotificationClient = DeviceCallback(tx.clone()).into();
    unsafe { enumerator.RegisterEndpointNotificationCallback(&notifier) }
        .map_err(|e| e.to_string())?;

    let mut regs: HashMap<String, Registration> = HashMap::new();
    // Locked mics seen connected on the previous pass (to log disconnect/return).
    let mut was_present: HashMap<String, bool> = HashMap::new();

    loop {
        pass(shared, &tx, &enumerator, &mut regs, &mut was_present);
        ctx.request_repaint();
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(Event::Wake) | Err(RecvTimeoutError::Timeout) => {
                // Coalesce bursts (a dock connecting fires many events).
                while rx.try_recv().is_ok() {}
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    unsafe {
        let _ = enumerator.UnregisterEndpointNotificationCallback(&notifier);
    }
    Ok(())
}

/// One scan-and-enforce pass.
fn pass(
    shared: &Arc<Mutex<Shared>>,
    tx: &Sender<Event>,
    enumerator: &IMMDeviceEnumerator,
    regs: &mut HashMap<String, Registration>,
    was_present: &mut HashMap<String, bool>,
) {
    let devices = enumerate(enumerator);
    let config = lock(shared).config.clone();
    let name_of = |id: &str| -> String {
        devices
            .iter()
            .find(|d| d.info.id == id)
            .map(|d| d.info.name.clone())
            .unwrap_or_else(|| "another device".into())
    };
    let mut events: Vec<String> = Vec::new();

    // Microphone locks.
    let locked_ids: HashSet<&str> = config.locks.iter().map(|l| l.id.as_str()).collect();
    regs.retain(|id, _| locked_ids.contains(id.as_str()));
    was_present.retain(|id, _| locked_ids.contains(id.as_str()));
    let mut refreshed_volumes: HashMap<String, f32> = HashMap::new();
    for l in &config.locks {
        let active = devices.iter().find(|d| d.info.id == l.id);
        let before = was_present.insert(l.id.clone(), active.is_some());
        let Some(active) = active else {
            regs.remove(&l.id);
            if before == Some(true) {
                events.push(format!("{} disconnected; will lock again when it returns", l.name));
            }
            continue;
        };
        if before == Some(false) {
            events.push(format!("{} reconnected", l.name));
        }
        if !regs.contains_key(&l.id) {
            let reg = (|| -> windows::core::Result<Registration> {
                let endpoint: IAudioEndpointVolume =
                    unsafe { active.device.Activate(CLSCTX_ALL, None)? };
                let callback: IAudioEndpointVolumeCallback = VolumeCallback(tx.clone()).into();
                unsafe { endpoint.RegisterControlChangeNotify(&callback)? };
                Ok(Registration { endpoint, callback })
            })();
            match reg {
                Ok(r) => {
                    regs.insert(l.id.clone(), r);
                }
                Err(e) => events.push(format!("{}: cannot watch level ({e})", l.name)),
            }
        }
        if config.paused {
            continue;
        }
        if let Some(reg) = regs.get(&l.id) {
            let current = unsafe { reg.endpoint.GetMasterVolumeLevelScalar() }.unwrap_or(l.level);
            if logic::volume_differs(current, l.level) {
                match unsafe { reg.endpoint.SetMasterVolumeLevelScalar(l.level, std::ptr::null()) } {
                    Ok(()) => {
                        events.push(logic::log_message_volume(&l.name, current, l.level));
                        refreshed_volumes.insert(l.id.clone(), l.level);
                    }
                    Err(e) => events.push(format!("{}: could not restore level ({e})", l.name)),
                }
            }
        }
    }

    // Default device pins.
    let mut defaults: [Option<String>; 4] = Default::default();
    for slot in Slot::ALL {
        let mut current = default_id(enumerator, slot);
        if let Some(pin) = config.pins.get(slot)
            && !config.paused
        {
            let available = devices.iter().any(|d| d.info.id == pin.id);
            if logic::pin_action(current.as_deref(), &pin.id, available) == logic::PinAction::Restore
            {
                let roles: &[ERole] = if slot.is_comms() {
                    &[eCommunications]
                } else {
                    &[eConsole, eMultimedia]
                };
                let switched_to = current.as_deref().map(&name_of).unwrap_or_else(|| "no device".into());
                match set_default_endpoint(&pin.id, roles) {
                    Ok(()) => {
                        events.push(logic::log_message_default(slot, &switched_to, &pin.name));
                        current = Some(pin.id.clone());
                    }
                    Err(e) => events.push(format!(
                        "Could not restore {} to {} ({e})",
                        slot.short(),
                        pin.name
                    )),
                }
            }
        }
        defaults[slot.index()] = current;
    }

    let mut infos: Vec<DeviceInfo> = devices.into_iter().map(|d| d.info).collect();
    for d in &mut infos {
        if let Some(v) = refreshed_volumes.get(&d.id) {
            d.volume = Some(*v);
        }
    }
    let mut s = lock(shared);
    for e in events {
        s.log.push(now_string(), e);
    }
    s.snapshot = Snapshot {
        ready: true,
        error: None,
        devices: infos,
        defaults,
    };
}
