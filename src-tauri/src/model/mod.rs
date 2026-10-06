use futures_util::StreamExt;
use reqwest::{Client, StatusCode, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::watch;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    pub provider: String,
    pub base_url: String,
    pub model: Option<String>,
    pub timeout_ms: u64,
    pub retry_once: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub function: ToolFunctionCall,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ToolFunctionCall {
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub risk_level: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub config: ProviderConfig,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: Option<Value>,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: Option<u32>,
    pub repetition_penalty: Option<f32>,
    pub max_tokens: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub owned_by: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TokenUsage {
    #[serde(default)]
    pub prompt_tokens: u64,
    #[serde(default)]
    pub completion_tokens: u64,
    #[serde(default)]
    pub total_tokens: u64,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelErrorCode {
    InvalidConfig,
    ConnectionFailed,
    Timeout,
    HttpError,
    InvalidResponse,
    ModelUnavailable,
    ContextOverflow,
    MalformedToolCall,
    StreamInterrupted,
    Cancelled,
    Internal,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelError {
    pub code: ModelErrorCode,
    pub message: String,
    pub status: Option<u16>,
    pub retryable: bool,
}

impl ModelError {
    fn new(code: ModelErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            status: None,
            retryable: false,
        }
    }

    fn from_reqwest(error: reqwest::Error) -> Self {
        if error.is_timeout() {
            Self {
                code: ModelErrorCode::Timeout,
                message: "Model request timed out".into(),
                status: None,
                retryable: true,
            }
        } else if error.is_connect() {
            Self {
                code: ModelErrorCode::ConnectionFailed,
                message: "Cannot connect to the model provider".into(),
                status: None,
                retryable: true,
            }
        } else {
            Self {
                code: ModelErrorCode::StreamInterrupted,
                message: error.to_string(),
                status: error.status().map(|value| value.as_u16()),
                retryable: true,
            }
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatResponse {
    pub message: ChatMessage,
    pub usage: Option<TokenUsage>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamDeltaEvent {
    request_id: String,
    content: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamCompleteEvent {
    request_id: String,
    content: String,
    tool_calls: Vec<ToolCall>,
    usage: Option<TokenUsage>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct StreamErrorEvent {
    request_id: String,
    error: ModelError,
}

#[derive(Clone)]
pub struct ModelState {
    inner: Arc<ModelStateInner>,
}

struct ModelStateInner {
    api_key: Mutex<Option<String>>,
    cancellations: Mutex<HashMap<String, watch::Sender<bool>>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl Default for ModelState {
    fn default() -> Self {
        Self {
            inner: Arc::new(ModelStateInner {
                api_key: Mutex::new(None),
                cancellations: Mutex::new(HashMap::new()),
                next_id: std::sync::atomic::AtomicU64::new(1),
            }),
        }
    }
}

fn endpoint(config: &ProviderConfig, resource: &str) -> Result<Url, ModelError> {
    if config.timeout_ms == 0 || config.timeout_ms > 600_000 {
        return Err(ModelError::new(
            ModelErrorCode::InvalidConfig,
            "Timeout must be between 1 and 600000 ms",
        ));
    }
    let mut base = Url::parse(&config.base_url)
        .map_err(|_| ModelError::new(ModelErrorCode::InvalidConfig, "Base URL is invalid"))?;
    if !matches!(base.scheme(), "http" | "https")
        || !base.username().is_empty()
        || base.password().is_some()
    {
        return Err(ModelError::new(
            ModelErrorCode::InvalidConfig,
            "Base URL must use HTTP(S) without embedded credentials",
        ));
    }
    if !base.path().ends_with('/') {
        base.set_path(&format!("{}/", base.path()));
    }
    base.join(resource).map_err(|_| {
        ModelError::new(
            ModelErrorCode::InvalidConfig,
            "Provider endpoint cannot be constructed",
        )
    })
}

fn client(config: &ProviderConfig) -> Result<Client, ModelError> {
    Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_millis(config.timeout_ms))
        .build()
        .map_err(ModelError::from_reqwest)
}

fn api_key(state: &ModelState) -> Result<Option<String>, ModelError> {
    state
        .inner
        .api_key
        .lock()
        .map(|value| value.clone())
        .map_err(|_| ModelError::new(ModelErrorCode::Internal, "API key state is unavailable"))
}

fn with_auth(builder: reqwest::RequestBuilder, key: Option<&str>) -> reqwest::RequestBuilder {
    match key.filter(|value| !value.is_empty()) {
        Some(value) => builder.bearer_auth(value),
        None => builder,
    }
}

async fn map_http(response: reqwest::Response) -> Result<reqwest::Response, ModelError> {
    let status = response.status();
    if status.is_success() {
        return Ok(response);
    }
    let body = response.text().await.unwrap_or_default();
    Err(classify_http_error(status, &body))
}

fn classify_http_error(status: StatusCode, body: &str) -> ModelError {
    let lower = body.to_lowercase();
    let code = if status == StatusCode::NOT_FOUND && lower.contains("model") {
        ModelErrorCode::ModelUnavailable
    } else if status == StatusCode::BAD_REQUEST
        && (lower.contains("context") || lower.contains("token limit"))
    {
        ModelErrorCode::ContextOverflow
    } else {
        ModelErrorCode::HttpError
    };
    let message = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("Provider returned HTTP {}", status.as_u16()));
    ModelError {
        code,
        message,
        status: Some(status.as_u16()),
        retryable: status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS,
    }
}

fn chat_body(request: &ChatRequest, stream: bool) -> Result<Value, ModelError> {
    let model = request
        .config
        .model
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ModelError::new(ModelErrorCode::InvalidConfig, "Select a model first"))?;
    if request.messages.is_empty() {
        return Err(ModelError::new(
            ModelErrorCode::InvalidConfig,
            "At least one chat message is required",
        ));
    }
    if request.tools.iter().any(|tool| {
        !matches!(
            tool.risk_level.as_str(),
            "READ" | "WRITE" | "DESTRUCTIVE" | "DANGEROUS"
        )
    }) {
        return Err(ModelError::new(
            ModelErrorCode::InvalidConfig,
            "Tool risk level is invalid",
        ));
    }
    let messages: Vec<Value> = request
        .messages
        .iter()
        .map(|message| {
            let mut value = json!({ "role": &message.role, "content": &message.content });
            if let Some(name) = &message.name {
                value["name"] = json!(name);
            }
            if let Some(id) = &message.tool_call_id {
                value["tool_call_id"] = json!(id);
            }
            if !message.tool_calls.is_empty() {
                value["tool_calls"] =
                    Value::Array(message.tool_calls.iter().map(outbound_tool_call).collect());
            }
            value
        })
        .collect();
    let tools: Vec<Value> = request.tools.iter().map(|tool| json!({
        "type": "function",
        "function": { "name": tool.name, "description": tool.description, "parameters": tool.parameters }
    })).collect();
    let mut body = json!({
        "model": model,
        "messages": messages,
        "temperature": request.temperature,
        "top_p": request.top_p,
        "max_tokens": request.max_tokens,
        "stream": stream,
    });
    if let Some(top_k) = request.top_k {
        body["top_k"] = json!(top_k);
    }
    if let Some(penalty) = request.repetition_penalty {
        body["repetition_penalty"] = json!(penalty);
    }
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
        body["tool_choice"] = request.tool_choice.clone().unwrap_or_else(|| json!("auto"));
    }
    if stream {
        body["stream_options"] = json!({ "include_usage": true });
    }
    Ok(body)
}

fn outbound_tool_call(call: &ToolCall) -> Value {
    json!({ "id": call.id, "type": call.kind, "function": { "name": call.function.name, "arguments": call.function.arguments.to_string() } })
}

fn parse_tool_calls(value: Option<&Value>) -> Result<Vec<ToolCall>, ModelError> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|call| {
            let kind = call.get("type").and_then(Value::as_str).unwrap_or_default();
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let name = call
                .pointer("/function/name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let raw = call
                .pointer("/function/arguments")
                .and_then(Value::as_str)
                .unwrap_or("{}");
            if id.is_empty() || name.is_empty() || kind != "function" {
                return Err(ModelError::new(
                    ModelErrorCode::MalformedToolCall,
                    "Tool call must contain an id and a function",
                ));
            }
            let arguments = serde_json::from_str(raw).map_err(|_| {
                ModelError::new(
                    ModelErrorCode::MalformedToolCall,
                    format!("Tool call {name} has invalid JSON arguments"),
                )
            })?;
            Ok(ToolCall {
                id,
                kind: "function".into(),
                function: ToolFunctionCall { name, arguments },
            })
        })
        .collect()
}

#[derive(Default)]
struct PendingToolCall {
    id: String,
    kind: String,
    name: String,
    arguments: String,
}

fn merge_tool_deltas(pending: &mut Vec<PendingToolCall>, value: Option<&Value>) {
    for call in value.and_then(Value::as_array).into_iter().flatten() {
        let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
        while pending.len() <= index {
            pending.push(PendingToolCall::default());
        }
        if let Some(id) = call.get("id").and_then(Value::as_str) {
            pending[index].id.push_str(id);
        }
        if let Some(kind) = call.get("type").and_then(Value::as_str) {
            pending[index].kind.push_str(kind);
        }
        if let Some(name) = call.pointer("/function/name").and_then(Value::as_str) {
            pending[index].name.push_str(name);
        }
        if let Some(arguments) = call.pointer("/function/arguments").and_then(Value::as_str) {
            pending[index].arguments.push_str(arguments);
        }
    }
}

fn finish_tool_calls(pending: Vec<PendingToolCall>) -> Result<Vec<ToolCall>, ModelError> {
    pending
        .into_iter()
        .map(|call| {
            if call.id.is_empty() || call.name.is_empty() || call.kind != "function" {
                return Err(ModelError::new(
                    ModelErrorCode::MalformedToolCall,
                    "Streamed tool call must contain an id and a function",
                ));
            }
            let arguments = serde_json::from_str(&call.arguments).map_err(|_| {
                ModelError::new(
                    ModelErrorCode::MalformedToolCall,
                    format!("Tool call {} has invalid JSON arguments", call.name),
                )
            })?;
            Ok(ToolCall {
                id: call.id,
                kind: "function".into(),
                function: ToolFunctionCall {
                    name: call.name,
                    arguments,
                },
            })
        })
        .collect()
}

fn validate_received_tools(
    calls: Vec<ToolCall>,
    definitions: &[ToolDefinition],
) -> Result<Vec<ToolCall>, ModelError> {
    for call in &calls {
        let definition = definitions
            .iter()
            .find(|tool| tool.name == call.function.name)
            .ok_or_else(|| {
                ModelError::new(
                    ModelErrorCode::MalformedToolCall,
                    format!("Model requested unknown tool {}", call.function.name),
                )
            })?;
        validate_schema_value(&call.function.arguments, &definition.parameters, "$arguments")
            .map_err(|detail| {
                ModelError::new(
                    ModelErrorCode::MalformedToolCall,
                    format!("Tool call {} is invalid: {detail}", call.function.name),
                )
            })?;
    }
    Ok(calls)
}

fn validate_schema_value(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    match schema.get("type").and_then(Value::as_str) {
        Some("object") => {
            let object = value
                .as_object()
                .ok_or_else(|| format!("{path} must be an object"))?;
            if let Some(required) = schema.get("required").and_then(Value::as_array) {
                for name in required.iter().filter_map(Value::as_str) {
                    if !object.contains_key(name) {
                        return Err(format!("{path}.{name} is required"));
                    }
                }
            }
            let properties = schema.get("properties").and_then(Value::as_object);
            for (name, current) in object {
                if let Some(property_schema) = properties.and_then(|items| items.get(name)) {
                    validate_schema_value(current, property_schema, &format!("{path}.{name}"))?;
                } else if schema.get("additionalProperties") == Some(&Value::Bool(false)) {
                    return Err(format!("{path}.{name} is not allowed"));
                }
            }
        }
        Some("array") => {
            let items = value
                .as_array()
                .ok_or_else(|| format!("{path} must be an array"))?;
            if let Some(item_schema) = schema.get("items") {
                for (index, item) in items.iter().enumerate() {
                    validate_schema_value(item, item_schema, &format!("{path}[{index}]"))?;
                }
            }
        }
        Some("string") if !value.is_string() => return Err(format!("{path} must be a string")),
        Some("number") if !value.is_number() => return Err(format!("{path} must be a number")),
        Some("integer") if value.as_i64().is_none() && value.as_u64().is_none() => {
            return Err(format!("{path} must be an integer"));
        }
        Some("boolean") if !value.is_boolean() => {
            return Err(format!("{path} must be a boolean"));
        }
        Some("null") if !value.is_null() => return Err(format!("{path} must be null")),
        _ => {}
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        if !values.contains(value) {
            return Err(format!("{path} is not an allowed value"));
        }
    }
    Ok(())
}

#[derive(Default)]
struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, ModelError> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        while let Some((position, delimiter)) = find_sse_delimiter(&self.buffer) {
            let raw: Vec<u8> = self.buffer.drain(..position).collect();
            self.buffer.drain(..delimiter);
            let text = String::from_utf8(raw).map_err(|_| {
                ModelError::new(
                    ModelErrorCode::InvalidResponse,
                    "Provider stream is not UTF-8",
                )
            })?;
            let data = text
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>()
                .join("\n");
            if !data.is_empty() {
                frames.push(data);
            }
        }
        Ok(frames)
    }
}

fn find_sse_delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    buffer
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            buffer
                .windows(2)
                .position(|part| part == b"\n\n")
                .map(|position| (position, 2))
        })
}

