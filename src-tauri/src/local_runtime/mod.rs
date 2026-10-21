use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{BufRead, BufReader, Read},
    net::TcpListener,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RuntimeState {
    #[default]
    Stopped,
    Starting,
    LoadingModel,
    Ready,
    Failed,
    Stopping,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeBackend {
    #[default]
    Auto,
    Vulkan,
    Cuda,
    Cpu,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LocalRuntimeConfig {
    pub model_path: String,
    #[serde(default)]
    pub binary_path: Option<String>,
    #[serde(default)]
    pub use_custom_runtime: bool,
    #[serde(default)]
    pub backend: RuntimeBackend,
    pub context_length: u32,
    #[serde(default)]
    pub gpu_offload: Option<i32>,
    pub cpu_threads: u16,
    pub parallelism: u16,
    pub startup_timeout_ms: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeCapabilities {
    pub chat: bool,
    pub streaming: bool,
    pub tool_calling: bool,
    pub tool_choice_required: bool,
    pub agent_mode: bool,
    pub tested_at: Option<u64>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatus {
    pub state: RuntimeState,
    pub model_path: Option<String>,
    pub model_size: Option<u64>,
    pub backend: Option<RuntimeBackend>,
    pub runtime_source: Option<String>,
    pub runtime_version: Option<String>,
    pub port: Option<u16>,
    pub pid: Option<u32>,
    pub error: Option<String>,
    pub logs: Vec<String>,
    pub capabilities: RuntimeCapabilities,
}

impl Default for RuntimeStatus {
    fn default() -> Self {
        Self {
            state: RuntimeState::Stopped,
            model_path: None,
            model_size: None,
            backend: None,
            runtime_source: None,
            runtime_version: None,
            port: None,
            pid: None,
            error: None,
            logs: Vec::new(),
            capabilities: RuntimeCapabilities::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HardwareProfile {
    pub os: String,
    pub architecture: String,
    pub cpu: Option<String>,
    pub cpu_threads: usize,
    pub ram_bytes: Option<u64>,
    pub gpu: Option<String>,
    pub vram_bytes: Option<u64>,
    pub available_backends: Vec<RuntimeBackend>,
    pub recommended_backend: RuntimeBackend,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeManifest {
    runtime_version: String,
    llama_cpp_version: String,
    llama_cpp_commit: String,
    build_date: String,
    profiles: Vec<RuntimeProfile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeProfile {
    os: String,
    arch: String,
    backend: RuntimeBackend,
    directory: String,
    binary: String,
    sha256: String,
    required_companion_libraries: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiscovery {
    pub source: String,
    pub runtime_version: Option<String>,
    pub llama_cpp_version: Option<String>,
    pub llama_cpp_commit: Option<String>,
    pub build_date: Option<String>,
    pub bundled_backends: Vec<RuntimeBackend>,
    pub available_backends: Vec<RuntimeBackend>,
    pub recommended_backend: RuntimeBackend,
    pub health: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelInspection {
    pub path: String,
    pub size_bytes: u64,
    pub valid_gguf: bool,
    pub gguf_version: Option<u32>,
    pub tensor_count: Option<u64>,
    pub metadata_count: Option<u64>,
    pub estimated_ram_bytes: u64,
    pub estimated_vram_bytes: Option<u64>,
    pub recommendation: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug)]
struct RuntimeExecutable {
    path: PathBuf,
    directory: PathBuf,
    backend: RuntimeBackend,
    source: String,
    version: Option<String>,
}

struct RuntimeInner {
    status: RuntimeStatus,
    child: Option<Child>,
    config: Option<LocalRuntimeConfig>,
    generation: u64,
}

#[derive(Clone)]
pub struct LocalRuntimeManager {
    inner: Arc<Mutex<RuntimeInner>>,
}

impl Default for LocalRuntimeManager {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(RuntimeInner {
                status: RuntimeStatus::default(),
                child: None,
                config: None,
                generation: 0,
            })),
        }
    }
}

impl Drop for LocalRuntimeManager {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }
        if let Ok(mut inner) = self.inner.lock() {
            terminate_owned_child(&mut inner);
        }
    }
}

impl LocalRuntimeManager {
    pub fn status(&self) -> RuntimeStatus {
        self.inner
            .lock()
            .map(|inner| inner.status.clone())
            .unwrap_or_else(|_| RuntimeStatus {
                state: RuntimeState::Failed,
                error: Some("Local runtime state is unavailable".into()),
                ..RuntimeStatus::default()
            })
    }

    pub fn endpoint_if_ready(&self) -> Result<(String, String), String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Local runtime state is unavailable")?;
        if inner.status.state != RuntimeState::Ready {
            return Err(format!(
                "GravityForge Local runtime is not ready ({:?})",
                inner.status.state
            ));
        }
        let port = inner
            .status
            .port
            .ok_or("Local runtime port is unavailable")?;
        let model = Path::new(
            inner
                .status
                .model_path
                .as_deref()
                .ok_or("Local model is unavailable")?,
        )
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("local-model")
        .to_owned();
        Ok((format!("http://127.0.0.1:{port}/v1"), model))
    }

    pub fn shutdown(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            terminate_owned_child(&mut inner);
            inner.status.state = RuntimeState::Stopped;
        }
    }
}

fn emit_status(app: &AppHandle, manager: &LocalRuntimeManager) {
    let _ = app.emit("local-runtime-status", manager.status());
}

fn set_state(
    manager: &LocalRuntimeManager,
    app: &AppHandle,
    state: RuntimeState,
    error: Option<String>,
) {
    if let Ok(mut inner) = manager.inner.lock() {
        inner.status.state = state;
        inner.status.error = error;
    }
    emit_status(app, manager);
}

fn reserve_port() -> Result<u16, String> {
    TcpListener::bind(("127.0.0.1", 0))
        .and_then(|socket| socket.local_addr())
        .map(|address| address.port())
        .map_err(|error| format!("Cannot reserve a local runtime port: {error}"))
}

fn source_runtime_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries/runtime")
}

fn bundled_runtime_root(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .resource_dir()
        .ok()
        .map(|root| root.join("binaries/runtime"))
        .filter(|root| root.join("manifest.json").is_file())
        .or_else(|| {
            let root = source_runtime_root();
            root.join("manifest.json").is_file().then_some(root)
        })
}

fn load_manifest(root: &Path) -> Result<RuntimeManifest, String> {
    let text = fs::read_to_string(root.join("manifest.json"))
        .map_err(|error| format!("Bundled runtime manifest is unavailable: {error}"))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Bundled runtime manifest is invalid: {error}"))
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|error| format!("Cannot verify bundled runtime: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Cannot verify bundled runtime: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn profile_is_complete(root: &Path, profile: &RuntimeProfile) -> bool {
    let directory = root.join(&profile.directory);
    directory.join(&profile.binary).is_file()
        && profile
            .required_companion_libraries
            .iter()
            .all(|library| directory.join(library).exists())
}

fn command_for_runtime(executable: &RuntimeExecutable) -> Command {
    let mut command = Command::new(&executable.path);
    command.current_dir(&executable.directory);
    #[cfg(target_os = "linux")]
    {
        let mut paths = vec![executable.directory.clone()];
        if let Some(existing) = env::var_os("LD_LIBRARY_PATH") {
            paths.extend(env::split_paths(&existing));
        }
        if let Ok(joined) = env::join_paths(paths) {
            command.env("LD_LIBRARY_PATH", joined);
        }
    }
    command
}

fn vulkan_usable(executable: &RuntimeExecutable) -> Result<bool, String> {
    let output = command_for_runtime(executable)
        .arg("--list-devices")
        .output()
        .map_err(|error| format!("Vulkan runtime preflight failed: {error}"))?;
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.status.success()
        && !text.contains("(none)")
        && text.to_ascii_lowercase().contains("vulkan"))
}

fn bundled_profile(
    root: &Path,
    manifest: &RuntimeManifest,
    backend: RuntimeBackend,
) -> Result<Option<RuntimeExecutable>, String> {
    let Some(profile) = manifest.profiles.iter().find(|profile| {
        profile.os == env::consts::OS
            && profile.arch == env::consts::ARCH
            && profile.backend == backend
    }) else {
        return Ok(None);
    };
    if !profile_is_complete(root, profile) {
        return Err(format!(
            "Bundled {backend:?} runtime is incomplete; reinstall GravityForge"
        ));
    }
    let directory = root.join(&profile.directory);
    let path = directory.join(&profile.binary);
    if sha256(&path)? != profile.sha256 {
        return Err(format!(
            "Bundled {backend:?} runtime failed SHA-256 verification"
        ));
    }
    Ok(Some(RuntimeExecutable {
        path,
        directory,
        backend,
        source: "Bundled".into(),
        version: Some(manifest.llama_cpp_version.clone()),
    }))
}

fn select_binary(
    app: &AppHandle,
    config: &LocalRuntimeConfig,
) -> Result<RuntimeExecutable, String> {
    if !config.use_custom_runtime {
        if let Some(root) = bundled_runtime_root(app) {
            let manifest = load_manifest(&root)?;
            let requested = match config.backend {
                RuntimeBackend::Auto => {
                    if let Some(vulkan) = bundled_profile(&root, &manifest, RuntimeBackend::Vulkan)?
                    {
                        if vulkan_usable(&vulkan).unwrap_or(false) {
                            RuntimeBackend::Vulkan
                        } else {
                            RuntimeBackend::Cpu
                        }
                    } else {
                        RuntimeBackend::Cpu
                    }
                }
                backend => backend,
            };
            if let Some(executable) = bundled_profile(&root, &manifest, requested)? {
                if requested == RuntimeBackend::Vulkan && !vulkan_usable(&executable)? {
                    return Err(
                        "Vulkan is unavailable or has no usable hardware device; CPU fallback is available"
                            .into(),
                    );
                }
                return Ok(executable);
            }
        }
    }

    let custom = config
        .binary_path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("GRAVITYFORGE_LLAMA_SERVER").map(PathBuf::from))
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            "No compatible bundled runtime was found. Select a custom llama-server or set GRAVITYFORGE_LLAMA_SERVER for debugging."
                .to_owned()
        })?;
    let directory = custom.parent().unwrap_or(Path::new(".")).to_path_buf();
    Ok(RuntimeExecutable {
        path: custom,
        directory,
        backend: config.backend,
        source: "Custom".into(),
        version: None,
    })
}

