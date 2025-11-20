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
      serverInfo: null, // Added
      _pollIntervalId: null, // Added

      fetchData: async (showLoading = true) => {
        if (showLoading) set({ loading: true });
        set({ error: null });
        try {
          // Get token from bun localStorage
          const token = localStorage.getItem('authToken') || '';

          // Use Promise.allSettled to allow partial failures
          const results = await Promise.allSettled([
            invoke('check_package_status'),
            invoke('get_images', { token }),
            invoke('get_clients', { token }),
            invoke('get_server_info'),
            invoke('get_zpool_list'),
            invoke('list_zpools'),
          ]);

          const [servicesRes, mastersRes, clientsRes, serverInfoRes, zpoolStatsRes, zpoolsRes] = results;

          // Handle individual failures
          if (servicesRes.status === 'rejected') {
            console.error('Failed to fetch services:', servicesRes.reason);
          }
          if (mastersRes.status === 'rejected') {
            console.error('Failed to fetch images:', mastersRes.reason);
          }
          if (clientsRes.status === 'rejected') {
            console.error('Failed to fetch clients:', clientsRes.reason);
          }
          if (serverInfoRes.status === 'rejected') {
            console.error('Failed to fetch server info:', serverInfoRes.reason);
          }
          if (zpoolStatsRes.status === 'rejected') {
            console.error('Failed to fetch zpool stats:', zpoolStatsRes.reason);
          }
          if (zpoolsRes.status === 'rejected') {
            console.error('Failed to fetch zpools:', zpoolsRes.reason);
          }

          // Extract data or default to empty
          const servicesData = servicesRes.status === 'fulfilled' ? servicesRes.value : [];
          const mastersData = mastersRes.status === 'fulfilled' ? mastersRes.value : [];
          const clientsData = clientsRes.status === 'fulfilled' ? clientsRes.value : {};
          const serverInfoData = serverInfoRes.status === 'fulfilled' ? serverInfoRes.value : null;
          const zpoolStatsData = zpoolStatsRes.status === 'fulfilled' && Array.isArray(zpoolStatsRes.value) ? zpoolStatsRes.value[0] : null;
          const zpoolsData = zpoolsRes.status === 'fulfilled' ? zpoolsRes.value : [];

          // Batch state updates to reduce re-renders
          const newState = {
            clients: clientsData ? Object.values(clientsData) : [],
            masters: mastersData || [],
            services: Array.isArray(servicesData) ? servicesData : (servicesData ? Object.values(servicesData) : []),
            serverInfo: serverInfoData,
            zpoolStats: zpoolStatsData,
            zpools: zpoolsData,
          };

          // Set default snapshot selection for Add Client modal only if not already set and snapshots exist
          const { selectedSnapshot } = get();
          if (!selectedSnapshot && mastersData?.length > 0 && mastersData[0].snapshots?.length > 0) {
            newState.selectedSnapshot = mastersData[0].snapshots[mastersData[0].snapshots.length - 1].name;
          } else if (mastersData?.flatMap((m) => m.snapshots || []).length === 0) {
            newState.selectedSnapshot = '';
          }

          // If all requests failed, set a general error
          if (results.every(r => r.status === 'rejected')) {
            set({ error: 'Failed to load application data. Please check the backend connection.' });
          }

          set(newState);
        } catch (err) {
          set({ error: `Unexpected error loading data: ${err}` });
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
          } catch (err) {
            // Log polling errors but don't set global error state to avoid UI flickering
            console.warn('Client status polling failed:', err);
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
