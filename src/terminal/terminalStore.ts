import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";
import { terminalApi } from "./terminalApi";
import type { TerminalInfo } from "./types";

interface OutputEvent { id: string; data: number[] }
type OutputListener = (data: Uint8Array) => void;
const buffers = new Map<string, Uint8Array[]>();
const listeners = new Map<string, Set<OutputListener>>();
let listening: Promise<void> | null = null;

function appendOutput(id: string, data: Uint8Array) {
  const chunks = buffers.get(id) ?? [];
  chunks.push(data);
  let size = chunks.reduce((total, chunk) => total + chunk.length, 0);
  while (size > 1024 * 1024 && chunks.length > 1) {
    size -= chunks.shift()!.length;
  }
  buffers.set(id, chunks);
  listeners.get(id)?.forEach((listener) => listener(data));
}

export function clearTerminalOutput(id: string): void {
  buffers.delete(id);
}

export function terminalOutput(id: string): Uint8Array[] {
  return buffers.get(id) ?? [];
}

export function subscribeTerminalOutput(id: string, listener: OutputListener): () => void {
  const callbacks = listeners.get(id) ?? new Set<OutputListener>();
  callbacks.add(listener);
  listeners.set(id, callbacks);
  return () => {
    callbacks.delete(listener);
    if (!callbacks.size) listeners.delete(id);
  };
}

interface TerminalState {
  terminals: TerminalInfo[];
  activeId: string | null;
  error: string | null;
  initialize: () => Promise<void>;
  create: (workspaceRoot: string) => Promise<void>;
  activate: (id: string) => void;
  write: (id: string, data: Uint8Array) => Promise<void>;
  resize: (id: string, cols: number, rows: number) => Promise<void>;
  interrupt: (id: string) => Promise<void>;
  kill: (id: string) => Promise<void>;
  close: (id: string) => Promise<void>;
  clearError: () => void;
}

function message(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export const useTerminalStore = create<TerminalState>((set, get) => ({
  terminals: [],
  activeId: null,
  error: null,
  initialize: () => {
    if (!listening) {
      listening = Promise.all([
        listen<OutputEvent>("terminal-output", ({ payload }) => {
          appendOutput(payload.id, new Uint8Array(payload.data));
        }),
        listen<TerminalInfo>("terminal-status", ({ payload }) => {
          set((state) => ({
            terminals: state.terminals.map((terminal) =>
              terminal.id === payload.id ? payload : terminal),
          }));
        }),
      ]).then(() => undefined).catch((error: unknown) => {
        listening = null;
        set({ error: message(error) });
      });
    }
    return listening;
  },
  create: async (workspaceRoot) => {
    try {
      await get().initialize();
      const shell = localStorage.getItem("gravityforge.terminal.shell") || undefined;
      const terminal = await terminalApi.create(workspaceRoot, shell);
      set((state) => ({
        terminals: [...state.terminals, terminal],
        activeId: terminal.id,
        error: null,
      }));
      const current = await terminalApi.status(terminal.id);
      set((state) => ({
        terminals: state.terminals.map((item) => item.id === current.id ? current : item),
      }));
    } catch (error) {
      set({ error: message(error) });
    }
  },
  activate: (id) => set({ activeId: id }),
  write: async (id, data) => {
    try { await terminalApi.write(id, data); }
    catch (error) { set({ error: message(error) }); }
  },
  resize: async (id, cols, rows) => {
    try { await terminalApi.resize(id, cols, rows); }
    catch (error) { set({ error: message(error) }); }
  },
  interrupt: async (id) => {
    try { await terminalApi.interrupt(id); }
    catch (error) { set({ error: message(error) }); }
  },
  kill: async (id) => {
    try { await terminalApi.kill(id); }
    catch (error) { set({ error: message(error) }); }
  },
  close: async (id) => {
    try {
      await terminalApi.close(id);
      buffers.delete(id);
      set((state) => {
        const terminals = state.terminals.filter((terminal) => terminal.id !== id);
        return { terminals, activeId: state.activeId === id ? terminals.at(-1)?.id ?? null : state.activeId, error: null };
      });
    } catch (error) { set({ error: message(error) }); }
  },
  clearError: () => set({ error: null }),
}));



