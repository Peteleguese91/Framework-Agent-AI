import { invoke } from "@tauri-apps/api/core";

export type RuntimeState = "STOPPED" | "STARTING" | "LOADING_MODEL" | "READY" | "FAILED" | "STOPPING";
export type RuntimeBackend = "auto" | "vulkan" | "cuda" | "cpu";

export interface RuntimeCapabilities {
  chat: boolean; streaming: boolean; toolCalling: boolean; toolChoiceRequired: boolean;
  agentMode: boolean; testedAt?: number; detail?: string;
}
export interface LocalRuntimeConfig {
  modelPath: string; binaryPath?: string; useCustomRuntime: boolean; backend: RuntimeBackend; contextLength: number;
  gpuOffload?: number; cpuThreads: number; parallelism: number; startupTimeoutMs: number;
}
export interface RuntimeStatus {
  state: RuntimeState; modelPath?: string; modelSize?: number; backend?: RuntimeBackend;
  runtimeSource?: string; runtimeVersion?: string;
  port?: number; pid?: number; error?: string; logs: string[]; capabilities: RuntimeCapabilities;
}
export interface HardwareProfile {
  os: string; architecture: string; cpu?: string; cpuThreads: number; ramBytes?: number;
  gpu?: string; vramBytes?: number; availableBackends: RuntimeBackend[]; recommendedBackend: RuntimeBackend;
}
export interface RuntimeDiscovery {
  source: string; runtimeVersion?: string; llamaCppVersion?: string; llamaCppCommit?: string; buildDate?: string;
  bundledBackends: RuntimeBackend[]; availableBackends: RuntimeBackend[]; recommendedBackend: RuntimeBackend;
  health: string; warnings: string[];
}
export interface ModelInspection {
  path: string; sizeBytes: number; validGguf: boolean; ggufVersion?: number; tensorCount?: number; metadataCount?: number;
  estimatedRamBytes: number; estimatedVramBytes?: number; recommendation: string; warnings: string[];
}

export const EMPTY_CAPABILITIES: RuntimeCapabilities = {
  chat: false, streaming: false, toolCalling: false, toolChoiceRequired: false, agentMode: false,
};
export const STOPPED_RUNTIME: RuntimeStatus = {
  state: "STOPPED", logs: [], capabilities: EMPTY_CAPABILITIES,
};

export const localRuntimeApi = {
  status: () => invoke<RuntimeStatus>("local_runtime_status"),
  hardware: () => invoke<HardwareProfile>("local_runtime_hardware_profile"),
  discovery: () => invoke<RuntimeDiscovery>("local_runtime_discovery"),
  inspectModel: (modelPath: string, gpuOffload?: number) => invoke<ModelInspection>("local_runtime_inspect_model", { modelPath, gpuOffload }),
  start: (config: LocalRuntimeConfig) => invoke<RuntimeStatus>("local_runtime_start", { config }),
  stop: () => invoke<RuntimeStatus>("local_runtime_stop"),
  restart: () => invoke<RuntimeStatus>("local_runtime_restart"),
  testCapabilities: () => invoke<RuntimeCapabilities>("local_runtime_test_capabilities"),
};
