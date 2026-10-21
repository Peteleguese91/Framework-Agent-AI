import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { useModelStore } from "./modelStore";
import { providerSettingsSchema, type ProviderSettings } from "./types";
import { localRuntimeApi, type LocalRuntimeConfig, type ModelInspection, type RuntimeCapabilities } from "./localRuntime";

function number(value: string): number { return Number(value); }
function size(bytes?: number): string { return bytes ? `${(bytes / 1024 ** 3).toFixed(2)} GiB` : "Unknown"; }

function CapabilityRows({ capabilities }: { capabilities: RuntimeCapabilities }) {
  return <div className="capability-grid wide" aria-label="Runtime capabilities">
    <strong>Capability Status</strong>
    <span>Chat <b className={capabilities.chat ? "pass" : "fail"}>{capabilities.chat ? "PASS" : "NOT TESTED / FAIL"}</b></span>
    <span>Streaming <b className={capabilities.streaming ? "pass" : "fail"}>{capabilities.streaming ? "PASS" : "NOT TESTED / FAIL"}</b></span>
    <span>Tool Calling <b className={capabilities.toolCalling ? "pass" : "fail"}>{capabilities.toolCalling ? "PASS" : "NOT TESTED / FAIL"}</b></span>
    <span>tool_choice required <b className={capabilities.toolChoiceRequired ? "pass" : "fail"}>{capabilities.toolChoiceRequired ? "PASS" : "NOT TESTED / FAIL"}</b></span>
    <span>Agent Mode <b className={capabilities.agentMode ? "pass" : "fail"}>{capabilities.agentMode ? "AVAILABLE" : "UNAVAILABLE"}</b></span>
  </div>;
}

