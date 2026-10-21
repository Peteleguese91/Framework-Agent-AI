use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, State};

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VoicePhase {
    #[default]
    Off,
    Idle,
    Listening,
    Transcribing,
    Thinking,
    Speaking,
    Interrupted,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatus {
    phase: VoicePhase,
    microphone_active: bool,
    playback_active: bool,
    error: Option<VoiceError>,
}

impl Default for VoiceStatus {
    fn default() -> Self {
        Self {
            phase: VoicePhase::Off,
            microphone_active: false,
            playback_active: false,
            error: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum VoiceErrorCode {
    NoMicrophone,
    MicPermissionDenied,
    SttModelMissing,
    SttFailed,
    TtsModelMissing,
    TtsFailed,
    AudioOutputFailed,
    DeviceDisconnected,
    VoiceInterrupted,
    InvalidConfig,
    NoSpeech,
    MicSelfTestRequired,
}

#[derive(Clone, Debug, Serialize)]
pub struct VoiceError {
    code: VoiceErrorCode,
    message: String,
}

impl VoiceError {
    fn new(code: VoiceErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    id: String,
    name: String,
    is_default: bool,
    kind: String,
    category: String,
    node_id: Option<u32>,
    media_class: String,
    sample_specification: String,
    profile: String,
    active_port: String,
    muted: bool,
    volume_percent: f32,
    available: bool,
    hardware_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFingerprint {
    device_id: String,
    hardware_id: String,
    node_id: Option<u32>,
    media_class: String,
    sample_specification: String,
    profile: String,
    active_port: String,
}

impl From<&AudioDevice> for DeviceFingerprint {
    fn from(device: &AudioDevice) -> Self {
        Self {
            device_id: device.id.clone(),
            hardware_id: device.hardware_id.clone(),
            node_id: device.node_id,
            media_class: device.media_class.clone(),
            // Lock the format requested by AudioCaptureSession. The source's
            // advertised native format is dynamic under PipeWire and may change
            // after negotiation even though the physical node and route did not.
            sample_specification: "s16le 2ch 48000Hz".into(),
            profile: device.profile.clone(),
            active_port: device.active_port.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SttConfig {
    executable: String,
    model_path: String,
    language: String,
    vad_threshold: f32,
    minimum_speech_ms: u64,
    silence_timeout_ms: u64,
    #[serde(default)]
    diagnostic_mode: bool,
    #[serde(default)]
    diagnostic_directory: String,
    #[serde(default = "default_true")]
    use_vad: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TtsConfig {
    engine: String,
    executable: String,
    model_path: String,
    config_path: String,
    voice: String,
    language: String,
    output_device: String,
    speech_rate: f32,
    volume: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    text: String,
    detected_language: Option<String>,
    duration_ms: u64,
    speech_ms: u64,
    speech_end_to_transcript_ms: u64,
    language_requested: String,
    model_name: String,
    audio: AudioDiagnostics,
    diagnostic_files: Vec<String>,
    whisper_warnings: Vec<String>,
    capture_session_id: String,
    device_fingerprint: DeviceFingerprint,
    raw_capture_hash: String,
    post_resample_hash: String,
    whisper_input_hash: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDiagnostics {
    input_sample_rate: u32,
    input_channels: u16,
    input_bits_per_sample: u16,
    processed_sample_rate: u32,
    processed_channels: u16,
    raw_duration_ms: u64,
    processed_duration_ms: u64,
    vad_start_ms: Option<u64>,
    vad_end_ms: Option<u64>,
    vad_duration_ms: u64,
    rms: f32,
    peak: f32,
    clipped_percent: f32,
    silence_percent: f32,
    level: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputLevel {
    rms: f32,
    peak: f32,
    zero_percent: f32,
    level: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicrophoneSelfTest {
    result: String,
    device_id: String,
    category: String,
    duration_ms: u64,
    sample_rate: u32,
    channels: u16,
    level: InputLevel,
    playback_completed: bool,
}

struct Recording {
    child: Child,
    path: PathBuf,
    started: Instant,
    session_id: String,
    fingerprint: DeviceFingerprint,
}

pub struct VoiceEngine {
    status: Mutex<VoiceStatus>,
    recording: Mutex<Option<Recording>>,
    playback: Mutex<Option<Child>>,
    runtime: Mutex<VoiceRuntimeStatus>,
    certified_input: Mutex<Option<DeviceFingerprint>>,
    stt_pid: Mutex<Option<u32>>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceRuntimeStatus {
    state: String,
    stt_ready: bool,
    tts_ready: bool,
    detail: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceHardwareRecommendation {
    total_memory_gib: f32,
    cpu_threads: usize,
    supported_gpu: bool,
    recommended_profile: String,
}

#[tauri::command]
pub fn voice_hardware_recommendation() -> VoiceHardwareRecommendation {
    let total_kib = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.starts_with("MemTotal:"))
                .and_then(|line| line.split_whitespace().nth(1))
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap_or(0);
    let total_memory_gib = total_kib as f32 / 1_048_576.0;
    let cpu_threads = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    // The current whisper.cpp build is CPU-only. Do not advertise an
    // unverified GPU merely because graphics hardware exists on the host.
    let supported_gpu = false;
    VoiceHardwareRecommendation {
        total_memory_gib,
        cpu_threads,
        supported_gpu,
        recommended_profile: if total_memory_gib >= 24.0 && cpu_threads >= 8 {
            "BALANCED".into()
        } else {
            "LOW_RESOURCE".into()
        },
    }
}

impl Default for VoiceEngine {
    fn default() -> Self {
        Self {
            status: Mutex::new(VoiceStatus::default()),
            recording: Mutex::new(None),
            playback: Mutex::new(None),
            runtime: Mutex::new(VoiceRuntimeStatus::default()),
            certified_input: Mutex::new(None),
            stt_pid: Mutex::new(None),
        }
    }
}

impl VoiceEngine {
    fn set_phase(&self, phase: VoicePhase) {
        let mut status = self.status.lock().expect("voice status lock poisoned");
        status.phase = phase;
        status.error = None;
        status.microphone_active = phase == VoicePhase::Listening;
        status.playback_active = phase == VoicePhase::Speaking;
    }
    fn fail(&self, error: VoiceError) -> VoiceError {
        let mut status = self.status.lock().expect("voice status lock poisoned");
        status.phase = VoicePhase::Failed;
        status.microphone_active = false;
        status.playback_active = false;
        status.error = Some(error.clone());
        error
    }
    pub fn stop(&self) {
        if let Some(pid) = self.stt_pid.lock().expect("STT PID lock poisoned").take() {
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
        if let Some(mut recording) = self
            .recording
            .lock()
            .expect("recording lock poisoned")
            .take()
        {
            terminate(&mut recording.child);
            let _ = fs::remove_file(recording.path);
        }
        if let Some(mut playback) = self.playback.lock().expect("playback lock poisoned").take() {
            terminate(&mut playback);
        }
        self.set_phase(VoicePhase::Interrupted);
    }
}

fn executable(value: &str) -> Result<PathBuf, VoiceError> {
    if value.is_empty() || value.len() > 512 || value.contains('\0') {
        return Err(VoiceError::new(
            VoiceErrorCode::InvalidConfig,
            "Invalid executable",
        ));
    }
    let candidate = PathBuf::from(value);
    if candidate.components().count() > 1 {
        return candidate.canonicalize().map_err(|_| {
            VoiceError::new(
                VoiceErrorCode::InvalidConfig,
                format!("Executable not found: {value}"),
            )
        });
    }
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join(value))
                .find(|item| item.is_file())
        })
        .ok_or_else(|| {
            VoiceError::new(
                VoiceErrorCode::InvalidConfig,
                format!("Executable not found: {value}"),
            )
        })
}
fn existing_file(value: &str, code: VoiceErrorCode, label: &str) -> Result<PathBuf, VoiceError> {
    let path = PathBuf::from(value)
        .canonicalize()
        .map_err(|_| VoiceError::new(code, format!("{label} not found")))?;
    if !path.is_file() {
        return Err(VoiceError::new(code, format!("{label} is not a file")));
    }
    Ok(path)
}
fn temp_wav(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "gravityforge-{label}-{}-{stamp}.wav",
        std::process::id()
    ))
}
fn capture_session_id() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{stamp}", std::process::id())
}

fn pcm_sha256(samples: &[i16]) -> String {
    let mut hasher = Sha256::new();
    for sample in samples {
        hasher.update(sample.to_le_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn start_audio_capture(device: &AudioDevice, label: &str) -> Result<Recording, VoiceError> {
    let session_id = capture_session_id();
    let path = temp_wav(&format!("{label}-{session_id}"));
    let child = Command::new("pw-record")
        .args([
            "--target",
            &device.id,
            "--rate",
            "48000",
            "--channels",
            "2",
            "--format",
            "s16",
            path.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            VoiceError::new(
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    VoiceErrorCode::MicPermissionDenied
                } else {
                    VoiceErrorCode::NoMicrophone
                },
                e.to_string(),
            )
        })?;
    Ok(Recording {
        child,
        path,
        started: Instant::now(),
        session_id,
        fingerprint: DeviceFingerprint::from(device),
    })
}
struct TempFiles(Vec<PathBuf>);
impl Drop for TempFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = fs::remove_file(path);
        }
    }
}
fn terminate(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    for _ in 0..20 {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let _ = child.kill();
    let _ = child.wait();
}
fn run_output(program: &str, args: &[&str]) -> Result<String, VoiceError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| VoiceError::new(VoiceErrorCode::DeviceDisconnected, e.to_string()))?;
    if !output.status.success() {
        return Err(VoiceError::new(
            VoiceErrorCode::DeviceDisconnected,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
fn list_pulse(kind: &str, default: &str) -> Result<Vec<AudioDevice>, VoiceError> {
    let plural = if kind == "input" { "sources" } else { "sinks" };
    let text = run_output("pactl", &["-f", "json", "list", plural])?;
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| VoiceError::new(VoiceErrorCode::DeviceDisconnected, e.to_string()))?;
    Ok(value
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .filter_map(|item| {
            let id = item.get("name")?.as_str()?.to_owned();
            let properties = item.get("properties").unwrap_or(&Value::Null);
            let monitor = kind == "input"
                && (id.ends_with(".monitor")
                    || item
                        .get("monitor_source")
                        .and_then(Value::as_str)
                        .is_some_and(|value| !value.is_empty())
                    || properties.get("device.class").and_then(Value::as_str) == Some("monitor"));
            let media_class = properties
                .get("media.class")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let factory = properties
                .get("factory.name")
                .and_then(Value::as_str)
                .unwrap_or("");
            let category = if kind == "output" {
                "Output"
            } else if monitor {
                "Monitor"
            } else if media_class == "Audio/Source" && factory == "api.alsa.pcm.source" {
                "Microphone"
            } else if factory.contains("virtual")
                || id.contains("virtual")
                || id.contains("loopback")
            {
                "Virtual"
            } else {
                "Unknown"
            }
            .to_owned();
            let name = item
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or(&id)
                .to_owned();
            let volume_percent = item
                .get("volume")
                .and_then(Value::as_object)
                .and_then(|channels| channels.values().next())
                .and_then(|channel| channel.get("value_percent"))
                .and_then(Value::as_str)
                .and_then(|value| value.trim_end_matches('%').parse::<f32>().ok())
                .unwrap_or(0.0);
            let active_port = item
                .get("active_port")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            let available = item
                .get("ports")
                .and_then(Value::as_array)
                .and_then(|ports| {
                    ports.iter().find(|port| {
                        port.get("name").and_then(Value::as_str) == Some(active_port.as_str())
                    })
                })
                .and_then(|port| port.get("availability"))
                .and_then(Value::as_str)
                != Some("not available");
            Some(AudioDevice {
                is_default: id == default,
                id,
                name,
                kind: kind.to_owned(),
                category,
                node_id: properties
                    .get("object.id")
                    .and_then(Value::as_str)
                    .and_then(|value| value.parse().ok()),
                media_class,
                sample_specification: item
                    .get("sample_specification")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                profile: properties
                    .get("device.profile.name")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                active_port,
                muted: item.get("mute").and_then(Value::as_bool).unwrap_or(false),
                volume_percent,
                available,
                hardware_id: [
                    properties.get("device.name").and_then(Value::as_str),
                    properties.get("device.bus-id").and_then(Value::as_str),
                    properties.get("api.alsa.path").and_then(Value::as_str),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join("|"),
            })
        })
        .collect())
}
#[derive(Clone, Debug)]
struct PcmWav {
    sample_rate: u32,
    channels: u16,
    bits_per_sample: u16,
    samples: Vec<i16>,
}

fn read_pcm16_wav(path: &Path) -> Result<PcmWav, VoiceError> {
    let bytes =
        fs::read(path).map_err(|e| VoiceError::new(VoiceErrorCode::SttFailed, e.to_string()))?;
    if bytes.len() < 44 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(VoiceError::new(
            VoiceErrorCode::SttFailed,
            "Captured audio is not WAV",
        ));
    }
    let mut offset = 12usize;
    let mut format = None;
    let mut data = None;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let start = offset + 8;
        let end = start.saturating_add(size).min(bytes.len());
        if id == b"fmt " && size >= 16 && end <= bytes.len() {
            format = Some((
                u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()),
                u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()),
                u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()),
                u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap()),
            ));
        } else if id == b"data" {
            data = Some(&bytes[start..end]);
        }
        offset = start.saturating_add(size + (size % 2));
    }
    let (audio_format, channels, sample_rate, bits_per_sample) = format
        .ok_or_else(|| VoiceError::new(VoiceErrorCode::SttFailed, "WAV format chunk missing"))?;
    if audio_format != 1 || bits_per_sample != 16 || channels == 0 || sample_rate == 0 {
        return Err(VoiceError::new(
            VoiceErrorCode::SttFailed,
            "Expected PCM 16-bit WAV audio",
        ));
    }
    let samples = data
        .ok_or_else(|| VoiceError::new(VoiceErrorCode::SttFailed, "WAV data chunk missing"))?
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
        .collect();
    Ok(PcmWav {
        sample_rate,
        channels,
        bits_per_sample,
        samples,
    })
}

fn downmix_and_resample(input: &PcmWav, output_rate: u32) -> Vec<i16> {
    let channels = usize::from(input.channels);
    let mono: Vec<f64> = input
        .samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().map(|sample| f64::from(*sample)).sum::<f64>() / channels as f64)
        .collect();
    if mono.is_empty() || input.sample_rate == output_rate {
        return mono
            .into_iter()
            .map(|sample| {
                sample
                    .round()
                    .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
            })
            .collect();
    }
    let output_len = ((mono.len() as u64 * u64::from(output_rate)) / u64::from(input.sample_rate))
        .max(1) as usize;
    (0..output_len)
        .map(|index| {
            let source = index as f64 * f64::from(input.sample_rate) / f64::from(output_rate);
            let left = source.floor() as usize;
            let right = (left + 1).min(mono.len() - 1);
            let fraction = source - left as f64;
            (mono[left] * (1.0 - fraction) + mono[right] * fraction)
                .round()
                .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
        })
        .collect()
}

fn write_pcm16_wav(
    path: &Path,
    sample_rate: u32,
    channels: u16,
    samples: &[i16],
) -> Result<(), VoiceError> {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::with_capacity(44 + data_len as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&channels.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * u32::from(channels) * 2).to_le_bytes());
    bytes.extend_from_slice(&(channels * 2).to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    fs::write(path, bytes).map_err(|e| VoiceError::new(VoiceErrorCode::SttFailed, e.to_string()))
}

fn input_level(path: &Path) -> Result<InputLevel, VoiceError> {
    let wav = read_pcm16_wav(path)?;
    let total = wav.samples.len().max(1);
    let values = wav
        .samples
        .iter()
        .map(|sample| f64::from(*sample) / 32768.0);
    let mut squares = 0.0;
    let mut peak = 0.0f64;
    let mut zeros = 0usize;
    for (sample, value) in wav.samples.iter().zip(values) {
        squares += value * value;
        peak = peak.max(value.abs());
        if *sample == 0 {
            zeros += 1;
        }
    }
    let rms = (squares / total as f64).sqrt() as f32;
    let peak = peak as f32;
    let zero_percent = zeros as f32 * 100.0 / total as f32;
    let level = if zero_percent >= 99.0 || peak < 0.001 {
        "NO_SIGNAL"
    } else if rms < 0.003 {
        "LOW"
    } else if peak >= 0.999 {
        "CLIPPING"
    } else {
        "OK"
    };
    Ok(InputLevel {
        rms,
        peak,
        zero_percent,
        level: level.into(),
    })
}

fn segment_and_measure(
    input: &PcmWav,
    threshold: f32,
    minimum_speech_ms: u64,
    silence_timeout_ms: u64,
) -> Result<(Vec<i16>, AudioDiagnostics, u64), VoiceError> {
    if !(0.001..=1.0).contains(&threshold) {
        return Err(VoiceError::new(
            VoiceErrorCode::InvalidConfig,
            "VAD threshold must be between 0.001 and 1",
        ));
    }
    let samples = downmix_and_resample(input, 16_000);
    let frame = 320usize;
    let frame_rms: Vec<f64> = samples
        .chunks(frame)
        .map(|chunk| {
            (chunk
                .iter()
                .map(|sample| {
                    let value = f64::from(*sample) / 32768.0;
                    value * value
                })
                .sum::<f64>()
                / chunk.len().max(1) as f64)
                .sqrt()
        })
        .collect();
    let active: Vec<usize> = frame_rms
        .iter()
        .enumerate()
        .filter_map(|(index, rms)| (*rms >= f64::from(threshold)).then_some(index))
        .collect();
    let speech_ms = active.len() as u64 * 20;
    let values: Vec<f64> = samples
        .iter()
        .map(|sample| f64::from(*sample) / 32768.0)
        .collect();
    let rms = (values.iter().map(|value| value * value).sum::<f64>() / values.len().max(1) as f64)
        .sqrt() as f32;
    let peak = values
        .iter()
        .map(|value| value.abs())
        .fold(0.0f64, f64::max) as f32;
    let clipped = values.iter().filter(|value| value.abs() >= 0.999).count();
    let silent_frames = frame_rms
        .iter()
        .filter(|rms| **rms < f64::from(threshold))
        .count();
    let raw_duration_ms = (input.samples.len() as u64 * 1000)
        / (u64::from(input.sample_rate) * u64::from(input.channels));
    let processed_duration_ms = samples.len() as u64 * 1000 / 16_000;
    let pre_roll_frames = 15usize; // 300 ms
    let post_roll_frames = silence_timeout_ms.div_ceil(20).max(20) as usize; // at least 400 ms
    let (start_frame, end_frame) = match (active.first(), active.last()) {
        (Some(start), Some(end)) => (
            start.saturating_sub(pre_roll_frames),
            (end + 1 + post_roll_frames).min(frame_rms.len()),
        ),
        _ => (0, 0),
    };
    let segmented = if speech_ms >= minimum_speech_ms {
        samples[start_frame * frame..(end_frame * frame).min(samples.len())].to_vec()
    } else {
        Vec::new()
    };
    let diagnostics = AudioDiagnostics {
        input_sample_rate: input.sample_rate,
        input_channels: input.channels,
        input_bits_per_sample: input.bits_per_sample,
        processed_sample_rate: 16_000,
        processed_channels: 1,
        raw_duration_ms,
        processed_duration_ms,
        vad_start_ms: (!active.is_empty()).then_some(start_frame as u64 * 20),
        vad_end_ms: (!active.is_empty()).then_some(end_frame as u64 * 20),
        vad_duration_ms: segmented.len() as u64 * 1000 / 16_000,
        rms,
        peak,
        clipped_percent: clipped as f32 * 100.0 / values.len().max(1) as f32,
        silence_percent: silent_frames as f32 * 100.0 / frame_rms.len().max(1) as f32,
        level: if rms < threshold {
            "LOW"
        } else if peak >= 0.999 {
            "CLIPPING"
        } else {
            "OK"
        }
        .into(),
    };
    Ok((segmented, diagnostics, speech_ms))
}

#[tauri::command]
pub fn voice_status(engine: State<'_, VoiceEngine>) -> VoiceStatus {
    engine
        .status
        .lock()
        .expect("voice status lock poisoned")
        .clone()
}

#[tauri::command]
pub fn voice_devices() -> Result<Vec<AudioDevice>, VoiceError> {
    let source = run_output("pactl", &["get-default-source"])?
        .trim()
        .to_owned();
    let sink = run_output("pactl", &["get-default-sink"])?
        .trim()
        .to_owned();
    let mut devices = list_pulse("input", &source)?;
    devices.extend(list_pulse("output", &sink)?);
    devices.sort_by_key(|device| match device.category.as_str() {
        "Microphone" => 0,
        "Unknown" => 1,
        "Virtual" => 2,
        "Monitor" => 3,
        _ => 4,
    });
    Ok(devices)
}

#[tauri::command]
pub async fn voice_microphone_self_test(
    input_device: String,
    output_device: String,
    play_back: bool,
    app: AppHandle,
    engine: State<'_, VoiceEngine>,
) -> Result<MicrophoneSelfTest, VoiceError> {
    engine.stop();
    if !engine
        .runtime
        .lock()
        .expect("voice runtime lock poisoned")
        .stt_ready
    {
        return Err(VoiceError::new(
            VoiceErrorCode::SttFailed,
            "Voice Input / Speech-to-Text is disabled",
        ));
    }
    *engine
        .certified_input
        .lock()
        .expect("certified input lock poisoned") = None;
    let device = voice_devices()?
        .into_iter()
        .find(|device| device.kind == "input" && device.id == input_device)
        .ok_or_else(|| {
            VoiceError::new(
                VoiceErrorCode::DeviceDisconnected,
                "Selected input is unavailable",
            )
        })?;
    if device.category != "Microphone" {
        return Ok(MicrophoneSelfTest {
            result: "FAIL_LIKELY_MONITOR_LOOPBACK".into(),
            device_id: input_device,
            category: device.category,
            duration_ms: 0,
            sample_rate: 0,
            channels: 0,
            level: InputLevel {
                rms: 0.0,
                peak: 0.0,
                zero_percent: 100.0,
                level: "NO_SIGNAL".into(),
            },
            playback_completed: false,
        });
    }
    if device.muted || !device.available {
        return Ok(MicrophoneSelfTest {
            result: "FAIL_DEVICE_UNAVAILABLE".into(),
            device_id: input_device,
            category: device.category,
            duration_ms: 0,
            sample_rate: 0,
            channels: 0,
            level: InputLevel {
                rms: 0.0,
                peak: 0.0,
                zero_percent: 100.0,
                level: "NO_SIGNAL".into(),
            },
            playback_completed: false,
        });
    }
    let mut recording = start_audio_capture(&device, "microphone-self-test")?;
    let path = recording.path.clone();
    let _cleanup = TempFiles(vec![path.clone()]);
    engine.set_phase(VoicePhase::Listening);
    for _ in 0..25 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Ok(level) = input_level(&path) {
            let _ = app.emit("voice-input-level", &level);
        }
    }
    terminate(&mut recording.child);
    engine.set_phase(VoicePhase::Idle);
    let wav = read_pcm16_wav(&path)?;
    let level = input_level(&path)?;
    let duration_ms =
        wav.samples.len() as u64 * 1000 / (u64::from(wav.sample_rate) * u64::from(wav.channels));
    let pass = level.level != "NO_SIGNAL";
    let mut playback_completed = false;
    if play_back && pass {
        let mut command = Command::new("pw-play");
        if !output_device.is_empty() {
            command.args(["--target", &output_device]);
        }
        let status = command
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| VoiceError::new(VoiceErrorCode::AudioOutputFailed, e.to_string()))?;
        playback_completed = status.success();
    }
    let result = if pass {
        "PASS_VOICE_SIGNAL_DETECTED"
    } else {
        "FAIL_NO_SIGNAL"
    }
    .to_owned();
    if pass {
        *engine
            .certified_input
            .lock()
            .expect("certified input lock poisoned") = Some(recording.fingerprint);
    }
    Ok(MicrophoneSelfTest {
        result,
        device_id: input_device,
        category: device.category,
        duration_ms,
        sample_rate: wav.sample_rate,
        channels: wav.channels,
        level,
        playback_completed,
    })
}

#[tauri::command]
pub fn voice_runtime_status(engine: State<'_, VoiceEngine>) -> VoiceRuntimeStatus {
    engine
        .runtime
        .lock()
        .expect("voice runtime lock poisoned")
        .clone()
}

#[tauri::command]
pub fn voice_runtime_load(
    stt: Option<SttConfig>,
    tts: Option<TtsConfig>,
    engine: State<'_, VoiceEngine>,
) -> Result<VoiceRuntimeStatus, VoiceError> {
    let stt_ready = if let Some(config) = stt {
        executable(&config.executable).map_err(|_| {
            VoiceError::new(
                VoiceErrorCode::SttFailed,
                "whisper-cli executable is missing",
            )
        })?;
        existing_file(
            &config.model_path,
            VoiceErrorCode::SttModelMissing,
            "STT model",
        )?;
        true
    } else {
        false
    };
    let tts_ready = if let Some(config) = tts {
        executable(&config.executable)
            .map_err(|_| VoiceError::new(VoiceErrorCode::TtsFailed, "TTS executable is missing"))?;
        if config.engine == "piper" {
            existing_file(
                &config.model_path,
                VoiceErrorCode::TtsModelMissing,
                "Piper voice model",
            )?;
        }
        true
    } else {
        false
    };
    let status = VoiceRuntimeStatus {
        state: "READY".into(),
        stt_ready,
        tts_ready,
        detail: Some("Configured for on-demand execution; no voice model is resident".into()),
    };
    *engine.runtime.lock().expect("voice runtime lock poisoned") = status.clone();
    Ok(status)
}

#[tauri::command]
pub fn voice_runtime_unload(engine: State<'_, VoiceEngine>) -> VoiceRuntimeStatus {
    engine.stop();
    *engine
        .certified_input
        .lock()
        .expect("certified input lock poisoned") = None;
    let status = VoiceRuntimeStatus {
        state: "UNLOADED".into(),
        ..VoiceRuntimeStatus::default()
    };
    *engine.runtime.lock().expect("voice runtime lock poisoned") = status.clone();
    status
}

#[tauri::command]
pub fn voice_enable(engine: State<'_, VoiceEngine>) -> VoiceStatus {
    engine.set_phase(VoicePhase::Idle);
    voice_status(engine)
}

#[tauri::command]
pub fn voice_disable(engine: State<'_, VoiceEngine>) -> VoiceStatus {
    engine.stop();
    *engine
        .certified_input
        .lock()
        .expect("certified input lock poisoned") = None;
    engine.set_phase(VoicePhase::Off);
    voice_status(engine)
}

#[tauri::command]
pub fn voice_start_listening(
    input_device: String,
    engine: State<'_, VoiceEngine>,
) -> Result<VoiceStatus, VoiceError> {
    engine.stop();
    if !engine
        .runtime
        .lock()
        .expect("voice runtime lock poisoned")
        .stt_ready
    {
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::SttFailed,
            "Voice Input / Speech-to-Text is disabled",
        )));
    }
    if input_device.is_empty() {
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::NoMicrophone,
            "Select an input device",
        )));
    }
    let device = voice_devices()?
        .into_iter()
        .find(|device| device.kind == "input" && device.id == input_device)
        .ok_or_else(|| {
            engine.fail(VoiceError::new(
                VoiceErrorCode::DeviceDisconnected,
                "Selected input is unavailable",
            ))
        })?;
    let current_fingerprint = DeviceFingerprint::from(&device);
    if engine
        .certified_input
        .lock()
        .expect("certified input lock poisoned")
        .as_ref()
        != Some(&current_fingerprint)
    {
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::MicSelfTestRequired,
            "SELF TEST INVALIDATED: the selected device, node, format, profile, or route changed",
        )));
    }
    let recording = start_audio_capture(&device, "capture").map_err(|error| engine.fail(error))?;
    *engine.recording.lock().expect("recording lock poisoned") = Some(recording);
    engine.set_phase(VoicePhase::Listening);
    Ok(voice_status(engine))
}

