import { create } from 'zustand';

export const useToastStore = create((set, get) => ({
    toasts: [],
    show: (type, message, duration = 5000) => {
        const id = crypto.randomUUID();
        const toast = { id, type, message, duration };

        set((state) => ({ toasts: [...state.toasts, toast] }));

        if (duration > 0) {
            setTimeout(() => {
                get().dismiss(id);
            }, duration);
        }

        return id;
    },
    success: (message, duration) => get().show("success", message, duration),
    error: (message, duration) => get().show("error", message, duration),
    warning: (message, duration) => get().show("warning", message, duration),
    info: (message, duration) => get().show("info", message, duration),
    dismiss: (id) => set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),
    clear: () => set({ toasts: [] }),
}));