fn validate_config(config: &LocalRuntimeConfig) -> Result<(PathBuf, u64), String> {
    let model = fs::canonicalize(&config.model_path)
        .map_err(|error| format!("GGUF model does not exist: {error}"))?;
    if !model.is_file()
        || model
            .extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| !extension.eq_ignore_ascii_case("gguf"))
            .unwrap_or(true)
    {
        return Err("Model path must reference a .gguf file".into());
    }
    if !(1024..=1_048_576).contains(&config.context_length)
        || config.cpu_threads == 0
        || config.parallelism == 0
        || !(1_000..=600_000).contains(&config.startup_timeout_ms)
    {
        return Err("Local runtime configuration is outside supported limits".into());
    }
    let size = fs::metadata(&model)
        .map_err(|error| format!("Cannot inspect GGUF model: {error}"))?
        .len();
    let mut magic = [0_u8; 4];
    fs::File::open(&model)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|error| format!("Cannot read GGUF model header: {error}"))?;
    if &magic != b"GGUF" {
        return Err("Model file does not contain a valid GGUF header".into());
    }
    Ok((model, size))
}

fn inspect_model(path: &Path, gpu_offload: Option<i32>) -> Result<ModelInspection, String> {
    let model =
        fs::canonicalize(path).map_err(|error| format!("GGUF model does not exist: {error}"))?;
    let size_bytes = fs::metadata(&model)
        .map_err(|error| format!("Cannot inspect GGUF model: {error}"))?
        .len();
    let mut header = [0_u8; 24];
    fs::File::open(&model)
        .and_then(|mut file| file.read_exact(&mut header))
        .map_err(|error| format!("Cannot read GGUF model header: {error}"))?;
    let valid_gguf = &header[..4] == b"GGUF";
    if !valid_gguf {
        return Err("Model file does not contain a valid GGUF header".into());
    }
    let gguf_version = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let tensor_count = u64::from_le_bytes(header[8..16].try_into().unwrap());
    let metadata_count = u64::from_le_bytes(header[16..24].try_into().unwrap());
    let estimated_ram_bytes = size_bytes.saturating_add(size_bytes / 5);
    let estimated_vram_bytes = gpu_offload.map(|layers| {
        if layers <= 0 {
            0
        } else {
            size_bytes.saturating_mul((layers as u64).min(100)) / 100
        }
    });
    Ok(ModelInspection {
        path: model.to_string_lossy().into_owned(),
        size_bytes,
        valid_gguf,
        gguf_version: Some(gguf_version),
        tensor_count: Some(tensor_count),
        metadata_count: Some(metadata_count),
        estimated_ram_bytes,
        estimated_vram_bytes,
        recommendation: if gpu_offload == Some(0) {
            "CPU-only; expect lower generation speed".into()
        } else {
            "Partial GPU offload may reduce RAM pressure when supported".into()
        },
        warnings: vec!["Memory figures are estimates, not guarantees".into()],
    })
}

