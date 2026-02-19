// Villa Kunterbunt — Zustand store with localStorage persistence.
// Supports multiple dashboards, each with its own widgets and chat history.

import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";
import type {
  WidgetConfig,
  ChatMessage,
  LayoutAction,
  LayoutPosition,
  Dashboard,
} from "./types";

// ---------------------------------------------------------------------------
// Default layout — shown on first visit (no localStorage yet)
// ---------------------------------------------------------------------------

export const DEFAULT_WIDGETS: WidgetConfig[] = [
  {
    id: "default-stats",
    type: "stats-card",
    title: "System Overview",
    dataSource: { type: "api", endpoint: "/api/stats", refreshInterval: 30000 },
    layout: { x: 0, y: 0, w: 12, h: 2 },
  },
  {
    id: "default-trends",
    type: "time-series",
    title: "Event Trends",
    dataSource: { type: "api", endpoint: "/api/compute/trends", refreshInterval: 60000 },
    layout: { x: 0, y: 2, w: 6, h: 4 },
  },
];

export const DEFAULT_WELCOME_MESSAGE: ChatMessage = {
  id: "welcome",
  role: "assistant",
  content:
    "Welcome to Villa Kunterbunt! I've set up an initial dashboard with system stats and event trends. Try asking:\n\n- \"Show me the relationship graph\"\n- \"Add a data table with entity details\"\n- \"Focus on anomalies\"",
  timestamp: Date.now(),
};

function createDefaultDashboard(): Dashboard {
  return {
    id: crypto.randomUUID(),
    name: "My Dashboard",
    widgets: DEFAULT_WIDGETS,
    chatMessages: [DEFAULT_WELCOME_MESSAGE],
    createdAt: Date.now(),
  };
}

// ---------------------------------------------------------------------------
// LayoutStore — thin persistence abstraction for future server migration
// ---------------------------------------------------------------------------

export interface LayoutStore {
  save(state: PersistedState): void;
  load(): Partial<PersistedState> | null;
  clear(): void;
}

/** The subset of VillaStore that gets persisted. */
interface PersistedState {
  dashboards: Dashboard[];
  activeDashboardId: string;
}

// ---------------------------------------------------------------------------
// Debounced localStorage adapter
// ---------------------------------------------------------------------------

const STORAGE_KEY = "villa-layout-v1";
const DEBOUNCE_MS = 500;

let debounceTimer: ReturnType<typeof setTimeout> | null = null;

/** localStorage wrapper that debounces writes by 500ms. */
const debouncedStorage = createJSONStorage(() => ({
  getItem: (name: string) => localStorage.getItem(name),
  setItem: (name: string, value: string) => {
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => {
      localStorage.setItem(name, value);
      debounceTimer = null;
    }, DEBOUNCE_MS);
  },
  removeItem: (name: string) => localStorage.removeItem(name),
}));

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Get the active dashboard from state, or the first one as fallback. */
function getActiveDashboard(state: { dashboards: Dashboard[]; activeDashboardId: string }): Dashboard | undefined {
  return state.dashboards.find((d) => d.id === state.activeDashboardId) ?? state.dashboards[0];
}

/** Update the active dashboard's fields immutably. */
function updateActiveDashboard(
  state: { dashboards: Dashboard[]; activeDashboardId: string },
  updater: (dashboard: Dashboard) => Partial<Dashboard>,
): { dashboards: Dashboard[] } {
  return {
    dashboards: state.dashboards.map((d) =>
      d.id === state.activeDashboardId ? { ...d, ...updater(d) } : d,
    ),
  };
}

// ---------------------------------------------------------------------------
// Store interface
// ---------------------------------------------------------------------------

export interface VillaStore {
  // Multi-dashboard state
  dashboards: Dashboard[];
  activeDashboardId: string;

  // Computed — derived from active dashboard for backward compatibility
  widgets: WidgetConfig[];
  chatMessages: ChatMessage[];

  // Undo state (ephemeral — not persisted)
  _lastRemoved: { widget: WidgetConfig; index: number } | null;

  // UI state (ephemeral)
  isChatOpen: boolean;

  // Dashboard actions
  createDashboard: (name: string) => string;
  deleteDashboard: (id: string) => void;
  switchDashboard: (id: string) => void;
  renameDashboard: (id: string, name: string) => void;

