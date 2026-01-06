import { create } from "zustand";

export const useToastStore = create((set, get) => ({
  toasts: [],
  show: (type, title, description, duration = 5000) => {
    let actualDescription = description;
    let actualDuration = duration;

    // Handle case where show("type", "message", duration) is called
    if (typeof description === "number") {
      actualDuration = description;
      actualDescription = undefined;
    }

    // Convert Error objects or other objects to strings to prevent React rendering issues
    if (actualDescription && typeof actualDescription === "object") {
      actualDescription =
        actualDescription.message || String(actualDescription);
    }

    // Ensure duration is a number
    if (typeof actualDuration !== "number") {
      actualDuration = 5000;
    }

    const id = crypto.randomUUID();
    const toast = {
      id,
      type,
      title,
      description: actualDescription,
      duration: actualDuration,
    };

    set((state) => ({ toasts: [...state.toasts, toast] }));

    if (actualDuration > 0) {
      setTimeout(() => {
        get().dismiss(id);
      }, actualDuration);
    }

    return id;
  },
  success: (title, description, duration) =>
    get().show("success", title, description, duration),
  error: (title, description, duration) =>
    get().show("error", title, description, duration),
  warning: (title, description, duration) =>
    get().show("warning", title, description, duration),
  info: (title, description, duration) =>
    get().show("info", title, description, duration),
  dismiss: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
  clear: () => set({ toasts: [] }),
}));
