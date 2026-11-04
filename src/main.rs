#![windows_subsystem = "windows"]

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::Networking::WinHttp::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const COMPACT_W: i32 = 94;
const EXPANDED_W: i32 = 244;
const COMPACT_H: i32 = 50;
const TIMER_ID: usize = 1;
const WM_STATE_CHANGED: u32 = WM_APP + 17;
const WM_MOUSELEAVE_MSG: u32 = 0x02A3;
const CLASS_NAME: &str = "AITrafficLight.Native.Window";
const PORT: u16 = 17321;

const CMD_ERROR: i32 = 101;
const CMD_WAITING: i32 = 102;
const CMD_WORKING: i32 = 103;
const CMD_AUTO: i32 = 104;
const CMD_IDLE: i32 = 105;
const CMD_NOTIFY_TEST: i32 = 106;
const CMD_EXIT: i32 = 199;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Error,
    Waiting,
    Working,
    Idle,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Auto,
    Manual(Status),
}

struct ApiOverride {
    status: Status,
    label: String,
    until: Instant,
}

struct AppState {
    mode: Mode,
    auto_agents: Vec<AgentView>,
    api_agents: HashMap<String, ApiOverride>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            mode: Mode::Auto,
            auto_agents: Vec::new(),
            api_agents: HashMap::new(),
        }
    }
}

#[derive(Clone)]
struct AgentView {
    app: String,
    status: Status,
    label: String,
    detail: String,
    source: &'static str,
}

#[derive(Clone)]
struct Snapshot {
    status: Status,
    label: String,
    detail: String,
    source: &'static str,
    agents: Vec<AgentView>,
}

impl AppState {
    fn snapshot(&mut self) -> Snapshot {
        self.api_agents
            .retain(|_, value| value.until > Instant::now());
        if let Mode::Manual(status) = self.mode {
            let label = match status {
                Status::Error => "异常 · 需要处理",
                Status::Waiting => "等待你的确认",
                Status::Working => "AI 正在工作",
                Status::Idle => "空闲",
            }
            .to_string();
            let agent = AgentView {
                app: "全部 AI".into(),
                status,
                label: label.clone(),
                detail: "手动状态".into(),
                source: "manual",
            };
            return Snapshot {
                status,
                label,
                detail: "手动状态".into(),
                source: "manual",
                agents: vec![agent],
            };
        }

        let mut agents = self.auto_agents.clone();
        for (app_name, api) in &self.api_agents {
            let view = AgentView {
                app: app_name.clone(),
                status: api.status,
                label: api.label.clone(),
                detail: "由接口上报".into(),
                source: "api",
            };
            if let Some(existing) = agents
                .iter_mut()
                .find(|x| x.app.eq_ignore_ascii_case(app_name))
            {
                *existing = view;
            } else {
                agents.push(view);
            }
        }
        agents.sort_by(|a, b| {
            status_rank(b.status)
                .cmp(&status_rank(a.status))
                .then_with(|| a.app.cmp(&b.app))
        });
        if agents.is_empty() {
            return Snapshot {
                status: Status::Idle,
                label: "未检测到 AI".into(),
                detail: "Cursor · Codex · WorkBuddy".into(),
                source: "auto",
                agents,
            };
        }
        let errors = agents.iter().filter(|x| x.status == Status::Error).count();
        let waiting = agents
            .iter()
            .filter(|x| x.status == Status::Waiting)
            .count();
        let working = agents
            .iter()
            .filter(|x| x.status == Status::Working)
            .count();
        let status = agents[0].status;
        let label = if agents.len() == 1 {
            agents[0].app.clone()
        } else {
            format!("{} 个 AI 软件", agents.len())
        };
        let mut parts = Vec::new();
        if errors > 0 {
            parts.push(format!("{} 异常", errors));
        }
        if waiting > 0 {
            parts.push(format!("{} 等待", waiting));
        }
        if working > 0 {
            parts.push(format!("{} 工作", working));
        }
        let detail = if parts.is_empty() {
            "全部空闲".into()
        } else {
            parts.join(" · ")
        };
        let source = if agents.iter().all(|x| x.source == agents[0].source) {
            agents[0].source
        } else {
            "mixed"
        };
        Snapshot {
            status,
            label,
            detail,
            source,
            agents,
        }
    }
}

struct UiState {
    hwnd: HWND,
    scale: f32,
    width: f32,
    target_width: f32,
    height: f32,
    target_height: f32,
    anchor_right: i32,
    dragging: bool,
    moved: bool,
    press_x: i32,
    press_y: i32,
    window_x: i32,
    window_y: i32,
    pulse: f32,
    pulse_tick: u8,
}

unsafe impl Send for UiState {}

static APP: OnceLock<Mutex<AppState>> = OnceLock::new();
static UI: OnceLock<Mutex<UiState>> = OnceLock::new();
static NOTIFY_RUNTIME: OnceLock<Mutex<NotifyRuntime>> = OnceLock::new();

#[derive(Clone)]
struct NotifyConfig {
    enabled: bool,
    provider: String,
    url: String,
    events: String,
    cooldown_seconds: u64,
}

struct NotifyRuntime {
    initialized: bool,
    last_keys: HashMap<String, String>,
    last_sent: HashMap<String, Instant>,
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
fn x_lparam(lp: LPARAM) -> i32 {
    (lp as u16 as i16) as i32
}
fn y_lparam(lp: LPARAM) -> i32 {
    ((lp >> 16) as u16 as i16) as i32
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Working => "working",
        Status::Waiting => "waiting",
        Status::Error => "error",
        Status::Idle => "idle",
    }
}

