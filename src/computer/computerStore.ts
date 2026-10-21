import { create } from "zustand";
import { computerApi, type ComputerStatus } from "./computerApi";

interface ComputerStore { status: ComputerStatus | null; busy: boolean; error: string | null; initialize: () => Promise<void>; enable: () => Promise<void>; disable: () => Promise<void>; stop: () => Promise<void> }
const message = (error: unknown) => error instanceof Error ? error.message : String(error);
export const useComputerStore = create<ComputerStore>((set) => ({
  status: null, busy: false, error: null,
  initialize: async () => { try { set({ status: await computerApi.status(), error: null }); } catch (error) { set({ error: message(error) }); } },
  enable: async () => { set({ busy: true, error: null }); try { set({ status: await computerApi.enable() }); } catch (error) { set({ error: message(error) }); } finally { set({ busy: false }); } },
  disable: async () => { set({ busy: true }); try { await computerApi.disable(); set({ status: await computerApi.status(), error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ busy: false }); } },
  stop: async () => { set({ busy: true }); try { await computerApi.stop(); set({ status: await computerApi.status(), error: null }); } catch (error) { set({ error: message(error) }); } finally { set({ busy: false }); } },
}));