  // Widget actions (operate on active dashboard)
  addWidget: (config: WidgetConfig) => void;
  removeWidget: (id: string) => void;
  undoRemove: () => void;
  updateWidgetLayout: (id: string, layout: Partial<LayoutPosition>) => void;
  updateAllLayouts: (layouts: { i: string; x: number; y: number; w: number; h: number }[]) => void;

  // Chat actions (operate on active dashboard)
  addChatMessage: (message: ChatMessage) => void;
  toggleChat: () => void;

  // Bulk actions (for LLM responses)
  applyActions: (actions: LayoutAction[]) => void;
}

// ---------------------------------------------------------------------------
// Store implementation
// ---------------------------------------------------------------------------

export const useVillaStore = create<VillaStore>()(
  persist(
    (set) => ({
      // -- State --
      dashboards: [],
      activeDashboardId: "",
      widgets: [],
      chatMessages: [],
      _lastRemoved: null,
      isChatOpen: false,

      // -- Dashboard actions --
      createDashboard: (name) => {
        const id = crypto.randomUUID();
        const dashboard: Dashboard = {
          id,
          name,
          widgets: [],
          chatMessages: [{
            id: `welcome-${id}`,
            role: "assistant",
            content: `Dashboard "${name}" created. Start chatting to add widgets!`,
            timestamp: Date.now(),
          }],
          createdAt: Date.now(),
        };
        set((s) => {
          const dashboards = [...s.dashboards, dashboard];
          const active = getActiveDashboard({ dashboards, activeDashboardId: id })!;
          return {
            dashboards,
            activeDashboardId: id,
            widgets: active.widgets,
            chatMessages: active.chatMessages,
          };
        });
        return id;
      },

      deleteDashboard: (id) =>
        set((s) => {
          if (s.dashboards.length <= 1) return s; // can't delete last
          const dashboards = s.dashboards.filter((d) => d.id !== id);
          const newActiveId = s.activeDashboardId === id ? dashboards[0].id : s.activeDashboardId;
          const active = getActiveDashboard({ dashboards, activeDashboardId: newActiveId })!;
          return {
            dashboards,
            activeDashboardId: newActiveId,
            widgets: active.widgets,
            chatMessages: active.chatMessages,
          };
        }),

      switchDashboard: (id) =>
        set((s) => {
          const active = getActiveDashboard({ dashboards: s.dashboards, activeDashboardId: id });
          if (!active) return s;
          return {
            activeDashboardId: id,
            widgets: active.widgets,
            chatMessages: active.chatMessages,
            _lastRemoved: null, // clear undo when switching
          };
        }),

      renameDashboard: (id, name) =>
        set((s) => ({
          dashboards: s.dashboards.map((d) =>
            d.id === id ? { ...d, name } : d,
          ),
        })),

      // -- Widget actions (operate on active dashboard) --
      addWidget: (config) =>
        set((s) => {
          const updated = updateActiveDashboard(s, (d) => ({
            widgets: [...d.widgets, config],
          }));
          const active = getActiveDashboard({ ...updated, activeDashboardId: s.activeDashboardId })!;
          return { ...updated, widgets: active.widgets };
        }),

      removeWidget: (id) =>
        set((s) => {
          const active = getActiveDashboard(s);
          if (!active) return s;
          const index = active.widgets.findIndex((w) => w.id === id);
          if (index === -1) return s;
          const widget = active.widgets[index];
          const updated = updateActiveDashboard(s, (d) => ({
            widgets: d.widgets.filter((w) => w.id !== id),
          }));
          const newActive = getActiveDashboard({ ...updated, activeDashboardId: s.activeDashboardId })!;
          return {
            ...updated,
            widgets: newActive.widgets,
            _lastRemoved: { widget, index },
          };
        }),

      undoRemove: () =>
        set((s) => {
          if (!s._lastRemoved) return s;
          const { widget, index } = s._lastRemoved;
          const updated = updateActiveDashboard(s, (d) => {
            const widgets = [...d.widgets];
            widgets.splice(Math.min(index, widgets.length), 0, widget);
            return { widgets };
          });
          const active = getActiveDashboard({ ...updated, activeDashboardId: s.activeDashboardId })!;
          return { ...updated, widgets: active.widgets, _lastRemoved: null };
        }),

      updateWidgetLayout: (id, layout) =>
        set((s) => {
          const updated = updateActiveDashboard(s, (d) => ({
            widgets: d.widgets.map((w) =>
              w.id === id ? { ...w, layout: { ...w.layout, ...layout } } : w,
            ),
          }));
          const active = getActiveDashboard({ ...updated, activeDashboardId: s.activeDashboardId })!;
          return { ...updated, widgets: active.widgets };
        }),

      updateAllLayouts: (layouts) =>
        set((s) => {
          const updated = updateActiveDashboard(s, (d) => ({
            widgets: d.widgets.map((w) => {
              const found = layouts.find((l) => l.i === w.id);
              if (!found) return w;
              return {
                ...w,
                layout: { ...w.layout, x: found.x, y: found.y, w: found.w, h: found.h },
              };
            }),
          }));
          const active = getActiveDashboard({ ...updated, activeDashboardId: s.activeDashboardId })!;
          return { ...updated, widgets: active.widgets };
        }),

      // -- Chat actions (operate on active dashboard) --
      addChatMessage: (message) =>
        set((s) => {
          const updated = updateActiveDashboard(s, (d) => ({
            chatMessages: [...d.chatMessages, message],
          }));
          const active = getActiveDashboard({ ...updated, activeDashboardId: s.activeDashboardId })!;
          return { ...updated, chatMessages: active.chatMessages };
        }),

      toggleChat: () => set((s) => ({ isChatOpen: !s.isChatOpen })),

      // -- Bulk LLM actions (operate on active dashboard) --
      applyActions: (actions) =>
        set((s) => {
          const updated = updateActiveDashboard(s, (d) => {
            let widgets = [...d.widgets];
            for (const action of actions) {
              switch (action.action) {
                case "add":
                  if (action.widget) widgets.push(action.widget);
                  break;
                case "remove":
                  if (action.widgetId)
                    widgets = widgets.filter((w) => w.id !== action.widgetId);
                  break;
                case "resize":
                  if (action.widgetId && action.dimensions) {
                    const dims = action.dimensions;
                    widgets = widgets.map((w) =>
                      w.id === action.widgetId
                        ? { ...w, layout: { ...w.layout, w: dims.w, h: dims.h } }
                        : w,
                    );
                  }
                  break;
                case "move":
                  break;
              }
            }
            return { widgets };
          });
          const active = getActiveDashboard({ ...updated, activeDashboardId: s.activeDashboardId })!;
          return { ...updated, widgets: active.widgets };
        }),
    }),
    {
      name: STORAGE_KEY,
      storage: debouncedStorage,
      version: 2,
      // Only persist dashboards — not transient UI state
      partialize: (state) => ({
        dashboards: state.dashboards,
        activeDashboardId: state.activeDashboardId,
      }),
      // Migrate from v1 (flat widgets/chatMessages) to v2 (multi-dashboard)
      migrate: (persisted: unknown, version: number) => {
        if (version < 2) {
          const old = persisted as {
            widgets?: WidgetConfig[];
            chatMessages?: ChatMessage[];
          };
          const defaultDash = createDefaultDashboard();
          // Carry over existing data if present
          if (old.widgets && old.widgets.length > 0) {
            defaultDash.widgets = old.widgets;
          }
          if (old.chatMessages && old.chatMessages.length > 0) {
            defaultDash.chatMessages = old.chatMessages;
          }
          return {
            dashboards: [defaultDash],
            activeDashboardId: defaultDash.id,
          };
        }
        return persisted as PersistedState;
      },
      // Sync computed fields after rehydration
      onRehydrateStorage: () => (state) => {
        if (!state) return;
        // If no dashboards exist (fresh install), create default
        if (state.dashboards.length === 0) {
          const defaultDash = createDefaultDashboard();
          state.dashboards = [defaultDash];
          state.activeDashboardId = defaultDash.id;
        }
        // Sync computed fields from active dashboard
        const active = getActiveDashboard(state);
        if (active) {
          state.widgets = active.widgets;
          state.chatMessages = active.chatMessages;
        }
      },
    },
  ),
);

// ---------------------------------------------------------------------------
// LayoutStore implementation — localStorage-backed, swappable to server later
// ---------------------------------------------------------------------------

export const layoutStore: LayoutStore = {
  save(state: PersistedState) {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ state }));
  },

  load(): Partial<PersistedState> | null {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    try {
      const parsed = JSON.parse(raw);
      return parsed?.state ?? null;
    } catch {
      return null;
    }
  },

  clear() {
    localStorage.removeItem(STORAGE_KEY);
  },
};