fn status_rank(status: Status) -> u8 {
    match status {
        Status::Error => 3,
        Status::Waiting => 2,
        Status::Working => 1,
        Status::Idle => 0,
    }
}

fn status_label(status: Status) -> &'static str {
    match status {
        Status::Error => "异常",
        Status::Waiting => "等待",
        Status::Working => "工作中",
        Status::Idle => "空闲",
    }
}

fn expanded_height(agent_count: usize) -> i32 {
    COMPACT_H + 10 + (agent_count.clamp(1, 4) as i32 * 24) + 8
}

fn scaled(value: i32, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

fn snapshot_json() -> String {
    let snap = APP.get().unwrap().lock().unwrap().snapshot();
    let apps = snap
        .agents
        .iter()
        .map(|agent| {
            format!(
                r#"{{"app":"{}","status":"{}","label":"{}","detail":"{}","source":"{}"}}"#,
                json_escape(&agent.app),
                status_name(agent.status),
                json_escape(&agent.label),
                json_escape(&agent.detail),
                agent.source,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"{{"status":"{}","label":"{}","detail":"{}","source":"{}","apps":[{}]}}"#,
        status_name(snap.status),
        json_escape(&snap.label),
        json_escape(&snap.detail),
        snap.source,
        apps
    )
}

fn extract_json_string(body: &str, key: &str) -> Option<String> {
    let marker = format!("\"{}\"", key);
    let start = body.find(&marker)? + marker.len();
    let after = &body[start..];
    let colon = after.find(':')? + 1;
    let value = after[colon..].trim_start();
    if !value.starts_with('"') {
        return None;
    }
    let value = &value[1..];
    let mut escaped = false;
    for (i, ch) in value.char_indices() {
        if ch == '"' && !escaped {
            return Some(value[..i].replace("\\\"", "\"").replace("\\\\", "\\"));
        }
        escaped = ch == '\\' && !escaped;
        if ch != '\\' {
            escaped = false;
        }
    }
    None
}

fn extract_json_u64(body: &str, key: &str) -> Option<u64> {
    let marker = format!("\"{}\"", key);
    let start = body.find(&marker)? + marker.len();
    let after = &body[start..];
    let colon = after.find(':')? + 1;
    after[colon..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse()
        .ok()
}

fn notification_path() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    root.join("AITrafficLight").join("notifications.json")
}

fn load_notify_config() -> Option<NotifyConfig> {
    let text = std::fs::read_to_string(notification_path()).ok()?;
    let enabled = text
        .split("\"enabled\"")
        .nth(1)?
        .split(':')
        .nth(1)?
        .trim_start()
        .starts_with("true");
    Some(NotifyConfig {
        enabled,
        provider: extract_json_string(&text, "provider")?.to_lowercase(),
        url: extract_json_string(&text, "url")?,
        events: extract_json_string(&text, "events")
            .unwrap_or_else(|| "waiting,error".into())
            .to_lowercase(),
        cooldown_seconds: extract_json_u64(&text, "cooldown_seconds")
            .unwrap_or(60)
            .clamp(5, 86400),
    })
}

fn parse_http_url(url: &str) -> Option<(bool, String, u16, String)> {
    let (secure, rest, default_port) = if let Some(rest) = url.strip_prefix("https://") {
        (true, rest, 443)
    } else if let Some(rest) = url.strip_prefix("http://") {
        (false, rest, 80)
    } else {
        return None;
    };
    let (authority, path) = rest
        .split_once('/')
        .map(|(a, p)| (a, format!("/{}", p)))
        .unwrap_or((rest, "/".into()));
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some((host, port)) = authority.rsplit_once(':') {
        if host.contains(':') {
            return None;
        }
        (host.to_string(), port.parse().ok()?)
    } else {
        (authority.to_string(), default_port)
    };
    Some((secure, host, port, path))
}

fn http_post(url: &str, headers: &str, body: &[u8]) -> bool {
    let Some((secure, host, port, path)) = parse_http_url(url) else {
        return false;
    };
    unsafe {
        let agent = wide("AI Traffic Light/0.4");
        let session = WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        );
        if session.is_null() {
            return false;
        }
        WinHttpSetTimeouts(session, 2500, 2500, 2500, 4000);
        let host_w = wide(&host);
        let connect = WinHttpConnect(session, host_w.as_ptr(), port, 0);
        if connect.is_null() {
            WinHttpCloseHandle(session);
            return false;
        }
        let verb = wide("POST");
        let path_w = wide(&path);
        let flags = if secure { WINHTTP_FLAG_SECURE } else { 0 };
        let request = WinHttpOpenRequest(
            connect,
            verb.as_ptr(),
            path_w.as_ptr(),
            null(),
            null(),
            null(),
            flags,
        );
        if request.is_null() {
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return false;
        }
        let headers_w = wide(headers);
        let sent = WinHttpSendRequest(
            request,
            headers_w.as_ptr(),
            headers_w.len().saturating_sub(1) as u32,
            if body.is_empty() {
                null()
            } else {
                body.as_ptr() as *const c_void
            },
            body.len() as u32,
            body.len() as u32,
            0,
        ) != 0;
        let received = sent && WinHttpReceiveResponse(request, null_mut()) != 0;
        let mut status_code = 0u32;
        let mut status_len = std::mem::size_of::<u32>() as u32;
        let queried = received
            && WinHttpQueryHeaders(
                request,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                &mut status_code as *mut _ as *mut c_void,
                &mut status_len,
                null_mut(),
            ) != 0;
        WinHttpCloseHandle(request);
        WinHttpCloseHandle(connect);
        WinHttpCloseHandle(session);
        queried && (200..300).contains(&status_code)
    }
}

fn notification_title(status: Status) -> &'static str {
    match status {
        Status::Error => "AI 任务异常",
        Status::Waiting => "AI 等待你的确认",
        Status::Working => "AI 正在工作",
        Status::Idle => "AI 已空闲",
    }
}

fn send_mobile(config: &NotifyConfig, snap: &Snapshot) -> bool {
    let title = notification_title(snap.status);
    let message = format!("{} · {}", snap.label, snap.detail);
    match config.provider.as_str() {
        "ntfy" => {
            let priority = if snap.status == Status::Error {
                "high"
            } else {
                "default"
            };
            let tags = if snap.status == Status::Error {
                "warning,robot"
            } else {
                "robot"
            };
            let headers = format!("Content-Type: text/plain; charset=utf-8\r\nTitle: AI Traffic Light\r\nPriority: {}\r\nTags: {}\r\n", priority, tags);
            http_post(&config.url, &headers, message.as_bytes())
        }
        "bark" => {
            let level = if snap.status == Status::Error {
                "timeSensitive"
            } else {
                "active"
            };
            let body = format!(
                r#"{{"title":"{}","body":"{}","group":"AI Traffic Light","level":"{}"}}"#,
                json_escape(title),
                json_escape(&message),
                level
            );
            http_post(
                &config.url,
                "Content-Type: application/json; charset=utf-8\r\n",
                body.as_bytes(),
            )
        }
        "generic" => {
            let body = format!(
                r#"{{"status":"{}","title":"{}","message":"{}","source":"{}"}}"#,
                status_name(snap.status),
                json_escape(title),
                json_escape(&message),
                snap.source
            );
            http_post(
                &config.url,
                "Content-Type: application/json; charset=utf-8\r\n",
                body.as_bytes(),
            )
        }
        _ => false,
    }
}

fn maybe_notify(snap: Snapshot) {
    let Some(config) = load_notify_config() else {
        return;
    };
    let mut runtime = NOTIFY_RUNTIME.get().unwrap().lock().unwrap();
    if !runtime.initialized {
        runtime.initialized = true;
        for agent in &snap.agents {
            runtime.last_keys.insert(
                agent.app.clone(),
                format!("{}|{}", status_name(agent.status), agent.label),
            );
        }
        return;
    }
    let mut pending = Vec::new();
    for agent in &snap.agents {
        let key = format!("{}|{}", status_name(agent.status), agent.label);
        if runtime.last_keys.get(&agent.app) == Some(&key) {
            continue;
        }
        runtime.last_keys.insert(agent.app.clone(), key);
        let wanted = config
            .events
            .split(',')
            .map(str::trim)
            .any(|x| x == status_name(agent.status));
        let cooling = runtime
            .last_sent
            .get(&agent.app)
            .is_some_and(|when| when.elapsed() < Duration::from_secs(config.cooldown_seconds));
        if config.enabled && wanted && !cooling {
            runtime.last_sent.insert(agent.app.clone(), Instant::now());
            pending.push(Snapshot {
                status: agent.status,
                label: format!("{} · {}", agent.app, agent.label),
                detail: agent.detail.clone(),
                source: agent.source,
                agents: vec![agent.clone()],
            });
        }
    }
    drop(runtime);
    for item in pending {
        let config = config.clone();
        thread::spawn(move || {
            let _ = send_mobile(&config, &item);
        });
    }
}

fn test_mobile_notification() -> Result<(), &'static str> {
    let config = load_notify_config().ok_or("No mobile notification configuration was found.")?;
    if !config.enabled {
        return Err("Mobile notifications are disabled.");
    }
    let snap = Snapshot {
        status: Status::Waiting,
        label: "手机通知连接成功".into(),
        detail: "AI 红绿灯测试消息".into(),
        source: "test",
        agents: vec![AgentView {
            app: "AI 红绿灯".into(),
            status: Status::Waiting,
            label: "手机通知连接成功".into(),
            detail: "AI 红绿灯测试消息".into(),
            source: "test",
        }],
    };
    if send_mobile(&config, &snap) {
        Ok(())
    } else {
        Err("Notification delivery failed. Check the URL and network.")
    }
}