async fn post_chat(
    request: &ChatRequest,
    key: Option<&str>,
    stream: bool,
) -> Result<reqwest::Response, ModelError> {
    let url = endpoint(&request.config, "chat/completions")?;
    let body = chat_body(request, stream)?;
    let client = client(&request.config)?;
    let attempts = if request.config.retry_once { 2 } else { 1 };
    let mut last = None;
    for attempt in 0..attempts {
        let sent = with_auth(client.post(url.clone()).json(&body), key)
            .send()
            .await;
        match sent {
            Ok(response) => match map_http(response).await {
                Ok(response) => return Ok(response),
                Err(error) if error.retryable && attempt + 1 < attempts => last = Some(error),
                Err(error) => return Err(error),
            },
            Err(error) => {
                let error = ModelError::from_reqwest(error);
                if error.retryable && attempt + 1 < attempts {
                    last = Some(error);
                } else {
                    return Err(error);
                }
            }
        }
    }
    Err(last.unwrap_or_else(|| ModelError::new(ModelErrorCode::Internal, "Model request failed")))
}

async fn stream_chat(
    app: AppHandle,
    request_id: String,
    request: ChatRequest,
    key: Option<String>,
    mut cancelled: watch::Receiver<bool>,
) -> Result<StreamCompleteEvent, ModelError> {
    let response = tokio::select! {
        result = post_chat(&request, key.as_deref(), true) => result?,
        _ = cancelled.changed() => return Err(ModelError::new(ModelErrorCode::Cancelled, "Generation cancelled")),
    };
    let mut stream = response.bytes_stream();
    let mut decoder = SseDecoder::default();
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    let mut usage = None;
    'stream: loop {
        let next = tokio::select! {
            next = stream.next() => next,
            _ = cancelled.changed() => return Err(ModelError::new(ModelErrorCode::Cancelled, "Generation cancelled")),
        };
        let Some(chunk) = next else { break };
        let chunk = chunk.map_err(ModelError::from_reqwest)?;
        for frame in decoder.push(&chunk)? {
            if frame == "[DONE]" {
                break 'stream;
            }
            let value: Value = serde_json::from_str(&frame).map_err(|_| {
                ModelError::new(
                    ModelErrorCode::InvalidResponse,
                    "Provider sent malformed stream JSON",
                )
            })?;
            if let Some(current) = value.get("usage") {
                usage = serde_json::from_value(current.clone()).ok();
            }
            let delta = value.pointer("/choices/0/delta");
            if let Some(text) = delta
                .and_then(|value| value.get("content"))
                .and_then(Value::as_str)
            {
                content.push_str(text);
                let _ = app.emit(
                    "model-stream-delta",
                    StreamDeltaEvent {
                        request_id: request_id.clone(),
                        content: text.to_owned(),
                    },
                );
            }
            merge_tool_deltas(
                &mut tool_calls,
                delta.and_then(|value| value.get("tool_calls")),
            );
        }
    }
    let tool_calls = validate_received_tools(finish_tool_calls(tool_calls)?, &request.tools)?;
    Ok(StreamCompleteEvent {
        request_id,
        content,
        tool_calls,
        usage,
    })
}

