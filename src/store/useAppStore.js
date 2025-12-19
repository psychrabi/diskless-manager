import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { shallow } from "zustand/shallow";

// Export shallow comparison helper for consumers
export { shallow };

export const useAppStore = create()(
  persist(
    (set, get) => ({
      clients: [],
      masters: [],
      services: [],
      dependencies: [],
      services_status: {},
      zpoolStats: null,
      zpools: [],
      ramUsage: null,
      arcStat: null,
      appConfig: null, // Renamed from config to avoid conflict with service config string
      //  serviceConfig removed
      // Actually, I should check if SettingsManagement uses serviceConfig?
      // ServiceConfigModal logic was moved out.
      // SettingsManagement logic does NOT use serviceConfig from store.
      // Wait, I should verify before deleting.

      error: null,
      loading: true,
      selectedSnapshot: "",
      checkingConfig: true,

      setClients: (clients) => set({ clients }),
      setMasters: (masters) => set({ masters }),
      setServices: (services) => set({ services }),
      setDependencies: (dependencies) => set({ dependencies }),
      // setServiceConfig: removed
      setAppConfig: (appConfig) => set({ appConfig }), // Added for global config
      setError: (error) => set({ error }),
      setLoading: (loading) => set({ loading }),
      setSelectedSnapshot: (selectedSnapshot) => set({ selectedSnapshot }),
      setCheckingConfig: (checkingConfig) => set({ checkingConfig }),

      saving: true,
      setSaving: (saving) => set({ saving }),

      serverInfo: null, // Added
      licenseInfo: null, // Added
      _pollIntervalId: null, // Added

      fetchClients: async () => {
        try {
          const token = localStorage.getItem("authToken") || "";
          const clientsRes = await invoke("get_clients", { token });
          const clientsData = clientsRes ? Object.values(clientsRes) : [];
          set({ clients: clientsData });
        } catch (err) {
          console.error("Failed to fetch clients:", err);
        }
      },

      fetchImages: async () => {
        try {
          const token = localStorage.getItem("authToken") || "";
          const mastersRes = await invoke("get_images", { token });
          set({ masters: mastersRes || [] });
        } catch (err) {
          console.error("Failed to fetch images:", err);
        }
      },

      fetchServices: async () => {
        try {
          const servicesRes = await invoke("list_services");
          const servicesData = Array.isArray(servicesRes)
            ? servicesRes
            : servicesRes
            ? Object.values(servicesRes)
            : [];
          set({ services: servicesData });
        } catch (err) {
          console.error("Failed to fetch services:", err);
        }
      },

      startService: async (name) => {
        try {
          await invoke("start_service", { name });
          set({
            services: get().services.map((service) =>
              service.name === name
                ? { ...service, status: "running" }
                : service
            ),
          });
          await get().fetchServices();
        } catch (err) {
          console.error("Failed to start service:", err);
        }
      },

      stopService: async (name) => {
        try {
          await invoke("stop_service", { name });
          set({
            services: get().services.map((service) =>
              service.name === name
                ? { ...service, status: "stopped" }
                : service
            ),
          });
          await get().fetchServices();
        } catch (err) {
          console.error("Failed to stop service:", err);
        }
      },

      restartService: async (name) => {
        try {
          await invoke("restart_service", { name });
          set({
            services: get().services.map((service) =>
              service.name === name
                ? { ...service, status: "restarting" }
                : service
            ),
          });
          await get().fetchServices();
        } catch (err) {
          console.error("Failed to restart service:", err);
        }
      },

      fetchServerInfo: async () => {
        try {
          const serverInfoRes = await invoke("get_system_info");
          set({ serverInfo: serverInfoRes });
        } catch (err) {
          console.error("Failed to fetch server info:", err);
        }
      },

      fetchDependencies: async () => {
        try {
          const dependenciesRes = await invoke("check_dependencies");
          set({ dependencies: dependenciesRes });
        } catch (err) {
          console.error("Failed to fetch dependencies:", err);
        }
      },

      fetchLicenseInfo: async () => {
        try {
          const licenseRes = await invoke("get_license_info");
          set({ licenseInfo: licenseRes });
        } catch (err) {
          console.error("Failed to fetch license info:", err);
        }
      },

      fetchDisks: async () => {
        try {
          const [zpoolStatsRes, zpoolsRes] = await Promise.all([
            invoke("get_zpool_list"),
            invoke("list_zpools"),
          ]);
          set({
            zpoolStats: Array.isArray(zpoolStatsRes) ? zpoolStatsRes[0] : null,
            zpools: zpoolsRes || [],
          });
        } catch (err) {
          console.error("Failed to fetch disk info:", err);
        }
      },
      fetchRamUsage: async () => {
        try {
          const ramUsageRes = await invoke("get_ram_usage");
          set({ ramUsage: ramUsageRes });
        } catch (err) {
          console.error("Failed to fetch ram usage:", err);
        }
      },

      fetchArcStat: async () => {
        try {
          const arcStatRes = await invoke("get_zfs_arcstat");
          set({ arcStat: arcStatRes });
        } catch (err) {
          console.error("Failed to fetch arc stat:", err);
        }
      },

      fetchData: async (showLoading = true) => {
        if (showLoading) set({ loading: true });
        set({ error: null });
        const {
          fetchClients,
          fetchImages,
          fetchServices,
          fetchDependencies,
          fetchServerInfo,
          fetchDisks,
          fetchLicenseInfo,
          fetchRamUsage,
          fetchArcStat,
        } = get();

        try {
          await Promise.allSettled([
            fetchClients(),
            fetchImages(),
            fetchServices(),
            fetchDependencies(),
            fetchServerInfo(),
            fetchDisks(),
            fetchLicenseInfo(),
            fetchRamUsage(),
            fetchArcStat(),
          ]);
        } catch (err) {
          set({ error: `Unexpected error loading data: ${err}` });
        } finally {
          if (showLoading) set({ loading: false });
          // console.log(get().services)
        }
      },

      // Lightweight polling to keep client statuses fresh
      startClientStatusPolling: () => {
        const { _pollIntervalId } = get();
        if (_pollIntervalId) return; // already running
        const id = setInterval(async () => {
          try {
            // Get token from localStorage
            const token = localStorage.getItem("authToken") || "";
            const clientsRes = await invoke("get_clients", { token });
            const newClients = clientsRes ? Object.values(clientsRes) : [];

            // Only update if data has changed to prevent unnecessary re-renders
            const { clients: currentClients } = get();

            // Optimized diff: Check length first, then check specific status fields
            let hasChanged = false;

            if (currentClients.length !== newClients.length) {
              hasChanged = true;
            } else {
              // Create a Map for O(1) lookups by ID if IDs exist, else fallback to index
              const newClientsMap = new Map();
              newClients.forEach((c) => {
                if (c.id) newClientsMap.set(c.id, c);
              });

              if (newClientsMap.size > 0) {
                for (const oldClient of currentClients) {
                  if (!oldClient.id) {
                    hasChanged = true;
                    break;
                  } // Fallback if data structure inconsistent
                  const newClient = newClientsMap.get(oldClient.id);
                  if (!newClient) {
                    hasChanged = true;
                    break;
                  } // Client removed

                  // Compare only volatile fields
                  if (
                    oldClient.status !== newClient.status ||
                    oldClient.online !== newClient.online ||
                    oldClient.ip !== newClient.ip
                  ) {
                    // IP might change with DHCP lease
                    hasChanged = true;
                    break;
                  }
                }
              } else {
                // Fallback to JSON compare if no IDs
                if (
                  JSON.stringify(currentClients) !== JSON.stringify(newClients)
                )
                  hasChanged = true;
              }
            }

            if (hasChanged) {
              set({ clients: newClients });
            }
          } catch (err) {
            // Log polling errors but don't set global error state to avoid UI flickering
            console.warn("Client status polling failed:", err);
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
          const cfg = await invoke("read_config");
          set({ appConfig: cfg });
        } catch (err) {
          set({
            error: `Failed to load config: ${
              err.message ||
              "Check config file in the ~/.config/com.diskless-server."
            }`,
          });
        } finally {
          set({ checkingConfig: false, loading: false });
        }
      },
    }),
    {
      name: "diskless", // name of the item in the storage (must be unique)
      storage: createJSONStorage(() => localStorage), // (optional) by default, 'localStorage' is used
    }
  )
);
