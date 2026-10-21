use ashpd::desktop::{
    remote_desktop::{Axis, DeviceType, KeyState, RemoteDesktop},
    screencast::{CursorMode, Screencast, SourceType, Stream},
    screenshot::Screenshot,
    PersistMode, Session,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;

const MAX_TEXT_CHARS: usize = 4096;

#[derive(Debug)]
struct PortalSession {
    remote: RemoteDesktop<'static>,
    session: Session<'static, RemoteDesktop<'static>>,
    streams: Vec<Stream>,
}

#[derive(Debug, Default)]
struct Inner {
    portal: Option<PortalSession>,
}

pub struct ComputerController {
    enabled: AtomicBool,
    generation: AtomicU64,
    inner: tokio::sync::Mutex<Inner>,
    audit: Mutex<Vec<AuditEntry>>,
}

impl Default for ComputerController {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            inner: tokio::sync::Mutex::new(Inner::default()),
            audit: Mutex::new(Vec::new()),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ComputerStatus {
    enabled: bool,
    backend: &'static str,
    authorized: bool,
    display_count: usize,
    window_control: &'static str,
    screenshot_requires_consent: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    timestamp_ms: u128,
    operation: String,
    arguments: Value,
    risk: &'static str,
    approved: bool,
    outcome: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum ComputerRequest {
    GetDisplays,
    GetActiveWindow,
    ListWindows,
    Screenshot,
    OpenApp {
        application: String,
        #[serde(default)]
        arguments: Vec<String>,
    },
    FocusWindow {
        window_id: String,
    },
    MoveMouse {
        x: i32,
        y: i32,
    },
    Click {
        button: MouseButton,
        x: Option<i32>,
        y: Option<i32>,
    },
    DoubleClick {
        button: MouseButton,
        x: Option<i32>,
        y: Option<i32>,
    },
    Scroll {
        delta_x: i32,
        delta_y: i32,
    },
    TypeText {
        text: String,
    },
    KeyPress {
        key: String,
    },
    Hotkey {
        keys: Vec<String>,
    },
    CloseWindow {
        window_id: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

impl ComputerRequest {
    fn name(&self) -> &'static str {
        match self {
            Self::GetDisplays => "computer.get_displays",
            Self::GetActiveWindow => "computer.get_active_window",
            Self::ListWindows => "computer.list_windows",
            Self::Screenshot => "computer.screenshot",
            Self::OpenApp { .. } => "computer.open_app",
            Self::FocusWindow { .. } => "computer.focus_window",
            Self::MoveMouse { .. } => "computer.move_mouse",
            Self::Click { .. } => "computer.click",
            Self::DoubleClick { .. } => "computer.double_click",
            Self::Scroll { .. } => "computer.scroll",
            Self::TypeText { .. } => "computer.type_text",
            Self::KeyPress { .. } => "computer.key_press",
            Self::Hotkey { .. } => "computer.hotkey",
            Self::CloseWindow { .. } => "computer.close_window",
        }
    }
    fn risk(&self) -> &'static str {
        match self {
            Self::GetDisplays | Self::GetActiveWindow | Self::ListWindows | Self::Screenshot => {
                "SAFE"
            }
            Self::CloseWindow { .. } => "DANGEROUS",
            Self::Hotkey { keys } if dangerous_hotkey(keys) => "DANGEROUS",
            Self::TypeText { text } if sensitive_text(text) => "DANGEROUS",
            _ => "MODERATE",
        }
    }
    fn redacted(&self) -> Value {
        match self {
            Self::TypeText { text } => {
                json!({"operation":"type_text","text":"[REDACTED]","characters":text.chars().count()})
            }
            Self::OpenApp {
                application,
                arguments,
            } => json!({"operation":"open_app","application":application,"arguments":arguments}),
            Self::FocusWindow { window_id } => {
                json!({"operation":"focus_window","windowId":window_id})
            }
            Self::MoveMouse { x, y } => json!({"operation":"move_mouse","x":x,"y":y}),
            Self::Click { button, x, y } => {
                json!({"operation":"click","button":button,"x":x,"y":y})
            }
            Self::DoubleClick { button, x, y } => {
                json!({"operation":"double_click","button":button,"x":x,"y":y})
            }
            Self::Scroll { delta_x, delta_y } => {
                json!({"operation":"scroll","deltaX":delta_x,"deltaY":delta_y})
            }
            Self::KeyPress { key } => json!({"operation":"key_press","key":key}),
            Self::Hotkey { keys } => json!({"operation":"hotkey","keys":keys}),
            Self::CloseWindow { window_id } => {
                json!({"operation":"close_window","windowId":window_id})
            }
            _ => json!({"operation": self.name().trim_start_matches("computer.")}),
        }
    }
}

impl Serialize for ComputerRequest {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.redacted().serialize(serializer)
    }
}

impl ComputerController {
    pub fn stop(&self) {
        self.enabled.store(false, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
    }
    async fn close_session(&self) {
        let old = self.inner.lock().await.portal.take();
        if let Some(portal) = old {
            let _ = portal.session.close().await;
        }
    }
    fn checkpoint(&self, generation: u64) -> Result<(), String> {
        if !self.enabled.load(Ordering::SeqCst)
            || self.generation.load(Ordering::SeqCst) != generation
        {
            Err("Computer Control stopped".into())
        } else {
            Ok(())
        }
    }
    fn log(&self, request: &ComputerRequest, approved: bool, outcome: String) {
        let entry = AuditEntry {
            timestamp_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
            operation: request.name().into(),
            arguments: request.redacted(),
            risk: request.risk(),
            approved,
            outcome,
        };
        let mut audit = self.audit.lock().expect("computer audit lock poisoned");
        audit.push(entry);
        if audit.len() > 500 {
            audit.remove(0);
        }
    }
}

#[tauri::command]
pub async fn computer_status(
    controller: State<'_, ComputerController>,
) -> Result<ComputerStatus, String> {
    let inner = controller.inner.lock().await;
    Ok(ComputerStatus {
        enabled: controller.enabled.load(Ordering::SeqCst),
        backend: if cfg!(target_os = "linux") {
            "xdg-desktop-portal"
        } else {
            "unsupported"
        },
        authorized: inner.portal.is_some(),
        display_count: inner.portal.as_ref().map_or(0, |p| p.streams.len()),
        window_control: "limited-on-wayland",
        screenshot_requires_consent: true,
    })
}

#[tauri::command]
pub async fn computer_enable(
    controller: State<'_, ComputerController>,
) -> Result<ComputerStatus, String> {
    controller.stop();
    controller.close_session().await;
    if std::env::var("XDG_SESSION_TYPE").unwrap_or_default() != "wayland" {
        return Err("Only the consent-based Wayland portal backend is implemented".into());
    }
    let remote = RemoteDesktop::new()
        .await
        .map_err(|e| format!("RemoteDesktop portal unavailable: {e}"))?;
    let screencast = Screencast::new()
        .await
        .map_err(|e| format!("ScreenCast portal unavailable: {e}"))?;
    let session = remote
        .create_session()
        .await
        .map_err(|e| format!("Cannot create portal session: {e}"))?;
    remote
        .select_devices(
            &session,
            DeviceType::Keyboard | DeviceType::Pointer,
            None,
            PersistMode::DoNot,
        )
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;
    screencast
        .select_sources(
            &session,
            CursorMode::Embedded,
            SourceType::Monitor.into(),
            true,
            None,
            PersistMode::DoNot,
        )
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| e.to_string())?;
    let selected = remote
        .start(&session, None)
        .await
        .map_err(|e| e.to_string())?
        .response()
        .map_err(|e| format!("Desktop permission denied: {e}"))?;
    let streams = selected.streams().unwrap_or_default().to_vec();
    if streams.is_empty() {
        let _ = session.close().await;
        return Err("Portal returned no display stream".into());
    }
    controller.inner.lock().await.portal = Some(PortalSession {
        remote,
        session,
        streams,
    });
    controller.generation.fetch_add(1, Ordering::SeqCst);
    controller.enabled.store(true, Ordering::SeqCst);
    computer_status(controller).await
}

#[tauri::command]
pub async fn computer_disable(controller: State<'_, ComputerController>) -> Result<(), String> {
    controller.stop();
    controller.close_session().await;
    Ok(())
}
#[tauri::command]
pub async fn computer_stop(controller: State<'_, ComputerController>) -> Result<(), String> {
    computer_disable(controller).await
}
#[tauri::command]
pub fn computer_audit_log(
    controller: State<'_, ComputerController>,
) -> Result<Vec<AuditEntry>, String> {
    Ok(controller
        .audit
        .lock()
        .map_err(|_| "Audit lock poisoned")?
        .clone())
}

#[tauri::command]
pub async fn computer_execute(
    request: ComputerRequest,
    actor: String,
    approved: bool,
    controller: State<'_, ComputerController>,
) -> Result<Value, String> {
    if actor != "AGENT" {
        return Err("Only AGENT may use Computer Control".into());
    }
    if request.risk() == "DANGEROUS" && !approved {
        return Err("Dangerous computer action requires explicit approval".into());
    }
    if !controller.enabled.load(Ordering::SeqCst) {
        return Err("Computer Control is disabled; the user must enable it in the UI".into());
    }
    let generation = controller.generation.load(Ordering::SeqCst);
    let result = execute(&controller, &request, generation).await;
    controller.log(
        &request,
        approved,
        match &result {
            Ok(_) => "success".into(),
            Err(e) => format!("failure: {e}"),
        },
    );
    result
}

async fn execute(
    controller: &ComputerController,
    request: &ComputerRequest,
    generation: u64,
) -> Result<Value, String> {
    controller.checkpoint(generation)?;
    match request {
        ComputerRequest::GetDisplays => {
            let inner = controller.inner.lock().await;
            let portal = inner
                .portal
                .as_ref()
                .ok_or("Portal session is not authorized")?;
            Ok(
                json!({"backend":"xdg-desktop-portal","displays": portal.streams.iter().enumerate().map(|(index, s)| json!({"id":s.id().map(str::to_owned).unwrap_or_else(||format!("display-{index}")),"streamId":s.pipe_wire_node_id(),"position":s.position(),"size":s.size()})).collect::<Vec<_>>() }),
            )
        }
        ComputerRequest::Screenshot => {
            let response = Screenshot::request()
                .interactive(false)
                .modal(true)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .response()
                .map_err(|e| e.to_string())?;
            let uri = response.uri();
            let path = uri
                .to_file_path()
                .map_err(|_| "Screenshot portal returned a non-file URI")?;
            let (width, height) = png_dimensions(&path)?;
            Ok(
                json!({"path":path,"uri":uri.as_str(),"format":"png","width":width,"height":height,"modelVisible":false,"verification":"portal-screenshot"}),
            )
        }
        ComputerRequest::ListWindows => list_windows(),
        ComputerRequest::GetActiveWindow => {
            Err("Active-window discovery is not exposed by the standard Wayland portal".into())
        }
        ComputerRequest::OpenApp {
            application,
            arguments,
        } => open_app(application, arguments),
        ComputerRequest::FocusWindow { window_id } => focus_window(window_id),
        ComputerRequest::CloseWindow { .. } => {
            Err("Closing arbitrary windows is not exposed by the standard Wayland portal".into())
        }
        _ => execute_input(controller, request, generation).await,
    }
}

async fn execute_input(
    controller: &ComputerController,
    request: &ComputerRequest,
    generation: u64,
) -> Result<Value, String> {
    let inner = controller.inner.lock().await;
    let portal = inner
        .portal
        .as_ref()
        .ok_or("Portal session is not authorized")?;
    match request {
        ComputerRequest::MoveMouse { x, y } => {
            move_absolute(controller, portal, generation, *x, *y).await?
        }
        ComputerRequest::Click { button, x, y } | ComputerRequest::DoubleClick { button, x, y } => {
            if let (Some(x), Some(y)) = (x, y) {
                move_absolute(controller, portal, generation, *x, *y).await?;
            }
            let count = if matches!(request, ComputerRequest::DoubleClick { .. }) {
                2
            } else {
                1
            };
            for _ in 0..count {
                controller.checkpoint(generation)?;
                portal
                    .remote
                    .notify_pointer_button(&portal.session, button_code(*button), KeyState::Pressed)
                    .await
                    .map_err(|e| e.to_string())?;
                controller.checkpoint(generation)?;
                portal
                    .remote
                    .notify_pointer_button(
                        &portal.session,
                        button_code(*button),
                        KeyState::Released,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                tokio::time::sleep(std::time::Duration::from_millis(80)).await;
            }
        }
        ComputerRequest::Scroll { delta_x, delta_y } => {
            if *delta_x != 0 {
                controller.checkpoint(generation)?;
                portal
                    .remote
                    .notify_pointer_axis_discrete(&portal.session, Axis::Horizontal, *delta_x)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            if *delta_y != 0 {
                controller.checkpoint(generation)?;
                portal
                    .remote
                    .notify_pointer_axis_discrete(&portal.session, Axis::Vertical, *delta_y)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        ComputerRequest::TypeText { text } => {
            validate_text(text)?;
            for ch in text.chars() {
                send_key(controller, portal, generation, char_keysym(ch)).await?;
            }
        }
        ComputerRequest::KeyPress { key } => {
            send_key(controller, portal, generation, key_keysym(key)?).await?
        }
        ComputerRequest::Hotkey { keys } => {
            if keys.len() < 2 || keys.len() > 5 {
                return Err("Hotkey requires 2 to 5 keys".into());
            }
            let symbols = keys
                .iter()
                .map(|key| key_keysym(key))
                .collect::<Result<Vec<_>, _>>()?;
            for sym in &symbols {
                controller.checkpoint(generation)?;
                portal
                    .remote
                    .notify_keyboard_keysym(&portal.session, *sym, KeyState::Pressed)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            for sym in symbols.iter().rev() {
                controller.checkpoint(generation)?;
                portal
                    .remote
                    .notify_keyboard_keysym(&portal.session, *sym, KeyState::Released)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        _ => return Err("Not an input operation".into()),
    }
    Ok(json!({"executed":true,"verificationRequired":true}))
}

async fn move_absolute(
    controller: &ComputerController,
    portal: &PortalSession,
    generation: u64,
    x: i32,
    y: i32,
) -> Result<(), String> {
    let stream = portal
        .streams
        .iter()
        .find(|s| {
            let (px, py) = s.position().unwrap_or((0, 0));
            let (w, h) = s.size().unwrap_or((0, 0));
            x >= px && y >= py && x < px + w && y < py + h
        })
        .ok_or("Coordinates are outside authorized displays")?;
    let (px, py) = stream.position().unwrap_or((0, 0));
    controller.checkpoint(generation)?;
    portal
        .remote
        .notify_pointer_motion_absolute(
            &portal.session,
            stream.pipe_wire_node_id(),
            f64::from(x - px),
            f64::from(y - py),
        )
        .await
        .map_err(|e| e.to_string())
}
async fn send_key(
    controller: &ComputerController,
    portal: &PortalSession,
    generation: u64,
    sym: i32,
) -> Result<(), String> {
    controller.checkpoint(generation)?;
    portal
        .remote
        .notify_keyboard_keysym(&portal.session, sym, KeyState::Pressed)
        .await
        .map_err(|e| e.to_string())?;
    controller.checkpoint(generation)?;
    portal
        .remote
        .notify_keyboard_keysym(&portal.session, sym, KeyState::Released)
        .await
        .map_err(|e| e.to_string())
}
fn button_code(button: MouseButton) -> i32 {
    match button {
        MouseButton::Left => 272,
        MouseButton::Right => 273,
        MouseButton::Middle => 274,
    }
}
fn validate_text(text: &str) -> Result<(), String> {
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err("Text exceeds 4096 characters".into());
    }
    if text
        .chars()
        .any(|c| c == '\0' || (c.is_control() && c != '\n' && c != '\t'))
    {
        return Err("Text contains unsupported control characters".into());
    }
    Ok(())
}
fn char_keysym(ch: char) -> i32 {
    let value = ch as u32;
    if value <= 0xff {
        value as i32
    } else {
        (0x0100_0000 | value) as i32
    }
}
fn key_keysym(key: &str) -> Result<i32, String> {
    let lower = key.to_ascii_lowercase();
    let sym = match lower.as_str() {
        "enter" => 0xff0d,
        "escape" | "esc" => 0xff1b,
        "tab" => 0xff09,
        "backspace" => 0xff08,
        "space" => 0x20,
        "left" => 0xff51,
        "up" => 0xff52,
        "right" => 0xff53,
        "down" => 0xff54,
        "delete" => 0xffff,
        "home" => 0xff50,
        "end" => 0xff57,
        "pageup" => 0xff55,
        "pagedown" => 0xff56,
        "ctrl" | "control" => 0xffe3,
        "shift" => 0xffe1,
        "alt" => 0xffe9,
        "meta" | "super" => 0xffeb,
        _ if lower.len() == 1 => char_keysym(lower.chars().next().unwrap()),
        _ if lower.starts_with('f') => {
            let number = lower[1..].parse::<i32>().map_err(|_| "Unsupported key")?;
            if !(1..=12).contains(&number) {
                return Err("Unsupported key".into());
            }
            0xffbd + number
        }
        _ => return Err(format!("Unsupported key: {key}")),
    };
    Ok(sym)
}
fn dangerous_hotkey(keys: &[String]) -> bool {
    let values = keys
        .iter()
        .map(|k| k.to_ascii_lowercase())
        .collect::<Vec<_>>();
    (values.contains(&"alt".into()) && values.contains(&"f4".into()))
        || ((values.contains(&"ctrl".into()) || values.contains(&"control".into()))
            && (values.contains(&"q".into())
                || values.contains(&"w".into())
                || values.contains(&"delete".into())))
}
fn sensitive_text(text: &str) -> bool {
    let value = text.to_ascii_lowercase();
    ["password", "passwd", "api_key", "secret", "token", "sudo"]
        .iter()
        .any(|needle| value.contains(needle))
}
fn valid_app(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._+-".contains(c))
}
fn resolve_executable(app: &str) -> Option<std::path::PathBuf> {
    if !valid_app(app) {
        return None;
    }
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(|dir| Path::new(dir).join(app))
        .find(|path| path.is_file())
}
fn open_app(application: &str, arguments: &[String]) -> Result<Value, String> {
    if arguments.len() > 32 || arguments.iter().any(|a| a.len() > 1024 || a.contains('\0')) {
        return Err("Invalid application arguments".into());
    }
    let executable =
        resolve_executable(application).ok_or("Application executable was not found in PATH")?;
    let child = Command::new(&executable)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(
        json!({"launched":true,"pid":child.id(),"executable":executable,"verificationRequired":true}),
    )
}
fn qdbus(args: &[&str]) -> Result<String, String> {
    let output = Command::new("qdbus")
        .args(args)
        .output()
        .map_err(|e| format!("qdbus unavailable: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
fn png_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Cannot read portal screenshot: {e}"))?;
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return Err("Portal screenshot is not a valid PNG".into());
    }
    Ok((
        u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
        u32::from_be_bytes(bytes[20..24].try_into().unwrap()),
    ))
}
fn quoted_fields(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for ch in text.chars() {
        if escaped {
            value.push(ch);
            escaped = false;
            continue;
        }
        if quoted && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' {
            if quoted {
                out.push(std::mem::take(&mut value));
            }
            quoted = !quoted;
            continue;
        }
        if quoted {
            value.push(ch)
        }
    }
    out
}
fn list_windows() -> Result<Value, String> {
    let output = qdbus(&[
        "--literal",
        "org.kde.KWin",
        "/WindowsRunner",
        "org.kde.krunner1.Match",
        "",
    ])?;
    let fields = quoted_fields(&output);
    let windows = fields
        .iter()
        .enumerate()
        .filter_map(|(index, id)| {
            if id.starts_with("0_") {
                fields
                    .get(index + 1)
                    .map(|title| json!({"id":id,"title":title}))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    Ok(json!({"windows":windows,"backend":"kwin-windows-runner","limited":true}))
}
fn focus_window(window_id: &str) -> Result<Value, String> {
    if !window_id.starts_with("0_")
        || window_id.len() > 128
        || !window_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
    {
        return Err("Invalid KWin window id".into());
    }
    qdbus(&[
        "org.kde.KWin",
        "/WindowsRunner",
        "org.kde.krunner1.Run",
        window_id,
        "",
    ])?;
    Ok(json!({"focused":true,"windowId":window_id,"verificationRequired":true}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_app_and_redaction() {
        assert!(valid_app("kate"));
        assert!(!valid_app("kate; rm"));
        let req = ComputerRequest::TypeText {
            text: "secret".into(),
        };
        assert_eq!(req.redacted()["text"], "[REDACTED]");
    }
    #[test]
    fn classifies_sensitive_actions() {
        assert_eq!(
            ComputerRequest::CloseWindow {
                window_id: "x".into()
            }
            .risk(),
            "DANGEROUS"
        );
        assert!(dangerous_hotkey(&["Alt".into(), "F4".into()]));
        assert_eq!(ComputerRequest::GetDisplays.risk(), "SAFE");
    }
    #[test]
    fn rejects_bad_text_and_keys() {
        assert!(validate_text("a\0b").is_err());
        assert!(key_keysym("F12").is_ok());
        assert!(key_keysym("F13").is_err());
    }
}

#[cfg(all(test, target_os = "linux"))]
mod real_tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    #[ignore = "requires a real KDE Wayland session and visible portal consent"]
    async fn real_wayland_portal_input_screenshot_and_stop() {
        assert_eq!(std::env::var("XDG_SESSION_TYPE").as_deref(), Ok("wayland"));
        let remote = RemoteDesktop::new().await.expect("RemoteDesktop portal");
        let screencast = Screencast::new().await.expect("ScreenCast portal");
        let session = remote.create_session().await.expect("create session");
        remote
            .select_devices(
                &session,
                DeviceType::Keyboard | DeviceType::Pointer,
                None,
                PersistMode::DoNot,
            )
            .await
            .expect("select devices")
            .response()
            .expect("select response");
        screencast
            .select_sources(
                &session,
                CursorMode::Embedded,
                SourceType::Monitor.into(),
                true,
                None,
                PersistMode::DoNot,
            )
            .await
            .expect("select monitor")
            .response()
            .expect("monitor response");
        let response = remote
            .start(&session, None)
            .await
            .expect("start request")
            .response()
            .expect("user portal consent");
        let stream = response
            .streams()
            .and_then(|streams| streams.first())
            .expect("authorized display stream");
        let (width, height) = stream.size().expect("display size");
        let mut child = Command::new("kate")
            .arg("--new")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("launch kate without shell");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        remote
            .notify_pointer_motion_absolute(
                &session,
                stream.pipe_wire_node_id(),
                f64::from(width / 2),
                f64::from(height / 2),
            )
            .await
            .expect("move pointer");
        remote
            .notify_pointer_button(&session, 272, KeyState::Pressed)
            .await
            .expect("click down");
        remote
            .notify_pointer_button(&session, 272, KeyState::Released)
            .await
            .expect("click up");
        for ch in "GravityForge portal smoke test".chars() {
            let sym = char_keysym(ch);
            remote
                .notify_keyboard_keysym(&session, sym, KeyState::Pressed)
                .await
                .expect("key down");
            remote
                .notify_keyboard_keysym(&session, sym, KeyState::Released)
                .await
                .expect("key up");
        }
        let shot = Screenshot::request()
            .interactive(false)
            .modal(true)
            .send()
            .await
            .expect("screenshot request")
            .response();
        session
            .close()
            .await
            .expect("immediate stop closes session");
        let _ = child.kill();
        let _ = child.wait();
        let shot = shot.expect("screenshot consent");
        let path = shot.uri().to_file_path().expect("screenshot path");
        let dimensions = png_dimensions(&path).expect("valid PNG dimensions");
        assert!(path.is_file());
        assert!(dimensions.0 > 0 && dimensions.1 > 0);
        assert!(child.id() > 0);
    }
}
