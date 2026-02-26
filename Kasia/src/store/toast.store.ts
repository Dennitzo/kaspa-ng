import { v4 } from "uuid";
import { create } from "zustand";

export type ToastType = "success" | "error" | "info" | "warning";

export type Toast = {
  id: string;
  type: ToastType;
  message: string;
};

const DEFAULT_TIMEOUT: number = 5000;

type ToastStore = {
  toasts: Toast[];
  add: (type: ToastType, message: string, timeout?: number) => string; // returns toast id
  remove: (id: string) => void;
  removeAll: () => void;
};

// track timeout IDs outside the store
const timeoutIds = new Map<string, ReturnType<typeof setTimeout>>();

// generate unique IDs
const generateId = () => {
  return `toast-${v4()}-${Date.now()}`;
};

export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],
  add: (type, message, timeout) => {
    const id = generateId();
    const newToast = { id, type, message };
    const effectiveTimeout = timeout ?? DEFAULT_TIMEOUT;

    // add new toast at the front
    set((state) => ({ toasts: [newToast, ...state.toasts] }));

    // store the timeout ID so we can cancel it later
    const timeoutId = setTimeout(() => {
      set((state) => ({
        toasts: state.toasts.filter((t) => t.id !== id),
      }));
      timeoutIds.delete(id); // clean up the timeout ID
    }, effectiveTimeout);

    timeoutIds.set(id, timeoutId);

    return id; // return the toast id for potential cancellation
  },
  remove: (id) => {
    // cancel the timeout if it exists
    const timeoutId = timeoutIds.get(id);
    if (timeoutId) {
      clearTimeout(timeoutId);
      timeoutIds.delete(id);
    }

    set((state) => ({
      toasts: state.toasts.filter((t) => t.id !== id),
    }));
  },
  removeAll: () => {
    // cancel all timeouts
    timeoutIds.forEach((timeoutId) => clearTimeout(timeoutId));
    timeoutIds.clear();
    set({ toasts: [] });
  },
}));