#[tauri::command]
pub async fn voice_stop_and_transcribe(
    config: SttConfig,
    engine: State<'_, VoiceEngine>,
) -> Result<Transcript, VoiceError> {
    if !(100..=5000).contains(&config.silence_timeout_ms) {
        return Err(VoiceError::new(
            VoiceErrorCode::InvalidConfig,
            "Silence timeout must be between 100 and 5000 ms",
        ));
    }
    let mut recording = engine
        .recording
        .lock()
        .expect("recording lock poisoned")
        .take()
        .ok_or_else(|| {
            engine.fail(VoiceError::new(
                VoiceErrorCode::NoMicrophone,
                "No active recording",
            ))
        })?;
    terminate(&mut recording.child);
    let speech_end = Instant::now();
    engine.set_phase(VoicePhase::Transcribing);
    let result = transcribe(
        &recording.path,
        &config,
        recording.started.elapsed(),
        speech_end,
        recording.session_id,
        recording.fingerprint,
        &engine,
    )
    .await;
    let _ = fs::remove_file(&recording.path);
    match result {
        Ok(value) => {
            engine.set_phase(VoicePhase::Thinking);
            Ok(value)
        }
        Err(error) => Err(engine.fail(error)),
    }
}

/// Records exactly once, plays that capture, then transcribes the same closed WAV.
/// This command is intentionally diagnostic: it never opens a second capture stream.
#[tauri::command]
pub async fn voice_capture_integrity_test(
    input_device: String,
    output_device: String,
    mut config: SttConfig,
    app: AppHandle,
    engine: State<'_, VoiceEngine>,
) -> Result<Transcript, VoiceError> {
    engine.stop();
    if !engine
        .runtime
        .lock()
        .expect("voice runtime lock poisoned")
        .stt_ready
    {
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::SttFailed,
            "Voice Input / Speech-to-Text is disabled",
        )));
    }
    let device = voice_devices()?
        .into_iter()
        .find(|device| device.kind == "input" && device.id == input_device)
        .ok_or_else(|| {
            engine.fail(VoiceError::new(
                VoiceErrorCode::DeviceDisconnected,
                "Selected input is unavailable",
            ))
        })?;
    let fingerprint = DeviceFingerprint::from(&device);
    if engine
        .certified_input
        .lock()
        .expect("certified input lock poisoned")
        .as_ref()
        != Some(&fingerprint)
    {
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::MicSelfTestRequired,
            "SELF TEST INVALIDATED: run the microphone self-test on this exact configuration",
        )));
    }

    let mut recording =
        start_audio_capture(&device, "integrity-test").map_err(|error| engine.fail(error))?;
    let path = recording.path.clone();
    engine.set_phase(VoicePhase::Listening);
    for _ in 0..35 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Ok(level) = input_level(&path) {
            let _ = app.emit("voice-input-level", &level);
        }
    }
    terminate(&mut recording.child);
    let speech_end = Instant::now();

    let mut playback = Command::new("pw-play");
    if !output_device.is_empty() {
        playback.args(["--target", &output_device]);
    }
    let playback_status = playback
        .arg(&path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| {
            engine.fail(VoiceError::new(
                VoiceErrorCode::AudioOutputFailed,
                e.to_string(),
            ))
        })?;
    if !playback_status.success() {
        let _ = fs::remove_file(&path);
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::AudioOutputFailed,
            "Diagnostic playback failed",
        )));
    }

    engine.set_phase(VoicePhase::Transcribing);
    // The integrity test keeps uniquely named copies in the OS temporary directory
    // only long enough for the out-of-process direct Whisper comparison.
    config.diagnostic_mode = true;
    config.diagnostic_directory = std::env::temp_dir().to_string_lossy().into_owned();
    let result = transcribe(
        &path,
        &config,
        recording.started.elapsed(),
        speech_end,
        recording.session_id,
        recording.fingerprint,
        &engine,
    )
    .await;
    let _ = fs::remove_file(&path);
    match result {
        Ok(value) => {
            engine.set_phase(VoicePhase::Idle);
            Ok(value)
        }
        Err(error) => Err(engine.fail(error)),
    }
}