fn runtime_args(
    config: &LocalRuntimeConfig,
    backend: RuntimeBackend,
    model: &Path,
    port: u16,
) -> Vec<String> {
    let mut args = vec![
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--model".into(),
        model.to_string_lossy().into_owned(),
        "--ctx-size".into(),
        config.context_length.to_string(),
        "--threads".into(),
        config.cpu_threads.to_string(),
        "--parallel".into(),
        config.parallelism.to_string(),
        "--flash-attn".into(),
        "on".into(),
    ];
    if backend == RuntimeBackend::Cpu {
        args.extend(["--n-gpu-layers".into(), "0".into()]);
    } else if let Some(layers) = config.gpu_offload {
        args.extend(["--n-gpu-layers".into(), layers.max(0).to_string()]);
    }
    args
}

fn push_log(inner: &Arc<Mutex<RuntimeInner>>, line: String) {
    if let Ok(mut state) = inner.lock() {
        state.status.logs.push(line);
        let extra = state.status.logs.len().saturating_sub(200);
        if extra > 0 {
            state.status.logs.drain(..extra);
        }
    }
}

fn drain_output<R: std::io::Read + Send + 'static>(reader: R, inner: Arc<Mutex<RuntimeInner>>) {
    thread::spawn(move || {
        for line in BufReader::new(reader).lines().map_while(Result::ok) {
            push_log(&inner, line);
        }
    });
}