export function ModelSettings() {
  const store = useModelStore();
  const [draft, setDraft] = useState<ProviderSettings>(store.settings);
  const [local, setLocal] = useState<LocalRuntimeConfig>(store.localConfig);
  const [validation, setValidation] = useState<string | null>(null);
  const [inspection, setInspection] = useState<ModelInspection | null>(null);
  useEffect(() => {
    if (store.settingsOpen) { setDraft(store.settings); setLocal(store.localConfig); setValidation(null); }
  }, [store.settingsOpen, store.settings, store.localConfig]);
  if (!store.settingsOpen) return null;

  const isLocal = draft.provider === "gravityforge-local";
  const change = <K extends keyof ProviderSettings>(key: K, value: ProviderSettings[K]) => setDraft((current) => ({ ...current, [key]: value }));
  const localChange = <K extends keyof LocalRuntimeConfig>(key: K, value: LocalRuntimeConfig[K]) => setLocal((current) => ({ ...current, [key]: value }));
  const chooseModel = async () => {
    const selected = await open({ multiple: false, directory: false, filters: [{ name: "GGUF models", extensions: ["gguf"] }] });
    if (typeof selected === "string") {
      localChange("modelPath", selected);
      try { setInspection(await localRuntimeApi.inspectModel(selected, local.gpuOffload)); setValidation(null); }
      catch (error) { setInspection(null); setValidation(error instanceof Error ? error.message : String(error)); }
    }
  };
  const chooseBinary = async () => {
    const selected = await open({ multiple: false, directory: false });
    if (typeof selected === "string") localChange("binaryPath", selected);
  };
  const apply = async (): Promise<boolean> => {
    const normalized = isLocal
      ? { ...draft, baseUrl: "http://127.0.0.1/v1", contextLength: local.contextLength, capabilities: { ...draft.capabilities, contextWindow: local.contextLength } }
      : { ...draft, baseUrl: draft.baseUrl || "http://127.0.0.1:11434/v1" };
    const parsed = providerSettingsSchema.safeParse(normalized);
    if (!parsed.success) { setValidation(parsed.error.issues[0]?.message ?? "Invalid model settings"); return false; }
    if (isLocal && (!local.modelPath.toLowerCase().endsWith(".gguf") || local.cpuThreads < 1 || local.parallelism < 1)) {
      setValidation("Select a GGUF model and use positive CPU threads and parallelism."); return false;
    }
    try {
      store.saveLocalConfig(local);
      await store.saveSettings(parsed.data);
      setValidation(null); return true;
    } catch (error) { setValidation(error instanceof Error ? error.message : String(error)); return false; }
  };

  return <div className="settings-backdrop" role="presentation">
    <section className="model-settings" role="dialog" aria-modal="true" aria-label="AI Runtime settings">
      <header><div><span>SETTINGS</span><h2>AI Runtime</h2></div><button onClick={store.closeSettings} aria-label="Close runtime settings">×</button></header>
      <div className="settings-grid">
        <label>Runtime<select value={isLocal ? "gravityforge-local" : "ollama"} onChange={(event) => {
          const provider = event.target.value as "gravityforge-local" | "ollama";
          setDraft((current) => ({ ...current, provider, baseUrl: provider === "ollama" ? "http://127.0.0.1:11434/v1" : "http://127.0.0.1/v1", model: "", capabilities: { ...current.capabilities, supportsTools: false, supportsStreaming: provider === "ollama" } }));
        }}><option value="gravityforge-local">GravityForge Local</option><option value="ollama">Ollama</option></select></label>

        {isLocal ? <>
          <div className="runtime-summary wide">
            <strong>GravityForge Local</strong>
            <span>Runtime source <b>{store.runtime.runtimeSource ?? store.discovery?.source ?? "Bundled"}</b></span>
            <span>Local Runtime <b>{store.discovery?.llamaCppVersion ?? "Unavailable"}</b></span>
            <span>Runtime health <b>{store.runtime.state === "READY" ? "READY" : store.discovery?.health ?? store.runtime.state}</b></span>
            {store.discovery?.warnings.map((warning) => <em key={warning}>{warning}</em>)}
          </div>
          <label>Runtime State<input readOnly value={store.runtime.state} /></label>
          <label className="wide path-input">Model GGUF<input readOnly value={local.modelPath} placeholder="Select a local .gguf file" /><button type="button" onClick={() => void chooseModel()}>Browse</button></label>
          <label>Backend<select value={local.backend} onChange={(event) => localChange("backend", event.target.value as LocalRuntimeConfig["backend"])}>
            <option value="auto">Auto</option><option value="vulkan">Vulkan</option><option value="cuda">CUDA</option><option value="cpu">CPU</option>
          </select></label>
          <label>Model size<input readOnly value={size(store.runtime.modelSize)} /></label>
          <label>Context<input type="number" min="1024" max="1048576" value={local.contextLength} onChange={(event) => localChange("contextLength", number(event.target.value))} /></label>
          <label>GPU Offload<input type="number" min="0" placeholder="Auto" value={local.gpuOffload ?? ""} onChange={(event) => localChange("gpuOffload", event.target.value === "" ? undefined : number(event.target.value))} /></label>
          <label>CPU Threads<input type="number" min="1" value={local.cpuThreads} onChange={(event) => localChange("cpuThreads", number(event.target.value))} /></label>
          <label>Parallelism<input type="number" min="1" max="16" value={local.parallelism} onChange={(event) => localChange("parallelism", number(event.target.value))} /></label>
          <label>Startup timeout ms<input type="number" min="1000" max="600000" value={local.startupTimeoutMs} onChange={(event) => localChange("startupTimeoutMs", number(event.target.value))} /></label>
          <label className="check wide"><input type="checkbox" checked={local.useCustomRuntime} onChange={(event) => localChange("useCustomRuntime", event.target.checked)} />Use custom llama-server (advanced)</label>
          {local.useCustomRuntime ? <label className="wide path-input">Custom llama-server<input readOnly value={local.binaryPath ?? ""} placeholder="Select an executable" /><button type="button" onClick={() => void chooseBinary()}>Browse</button></label> : null}
          {store.hardware ? <div className="hardware-summary wide"><strong>Detected hardware</strong><span>{store.hardware.cpu ?? "CPU unknown"}</span><span>{store.hardware.gpu ?? "No GPU detected"}</span><span>{size(store.hardware.ramBytes)} RAM · {store.hardware.architecture}</span></div> : null}
          {inspection ? <div className="model-inspection wide"><strong>Model compatibility estimate</strong><span>GGUF v{inspection.ggufVersion} · {size(inspection.sizeBytes)}</span><span>Estimated RAM: {size(inspection.estimatedRamBytes)}</span><span>{inspection.recommendation}</span><em>{inspection.warnings.join(" · ")}</em></div> : null}
          <CapabilityRows capabilities={store.runtime.capabilities} />
        </> : <>
          <label className="wide">Ollama URL<input value={draft.baseUrl} onChange={(event) => change("baseUrl", event.target.value)} /></label>
          <label className="wide">Model<select value={draft.model} onChange={(event) => change("model", event.target.value)}><option value="">Select a model</option>{store.models.map((model) => <option key={model.id} value={model.id}>{model.id}</option>)}</select></label>
        </>}

        <label>Temperature<input type="number" step="0.1" min="0" max="2" value={draft.temperature} onChange={(event) => change("temperature", number(event.target.value))} /></label>
        <label>Max output tokens<input type="number" min="1" value={draft.maxOutputTokens} onChange={(event) => change("maxOutputTokens", number(event.target.value))} /></label>
      </div>
      {!isLocal && (store.remoteEndpoint || (() => { try { return !["localhost", "127.0.0.1", "::1"].includes(new URL(draft.baseUrl).hostname); } catch { return false; } })()) ? <p className="remote-warning">Remote endpoint: prompts and tool definitions leave this device.</p> : null}
      {validation || store.error || store.runtime.error ? <p className="settings-error">{validation ?? store.error ?? store.runtime.error}</p> : null}
      <footer>
        <span className={`connection-state ${store.connection}`}>{isLocal ? store.runtime.state : store.connection}</span>
        {isLocal ? <>
          <button onClick={() => void store.unloadRuntime()} disabled={store.runtime.state === "STOPPED" || store.runtime.state === "STOPPING"}>Unload</button>
          <button onClick={() => void store.restartRuntime()} disabled={store.runtime.state !== "READY"}>Restart</button>
          <button onClick={() => void store.testCapabilities()} disabled={store.runtime.state !== "READY"}>Test Capabilities</button>
          <button className="primary-button" onClick={() => void (async () => { if (await apply()) await store.loadRuntime(); })()} disabled={["STARTING", "LOADING_MODEL", "STOPPING"].includes(store.runtime.state)}>Load</button>
        </> : <>
          <button onClick={() => void (async () => { if (await apply()) await store.refreshModels(); })()} disabled={store.connection === "connecting"}>Refresh Models</button>
          <button onClick={() => void (async () => { if (await apply()) await store.testConnection(); })()} disabled={store.connection === "connecting"}>Test Connection</button>
        </>}
        <button onClick={() => void (async () => { if (await apply()) store.closeSettings(); })()}>Save</button>
      </footer>
    </section>
  </div>;
}
