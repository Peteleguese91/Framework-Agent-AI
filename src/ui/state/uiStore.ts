import { create } from "zustand";

type Theme = "dark" | "light";

interface UiState {
  bottomPanelOpen: boolean;
  theme: Theme;
  toggleBottomPanel: () => void;
  toggleTheme: () => void;
}

export const useUiStore = create<UiState>((set) => ({
  bottomPanelOpen: true,
  theme: "dark",
  toggleBottomPanel: () => set((state) => ({ bottomPanelOpen: !state.bottomPanelOpen })),
  toggleTheme: () =>
    set((state) => {
      const theme = state.theme === "dark" ? "light" : "dark";
      document.documentElement.dataset.theme = theme;
      return { theme };
    }),
}));