fn terminate_owned_child(inner: &mut RuntimeInner) {
    inner.generation = inner.generation.wrapping_add(1);
    if let Some(mut child) = inner.child.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    inner.status.pid = None;
    inner.status.port = None;
}

async fn wait_until_ready(
    manager: &LocalRuntimeManager,
    app: &AppHandle,
    port: u16,
    timeout: Duration,
) -> Result<(), String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_millis(400))
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    let mut loading_emitted = false;
    while started.elapsed() < timeout {
        {
            let mut inner = manager
                .inner
                .lock()
                .map_err(|_| "Local runtime state is unavailable")?;
            if let Some(child) = inner.child.as_mut() {
                if let Some(exit) = child.try_wait().map_err(|error| error.to_string())? {
                    inner.child = None;
                    return Err(format!("llama-server exited during startup: {exit}"));
                }
            } else {
                return Err("llama-server disappeared during startup".into());
            }
        }
        if let Ok(response) = client
            .get(format!("http://127.0.0.1:{port}/health"))
            .send()
            .await
        {
            if response.status().is_success() {
                return Ok(());
            }
            if !loading_emitted {
                set_state(manager, app, RuntimeState::LoadingModel, None);
                loading_emitted = true;
            }
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    Err(format!(
        "llama-server did not become ready within {} ms",
        timeout.as_millis()
    ))
}

async fn start_runtime(
    manager: LocalRuntimeManager,
    app: AppHandle,
    config: LocalRuntimeConfig,
) -> Result<RuntimeStatus, String> {
    let (model, model_size) = validate_config(&config)?;
    let executable = select_binary(&app, &config)?;
    let port = reserve_port()?;
    {
        let mut inner = manager
            .inner
            .lock()
            .map_err(|_| "Local runtime state is unavailable")?;
        if !matches!(
            inner.status.state,
            RuntimeState::Stopped | RuntimeState::Failed
        ) {
            return Err("Local runtime is already active".into());
        }
        terminate_owned_child(&mut inner);
        inner.status = RuntimeStatus {
            state: RuntimeState::Starting,
            model_path: Some(model.to_string_lossy().into_owned()),
            model_size: Some(model_size),
            backend: Some(executable.backend),
            runtime_source: Some(executable.source.clone()),
            runtime_version: executable.version.clone(),
            port: Some(port),
            pid: None,
            error: None,
            logs: Vec::new(),
            capabilities: RuntimeCapabilities::default(),
        };
        inner.config = Some(config.clone());
    }
    emit_status(&app, &manager);
    let mut command = command_for_runtime(&executable);
    command
        .args(runtime_args(&config, executable.backend, &model, port))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let message = format!("Cannot start {}: {error}", executable.path.display());
            set_state(&manager, &app, RuntimeState::Failed, Some(message.clone()));
            return Err(message);
        }
    };
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    {
        let mut inner = manager
            .inner
            .lock()
            .map_err(|_| "Local runtime state is unavailable")?;
        inner.status.pid = Some(child.id());
        inner.child = Some(child);
        inner.generation = inner.generation.wrapping_add(1);
    }
    if let Some(stdout) = stdout {
        drain_output(stdout, manager.inner.clone());
    }
    if let Some(stderr) = stderr {
        drain_output(stderr, manager.inner.clone());
    }
    set_state(&manager, &app, RuntimeState::LoadingModel, None);
    if let Err(error) = wait_until_ready(
        &manager,
        &app,
        port,
        Duration::from_millis(config.startup_timeout_ms),
    )
    .await
    {
        if let Ok(mut inner) = manager.inner.lock() {
            terminate_owned_child(&mut inner);
            inner.status.state = RuntimeState::Failed;
            inner.status.error = Some(error.clone());
        }
        emit_status(&app, &manager);
        return Err(error);
    }
    set_state(&manager, &app, RuntimeState::Ready, None);
    monitor_child(manager.clone(), app);
    Ok(manager.status())
}

