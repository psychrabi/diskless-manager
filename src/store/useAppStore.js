import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";
import { shallow } from "zustand/shallow";
import * as api from "../api/commands";

// Export shallow comparison helper for consumers
export { shallow };

const normalizeServices = (servicesRes) => {
  if (Array.isArray(servicesRes)) return servicesRes;
  if (servicesRes) return Object.values(servicesRes);
  return [];
};

const updateServiceStatus = (services, name, status) =>
  services.map((service) =>
    service.name === name ? { ...service, status } : service
  );

const hasClientStatusChanges = (currentClients, newClients) => {
  if (currentClients.length !== newClients.length) {
    return true;
  }

  const newClientsMap = new Map();
  newClients.forEach((client) => {
    if (client.id) newClientsMap.set(client.id, client);
  });

  if (newClientsMap.size === 0) {
    return JSON.stringify(currentClients) !== JSON.stringify(newClients);
  }

  for (const oldClient of currentClients) {
    if (!oldClient.id) return true;

    const newClient = newClientsMap.get(oldClient.id);
    if (!newClient) return true;

    if (
      oldClient.status !== newClient.status ||
      oldClient.online !== newClient.online ||
      oldClient.ip !== newClient.ip
    ) {
      return true;
    }
  }

  return false;
};

export const useAppStore = create()(
  persist(
    (set, get) => {
      const runRequest = async (request, onSuccess, errorPrefix) => {
        try {
          const result = await request();
          onSuccess(result);
          return result;
        } catch (err) {
          console.error(errorPrefix, err);
          return null;
        }
      };

      const runServiceAction = async ({
        name,
        action,
        status,
        errorPrefix,
      }) => {
        try {
          await action(name);
          set({
            services: updateServiceStatus(get().services, name, status),
          });
          await get().fetchServices();
        } catch (err) {
          console.error(errorPrefix, err);
        }
      };

      return {
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

        serverInfo: null, // System information (hostname, OS, etc.)
        serverStatus: null, // Server status (services, clients, images counts)
        licenseInfo: null, // Added
        _pollIntervalId: null, // Added

        fetchClients: async () =>
          runRequest(
            () => api.listClients(),
            (clientsData) => set({ clients: clientsData || [] }),
            "Failed to fetch clients:"
          ),

        fetchMasters: async () =>
          runRequest(
            () => api.listMasters(),
            (mastersRes) => set({ masters: mastersRes || [] }),
            "Failed to fetch masters:"
          ),

        fetchImages: async () =>
          runRequest(
            () => api.listImages(),
            (imagesRes) => set({ images: imagesRes || [] }),
            "Failed to fetch images:"
          ),

        fetchDatasets: async (zpool) => {
          if (!zpool) {
            set({ datasets: [] });
            return;
          }

          await runRequest(
            () => api.listDatasets(zpool),
            (datasetsRes) => set({ datasets: datasetsRes || [] }),
            "Failed to fetch datasets:"
          );
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

        fetchServices: async () =>
          runRequest(
            () => api.listServices(),
            (servicesRes) => set({ services: normalizeServices(servicesRes) }),
            "Failed to fetch services:"
          ),

        startService: async (name) =>
          runServiceAction({
            name,
            action: api.startService,
            status: "running",
            errorPrefix: "Failed to start service:",
          }),

        stopService: async (name) =>
          runServiceAction({
            name,
            action: api.stopService,
            status: "stopped",
            errorPrefix: "Failed to stop service:",
          }),

        restartService: async (name) =>
          runServiceAction({
            name,
            action: api.restartService,
            status: "restarting",
            errorPrefix: "Failed to restart service:",
          }),

        fetchServerInfo: async () =>
          runRequest(
            () => api.getSystemInfo(),
            (serverInfoRes) => set({ serverInfo: serverInfoRes }),
            "Failed to fetch server info:"
          ),

        fetchServerStatus: async () =>
          runRequest(
            () => api.getServerStatus(),
            (serverStatusRes) => set({ serverStatus: serverStatusRes }),
            "Failed to fetch server status:"
          ),

        fetchDependencies: async () =>
          runRequest(
            () => api.checkDependencies(),
            (dependenciesRes) => set({ dependencies: dependenciesRes }),
            "Failed to fetch dependencies:"
          ),

        fetchLicenseInfo: async () =>
          runRequest(
            () => api.getLicenseInfo(),
            (licenseRes) => set({ licenseInfo: licenseRes }),
            "Failed to fetch license info:"
          ),

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

        fetchRamUsage: async () =>
          runRequest(
            () => api.getRamUsage(),
            (ramUsageRes) => set({ ramUsage: ramUsageRes }),
            "Failed to fetch ram usage:"
          ),

        fetchArcStat: async () =>
          runRequest(
            () => api.getZfsArcstat(),
            (arcStatRes) => set({ arcStat: arcStatRes }),
            "Failed to fetch arc stat:"
          ),

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
            fetchServerStatus,
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
              fetchServerStatus(),
              fetchDisks(),
              fetchLicenseInfo(),
              fetchRamUsage(),
              fetchArcStat(),
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
              const newClients = await api.listClients();
              const { clients: currentClients } = get();

              if (hasClientStatusChanges(currentClients, newClients)) {
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
      };
    },
    {
      name: "diskless", // name of the item in the storage (must be unique)
      storage: createJSONStorage(() => localStorage), // (optional) by default, 'localStorage' is used
      partialize: (state) => ({
        appConfig: state.appConfig,
      }),
    }
  )
);
