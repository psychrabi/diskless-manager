import { invoke } from '@tauri-apps/api/core';
import { create } from 'zustand';
import { createJSONStorage, persist } from 'zustand/middleware';

export const useAppStore = create()(
  persist(
    (set, get) => ({
      clients: [],
      masters: [],
      services: {},
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
          const [servicesRes, mastersRes, clientsRes, services_status] = await Promise.all([
            invoke('get_services', { 'zfsPool': 'diskless' }),
            invoke('get_masters', { 'zfsPool': 'diskless' }),
            invoke('get_clients'),
            invoke("check_services")
          ]);
          set({
            clients: clientsRes ? Object.values(clientsRes) : [],
            masters: mastersRes || [],
            services: servicesRes || {},
            services_status: services_status || {},
          });
          // Set default snapshot selection for Add Client modal only if not already set and snapshots exist
          const { selectedSnapshot } = get();
          if (!selectedSnapshot && mastersRes?.length > 0 && mastersRes[0].snapshots?.length > 0) {
            set({ selectedSnapshot: mastersRes[0].snapshots[mastersRes[0].snapshots.length - 1].name });
          } else if (mastersRes?.flatMap((m) => m.snapshots || []).length === 0) {
            set({ selectedSnapshot: '' });
          }
        } catch (err) {
          set({ error: `Failed to load data: ${err.message || 'Check backend connection.'}` });
        } finally {
          if (showLoading) set({ loading: false });
        }
      },

      fetchConfig: async () => {
        set({ checkingConfig: true, loading: true });
        try {
          const cfg = await invoke('get_config');
          set({ config: cfg });
        } catch (err) {
          set({ error: `Failed to load config: ${err.message || 'Check backend connection.'}` });
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