fn monitor_child(manager: LocalRuntimeManager, app: AppHandle) {
    let generation = manager
        .inner
        .lock()
        .map(|inner| inner.generation)
        .unwrap_or_default();
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(500));
        let exit = {
            let mut inner = match manager.inner.lock() {
                Ok(inner) => inner,
                Err(_) => return,
            };
            if inner.generation != generation || inner.status.state != RuntimeState::Ready {
                return;
            }
            inner
                .child
                .as_mut()
                .and_then(|child| child.try_wait().ok())
                .flatten()
        };
        if let Some(status) = exit {
            if let Ok(mut inner) = manager.inner.lock() {
                inner.child = None;
                inner.status.state = RuntimeState::Failed;
                inner.status.pid = None;
                inner.status.error = Some(format!("llama-server exited unexpectedly: {status}"));
            }
            emit_status(&app, &manager);
            return;
        }
    });
}

#[tauri::command]
pub fn local_runtime_status(manager: State<'_, LocalRuntimeManager>) -> RuntimeStatus {
    manager.status()
}

#[tauri::command]
pub async fn local_runtime_start(
    config: LocalRuntimeConfig,
    app: AppHandle,
    manager: State<'_, LocalRuntimeManager>,
) -> Result<RuntimeStatus, String> {
    start_runtime(manager.inner().clone(), app, config).await
}

#[tauri::command]
pub fn local_runtime_stop(
    app: AppHandle,
    manager: State<'_, LocalRuntimeManager>,
) -> Result<RuntimeStatus, String> {
    set_state(&manager, &app, RuntimeState::Stopping, None);
    {
        let mut inner = manager
            .inner
            .lock()
            .map_err(|_| "Local runtime state is unavailable")?;
        terminate_owned_child(&mut inner);
        inner.status.state = RuntimeState::Stopped;
        inner.status.error = None;
        inner.status.capabilities = RuntimeCapabilities::default();
    }
    emit_status(&app, &manager);
    Ok(manager.status())
}

