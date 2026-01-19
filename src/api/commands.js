// ============================================================================
// API Client
// ============================================================================

let authToken = null;

// Function to set the auth token after login
export function setAuthToken(token) {
  authToken = token;
  // Store in localStorage for persistence
  localStorage.setItem("authToken", token);
}

// Function to get the auth token
function getAuthToken() {
  if (!authToken) {
    authToken = localStorage.getItem("authToken");
  }
  return authToken;
}

// Function to make API requests
async function apiRequest(endpoint, options = {}) {
  const url = `http://127.0.0.1:8080${endpoint}`;
  const headers = {
    "Content-Type": "application/json",
    ...options.headers,
  };

  const token = getAuthToken();
  if (token) {
    headers["Authorization"] = `Bearer ${token}`;
  }

  const config = {
    ...options,
    headers,
  };

  const response = await fetch(url, config);

  if (!response.ok) {
    // Try to extract error message from response body
    let errorMessage = `API request failed: ${response.status} ${response.statusText}`;
    try {
      const contentType = response.headers.get("content-type");
      if (contentType && contentType.includes("application/json")) {
        const errorData = await response.json();
        if (errorData.error) {
          errorMessage = errorData.error;
        }
      }
    } catch {
      // If we can't parse the error response, use the default message
    }
    throw new Error(errorMessage);
  }

  // For endpoints that don't return JSON (like some delete operations)
  const contentType = response.headers.get("content-type");
  if (contentType && contentType.includes("application/json")) {
    return response.json();
  } else {
    return response.text();
  }
}

// ============================================================================
// Types
// ============================================================================

// export interface SystemInfo {
//   hostname: string;
//   os: string;
//   kernel: string;
//   uptime: string;
//   cpu_count: number;
//   memory_total: string;
//   memory_available: string;
// }

// export interface ServerStatus {
//   initialized: boolean;
//   services_running: number;
//   services_total: number;
//   clients_count: number;
//   images_count: number;
// }

// export interface DependencyStatus {
//   name: string;
//   installed: boolean;
//   version: string | null;
// }

// export interface ServiceInfo {
//   name: string;
//   display_name: string;
//   running: boolean;
//   enabled: boolean;
//   pid: number | null;
// }

// export interface ServiceStatus {
//   name: string;
//   active: boolean;
//   status: string;
//   pid: number | null;
//   memory: string | null;
//   uptime: string | null;
// }

// export interface Client {
//   id: string;
//   name: string;
//   mac_address: string;
//   ip_address: string | null;
//   image_id: string;
//   boot_mode: string;
//   enabled: boolean;
//   created_at: string;
//   updated_at: string;
// }

// export interface CreateClientRequest {
//   name: string;
//   mac_address: string;
//   ip_address?: string;
//   image_id: string;
//   boot_mode?: string;
// }

// export interface UpdateClientRequest {
//   name?: string;
//   ip_address?: string;
//   image_id?: string;
//   boot_mode?: string;
//   enabled?: boolean;
// }

// export interface BootLogEntry {
//   id: string;
//   client_id: string;
//   image_id: string | null;
//   boot_time: string;
//   success: boolean;
//   duration_ms: number | null;
//   message: string | null;
// }

// export interface Image {
//   id: string;
//   name: string;
//   os_type: "linux" | "windows";
//   size_gb: number;
//   path: string;
//   format: string;
//   status: string;
//   description: string | null;
//   parent_id: string | null;
//   checksum: string | null;
//   is_default: boolean;
//   created_at: string;
//   updated_at: string;
// }

// export interface CreateImageRequest {
//   name: string;
//   os_type: string;
//   size_gb: number;
//   format?: string;
//   description?: string;
// }

// export interface ImportImageRequest {
//   name: string;
//   source_path: string;
//   os_type: string;
//   description?: string;
// }

// export interface ImageInfo {
//   virtual_size: number;
//   actual_size: number;
//   format: string;
//   backing_file: string | null;
//   snapshots: string[];
// }

// export interface VersionInfo {
//   id: string;
//   base_name: string;
//   version: string;
//   image_id: string;
//   changelog: string | null;
//   is_latest: boolean;
//   is_stable: boolean;
//   created_at: string;
// }

// export interface Settings {
//   server: {
//     interface: string,
//     ip_address: string,
//     hostname: string,
//     domain: string,
//   };
//   dhcp: {
//     enabled: boolean,
//     range_start: string,
//     range_end: string,
//     subnet_mask: string,
//     gateway: string,
//     dns_servers: string[],
//     lease_time: number,
//   };
//   tftp: {
//     enabled: boolean,
//     root_dir: string,
//     port: number,
//   };
//   iscsi: {
//     enabled: boolean,
//     target_prefix: string,
//     portal_port: number,
//     targets_dir: string,
//   };
//   nfs: {
//     enabled: boolean,
//     exports_dir: string,
//   };
//   samba: {
//     enabled: boolean,
//     workgroup: string,
//     share_name: string,
//     share_path: string,
//     read_only: boolean,
//     guest_ok: boolean,
//   };
//   storage: {
//     images_dir: string,
//     snapshots_dir: string,
//   };
// }

