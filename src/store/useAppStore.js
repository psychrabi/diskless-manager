import { invoke } from '@tauri-apps/api/core';
import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';
import { shallow } from 'zustand/shallow';

// Export shallow comparison helper for consumers
export { shallow };

export const useAppStore = create()(
  persist(
    (set, get) => ({
      clients: [],
      masters: [],
      services: [],
      services_status: {},
      config: '',
      error: null,
      loading: true,
      selectedSnapshot: '',
      checkingConfig: true,
      title: '',
      setClients: (clients) => set({ clients }),
      setMasters: (masters) => set({ masters }),
      setServices: (services) => set({ services }),
      setConfig: (config) => set({ config }),
      setError: (error) => set({ error }),
      setLoading: (loading) => set({ loading }),
      setSelectedSnapshot: (selectedSnapshot) => set({ selectedSnapshot }),
      setCheckingConfig: (checkingConfig) => set({ checkingConfig }),
      setTitle: (title) => set({ title }),
      open: false,
      setOpen: (open) => set({ open }),
      saving: true,
      setSaving: (saving) => set({ saving }),
      serviceKey: '',
      setServiceKey: (serviceKey) => set({ serviceKey }),

      fetchData: async (showLoading = true) => {
        if (showLoading) set({ loading: true });
        set({ error: null });
        try {
          // Get token from bun localStorage
          const token = localStorage.getItem('authToken') || '';
          const [servicesRes, mastersRes, clientsRes] = await Promise.all([
            invoke('check_package_status'),
            invoke('get_images', { token }),
            invoke('get_clients', { token }),
          ]);
          
          // Batch state updates to reduce re-renders
          const newState = {
            clients: clientsRes ? Object.values(clientsRes) : [],
            masters: mastersRes || [],
            services: Array.isArray(servicesRes) ? servicesRes : (servicesRes ? Object.values(servicesRes) : []),
          };
          
          // Set default snapshot selection for Add Client modal only if not already set and snapshots exist
          const { selectedSnapshot } = get();
          if (!selectedSnapshot && mastersRes?.length > 0 && mastersRes[0].snapshots?.length > 0) {
            newState.selectedSnapshot = mastersRes[0].snapshots[mastersRes[0].snapshots.length - 1].name;
          } else if (mastersRes?.flatMap((m) => m.snapshots || []).length === 0) {
            newState.selectedSnapshot = '';
          }
          
          set(newState);
        } catch (err) {
          set({ error: `Failed to load data: ${err}` });
        } finally {
          if (showLoading) set({ loading: false });
        }
      },

      // Lightweight polling to keep client statuses fresh
      startClientStatusPolling: () => {
        const { _pollIntervalId } = get();
        if (_pollIntervalId) return; // already running
        const id = setInterval(async () => {
          try {
            // Get token from localStorage
            const token = localStorage.getItem('authToken') || '';
            const clientsRes = await invoke('get_clients', { token });
            const newClients = clientsRes ? Object.values(clientsRes) : [];
            
            // Only update if data has changed to prevent unnecessary re-renders
              const { clients: currentClients } = get();
              // Lightweight deep-diff: prefer id+important-fields comparison to avoid expensive stringify on large objects
              const clientsChanged = (() => {
                try {
                  if (currentClients.length !== newClients.length) return true;
                  // Build map of id -> snapshot key
                  const map = new Map();
                  for (const c of currentClients) {
                    if (c && c.id) {
                      map.set(c.id, JSON.stringify({ status: c.status, online: c.online }));
                    }
                  }
                  for (const nc of newClients) {
                    if (nc && nc.id) {
                      const prev = map.get(nc.id);
                      if (!prev) return true;
                      if (prev !== JSON.stringify({ status: nc.status, online: nc.online })) return true;
                    } else {
                      // Fallback to full compare if no id present
                      if (JSON.stringify(currentClients) !== JSON.stringify(newClients)) return true;
                    }
                  }
                  return false;
                } catch {
                  // If anything unexpected, fallback to full compare
                  return JSON.stringify(currentClients) !== JSON.stringify(newClients);
                }
              })();
              if (clientsChanged) set({ clients: newClients });
          } catch {
            // ignore transient errors
          }
        }, 30000);
        set({ _pollIntervalId: id });
      },
      stopClientStatusPolling: () => {
        const { _pollIntervalId } = get();
        if (_pollIntervalId) {
          clearInterval(_pollIntervalId);
          set({ _pollIntervalId: null });
        }
      },

      fetchConfig: async () => {
        set({ checkingConfig: true, loading: true });
        try {
          const cfg = await invoke('read_config');
          set({ config: cfg });
        } catch (err) {
          set({ error: `Failed to load config: ${err.message || 'Check config file in the ~/.config/com.diskless-server.'}` });
        } finally {
          set({ checkingConfig: false, loading: false });
        }
      },
    }),
    {
      name: 'diskless', // name of the item in the storage (must be unique)
      storage: createJSONStorage(() => localStorage), // (optional) by default, 'localStorage' is used
    },
  )
);