fn apply_api(body: &str) -> Result<(), ()> {
    let name = extract_json_string(body, "status")
        .ok_or(())?
        .to_lowercase();
    let label = extract_json_string(body, "label")
        .unwrap_or_default()
        .chars()
        .take(42)
        .collect::<String>();
    let supplied_app =
        extract_json_string(body, "app").map(|x| x.chars().take(24).collect::<String>());
    let inferred_app = {
        let lower = label.to_lowercase();
        [
            "Cursor",
            "Codex",
            "WorkBuddy",
            "Trae",
            "Windsurf",
            "ChatGPT",
            "VS Code",
        ]
        .into_iter()
        .find(|name| lower.contains(&name.to_lowercase()))
        .unwrap_or("AI")
        .to_string()
    };
    let app_name = supplied_app.clone().unwrap_or(inferred_app);
    let ttl = extract_json_u64(body, "ttl").unwrap_or(120).clamp(1, 86400);
    let mut app = APP.get().unwrap().lock().unwrap();
    match name.as_str() {
        "working" | "waiting" | "error" | "idle" if supplied_app.is_some() || name != "idle" => {
            let status = match name.as_str() {
                "working" => Status::Working,
                "waiting" => Status::Waiting,
                "error" => Status::Error,
                _ => Status::Idle,
            };
            app.api_agents.insert(
                app_name,
                ApiOverride {
                    status,
                    label: if label.is_empty() {
                        status_name(status).into()
                    } else {
                        label
                    },
                    until: Instant::now() + Duration::from_secs(ttl),
                },
            );
            app.mode = Mode::Auto;
        }
        "idle" => {
            app.api_agents.clear();
            app.mode = Mode::Manual(Status::Idle);
        }
        "auto" => {
            if let Some(name) = supplied_app {
                app.api_agents.remove(&name);
            } else {
                app.api_agents.clear();
            }
            app.mode = Mode::Auto;
        }
        _ => return Err(()),
    }
    drop(app);
    post_redraw();
    Ok(())
}