fn log_model(
    action: &str,
    config: &ProviderConfig,
    result: &Result<(), ModelError>,
    started: Instant,
    usage: Option<&TokenUsage>,
) {
    eprintln!(
        "{}",
        json!({
            "category": "MODEL", "phase": "end", "action": action, "provider": config.provider,
            "model": config.model, "outcome": if result.is_ok() { "ok" } else { "error" },
            "errorType": result.as_ref().err().map(|error| error.code),
            "durationMs": started.elapsed().as_millis(), "usage": usage,
        })
    );
}

fn log_model_start(action: &str, config: &ProviderConfig) {
    eprintln!(
        "{}",
        json!({
            "category": "MODEL", "phase": "start", "action": action,
            "provider": config.provider, "model": config.model,
        })
    );
}

#[tauri::command]
pub fn model_set_api_key(
    api_key: Option<String>,
    state: State<'_, ModelState>,
) -> Result<(), ModelError> {
    let cleaned = api_key
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    *state
        .inner
        .api_key
        .lock()
        .map_err(|_| ModelError::new(ModelErrorCode::Internal, "API key state is unavailable"))? =
        cleaned;
    Ok(())
}

fn parse_models(value: &Value) -> Result<Vec<ModelInfo>, ModelError> {
    value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ModelError::new(
                ModelErrorCode::InvalidResponse,
                "Models response is missing data",
            )
        })?
        .iter()
        .map(|model| {
            let id = model
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    ModelError::new(
                        ModelErrorCode::InvalidResponse,
                        "Model entry is missing id",
                    )
                })?;
            Ok(ModelInfo {
                id: id.to_owned(),
                owned_by: model
                    .get("owned_by")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

#[tauri::command]
pub async fn model_list_models(
    config: ProviderConfig,
    state: State<'_, ModelState>,
) -> Result<Vec<ModelInfo>, ModelError> {
    let started = Instant::now();
    log_model_start("list_models", &config);
    let result = async {
        let key = api_key(&state)?;
        let response = with_auth(
            client(&config)?.get(endpoint(&config, "models")?),
            key.as_deref(),
        )
        .send()
        .await
        .map_err(ModelError::from_reqwest)?;
        let value: Value = map_http(response).await?.json().await.map_err(|_| {
            ModelError::new(
                ModelErrorCode::InvalidResponse,
                "Models response is not valid JSON",
            )
        })?;
        parse_models(&value)
    }
    .await;
    let log_result = result
        .as_ref()
        .map(|_| ())
        .map_err(|error| (*error).clone());
    log_model("list_models", &config, &log_result, started, None);
    result
}

#[tauri::command]
pub async fn model_health_check(
    config: ProviderConfig,
    state: State<'_, ModelState>,
) -> Result<(), ModelError> {
    model_list_models(config, state).await.map(|_| ())
}

#[tauri::command]
pub async fn model_chat(
    request: ChatRequest,
    state: State<'_, ModelState>,
) -> Result<ChatResponse, ModelError> {
    let started = Instant::now();
    log_model_start("chat", &request.config);
    let result = async {
        let key = api_key(&state)?;
        let response = post_chat(&request, key.as_deref(), false).await?;
        let value: Value = response.json().await.map_err(|_| {
            ModelError::new(
                ModelErrorCode::InvalidResponse,
                "Chat response is not valid JSON",
            )
        })?;
        let message = value.pointer("/choices/0/message").ok_or_else(|| {
            ModelError::new(
                ModelErrorCode::InvalidResponse,
                "Chat response is missing assistant message",
            )
        })?;
        let tool_calls = validate_received_tools(
            parse_tool_calls(message.get("tool_calls"))?,
            &request.tools,
        )?;
        Ok(ChatResponse {
            message: ChatMessage {
                role: MessageRole::Assistant,
                content: message
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                name: None,
                tool_call_id: None,
                tool_calls,
            },
            usage: value
                .get("usage")
                .cloned()
                .and_then(|current| serde_json::from_value(current).ok()),
        })
    }
    .await;
    let log_result = result
        .as_ref()
        .map(|_| ())
        .map_err(|error| (*error).clone());
    log_model(
        "chat",
        &request.config,
        &log_result,
        started,
        result
            .as_ref()
            .ok()
            .and_then(|response| response.usage.as_ref()),
    );
    result
}

#[tauri::command]
pub fn model_stream_chat(
    request: ChatRequest,
    app: AppHandle,
    state: State<'_, ModelState>,
) -> Result<String, ModelError> {
    let started = Instant::now();
    log_model_start("stream_chat", &request.config);
    if let Err(error) = chat_body(&request, true) {
        log_model(
            "stream_chat",
            &request.config,
            &Err(error.clone()),
            started,
            None,
        );
        return Err(error);
    }
    let key = api_key(&state)?;
    let request_id = state
        .inner
        .next_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        .to_string();
    let (sender, receiver) = watch::channel(false);
    state
        .inner
        .cancellations
        .lock()
        .map_err(|_| {
            ModelError::new(
                ModelErrorCode::Internal,
                "Cancellation state is unavailable",
            )
        })?
        .insert(request_id.clone(), sender);
    let owned_state = state.inner.clone();
    let owned_id = request_id.clone();
    let config = request.config.clone();
    tauri::async_runtime::spawn(async move {
        let started = Instant::now();
        let result = stream_chat(app.clone(), owned_id.clone(), request, key, receiver).await;
        match &result {
            Ok(event) => {
                let _ = app.emit("model-stream-complete", event.clone());
                log_model(
                    "stream_chat",
                    &config,
                    &Ok(()),
                    started,
                    event.usage.as_ref(),
                );
            }
            Err(error) => {
                let _ = app.emit(
                    "model-stream-error",
                    StreamErrorEvent {
                        request_id: owned_id.clone(),
                        error: error.clone(),
                    },
                );
                log_model("stream_chat", &config, &Err(error.clone()), started, None);
            }
        }
        if let Ok(mut cancellations) = owned_state.cancellations.lock() {
            cancellations.remove(&owned_id);
        }
    });
    Ok(request_id)
}

#[tauri::command]
pub fn model_cancel(request_id: String, state: State<'_, ModelState>) -> Result<(), ModelError> {
    let sender = state
        .inner
        .cancellations
        .lock()
        .map_err(|_| {
            ModelError::new(
                ModelErrorCode::Internal,
                "Cancellation state is unavailable",
            )
        })?
        .remove(&request_id)
        .ok_or_else(|| {
            ModelError::new(ModelErrorCode::InvalidConfig, "Generation is not active")
        })?;
    sender
        .send(true)
        .map_err(|_| ModelError::new(ModelErrorCode::Internal, "Generation already ended"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn validates_endpoint_and_maps_io_categories() {
        let config = ProviderConfig {
            provider: "lm-studio".into(),
            base_url: "http://localhost:1234/v1".into(),
            model: None,
            timeout_ms: 30_000,
            retry_once: true,
        };
        assert_eq!(
            endpoint(&config, "models").unwrap().as_str(),
            "http://localhost:1234/v1/models"
        );
        let mut invalid = config.clone();
        invalid.base_url = "file:///tmp".into();
        assert_eq!(
            endpoint(&invalid, "models").unwrap_err().code,
            ModelErrorCode::InvalidConfig
        );
    }

    #[test]
    fn parses_fragmented_sse_and_crlf() {
        let mut decoder = SseDecoder::default();
        assert!(decoder.push(b"data: {\"a\":").unwrap().is_empty());
        assert_eq!(
            decoder.push(b"1}\r\n\r\ndata: [DONE]\n\n").unwrap(),
            vec!["{\"a\":1}", "[DONE]"]
        );
    }

    #[test]
    fn merges_and_validates_tool_call_deltas() {
        let mut pending = Vec::new();
        merge_tool_deltas(
            &mut pending,
            Some(
                &json!([{"index":0,"id":"call_","type":"function","function":{"name":"square_","arguments":"{\"value\":"}}]),
            ),
        );
        merge_tool_deltas(
            &mut pending,
            Some(&json!([{"index":0,"id":"1","function":{"name":"number","arguments":"12}"}}])),
        );
        let calls = finish_tool_calls(pending).unwrap();
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].function.name, "square_number");
        assert_eq!(calls[0].function.arguments, json!({"value":12}));
    }

    #[test]
    fn rejects_malformed_tool_arguments() {
        let error = parse_tool_calls(Some(
            &json!([{"id":"x","function":{"name":"bad","arguments":"{"}}]),
        ))
        .unwrap_err();
        assert_eq!(error.code, ModelErrorCode::MalformedToolCall);
    }

    #[test]
    fn parses_openai_model_discovery() {
        let models = parse_models(&json!({
            "object": "list",
            "data": [{"id": "qwen3-coder", "owned_by": "local"}]
        }))
        .unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "qwen3-coder");
        assert_eq!(models[0].owned_by.as_deref(), Some("local"));
        assert_eq!(
            parse_models(&json!({"data": [{}]})).unwrap_err().code,
            ModelErrorCode::InvalidResponse
        );
    }

    #[test]
    fn classifies_provider_errors() {
        let overflow = classify_http_error(
            StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"context length exceeded"}}"#,
        );
        assert_eq!(overflow.code, ModelErrorCode::ContextOverflow);
        assert_eq!(overflow.message, "context length exceeded");
        assert!(!overflow.retryable);

        let busy = classify_http_error(StatusCode::TOO_MANY_REQUESTS, "");
        assert_eq!(busy.code, ModelErrorCode::HttpError);
        assert!(busy.retryable);
    }

    #[test]
    fn validates_tool_name_and_json_schema() {
        let definition = ToolDefinition {
            name: "square_number".into(),
            description: "Square a number".into(),
            parameters: json!({
                "type": "object",
                "properties": {"value": {"type": "number"}},
                "required": ["value"],
                "additionalProperties": false
            }),
            risk_level: "READ".into(),
        };
        let valid = ToolCall {
            id: "call-1".into(),
            kind: "function".into(),
            function: ToolFunctionCall {
                name: "square_number".into(),
                arguments: json!({"value": 12}),
            },
        };
        assert!(validate_received_tools(vec![valid.clone()], &[definition.clone()]).is_ok());

        let mut invalid = valid;
        invalid.function.arguments = json!({"value": "twelve"});
        assert_eq!(
            validate_received_tools(vec![invalid], &[definition])
                .unwrap_err()
                .code,
            ModelErrorCode::MalformedToolCall
        );
    }

    #[test]
    fn calls_an_openai_compatible_mock_server() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let server = thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut chunk = [0_u8; 4096];
                loop {
                    let count = socket.read(&mut chunk).unwrap();
                    if count == 0 {
                        break;
                    }
                    request.extend_from_slice(&chunk[..count]);
                    if let Some(header_end) =
                        request.windows(4).position(|part| part == b"\r\n\r\n")
                    {
                        let headers = String::from_utf8_lossy(&request[..header_end]);
                        let content_length = headers
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length:")
                                    .and_then(|value| value.trim().parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if request.len() >= header_end + 4 + content_length {
                            break;
                        }
                    }
                }
                let body = r#"{"choices":[{"message":{"role":"assistant","content":"hello"}}],"usage":{"prompt_tokens":2,"completion_tokens":1,"total_tokens":3}}"#;
                write!(
                    socket,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .unwrap();
                String::from_utf8(request).unwrap()
            });
            let request = ChatRequest {
                config: ProviderConfig {
                    provider: "openai-compatible".into(),
                    base_url: format!("http://{address}/v1"),
                    model: Some("mock-model".into()),
                    timeout_ms: 5_000,
                    retry_once: false,
                },
                messages: vec![ChatMessage {
                    role: MessageRole::User,
                    content: Some("hello".into()),
                    name: None,
                    tool_call_id: None,
                    tool_calls: Vec::new(),
                }],
                tools: Vec::new(),
                tool_choice: None,
                temperature: 0.7,
                top_p: 0.8,
                top_k: Some(20),
                repetition_penalty: Some(1.05),
                max_tokens: 128,
            };
            let response: Value = post_chat(&request, None, false)
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(
                response.pointer("/choices/0/message/content"),
                Some(&json!("hello"))
            );
            let raw_request = server.join().unwrap();
            assert!(raw_request.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(raw_request.contains("\"model\":\"mock-model\""));
        });
    }
}