async fn transcribe(
    path: &Path,
    config: &SttConfig,
    captured: Duration,
    speech_end: Instant,
    capture_session_id: String,
    device_fingerprint: DeviceFingerprint,
    engine: &VoiceEngine,
) -> Result<Transcript, VoiceError> {
    let executable = executable(&config.executable).map_err(|_| {
        VoiceError::new(
            VoiceErrorCode::SttFailed,
            "whisper-cli executable is missing",
        )
    })?;
    let model = existing_file(
        &config.model_path,
        VoiceErrorCode::SttModelMissing,
        "STT model",
    )?;
    let raw = read_pcm16_wav(path)?;
    let raw_capture_hash = pcm_sha256(&raw.samples);
    let (segment, audio, speech_ms) = segment_and_measure(
        &raw,
        config.vad_threshold,
        config.minimum_speech_ms,
        config.silence_timeout_ms,
    )?;
    if speech_ms < config.minimum_speech_ms {
        return Err(VoiceError::new(
            VoiceErrorCode::NoSpeech,
            "No speech above the configured VAD threshold",
        ));
    }
    let processed_path = temp_wav("processed");
    let vad_path = temp_wav("vad");
    let _temporary_audio = TempFiles(vec![processed_path.clone(), vad_path.clone()]);
    let processed = downmix_and_resample(&raw, 16_000);
    let post_resample_hash = pcm_sha256(&processed);
    write_pcm16_wav(&processed_path, 16_000, 1, &processed)?;
    if !segment.is_empty() {
        write_pcm16_wav(&vad_path, 16_000, 1, &segment)?;
    }
    let whisper_input = if config.use_vad {
        &vad_path
    } else {
        &processed_path
    };
    let whisper_input_hash = if config.use_vad {
        pcm_sha256(&segment)
    } else {
        post_resample_hash.clone()
    };
    let mut diagnostic_files = Vec::new();
    if config.diagnostic_mode {
        let directory = PathBuf::from(&config.diagnostic_directory)
            .canonicalize()
            .map_err(|_| {
                VoiceError::new(
                    VoiceErrorCode::InvalidConfig,
                    "Diagnostic directory must already exist",
                )
            })?;
        if !directory.is_dir() {
            return Err(VoiceError::new(
                VoiceErrorCode::InvalidConfig,
                "Diagnostic path is not a directory",
            ));
        }
        for (source, name) in [
            (path, "raw.wav"),
            (processed_path.as_path(), "resampled.wav"),
            (vad_path.as_path(), "vad.wav"),
        ] {
            let destination =
                directory.join(format!("gravityforge-stt-{capture_session_id}-{name}"));
            fs::copy(source, &destination)
                .map_err(|e| VoiceError::new(VoiceErrorCode::SttFailed, e.to_string()))?;
            diagnostic_files.push(destination.to_string_lossy().into_owned());
        }
    }
    let prefix = path.with_extension("transcript");
    let json_path = prefix.with_extension("transcript.json");
    let _temporary_json = TempFiles(vec![json_path.clone()]);
    let language = match config.language.as_str() {
        "auto" | "it" | "en" => config.language.as_str(),
        _ => {
            return Err(VoiceError::new(
                VoiceErrorCode::InvalidConfig,
                "Language must be auto, it, or en",
            ))
        }
    };
    let child = tokio::process::Command::new(executable)
        .args([
            "-m",
            model.to_string_lossy().as_ref(),
            "-f",
            whisper_input.to_string_lossy().as_ref(),
            "-l",
            language,
            "-ojf",
            "-of",
            prefix.to_string_lossy().as_ref(),
            "-np",
            "-nt",
            "--no-gpu",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| VoiceError::new(VoiceErrorCode::SttFailed, e.to_string()))?;
    *engine.stt_pid.lock().expect("STT PID lock poisoned") = child.id();
    let output_result = child.wait_with_output().await;
    *engine.stt_pid.lock().expect("STT PID lock poisoned") = None;
    let output =
        output_result.map_err(|e| VoiceError::new(VoiceErrorCode::SttFailed, e.to_string()))?;
    if !output.status.success() {
        return Err(VoiceError::new(
            VoiceErrorCode::SttFailed,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    let value: Value = serde_json::from_slice(
        &fs::read(&json_path)
            .map_err(|e| VoiceError::new(VoiceErrorCode::SttFailed, e.to_string()))?,
    )
    .map_err(|e| VoiceError::new(VoiceErrorCode::SttFailed, e.to_string()))?;
    let _ = fs::remove_file(json_path);
    let text = value
        .get("transcription")
        .and_then(Value::as_array)
        .map(|segments| {
            segments
                .iter()
                .filter_map(|segment| segment.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
        .trim()
        .to_owned();
    if text.is_empty() {
        return Err(VoiceError::new(
            VoiceErrorCode::SttFailed,
            "STT returned an empty transcript",
        ));
    }
    let detected_language = value
        .pointer("/result/language")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let warnings = String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.to_ascii_lowercase().contains("warning"))
        .map(str::to_owned)
        .collect();
    Ok(Transcript {
        text,
        detected_language,
        duration_ms: audio.raw_duration_ms.max(captured.as_millis() as u64),
        speech_ms,
        speech_end_to_transcript_ms: speech_end.elapsed().as_millis() as u64,
        language_requested: language.to_owned(),
        model_name: model
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        audio,
        diagnostic_files,
        whisper_warnings: warnings,
        capture_session_id,
        device_fingerprint,
        raw_capture_hash,
        post_resample_hash,
        whisper_input_hash,
    })
}

fn synthesize(text: &str, config: &TtsConfig, path: &Path) -> Result<(), VoiceError> {
    if text.trim().is_empty() || text.len() > 32_768 {
        return Err(VoiceError::new(
            VoiceErrorCode::InvalidConfig,
            "TTS text is empty or too long",
        ));
    }
    let program = executable(&config.executable)
        .map_err(|_| VoiceError::new(VoiceErrorCode::TtsFailed, "TTS executable is missing"))?;
    let mut command = Command::new(program);
    match config.engine.as_str() {
        "piper" => {
            let model = existing_file(
                &config.model_path,
                VoiceErrorCode::TtsModelMissing,
                "Piper voice model",
            )?;
            command
                .arg("--model")
                .arg(model)
                .arg("--output_file")
                .arg(path);
            if !config.config_path.is_empty() {
                let voice_config = existing_file(
                    &config.config_path,
                    VoiceErrorCode::TtsModelMissing,
                    "Piper voice config",
                )?;
                command.arg("--config").arg(voice_config);
            }
            command
                .arg("--length_scale")
                .arg((1.0 / config.speech_rate.clamp(0.5, 2.0)).to_string());
        }
        "espeak" => {
            let voice = if config.voice.is_empty() {
                if config.language == "it" {
                    "it"
                } else {
                    "en"
                }
            } else {
                &config.voice
            };
            command
                .arg("--stdin")
                .arg("-v")
                .arg(voice)
                .arg("-s")
                .arg(((175.0 * config.speech_rate.clamp(0.5, 2.0)) as u32).to_string())
                .arg("-w")
                .arg(path);
        }
        _ => {
            return Err(VoiceError::new(
                VoiceErrorCode::InvalidConfig,
                "TTS engine must be piper or espeak",
            ))
        }
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| VoiceError::new(VoiceErrorCode::TtsFailed, e.to_string()))?;
    child
        .stdin
        .take()
        .ok_or_else(|| VoiceError::new(VoiceErrorCode::TtsFailed, "TTS stdin unavailable"))?
        .write_all(text.as_bytes())
        .map_err(|e| VoiceError::new(VoiceErrorCode::TtsFailed, e.to_string()))?;
    let output = child
        .wait_with_output()
        .map_err(|e| VoiceError::new(VoiceErrorCode::TtsFailed, e.to_string()))?;
    if !output.status.success() {
        return Err(VoiceError::new(
            VoiceErrorCode::TtsFailed,
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub async fn voice_speak(
    text: String,
    config: TtsConfig,
    engine: State<'_, VoiceEngine>,
) -> Result<VoiceStatus, VoiceError> {
    if !engine
        .runtime
        .lock()
        .expect("voice runtime lock poisoned")
        .tts_ready
    {
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::TtsFailed,
            "Voice Output / Text-to-Speech is disabled",
        )));
    }
    if !(0.0..=1.0).contains(&config.volume) {
        return Err(engine.fail(VoiceError::new(
            VoiceErrorCode::InvalidConfig,
            "Volume must be between 0 and 1",
        )));
    }
    engine.stop();
    engine.set_phase(VoicePhase::Speaking);
    let path = temp_wav("speech");
    if let Err(error) = synthesize(&text, &config, &path) {
        let _ = fs::remove_file(&path);
        return Err(engine.fail(error));
    }
    let mut command = Command::new("pw-play");
    if !config.output_device.is_empty() {
        command.args(["--target", &config.output_device]);
    }
    command
        .args([
            "--volume",
            &config.volume.to_string(),
            path.to_string_lossy().as_ref(),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let child = command.spawn().map_err(|e| {
        engine.fail(VoiceError::new(
            VoiceErrorCode::AudioOutputFailed,
            e.to_string(),
        ))
    })?;
    *engine.playback.lock().expect("playback lock poisoned") = Some(child);
    loop {
        let done = {
            let mut guard = engine.playback.lock().expect("playback lock poisoned");
            match guard.as_mut() {
                Some(child) => child
                    .try_wait()
                    .map_err(|e| {
                        engine.fail(VoiceError::new(
                            VoiceErrorCode::AudioOutputFailed,
                            e.to_string(),
                        ))
                    })?
                    .is_some(),
                None => {
                    let _ = fs::remove_file(&path);
                    return Err(VoiceError::new(
                        VoiceErrorCode::VoiceInterrupted,
                        "Speech interrupted",
                    ));
                }
            }
        };
        if done {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    engine
        .playback
        .lock()
        .expect("playback lock poisoned")
        .take();
    let _ = fs::remove_file(path);
    engine.set_phase(VoicePhase::Idle);
    Ok(voice_status(engine))
}

#[tauri::command]
pub fn voice_interrupt(engine: State<'_, VoiceEngine>) -> VoiceStatus {
    engine.stop();
    voice_status(engine)
}

#[cfg(test)]
pub fn is_emergency_command(text: &str) -> bool {
    let normalized = text
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation())
        .to_lowercase();
    matches!(
        normalized.as_str(),
        "stop" | "fermati" | "annulla" | "cancel" | "mute" | "muto"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_locks_capture_format_not_dynamic_native_format() {
        let mut device = AudioDevice {
            id: "usb-mic".into(),
            name: "USB mic".into(),
            is_default: true,
            kind: "input".into(),
            category: "Microphone".into(),
            node_id: Some(55),
            media_class: "Audio/Source".into(),
            sample_specification: "s24le 1ch 48000Hz".into(),
            profile: "mono-fallback".into(),
            active_port: "analog-input-mic".into(),
            muted: false,
            volume_percent: 59.0,
            available: true,
            hardware_id: "usb-card|bus|hw:0".into(),
        };
        let before = DeviceFingerprint::from(&device);
        device.sample_specification = "s16le 2ch 48000Hz".into();
        let after = DeviceFingerprint::from(&device);
        assert_eq!(before, after);
        assert_eq!(after.sample_specification, "s16le 2ch 48000Hz");
    }

    fn test_wav(rate: u32, channels: u16, samples: Vec<i16>) -> PcmWav {
        PcmWav {
            sample_rate: rate,
            channels,
            bits_per_sample: 16,
            samples,
        }
    }
    #[test]
    fn emergency_commands_only_stop() {
        assert!(is_emergency_command(" Fermati! "));
        assert!(is_emergency_command("cancel"));
        assert!(!is_emergency_command("apri firefox"));
    }
    #[test]
    fn missing_models_are_structured() {
        let result = existing_file(
            "/definitely/missing",
            VoiceErrorCode::SttModelMissing,
            "STT model",
        );
        assert!(matches!(
            result.unwrap_err().code,
            VoiceErrorCode::SttModelMissing
        ));
    }
    #[test]
    fn temp_audio_is_scoped_and_removable() {
        let path = temp_wav("test");
        fs::write(&path, b"temporary").unwrap();
        fs::remove_file(&path).unwrap();
        assert!(!path.exists());
    }
    #[test]
    fn invalid_executable_is_rejected() {
        assert!(executable("").is_err());
        assert!(executable("bad\0name").is_err());
    }
    #[test]
    fn state_machine_and_interrupt_clear_playback() {
        let engine = VoiceEngine::default();
        engine.set_phase(VoicePhase::Idle);
        assert_eq!(engine.status.lock().unwrap().phase, VoicePhase::Idle);
        engine.set_phase(VoicePhase::Listening);
        assert!(engine.status.lock().unwrap().microphone_active);
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        *engine.playback.lock().unwrap() = Some(child);
        engine.stop();
        assert!(engine.playback.lock().unwrap().is_none());
        assert_eq!(engine.status.lock().unwrap().phase, VoicePhase::Interrupted);
    }
    #[test]
    fn stereo_48khz_is_downmixed_and_resampled_to_mono_16khz() {
        let frames = 4_800usize;
        let mut stereo = Vec::with_capacity(frames * 2);
        for index in 0..frames {
            let value = ((index as f32 * 0.1).sin() * 10_000.0) as i16;
            stereo.extend_from_slice(&[value, value]);
        }
        let output = downmix_and_resample(&test_wav(48_000, 2, stereo), 16_000);
        assert_eq!(output.len(), 1_600);
        assert!(output.iter().any(|sample| *sample != 0));
    }
    #[test]
    fn stereo_44khz_is_downmixed_and_resampled_to_mono_16khz() {
        let frames = 4_410usize;
        let stereo = (0..frames)
            .flat_map(|index| {
                let value = ((index as f32 * 0.1).sin() * 10_000.0) as i16;
                [value, value]
            })
            .collect();
        let output = downmix_and_resample(&test_wav(44_100, 2, stereo), 16_000);
        assert_eq!(output.len(), 1_600);
    }
    #[test]
    fn vad_keeps_pre_roll_and_post_speech_buffer() {
        let mut samples = vec![0; 16_000];
        samples.extend(vec![8_000; 8_000]);
        samples.extend(vec![0; 16_000]);
        let (segment, metrics, speech_ms) =
            segment_and_measure(&test_wav(16_000, 1, samples), 0.012, 250, 900).unwrap();
        assert!(speech_ms >= 480);
        assert_eq!(metrics.vad_start_ms, Some(700));
        assert!(metrics.vad_end_ms.unwrap() >= 2_380);
        assert!(segment.len() >= 26_000);
    }
    #[test]
    fn audio_level_reports_low_and_clipping_without_gain_changes() {
        let (_, low, _) =
            segment_and_measure(&test_wav(16_000, 1, vec![10; 16_000]), 0.012, 250, 900).unwrap();
        assert_eq!(low.level, "LOW");
        let (_, clipped, _) = segment_and_measure(
            &test_wav(16_000, 1, vec![i16::MAX; 16_000]),
            0.012,
            250,
            900,
        )
        .unwrap();
        assert_eq!(clipped.level, "CLIPPING");
        assert!(clipped.clipped_percent > 99.0);
    }
    #[test]
    fn pcm16_wav_round_trip_preserves_format() {
        let path = temp_wav("round-trip");
        let _cleanup = TempFiles(vec![path.clone()]);
        write_pcm16_wav(&path, 16_000, 1, &[1, -2, 3, -4]).unwrap();
        let parsed = read_pcm16_wav(&path).unwrap();
        assert_eq!(parsed.sample_rate, 16_000);
        assert_eq!(parsed.channels, 1);
        assert_eq!(parsed.samples, vec![1, -2, 3, -4]);
    }
    #[test]
    fn input_level_rejects_digital_silence() {
        let path = temp_wav("no-signal");
        let _cleanup = TempFiles(vec![path.clone()]);
        write_pcm16_wav(&path, 48_000, 1, &vec![0; 48_000]).unwrap();
        let level = input_level(&path).unwrap();
        assert_eq!(level.level, "NO_SIGNAL");
        assert_eq!(level.zero_percent, 100.0);
    }
    #[test]
    fn input_level_accepts_nonzero_voice_band_signal() {
        let path = temp_wav("voice-signal");
        let _cleanup = TempFiles(vec![path.clone()]);
        let samples = (0..48_000)
            .map(|index| ((index as f32 * 0.07).sin() * 4_000.0) as i16)
            .collect::<Vec<_>>();
        write_pcm16_wav(&path, 48_000, 1, &samples).unwrap();
        let level = input_level(&path).unwrap();
        assert_eq!(level.level, "OK");
        assert!(level.zero_percent < 1.0);
    }
}