fn post_redraw() {
    if let Some(app) = APP.get() {
        let snap = app.lock().unwrap().snapshot();
        maybe_notify(snap);
    }
    if let Some(ui) = UI.get() {
        let hwnd = ui.lock().unwrap().hwnd;
        unsafe {
            PostMessageW(hwnd, WM_STATE_CHANGED, 0, 0);
        }
    }
}

fn start_http() {
    thread::spawn(|| {
        let Ok(listener) = TcpListener::bind(("127.0.0.1", PORT)) else {
            return;
        };
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else {
                continue;
            };
            let _ = stream.set_read_timeout(Some(Duration::from_millis(800)));
            let mut buffer = [0u8; 8192];
            let Ok(size) = stream.read(&mut buffer) else {
                continue;
            };
            let request = String::from_utf8_lossy(&buffer[..size]);
            let is_get = request.starts_with("GET /status ") || request.starts_with("GET / ");
            let is_post = request.starts_with("POST /status ");
            let is_notify_test = request.starts_with("POST /notify/test ");
            let is_options = request.starts_with("OPTIONS /status ");
            let (code, payload) = if is_options {
                ("204 No Content", String::new())
            } else if is_get {
                ("200 OK", snapshot_json())
            } else if is_notify_test {
                match test_mobile_notification() {
                    Ok(()) => ("200 OK", r#"{"ok":true}"#.into()),
                    Err(message) => (
                        "503 Service Unavailable",
                        format!(r#"{{"ok":false,"error":"{}"}}"#, json_escape(message)),
                    ),
                }
            } else if is_post {
                let body = request.split("\r\n\r\n").nth(1).unwrap_or("");
                if apply_api(body).is_ok() {
                    ("200 OK", snapshot_json())
                } else {
                    ("400 Bad Request", r#"{"error":"invalid status"}"#.into())
                }
            } else {
                ("404 Not Found", r#"{"error":"not found"}"#.into())
            };
            let response = format!("HTTP/1.1 {}\r\nContent-Type: application/json; charset=utf-8\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", code, payload.as_bytes().len(), payload);
            let _ = stream.write_all(response.as_bytes());
        }
    });
}

struct WindowMatch {
    pid: u32,
    app: &'static str,
}

unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> i32 {
    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let len = GetWindowTextLengthW(hwnd);
    if len <= 0 {
        return 1;
    }
    let mut text = vec![0u16; len as usize + 1];
    GetWindowTextW(hwnd, text.as_mut_ptr(), text.len() as i32);
    let title = String::from_utf16_lossy(&text[..len as usize]).to_lowercase();
    let known = [
        ("cursor", "Cursor"),
        ("codex", "Codex"),
        ("workbuddy", "WorkBuddy"),
        ("work buddy", "WorkBuddy"),
        ("trae", "Trae"),
        ("windsurf", "Windsurf"),
        ("chatgpt", "ChatGPT"),
        ("visual studio code", "VS Code"),
    ];
    if let Some((_, app)) = known.iter().find(|(key, _)| title.contains(key)) {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let matches = &mut *(lparam as *mut Vec<WindowMatch>);
        matches.push(WindowMatch { pid, app });
    }
    1
}

fn filetime_u64(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
}

fn process_cpu_time(pid: u32) -> Option<u64> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut creation = std::mem::zeroed();
        let mut exit = std::mem::zeroed();
        let mut kernel = std::mem::zeroed();
        let mut user = std::mem::zeroed();
        let ok = GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user);
        CloseHandle(handle);
        if ok == 0 {
            None
        } else {
            Some(filetime_u64(kernel) + filetime_u64(user))
        }
    }
}

