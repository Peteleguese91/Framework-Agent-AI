import { useEffect, useState } from "react";
import { useModelStore } from "./modelStore";
import { providerSettingsSchema, type ProviderSettings } from "./types";

function number(value: string): number { return Number(value); }

export function ModelSettings() {
  const store = useModelStore();
  const [draft, setDraft] = useState<ProviderSettings>(store.settings);
  const [apiKey, setApiKey] = useState("");
  const [validation, setValidation] = useState<string | null>(null);
  useEffect(() => { if (store.settingsOpen) { setDraft(store.settings); setApiKey(""); setValidation(null); } }, [store.settingsOpen, store.settings]);
  if (!store.settingsOpen) return null;

  const change = <K extends keyof ProviderSettings>(key: K, value: ProviderSettings[K]) => setDraft((current) => ({ ...current, [key]: value }));
  const apply = async (): Promise<boolean> => {
    const parsed = providerSettingsSchema.safeParse(draft);
    if (!parsed.success) { setValidation(parsed.error.issues[0]?.message ?? "Invalid model settings"); return false; }
    try { await store.saveSettings(parsed.data, apiKey || undefined); setValidation(null); return true; }
    catch (error) { setValidation(error instanceof Error ? error.message : String(error)); return false; }
  };

  return <div className="settings-backdrop" role="presentation">
    <section className="model-settings" role="dialog" aria-modal="true" aria-label="Model settings">
      <header><div><span>SETTINGS</span><h2>Models</h2></div><button onClick={store.closeSettings} aria-label="Close model settings">×</button></header>
      <div className="settings-grid">
        <label>Provider<select value={draft.provider} onChange={(event) => change("provider", event.target.value as ProviderSettings["provider"])}><option value="lm-studio">LM Studio</option><option value="openai-compatible">OpenAI compatible</option></select></label>
        <label className="wide">Base URL<input value={draft.baseUrl} onChange={(event) => change("baseUrl", event.target.value)} /></label>
        <label className="wide">API key <small>session only</small><input type="password" value={apiKey} autoComplete="off" placeholder="Not stored" onChange={(event) => setApiKey(event.target.value)} /></label>
        <label className="wide">Model<select value={draft.model} onChange={(event) => change("model", event.target.value)}><option value="">Select a model</option>{store.models.map((model) => <option key={model.id} value={model.id}>{model.id}</option>)}</select></label>
        <label>Temperature<input type="number" step="0.1" min="0" max="2" value={draft.temperature} onChange={(event) => change("temperature", number(event.target.value))} /></label>
        <label>Top P<input type="number" step="0.05" min="0" max="1" value={draft.topP} onChange={(event) => change("topP", number(event.target.value))} /></label>
        <label>Top K<input type="number" min="0" value={draft.topK ?? ""} onChange={(event) => change("topK", event.target.value === "" ? null : number(event.target.value))} /></label>
        <label>Repetition penalty<input type="number" step="0.05" min="0.1" max="2" value={draft.repetitionPenalty ?? ""} onChange={(event) => change("repetitionPenalty", event.target.value === "" ? null : number(event.target.value))} /></label>
        <label>Context length<input type="number" min="1024" value={draft.contextLength} onChange={(event) => { const value = number(event.target.value); setDraft((current) => ({ ...current, contextLength: value, capabilities: { ...current.capabilities, contextWindow: value } })); }} /></label>
        <label>Max output tokens<input type="number" min="1" value={draft.maxOutputTokens} onChange={(event) => { const value = number(event.target.value); setDraft((current) => ({ ...current, maxOutputTokens: value, capabilities: { ...current.capabilities, maxOutputTokens: value } })); }} /></label>
        <label>Timeout ms<input type="number" min="1" value={draft.timeoutMs} onChange={(event) => change("timeoutMs", number(event.target.value))} /></label>
        <label className="check"><input type="checkbox" checked={draft.retryOnce} onChange={(event) => change("retryOnce", event.target.checked)} /> Retry one temporary failure</label>
        <fieldset className="wide"><legend>Capability overrides</legend>
          <label className="check"><input type="checkbox" checked={draft.capabilities.supportsTools} onChange={(event) => change("capabilities", { ...draft.capabilities, supportsTools: event.target.checked })} /> Tools</label>
          <label className="check"><input type="checkbox" checked={draft.capabilities.supportsStreaming} onChange={(event) => change("capabilities", { ...draft.capabilities, supportsStreaming: event.target.checked })} /> Streaming</label>
          <label className="check"><input type="checkbox" checked={draft.capabilities.supportsVision} onChange={(event) => change("capabilities", { ...draft.capabilities, supportsVision: event.target.checked })} /> Vision</label>
          <label className="check"><input type="checkbox" checked={draft.capabilities.supportsReasoning} onChange={(event) => change("capabilities", { ...draft.capabilities, supportsReasoning: event.target.checked })} /> Reasoning</label>
        </fieldset>
      </div>
      {store.remoteEndpoint || (() => { try { const host = new URL(draft.baseUrl).hostname; return !["localhost", "127.0.0.1", "::1"].includes(host); } catch { return false; } })() ? <p className="remote-warning">Remote endpoint: prompts and tool definitions leave this device.</p> : null}
      {validation || store.error ? <p className="settings-error">{validation ?? store.error}</p> : null}
      <footer>
        <span className={`connection-state ${store.connection}`}>{store.connection}</span>
        <button onClick={() => void (async () => { if (await apply()) await store.refreshModels(); })()} disabled={store.connection === "connecting"}>Refresh Models</button>
        <button onClick={() => void (async () => { if (await apply()) await store.testConnection(); })()} disabled={store.connection === "connecting"}>Test Connection</button>
        <button className="primary-button" onClick={() => void (async () => { if (await apply()) store.closeSettings(); })()}>Save</button>
      </footer>
    </section>
  </div>;
}