// ============================================================================
// Authentication
// ============================================================================

export async function login(username, password) {
  const data = await apiRequest("/api/auth/login", {
    method: "POST",
    body: JSON.stringify({ username, password }),
  });

  setAuthToken(data.token);
  return data;
}

export async function logout() {
  const data = await apiRequest("/api/auth/logout", {
    method: "POST",
  });

  // Clear the auth token
  setAuthToken(null);
  return data;
}

// ============================================================================
// System Commands
// ============================================================================

export async function getSystemInfo() {
  return apiRequest("/api/system/info");
}

export async function getServerStatus() {
  return apiRequest("/api/system/status");
}

export async function initializeServer() {
  return apiRequest("/api/system/initialize", { method: "POST" });
}

export async function checkDependencies() {
  return apiRequest("/api/system/dependencies");
}

export async function clearRamCache() {
  return apiRequest("/api/system/cache/clear", { method: "POST" });
}

export async function getSystemSettings() {
  return apiRequest("/api/system/settings");
}

export async function saveSystemSettings(settings) {
  return apiRequest("/api/system/settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}

export async function setupPrivilegedAccess(config) {
  return apiRequest("/api/system/privileged-access", {
    method: "POST",
    body: JSON.stringify(config),
  });
}

// ============================================================================
// Service Commands
// ============================================================================

export async function listServices() {
  return apiRequest("/api/services");
}

export async function getServiceStatus(name) {
  return apiRequest(`/api/services/${name}/status`);
}

export async function startService(name) {
  return apiRequest(`/api/services/${name}/start`, { method: "POST" });
}

export async function stopService(name) {
  return apiRequest(`/api/services/${name}/stop`, { method: "POST" });
}

export async function restartService(name) {
  return apiRequest(`/api/services/${encodeURIComponent(name)}/restart`, {
    method: "POST",
  });
}

export async function startAllServices() {
  return apiRequest("/api/services/all/start", { method: "POST" });
}

export async function stopAllServices() {
  return apiRequest("/api/services/all/stop", { method: "POST" });
}

export async function restartAllServices() {
  return apiRequest("/api/services/all/restart", { method: "POST" });
}

export async function configureServiceConfig(name, config) {
  return apiRequest(`/api/services/${name}/configure`, {
    method: "POST",
    body: JSON.stringify(config),
  });
}

// ============================================================================
// Client Commands
// ============================================================================

export async function listClients() {
  return apiRequest("/api/clients");
}

export async function getClient(id) {
  return apiRequest(`/api/clients/${id}`);
}

export async function addClient(request) {
  return apiRequest("/api/clients", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function updateClient(id, request) {
  return apiRequest(`/api/clients/${id}`, {
    method: "PUT",
    body: JSON.stringify(request),
  });
}

export async function deleteClient(id) {
  return apiRequest(`/api/clients/${id}`, {
    method: "DELETE",
  });
}

export async function getClientBootHistory(clientId, limit) {
  const params = limit ? `?limit=${limit}` : "";
  return apiRequest(`/api/clients/${clientId}/boot-history${params}`);
}

// ============================================================================
// Image Commands
// ============================================================================

export async function listImages() {
  return apiRequest("/api/images");
}

export async function listMasters() {
  return apiRequest("/api/masters");
}

export async function getImage(id) {
  return apiRequest(`/api/images/${id}`);
}

export async function renameImage(id, newName) {
  return apiRequest(`/api/images/${id}/rename`, {
    method: "PUT",
    body: JSON.stringify({ new_name: newName }),
  });
}

export async function createImage(request) {
  return apiRequest("/api/images", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function importImage(request) {
  return apiRequest("/api/images/import", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function deleteImage(id) {
  return apiRequest(`/api/images/${id}`, { method: "DELETE" });
}

export async function cloneImage(sourceId, newName) {
  return apiRequest(`/api/images/${sourceId}/clone`, {
    method: "POST",
    body: JSON.stringify({ new_name: newName }),
  });
}

export async function createSnapshot(sourceId, snapshotName) {
  return apiRequest(`/api/images/${sourceId}/snapshots`, {
    method: "POST",
    body: JSON.stringify({ snapshot_name: snapshotName }),
  });
}

export async function getSnapshots(imageId) {
  return apiRequest(`/api/images/${imageId}/snapshots`, {
    method: "GET",
  });
}

export async function getImageInfo(id) {
  return apiRequest(`/api/images/${id}/info`);
}

export async function resizeImage(id, newSizeGb) {
  return apiRequest(`/api/images/${id}/resize`, {
    method: "POST",
    body: JSON.stringify({ new_size_gb: newSizeGb }),
  });
}

export async function verifyImage(id) {
  return apiRequest(`/api/images/${id}/verify`, { method: "POST" });
}

// ============================================================================
// Version Commands
// ============================================================================

export async function listVersions(baseName) {
  return apiRequest(`/api/images/${baseName}/versions`);
}

export async function getVersionHistory(baseName) {
  return apiRequest(`/api/images/${baseName}/version-history`);
}

// ============================================================================
// Disk Commands
// ============================================================================

export async function listDisks() {
  return apiRequest("/api/disks");
}

export async function renameDisk(diskName, newName) {
  return apiRequest(`/api/disks/${diskName}/rename`, {
    method: "PUT",
    body: JSON.stringify({ new_name: newName }),
  });
}

export async function createZfsPool(poolConfig) {
  return apiRequest("/api/disks/pool", {
    method: "POST",
    body: JSON.stringify(poolConfig),
  });
}

export async function checkZfsPoolExists() {
  return apiRequest("/api/disks/pool/exists");
}

// ============================================================================
// Authentication Commands
// ============================================================================

export async function validateAuthToken() {
  const token = getAuthToken();
  return apiRequest("/api/auth/validate", {
    method: "POST",
    body: JSON.stringify({ token }),
  });
}

export async function updateAdminPassword(passwordData) {
  // Support both object format and legacy string format
  const body = typeof passwordData === 'string'
    ? { new_password: passwordData }
    : passwordData;

  return apiRequest("/api/auth/admin/password", {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export async function checkAdminExists() {
  return apiRequest("/api/auth/admin/exists");
}

// ============================================================================
// Logs Commands
// ============================================================================

export async function getLogs(unit = null, lines = 50) {
  const params = new URLSearchParams();
  if (unit) {
    params.append("unit", unit);
  }
  params.append("lines", lines);
  return apiRequest(`/api/logs?${params.toString()}`);
}

export async function clearLogs() {
  return apiRequest("/api/logs", { method: "DELETE" });
}

// ============================================================================
// License Commands
// ============================================================================

export async function getLicenseInfo() {
  return apiRequest("/api/license/info");
}

// ============================================================================
// Settings Commands
// ============================================================================

export async function getSettings() {
  return apiRequest("/api/system/settings");
}

export async function saveSettings(settings) {
  return apiRequest("/api/system/settings", {
    method: "PUT",
    body: JSON.stringify(settings),
  });
}

// ============================================================================
// Dashboard Commands
// ============================================================================

export async function getDefaultImageOverview() {
  return apiRequest("/api/dashboard/default-image");
}

export async function getClientOverview() {
  return apiRequest("/api/dashboard/clients");
}

export async function getClientIOMetrics() {
  return apiRequest("/api/dashboard/clients/io-metrics");
}

// ============================================================================
// Service Installation Commands
// ============================================================================

export async function installService(service) {
  return apiRequest("/api/services/install", {
    method: "POST",
    body: JSON.stringify({ service }),
  });
}

export async function configureSambaServer(shares) {
  return apiRequest("/api/services/samba/configure", {
    method: "POST",
    body: JSON.stringify({ shares }),
  });
}

// ============================================================================
// ZFS Commands
// ============================================================================

export async function listDatasets(zpool) {
  return apiRequest(`/api/zfs/datasets?zpool=${zpool}`);
}

export async function createZfsDataset(req) {
  return apiRequest("/api/zfs/datasets", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function deleteZfsDataset(dataset, recursive = true) {
  return apiRequest(`/api/zfs/datasets/${dataset}`, {
    method: "DELETE",
    body: JSON.stringify({ recursive }),
  });
}

export async function getZpoolList() {
  return apiRequest("/api/zfs/pools/stats");
}

export async function listZpools() {
  return apiRequest("/api/zfs/pools");
}

// ============================================================================
// System Monitoring Commands
// ============================================================================

export async function getRamUsage() {
  return apiRequest("/api/system/ram-usage");
}

export async function getZfsArcstat() {
  return apiRequest("/api/system/zfs-arcstat");
}

// ============================================================================
// Configuration Commands
// ============================================================================

export async function readConfig() {
  return apiRequest("/api/config");
}

// ============================================================================
// Network Commands
// ============================================================================

export async function getNetworkInterfaces() {
  return apiRequest("/api/system/network/interfaces");
}

export async function getInterfaceIp(interfaceName) {
  return apiRequest(
    `/api/system/network/interfaces/${encodeURIComponent(interfaceName)}/ip`
  );
}

export async function detectServerNetwork() {
  return apiRequest("/api/system/network/detect", { method: "POST" });
}

export async function applyNetworkSettings(settings) {
  return apiRequest("/api/system/network/apply", {
    method: "POST",
    body: JSON.stringify(settings),
  });
}

// ============================================================================
// Service Configuration Commands
// ============================================================================

export async function getServiceConfig(serviceName) {
  return apiRequest(`/api/services/${encodeURIComponent(serviceName)}/config`);
}

export async function saveServiceConfig(serviceName, config) {
  return apiRequest(
    `/api/services/${encodeURIComponent(serviceName)}/configure`,
    {
      method: "POST",
      body: JSON.stringify(config),
    }
  );
}

export async function configureService(serviceName) {
  return apiRequest(
    `/api/services/${encodeURIComponent(serviceName)}/configure`,
    {
      method: "POST",
    }
  );
}

// ============================================================================
// Snapshot Commands
// ============================================================================

export async function deleteSnapshot(masterName, snapshotName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(
      masterName
    )}/snapshots/${encodeURIComponent(snapshotName)}`,
    {
      method: "DELETE",
    }
  );
}

export async function rollbackImageSnapshot(masterName, snapshotName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(
      masterName
    )}/snapshots/${encodeURIComponent(snapshotName)}/rollback`,
    {
      method: "POST",
    }
  );
}

export async function setDefaultImage(masterName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/set-default`,
    {
      method: "POST",
    }
  );
}

// ============================================================================
// License Commands (Additional)
// ============================================================================

export async function activateLicense(key) {
  return apiRequest("/api/license/activate", {
    method: "POST",
    body: JSON.stringify({ key }),
  });
}

// ============================================================================
// Control Operations Commands
// ============================================================================

export async function shutdownClient(clientId, options = {}) {
  return apiRequest(`/api/clients/${clientId}/shutdown`, {
    method: "POST",
    body: JSON.stringify({
      force: options.force || false,
      delay_minutes: options.delay_minutes || null,
    }),
  });
}

export async function rebootClient(clientId, options = {}) {
  return apiRequest(`/api/clients/${clientId}/reboot`, {
    method: "POST",
    body: JSON.stringify({
      force: options.force || false,
      delay_minutes: options.delay_minutes || null,
    }),
  });
}

export async function remoteDesktopClient(clientId, credentials = {}) {
  return apiRequest(`/api/clients/${clientId}/remote-desktop`, {
    method: "POST",
    body: JSON.stringify({
      username: credentials.username || "diskless",
      password: credentials.password || "1",
    }),
  });
}

export async function cancelScheduledOperation(operationId) {
  return apiRequest(`/api/operations/${operationId}/cancel`, {
    method: "POST",
  });
}

export async function getAuditLogs(filters = {}) {
  const params = new URLSearchParams();
  if (filters.client_id) params.append("client_id", filters.client_id);
  if (filters.operation_type) params.append("operation_type", filters.operation_type);
  if (filters.start_date) params.append("start_date", filters.start_date);
  if (filters.end_date) params.append("end_date", filters.end_date);

  return apiRequest(`/api/audit-logs?${params.toString()}`);
}

// ============================================================================
// Scheduled Operations Commands
// ============================================================================

export async function getScheduledOperations(filters = {}) {
  const params = new URLSearchParams();
  if (filters.client_id) params.append("client_id", filters.client_id);
  if (filters.status) params.append("status", filters.status);

  return apiRequest(`/api/scheduled-operations?${params.toString()}`);
}

// ============================================================================
// User Management Commands
// ============================================================================


export async function listUsers() {
  return apiRequest("/api/users");
}

export async function getUser(userId) {
  return apiRequest(`/api/users/${userId}`);
}

export async function createUser(userData) {
  return apiRequest("/api/users", {
    method: "POST",
    body: JSON.stringify(userData),
  });
}

export async function updateUser(userId, updates) {
  return apiRequest(`/api/users/${userId}`, {
    method: "PUT",
    body: JSON.stringify(updates),
  });
}

export async function updateUserPassword(userId, password) {
  return apiRequest(`/api/users/${userId}/password`, {
    method: "PUT",
    body: JSON.stringify({ password }),
  });
}

export async function deleteUser(userId) {
  return apiRequest(`/api/users/${userId}`, {
    method: "DELETE",
  });
}

// ============================================================================
// SSH Commands
// ============================================================================

export async function testSshConnection(host, username, port = 22) {
  return apiRequest("/api/ssh/test-connection", {
    method: "POST",
    body: JSON.stringify({ host, username, port }),
  });
}

export async function executeSshCommand(host, username, command) {
  return apiRequest("/api/ssh/execute-command", {
    method: "POST",
    body: JSON.stringify({ host, username, command }),
  });
}

export async function getWindowsSystemInfo(host, username) {
  return apiRequest("/api/ssh/system-info", {
    method: "POST",
    body: JSON.stringify({ host, username }),
  });
}