#[tauri::command]
pub async fn local_runtime_restart(
    app: AppHandle,
    manager: State<'_, LocalRuntimeManager>,
) -> Result<RuntimeStatus, String> {
    let config = manager
        .inner
        .lock()
        .map_err(|_| "Local runtime state is unavailable")?
        .config
        .clone()
        .ok_or("No local runtime configuration has been loaded")?;
    local_runtime_stop(app.clone(), manager.clone())?;
    start_runtime(manager.inner().clone(), app, config).await
}

fn response_has_native_call(value: &Value) -> bool {
    value
        .pointer("/choices/0/message/tool_calls")
        .and_then(Value::as_array)
        .map(|calls| {
            calls.iter().any(|call| {
                call.get("type").and_then(Value::as_str) == Some("function")
                    && call
                        .pointer("/function/name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| !name.is_empty())
                    && call
                        .pointer("/function/arguments")
                        .and_then(Value::as_str)
                        .is_some_and(|arguments| serde_json::from_str::<Value>(arguments).is_ok())
            })
        })
        .unwrap_or(false)
}

#[tauri::command]
pub async fn local_runtime_test_capabilities(
    app: AppHandle,
    manager: State<'_, LocalRuntimeManager>,
) -> Result<RuntimeCapabilities, String> {
    let (base_url, model) = manager.endpoint_if_ready()?;
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|error| error.to_string())?;
    let chat_response = client
        .post(format!("{base_url}/chat/completions"))
        .json(&json!({
            "model": model,
            "messages": [{"role":"user","content":"Reply with exactly CHAT_OK"}],
            "temperature": 0,
            "max_tokens": 16,
            "stream": false
        }))
        .send()
        .await
        .map_err(|error| format!("Chat capability test failed: {error}"))?;
    let chat_status = chat_response.status();
    let chat_json: Value = chat_response.json().await.unwrap_or(Value::Null);
    let chat = chat_status.is_success()
        && chat_json
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .is_some();
    let stream_response = client
        .post(format!("{base_url}/chat/completions"))
        .json(&json!({
            "model": model,
            "messages": [{"role":"user","content":"Reply with exactly STREAM_OK"}],
            "temperature": 0,
            "max_tokens": 16,
            "stream": true
        }))
        .send()
        .await
        .map_err(|error| format!("Streaming capability test failed: {error}"))?;
    let streaming = stream_response.status().is_success()
        && stream_response
            .text()
            .await
            .unwrap_or_default()
            .contains("data:");
    let tool_response = client
        .post(format!("{base_url}/chat/completions"))
        .json(&json!({
            "model": model,
            "messages": [{"role":"user","content":"Call square_number with number 12. Do not answer in text."}],
            "tools": [{"type":"function","function":{"name":"square_number","description":"Return the square of a number.","parameters":{"type":"object","properties":{"number":{"type":"number"}},"required":["number"],"additionalProperties":false}}}],
            "tool_choice": "required",
            "temperature": 0,
            "max_tokens": 128,
            "stream": false
        }))
        .send()
        .await
        .map_err(|error| format!("Tool capability test failed: {error}"))?;
    let tool_status = tool_response.status();
    let tool_json: Value = tool_response
        .json()
        .await
        .map_err(|error| format!("Tool capability response is invalid: {error}"))?;
    let native = tool_status.is_success() && response_has_native_call(&tool_json);
    let capabilities = RuntimeCapabilities {
        chat,
        streaming,
        tool_calling: native,
        tool_choice_required: native,
        agent_mode: chat && native,
        tested_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_secs()),
        detail: (!native).then(|| "No structured message.tool_calls was returned; assistant content was not interpreted.".into()),
    };
    if let Ok(mut inner) = manager.inner.lock() {
        inner.status.capabilities = capabilities.clone();
    }
    emit_status(&app, &manager);
    Ok(capabilities)
}