fn start_detection() {
    thread::spawn(|| {
        let mut previous: HashMap<u32, (u64, Instant)> = HashMap::new();
        loop {
            let mut matches = Vec::<WindowMatch>::new();
            unsafe {
                EnumWindows(Some(enum_window), &mut matches as *mut _ as LPARAM);
            }
            let mut apps = HashSet::<&'static str>::new();
            let mut seen_pids = HashSet::new();
            let mut app_activity = HashMap::<&'static str, f64>::new();
            let mut current = HashMap::new();
            for item in &matches {
                apps.insert(item.app);
                if !seen_pids.insert(item.pid) {
                    continue;
                }
                if let Some(total) = process_cpu_time(item.pid) {
                    let now = Instant::now();
                    if let Some((old, when)) = previous.get(&item.pid) {
                        let seconds = now.duration_since(*when).as_secs_f64();
                        if seconds > 0.0 {
                            let activity = (total.saturating_sub(*old) as f64 / 10_000_000.0)
                                / seconds
                                * 100.0;
                            *app_activity.entry(item.app).or_default() += activity;
                        }
                    }
                    current.insert(item.pid, (total, now));
                }
            }
            previous = current;
            let mut app = APP.get().unwrap().lock().unwrap();
            app.auto_agents = apps
                .into_iter()
                .map(|name| {
                    let activity = app_activity.get(name).copied().unwrap_or(0.0);
                    let status = if activity >= 0.8 {
                        Status::Working
                    } else {
                        Status::Waiting
                    };
                    AgentView {
                        app: name.into(),
                        status,
                        label: if status == Status::Working {
                            "正在工作".into()
                        } else {
                            "等待活动".into()
                        },
                        detail: if status == Status::Working {
                            format!("进程活动 {:.1}", activity)
                        } else {
                            "已打开".into()
                        },
                        source: "auto",
                    }
                })
                .collect();
            drop(app);
            post_redraw();
            thread::sleep(Duration::from_secs(2));
        }
    });
}

fn config_path() -> PathBuf {
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    root.join("AITrafficLight").join("native-position.txt")
}

fn load_position() -> Option<(i32, i32)> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    let mut parts = text.trim().split(',');
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

fn save_position(x: i32, y: i32) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, format!("{},{}", x, y));
}

fn set_manual(status: Option<Status>) {
    let mut app = APP.get().unwrap().lock().unwrap();
    app.api_agents.clear();
    app.mode = status.map(Mode::Manual).unwrap_or(Mode::Auto);
    drop(app);
    post_redraw();
}

