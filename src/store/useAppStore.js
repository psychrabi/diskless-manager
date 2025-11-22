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
      licenseInfo: null, // Added
      _pollIntervalId: null, // Added

      fetchClients: async () => {
        try {
          const token = localStorage.getItem('authToken') || '';
          const clientsRes = await invoke('get_clients', { token });
          const clientsData = clientsRes ? Object.values(clientsRes) : [];
          set({ clients: clientsData });
        } catch (err) {
          console.error('Failed to fetch clients:', err);
        }
      },

      fetchImages: async () => {
        try {
          const token = localStorage.getItem('authToken') || '';
          const mastersRes = await invoke('get_images', { token });
          set({ masters: mastersRes || [] });
        } catch (err) {
          console.error('Failed to fetch images:', err);
        }
      },

      fetchServices: async () => {
        try {
          const servicesRes = await invoke('check_package_status');
          const servicesData = Array.isArray(servicesRes) ? servicesRes : (servicesRes ? Object.values(servicesRes) : []);
          set({ services: servicesData });
        } catch (err) {
          console.error('Failed to fetch services:', err);
        }
      },

      fetchServerInfo: async () => {
        try {
          const serverInfoRes = await invoke('get_server_info');
          set({ serverInfo: serverInfoRes });
        } catch (err) {
          console.error('Failed to fetch server info:', err);
        }
      },

      fetchLicenseInfo: async () => {
        try {
          const licenseRes = await invoke('get_license_info');
          set({ licenseInfo: licenseRes });
        } catch (err) {
          console.error('Failed to fetch license info:', err);
        }
      },

      fetchDisks: async () => {
        try {
          const [zpoolStatsRes, zpoolsRes] = await Promise.all([
            invoke('get_zpool_list'),
            invoke('list_zpools')
          ]);
          set({
            zpoolStats: Array.isArray(zpoolStatsRes) ? zpoolStatsRes[0] : null,
            zpools: zpoolsRes || []
          });
        } catch (err) {
          console.error('Failed to fetch disk info:', err);
        }
      },

      fetchData: async (showLoading = true) => {
        if (showLoading) set({ loading: true });
        set({ error: null });
        const { fetchClients, fetchImages, fetchServices, fetchServerInfo, fetchDisks, fetchLicenseInfo } = get();

        try {
          await Promise.allSettled([
            fetchClients(),
            fetchImages(),
            fetchServices(),
            fetchServerInfo(),
            fetchDisks(),
            fetchLicenseInfo()
          ]);
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