fn read_first_matching(path: &str, prefix: &str) -> Option<String> {
    fs::read_to_string(path).ok()?.lines().find_map(|line| {
        line.strip_prefix(prefix)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn command_line(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn detect_hardware() -> HardwareProfile {
    let cpu = read_first_matching("/proc/cpuinfo", "model name\t:")
        .or_else(|| command_line("sysctl", &["-n", "machdep.cpu.brand_string"]));
    let ram_bytes = fs::read_to_string("/proc/meminfo").ok().and_then(|text| {
        text.lines().find_map(|line| {
            line.strip_prefix("MemTotal:")
                .and_then(|value| value.split_whitespace().next())
                .and_then(|value| value.parse::<u64>().ok())
                .map(|kb| kb * 1024)
        })
    });
    let lspci = command_line("lspci", &[]).unwrap_or_default();
    let gpu = lspci
        .lines()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("vga compatible controller") || lower.contains("3d controller")
        })
        .map(str::to_owned);
    let lower = gpu.as_deref().unwrap_or_default().to_ascii_lowercase();
    let has_nvidia = lower.contains("nvidia");
    let has_amd = lower.contains("amd") || lower.contains("radeon");
    let mut backends = vec![RuntimeBackend::Cpu];
    if has_nvidia {
        backends.insert(0, RuntimeBackend::Cuda);
    }
    if has_amd {
        backends.insert(0, RuntimeBackend::Vulkan);
    }
    HardwareProfile {
        os: env::consts::OS.into(),
        architecture: env::consts::ARCH.into(),
        cpu,
        cpu_threads: thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        ram_bytes,
        gpu,
        vram_bytes: None,
        available_backends: backends,
        recommended_backend: if has_nvidia {
            RuntimeBackend::Cuda
        } else if has_amd {
            RuntimeBackend::Vulkan
        } else {
            RuntimeBackend::Cpu
        },
    }
}

#[tauri::command]
pub fn local_runtime_hardware_profile(app: AppHandle) -> HardwareProfile {
    let mut hardware = detect_hardware();
    if let Ok(discovery) = discover_runtime(&app) {
        hardware.available_backends = discovery.available_backends;
        hardware.recommended_backend = discovery.recommended_backend;
    }
    hardware
}

fn discover_runtime(app: &AppHandle) -> Result<RuntimeDiscovery, String> {
    let root = bundled_runtime_root(app).ok_or("Bundled runtime manifest was not found")?;
    let manifest = load_manifest(&root)?;
    let bundled_backends: Vec<_> = manifest
        .profiles
        .iter()
        .filter(|profile| {
            profile.os == env::consts::OS
                && profile.arch == env::consts::ARCH
                && profile_is_complete(&root, profile)
        })
        .map(|profile| profile.backend)
        .collect();
    let mut available_backends = Vec::new();
    let mut warnings = Vec::new();
    if bundled_backends.contains(&RuntimeBackend::Cpu) {
        available_backends.push(RuntimeBackend::Cpu);
    }
    if bundled_backends.contains(&RuntimeBackend::Vulkan) {
        match bundled_profile(&root, &manifest, RuntimeBackend::Vulkan)? {
            Some(executable) if vulkan_usable(&executable).unwrap_or(false) => {
                available_backends.insert(0, RuntimeBackend::Vulkan);
            }
            _ => warnings.push("Vulkan unavailable; CPU fallback is ready".into()),
        }
    }
    if env::consts::OS == "windows" {
        warnings.push("CUDA/Vulkan profiles are architected but not certified on Windows".into());
    }
    let recommended_backend = available_backends
        .first()
        .copied()
        .unwrap_or(RuntimeBackend::Cpu);
    Ok(RuntimeDiscovery {
        source: "Bundled".into(),
        runtime_version: Some(manifest.runtime_version),
        llama_cpp_version: Some(manifest.llama_cpp_version),
        llama_cpp_commit: Some(manifest.llama_cpp_commit),
        build_date: Some(manifest.build_date),
        bundled_backends,
        available_backends,
        recommended_backend,
        health: "READY".into(),
        warnings,
    })
}

#[tauri::command]
pub fn local_runtime_discovery(app: AppHandle) -> Result<RuntimeDiscovery, String> {
    discover_runtime(&app)
}

#[tauri::command]
pub fn local_runtime_inspect_model(
    model_path: String,
    gpu_offload: Option<i32>,
) -> Result<ModelInspection, String> {
    inspect_model(Path::new(&model_path), gpu_offload)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(path: &Path) -> LocalRuntimeConfig {
        LocalRuntimeConfig {
            model_path: path.to_string_lossy().into_owned(),
            binary_path: None,
            use_custom_runtime: false,
            backend: RuntimeBackend::Vulkan,
            context_length: 32768,
            gpu_offload: None,
            cpu_threads: 6,
            parallelism: 1,
            startup_timeout_ms: 120000,
        }
    }

    #[test]
    fn runtime_states_are_stable_and_serializable() {
        assert_eq!(
            serde_json::to_value(RuntimeState::LoadingModel).unwrap(),
            json!("LOADING_MODEL")
        );
        assert_eq!(
            LocalRuntimeManager::default().status().state,
            RuntimeState::Stopped
        );
    }

    #[test]
    fn reserves_dynamic_ports() {
        let first = reserve_port().unwrap();
        let second = reserve_port().unwrap();
        assert_ne!(first, 0);
        assert_ne!(second, 0);
    }

    #[test]
    fn rejects_missing_and_non_gguf_models() {
        let missing = config(Path::new("/definitely/missing/model.gguf"));
        assert!(validate_config(&missing)
            .unwrap_err()
            .contains("does not exist"));
        let file = tempfile::NamedTempFile::new().unwrap();
        assert!(validate_config(&config(file.path()))
            .unwrap_err()
            .contains(".gguf"));
    }

    #[test]
    fn builds_conservative_runtime_arguments() {
        let args = runtime_args(
            &config(Path::new("model.gguf")),
            RuntimeBackend::Vulkan,
            Path::new("model.gguf"),
            32123,
        );
        assert!(args.windows(2).any(|pair| pair == ["--port", "32123"]));
        assert!(args.windows(2).any(|pair| pair == ["--parallel", "1"]));
        assert!(!args.iter().any(|arg| arg == "--n-gpu-layers"));
    }

    #[test]
    fn cpu_profile_forces_zero_gpu_layers() {
        let args = runtime_args(
            &config(Path::new("model.gguf")),
            RuntimeBackend::Cpu,
            Path::new("model.gguf"),
            32123,
        );
        assert!(args.windows(2).any(|pair| pair == ["--n-gpu-layers", "0"]));
    }

    #[test]
    fn bundled_manifest_profiles_are_complete() {
        let root = source_runtime_root();
        let manifest = load_manifest(&root).unwrap();
        assert_eq!(manifest.runtime_version, "gravityforge-local-runtime-1");
        assert!(manifest
            .profiles
            .iter()
            .all(|profile| profile_is_complete(&root, profile)));
    }

    #[test]
    fn capability_parser_accepts_only_native_tool_calls() {
        assert!(response_has_native_call(
            &json!({"choices":[{"message":{"content":null,"tool_calls":[{"id":"1","type":"function","function":{"name":"square_number","arguments":"{\"number\":12}"}}]}}]})
        ));
        assert!(!response_has_native_call(
            &json!({"choices":[{"message":{"content":"<tool_call>{...}</tool_call>","tool_calls":[]}}]})
        ));
        assert!(!response_has_native_call(
            &json!({"choices":[{"message":{"content":"{\"name\":\"square_number\"}"}}]})
        ));
    }

    #[test]
    fn unavailable_runtime_cannot_supply_an_endpoint() {
        let manager = LocalRuntimeManager::default();
        let error = manager.endpoint_if_ready().unwrap_err();
        assert!(error.contains("not ready"));
    }

    #[cfg(unix)]
    #[test]
    fn owned_process_is_stopped_and_reaped() {
        let mut child = Command::new("sh").args(["-c", "sleep 30"]).spawn().unwrap();
        let pid = child.id();
        assert!(child.try_wait().unwrap().is_none());
        let manager = LocalRuntimeManager::default();
        {
            let mut inner = manager.inner.lock().unwrap();
            inner.child = Some(child);
            inner.status.state = RuntimeState::Ready;
            inner.status.pid = Some(pid);
        }
        manager.shutdown();
        assert_eq!(manager.status().state, RuntimeState::Stopped);
        assert_eq!(manager.status().pid, None);
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }

    #[cfg(unix)]
    #[test]
    fn child_crash_is_observable() {
        let mut child = Command::new("sh").args(["-c", "exit 17"]).spawn().unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(17));
        assert!(!status.success());
    }
}