unsafe fn show_context_menu(hwnd: HWND) {
    let menu = CreatePopupMenu();
    AppendMenuW(
        menu,
        MF_STRING,
        CMD_WORKING as usize,
        wide("● 绿色：正在工作").as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        CMD_WAITING as usize,
        wide("● 黄色：等待确认").as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        CMD_ERROR as usize,
        wide("● 红色：异常处理").as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(
        menu,
        MF_STRING,
        CMD_AUTO as usize,
        wide("自动识别").as_ptr(),
    );
    AppendMenuW(menu, MF_STRING, CMD_IDLE as usize, wide("空闲").as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(
        menu,
        MF_STRING,
        CMD_NOTIFY_TEST as usize,
        wide("测试手机通知").as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null());
    AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, wide("退出").as_ptr());
    let mut pt = POINT { x: 0, y: 0 };
    GetCursorPos(&mut pt);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON,
        pt.x,
        pt.y,
        0,
        hwnd,
        null(),
    );
    DestroyMenu(menu);
    match command {
        CMD_WORKING => set_manual(Some(Status::Working)),
        CMD_WAITING => set_manual(Some(Status::Waiting)),
        CMD_ERROR => set_manual(Some(Status::Error)),
        CMD_IDLE => set_manual(Some(Status::Idle)),
        CMD_AUTO => set_manual(None),
        CMD_NOTIFY_TEST => {
            let hwnd_value = hwnd as usize;
            thread::spawn(move || {
                let result = test_mobile_notification();
                let (text, flags) = match result {
                    Ok(()) => ("测试通知已发送，请检查手机。", MB_OK | MB_ICONINFORMATION),
                    Err(message) => (message, MB_OK | MB_ICONERROR),
                };
                unsafe {
                    MessageBoxW(
                        hwnd_value as HWND,
                        wide(text).as_ptr(),
                        wide("AI 红绿灯").as_ptr(),
                        flags,
                    );
                }
            });
        }
        CMD_EXIT => {
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe fn draw_crisp_text(hdc: HDC, width: i32, height: i32, scale: f32, snap: &Snapshot) {
    let p = |value: i32| scaled(value, scale);
    SetBkMode(hdc, TRANSPARENT as i32);
    let font_name = wide("Microsoft YaHei UI");
    let label_font = CreateFontW(
        -p(12),
        0,
        0,
        0,
        FW_SEMIBOLD as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_NATURAL_QUALITY,
        DEFAULT_PITCH as u32,
        font_name.as_ptr(),
    );
    let detail_font = CreateFontW(
        -p(9),
        0,
        0,
        0,
        FW_NORMAL as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_NATURAL_QUALITY,
        DEFAULT_PITCH as u32,
        font_name.as_ptr(),
    );
    let badge_font = CreateFontW(
        -p(7),
        0,
        0,
        0,
        FW_SEMIBOLD as i32,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        CLEARTYPE_NATURAL_QUALITY,
        DEFAULT_PITCH as u32,
        font_name.as_ptr(),
    );
    let previous_font = SelectObject(hdc, detail_font);

    if snap.agents.len() > 1 {
        SelectObject(hdc, badge_font);
        SetTextColor(hdc, rgb(255, 255, 255));
        let count = wide(&snap.agents.len().min(9).to_string());
        let mut rect = RECT {
            left: p(79),
            top: p(3),
            right: p(92),
            bottom: p(16),
        };
        DrawTextW(
            hdc,
            count.as_ptr(),
            -1,
            &mut rect,
            DT_SINGLELINE | DT_CENTER | DT_VCENTER,
        );
    }

    if width > p(155) {
        SelectObject(hdc, label_font);
        SetTextColor(hdc, rgb(245, 247, 250));
        let label = wide(&snap.label);
        let mut rect = RECT {
            left: p(96),
            top: p(7),
            right: width - p(8),
            bottom: p(28),
        };
        DrawTextW(
            hdc,
            label.as_ptr(),
            -1,
            &mut rect,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );
        SelectObject(hdc, detail_font);
        SetTextColor(hdc, rgb(143, 155, 173));
        let detail = wide(&snap.detail);
        rect = RECT {
            left: p(96),
            top: p(27),
            right: width - p(8),
            bottom: p(45),
        };
        DrawTextW(
            hdc,
            detail.as_ptr(),
            -1,
            &mut rect,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
        );

        if height > p(COMPACT_H + 8) {
            for (index, agent) in snap.agents.iter().take(4).enumerate() {
                let top = 58 + index as i32 * 24;
                let color = match agent.status {
                    Status::Error => rgb(255, 77, 95),
                    Status::Waiting => rgb(255, 189, 46),
                    Status::Working => rgb(40, 209, 124),
                    Status::Idle => rgb(102, 112, 128),
                };
                SetTextColor(hdc, rgb(229, 233, 240));
                let app_name = wide(&agent.app);
                rect = RECT {
                    left: p(28),
                    top: p(top),
                    right: width - p(72),
                    bottom: p(top + 20),
                };
                DrawTextW(
                    hdc,
                    app_name.as_ptr(),
                    -1,
                    &mut rect,
                    DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS,
                );
                SetTextColor(hdc, color);
                let state = wide(status_label(agent.status));
                rect = RECT {
                    left: width - p(72),
                    top: p(top),
                    right: width - p(16),
                    bottom: p(top + 20),
                };
                DrawTextW(
                    hdc,
                    state.as_ptr(),
                    -1,
                    &mut rect,
                    DT_SINGLELINE | DT_RIGHT | DT_VCENTER,
                );
            }
        }
    }

    SelectObject(hdc, previous_font);
    DeleteObject(label_font);
    DeleteObject(detail_font);
    DeleteObject(badge_font);
}

unsafe fn paint(hwnd: HWND) {
    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let screen_dc = BeginPaint(hwnd, &mut ps);
    let ui = UI.get().unwrap().lock().unwrap();
    let width = ui.width.round() as i32;
    let height = ui.height.round() as i32;
    let scale = ui.scale;
    let pulse = (ui.pulse.sin() + 1.0) * 0.5;
    drop(ui);
    let snap = APP.get().unwrap().lock().unwrap().snapshot();

    // Draw at 3x resolution, then downsample. This keeps the tiny widget crisp
    // without increasing its on-screen footprint.
    const SS: i32 = 3;
    let s = |value: i32| scaled(value, scale) * SS;
    let hdc = CreateCompatibleDC(screen_dc);
    let bitmap = CreateCompatibleBitmap(screen_dc, width * SS, height * SS);
    let old_bitmap = SelectObject(hdc, bitmap);
    let canvas = CreateSolidBrush(rgb(18, 23, 32));
    let canvas_rect = RECT {
        left: 0,
        top: 0,
        right: width * SS,
        bottom: height * SS,
    };
    FillRect(hdc, &canvas_rect, canvas);
    DeleteObject(canvas);

    let bg = CreateSolidBrush(rgb(18, 23, 32));
    let border = CreatePen(PS_SOLID, SS, rgb(49, 57, 71));
    let old_brush = SelectObject(hdc, bg);
    let old_pen = SelectObject(hdc, border);
    RoundRect(
        hdc,
        s(1),
        s(1),
        width * SS - s(1),
        s(height - 1),
        s(18),
        s(18),
    );

    for (status, cx) in [
        (Status::Error, 22),
        (Status::Waiting, 47),
        (Status::Working, 72),
    ] {
        let active = snap.agents.iter().any(|agent| agent.status == status);
        let base = match status {
            Status::Error => (255, 77, 95),
            Status::Waiting => (255, 189, 46),
            Status::Working => (40, 209, 124),
            Status::Idle => (80, 89, 104),
        };
        if active {
            let glow = CreateSolidBrush(rgb(
                (base.0 as f32 * 0.5) as u8,
                (base.1 as f32 * 0.5) as u8,
                (base.2 as f32 * 0.5) as u8,
            ));
            let previous_brush = SelectObject(hdc, glow);
            let previous_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
            Ellipse(hdc, s(cx - 11), s(14), s(cx + 11), s(36));
            SelectObject(hdc, previous_brush);
            SelectObject(hdc, previous_pen);
            DeleteObject(glow);
        }
        let brighten = if active {
            if status == Status::Working {
                0.08 + pulse * 0.12
            } else {
                0.12
            }
        } else {
            0.0
        };
        let color = if active {
            rgb(
                (base.0 as f32 + (255.0 - base.0 as f32) * brighten) as u8,
                (base.1 as f32 + (255.0 - base.1 as f32) * brighten) as u8,
                (base.2 as f32 + (255.0 - base.2 as f32) * brighten) as u8,
            )
        } else {
            rgb(51, 59, 72)
        };
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(
            PS_SOLID,
            SS,
            if active {
                rgb(160, 170, 184)
            } else {
                rgb(70, 80, 95)
            },
        );
        let previous_brush = SelectObject(hdc, brush);
        let previous_pen = SelectObject(hdc, pen);
        Ellipse(hdc, s(cx - 7), s(18), s(cx + 7), s(32));
        SelectObject(hdc, previous_brush);
        SelectObject(hdc, previous_pen);
        DeleteObject(brush);
        DeleteObject(pen);
    }

    if snap.agents.len() > 1 {
        let badge_brush = CreateSolidBrush(rgb(74, 124, 255));
        let previous_brush = SelectObject(hdc, badge_brush);
        let previous_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
        RoundRect(hdc, s(79), s(3), s(92), s(16), s(8), s(8));
        SelectObject(hdc, previous_brush);
        SelectObject(hdc, previous_pen);
        DeleteObject(badge_brush);
    }

    if width > scaled(155, scale) {
        if height > scaled(COMPACT_H + 8, scale) {
            let divider = CreatePen(PS_SOLID, SS, rgb(42, 50, 63));
            let previous_pen = SelectObject(hdc, divider);
            MoveToEx(hdc, s(10), s(50), null_mut());
            LineTo(hdc, width * SS - s(10), s(50));
            SelectObject(hdc, previous_pen);
            DeleteObject(divider);

            for (index, agent) in snap.agents.iter().take(4).enumerate() {
                let top = 58 + index as i32 * 24;
                let row_brush = CreateSolidBrush(rgb(25, 32, 43));
                let previous_brush = SelectObject(hdc, row_brush);
                let previous_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
                RoundRect(
                    hdc,
                    s(8),
                    s(top),
                    width * SS - s(8),
                    s(top + 20),
                    s(8),
                    s(8),
                );
                SelectObject(hdc, previous_brush);
                SelectObject(hdc, previous_pen);
                DeleteObject(row_brush);

                let color = match agent.status {
                    Status::Error => rgb(255, 77, 95),
                    Status::Waiting => rgb(255, 189, 46),
                    Status::Working => rgb(40, 209, 124),
                    Status::Idle => rgb(102, 112, 128),
                };
                let dot = CreateSolidBrush(color);
                let previous_brush = SelectObject(hdc, dot);
                let previous_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
                Ellipse(hdc, s(15), s(top + 7), s(21), s(top + 13));
                SelectObject(hdc, previous_brush);
                SelectObject(hdc, previous_pen);
                DeleteObject(dot);
            }
        }
    }

    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    DeleteObject(bg);
    DeleteObject(border);
    SetStretchBltMode(screen_dc, HALFTONE as i32);
    StretchBlt(
        screen_dc,
        0,
        0,
        width,
        height,
        hdc,
        0,
        0,
        width * SS,
        height * SS,
        SRCCOPY,
    );
    draw_crisp_text(screen_dc, width, height, scale, &snap);
    SelectObject(hdc, old_bitmap);
    DeleteObject(bitmap);
    DeleteDC(hdc);
    EndPaint(hwnd, &ps);
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            SetTimer(hwnd, TIMER_ID, 33, None);
            SetLayeredWindowAttributes(hwnd, 0, 250, LWA_ALPHA);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            paint(hwnd);
            0
        }
        WM_STATE_CHANGED => {
            InvalidateRect(hwnd, null(), 0);
            0
        }
        WM_TIMER => {
            let mut ui = UI.get().unwrap().lock().unwrap();
            let delta_w = ui.target_width - ui.width;
            let delta_h = ui.target_height - ui.height;
            let resizing = delta_w.abs() > 0.01 || delta_h.abs() > 0.01;
            if delta_w.abs() >= 0.7 {
                ui.width += delta_w * 0.24;
            } else {
                ui.width = ui.target_width;
            }
            if delta_h.abs() >= 0.7 {
                ui.height += delta_h * 0.22;
            } else {
                ui.height = ui.target_height;
            }
            ui.pulse = (ui.pulse + 0.08) % (std::f32::consts::PI * 2.0);
            ui.pulse_tick = ui.pulse_tick.wrapping_add(1);
            let scale = ui.scale;
            let pulse_frame =
                ui.pulse_tick % 3 == 0 && ui.height <= scaled(COMPACT_H, scale) as f32 + 1.0;
            let new_w = ui.width.round() as i32;
            let new_h = ui.height.round() as i32;
            let x = ui.anchor_right - new_w;
            let y = {
                let mut r = std::mem::zeroed();
                GetWindowRect(hwnd, &mut r);
                r.top
            };
            drop(ui);
            if resizing {
                SetWindowPos(hwnd, HWND_TOPMOST, x, y, new_w, new_h, SWP_NOACTIVATE);
                let radius = scaled(18, scale);
                let region = CreateRoundRectRgn(0, 0, new_w + 1, new_h + 1, radius, radius);
                SetWindowRgn(hwnd, region, 1);
            }
            let working = APP
                .get()
                .unwrap()
                .lock()
                .unwrap()
                .snapshot()
                .agents
                .iter()
                .any(|x| x.status == Status::Working);
            if resizing || (working && pulse_frame) {
                InvalidateRect(hwnd, null(), 0);
            }
            0
        }
        WM_MOUSEMOVE => {
            let x = x_lparam(lparam);
            let y = y_lparam(lparam);
            let mut ui = UI.get().unwrap().lock().unwrap();
            if ui.dragging {
                let dx = x - ui.press_x;
                let dy = y - ui.press_y;
                if dx.abs() + dy.abs() > 3 {
                    ui.moved = true;
                }
                let nx = ui.window_x + dx;
                let ny = ui.window_y + dy;
                SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    nx,
                    ny,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
                ui.anchor_right = nx + ui.width.round() as i32;
            } else {
                ui.target_width = scaled(EXPANDED_W, ui.scale) as f32;
                let count = APP.get().unwrap().lock().unwrap().snapshot().agents.len();
                ui.target_height = scaled(expanded_height(count), ui.scale) as f32;
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut tme);
            }
            0
        }
        WM_MOUSELEAVE_MSG => {
            let mut ui = UI.get().unwrap().lock().unwrap();
            ui.target_width = scaled(COMPACT_W, ui.scale) as f32;
            ui.target_height = scaled(COMPACT_H, ui.scale) as f32;
            ui.anchor_right = {
                let mut r = std::mem::zeroed();
                GetWindowRect(hwnd, &mut r);
                r.right
            };
            0
        }
        WM_LBUTTONDOWN => {
            SetCapture(hwnd);
            let mut rect = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rect);
            let mut ui = UI.get().unwrap().lock().unwrap();
            ui.dragging = true;
            ui.moved = false;
            ui.press_x = x_lparam(lparam);
            ui.press_y = y_lparam(lparam);
            ui.window_x = rect.left;
            ui.window_y = rect.top;
            0
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            let x = x_lparam(lparam);
            let mut ui = UI.get().unwrap().lock().unwrap();
            ui.dragging = false;
            let moved = ui.moved;
            let scale = ui.scale;
            let mut rect = std::mem::zeroed();
            GetWindowRect(hwnd, &mut rect);
            let compact_x = rect.right - scaled(COMPACT_W, ui.scale);
            save_position(compact_x, rect.top);
            drop(ui);
            if !moved {
                if (scaled(10, scale)..=scaled(34, scale)).contains(&x) {
                    set_manual(Some(Status::Error));
                } else if (scaled(35, scale)..=scaled(59, scale)).contains(&x) {
                    set_manual(Some(Status::Waiting));
                } else if (scaled(60, scale)..=scaled(84, scale)).contains(&x) {
                    set_manual(Some(Status::Working));
                }
            }
            0
        }
        WM_LBUTTONDBLCLK => {
            set_manual(None);
            0
        }
        WM_RBUTTONUP => {
            show_context_menu(hwnd);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_ID);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn main() {
    unsafe {
        let mutex_name = wide("Local\\AITrafficLight.Singleton");
        let instance_mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
        if instance_mutex.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        APP.set(Mutex::new(AppState::default())).ok();
        NOTIFY_RUNTIME
            .set(Mutex::new(NotifyRuntime {
                initialized: false,
                last_keys: HashMap::new(),
                last_sent: HashMap::new(),
            }))
            .ok();

        let module = GetModuleHandleW(null());
        let class = wide(CLASS_NAME);
        let wc = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(window_proc),
            hInstance: module,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        if RegisterClassW(&wc) == 0 {
            return;
        }

        let (mut x, mut y) = load_position().unwrap_or_else(|| {
            let mut work: RECT = std::mem::zeroed();
            SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut work as *mut _ as *mut c_void, 0);
            (work.right - COMPACT_W - 18, work.top + 140)
        });
        let monitor = MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST);
        let mut monitor_info: MONITORINFO = std::mem::zeroed();
        monitor_info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut monitor_info) != 0 {
            x = x.clamp(
                monitor_info.rcWork.left,
                monitor_info.rcWork.right - COMPACT_W,
            );
            y = y.clamp(
                monitor_info.rcWork.top,
                monitor_info.rcWork.bottom - COMPACT_H,
            );
        }
        let title = wide("AI Traffic Light");
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            x,
            y,
            COMPACT_W,
            COMPACT_H,
            null_mut(),
            null_mut(),
            module,
            null_mut(),
        );
        if hwnd.is_null() {
            return;
        }
        let dpi_scale = (GetDpiForWindow(hwnd) as f32 / 96.0).clamp(1.25, 2.5);
        let compact_w = scaled(COMPACT_W, dpi_scale);
        let compact_h = scaled(COMPACT_H, dpi_scale);
        if GetMonitorInfoW(monitor, &mut monitor_info) != 0 {
            x = x.clamp(
                monitor_info.rcWork.left,
                monitor_info.rcWork.right - compact_w,
            );
            y = y.clamp(
                monitor_info.rcWork.top,
                monitor_info.rcWork.bottom - compact_h,
            );
        }
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y,
            compact_w,
            compact_h,
            SWP_NOACTIVATE,
        );
        UI.set(Mutex::new(UiState {
            hwnd,
            scale: dpi_scale,
            width: compact_w as f32,
            target_width: compact_w as f32,
            height: compact_h as f32,
            target_height: compact_h as f32,
            anchor_right: x + compact_w,
            dragging: false,
            moved: false,
            press_x: 0,
            press_y: 0,
            window_x: x,
            window_y: y,
            pulse: 0.0,
            pulse_tick: 0,
        }))
        .ok();
        let radius = scaled(18, dpi_scale);
        let region = CreateRoundRectRgn(0, 0, compact_w + 1, compact_h + 1, radius, radius);
        SetWindowRgn(hwnd, region, 1);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        UpdateWindow(hwnd);

        start_http();
        start_detection();

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        CloseHandle(instance_mutex);
    }
}
