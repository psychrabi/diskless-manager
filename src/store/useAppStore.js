import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { shallow } from "zustand/shallow";
import * as api from "../api/commands";

// Export shallow comparison helper for consumers
export { shallow };

export const useAppStore = create()(
  persist(
    (set, get) => ({
      clients: [],
      masters: [],
      images: [],
      services: [],
      dependencies: [],
      services_status: {},
      zpoolStats: null,
      zpools: [],
      ramUsage: null,
      arcStat: null,
      appConfig: null, // Renamed from config to avoid conflict with service config string
      datasets: [],
      setDatasets: (datasets) => set({ datasets }),
      error: null,
      loading: true,
      selectedSnapshot: "",
      checkingConfig: true,

      setClients: (clients) => set({ clients }),
      setMasters: (masters) => set({ masters }),
      setServices: (services) => set({ services }),
      setDependencies: (dependencies) => set({ dependencies }),
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
          const clientsData = await api.listClients();
          set({ clients: clientsData || [] });
        } catch (err) {
          console.error("Failed to fetch clients:", err);
        }
      },

      fetchMasters: async () => {
        try {
          const mastersRes = await api.listMasters();

          set({ masters: mastersRes || [] });
        } catch (err) {
          console.error("Failed to fetch masters:", err);
        }
      },

      fetchImages: async () => {
        try {
          const imagesRes = await api.listImages();

          set({ images: imagesRes || [] });
        } catch (err) {
          console.error("Failed to fetch images:", err);
        }
      },

      fetchDatasets: async (zpool) => {
        if (!zpool) {
          set({ datasets: [] });
          return;
        }
        try {
          const datasetsRes = await api.listDatasets(zpool);
          set({ datasets: datasetsRes || [] });
        } catch (err) {
          console.error("Failed to fetch datasets:", err);
        }
      },

      createDataset: async (data) => {
        try {
          await api.createZfsDataset({
            zpool: data.zpool,
            name: data.name,
            usage_type: data.usage_type,
            size: data.size ?? "",
          });
          return {
            success: true,
            message: `Dataset ${data.name} created successfully.`,
          };
        } catch (err) {
          return {
            success: false,
            error: `Failed to create dataset: ${
              err.message || "An unknown error occurred"
            }`,
          };
        }
      },

      deleteDataset: async (name) => {
        try {
          const response = await api.deleteZfsDataset(name, true);
          return {
            success: true,
            message:
              response.message || `Dataset ${name} deleted successfully.`,
          };
        } catch (err) {
          return {
            success: false,
            error: `Failed to delete disk: ${
              err.error || "An unknown error occurred"
            }`,
          };
        }
      },

      fetchServices: async () => {
        try {
          const servicesRes = await api.listServices();
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
          await api.startService(name);
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
          await api.stopService(name);
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
          await api.restartService(name);
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
          const serverInfoRes = await api.getSystemInfo();
          set({ serverInfo: serverInfoRes });
        } catch (err) {
          console.error("Failed to fetch server info:", err);
        }
      },

      fetchDependencies: async () => {
        try {
          const dependenciesRes = await api.checkDependencies();
          set({ dependencies: dependenciesRes });
        } catch (err) {
          console.error("Failed to fetch dependencies:", err);
        }
      },

      fetchLicenseInfo: async () => {
        try {
          const licenseRes = await api.getLicenseInfo();
          set({ licenseInfo: licenseRes });
        } catch (err) {
          console.error("Failed to fetch license info:", err);
        }
      },

      fetchDisks: async () => {
        try {
          const [zpoolStatsRes, zpoolsRes] = await Promise.all([
            api.getZpoolList(),
            api.listZpools(),
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
          const ramUsageRes = await api.getRamUsage();
          set({ ramUsage: ramUsageRes });
        } catch (err) {
          console.error("Failed to fetch ram usage:", err);
        }
      },

      fetchArcStat: async () => {
        try {
          const arcStatRes = await api.getZfsArcstat();
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
          fetchMasters,
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
            fetchMasters(),
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
            const newClients = await api.listClients();

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
          const cfg = await api.readConfig();
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
      partialize: (state) => ({
        appConfig: state.appConfig,
      }),
    }
  )
);
