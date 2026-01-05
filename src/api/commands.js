import { invoke } from "@tauri-apps/api/core";

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
    console.log(token);
  }

  const config = {
    ...options,
    headers,
  };

  const response = await fetch(url, config);

  if (!response.ok) {
    throw new Error(
      `API request failed: ${response.status} ${response.statusText}`
    );
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
  const response = await fetch("http://127.0.0.1:8080/api/auth/login", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ username, password }),
  });

  if (!response.ok) {
    throw new Error(`Login failed: ${response.status} ${response.statusText}`);
  }

  const data = await response.json();
  setAuthToken(data.token);
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

// Note: initializeServer and checkDependencies might still need to use invoke
// if they're not available through the API yet
export async function initializeServer() {
  return invoke("initialize_server");
}

export async function checkDependencies() {
  return invoke("check_dependencies");
}

export async function clearRamCache() {
  // This command might not be available through the API yet
  return invoke("clear_ram_cache");
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
  return apiRequest(`/api/services/${name}/restart`, { method: "POST" });
}

// Note: startAllServices, stopAllServices, restartAllServices might not be available through the API yet
export async function startAllServices() {
  return invoke("start_all_services");
}

export async function stopAllServices() {
  return invoke("stop_all_services");
}

export async function restartAllServices() {
  return invoke("restart_all_services");
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
  return apiRequest(`/api/clients/${id}`, { method: "DELETE" });
}

export async function getClientBootHistory(clientId, limit) {
  const params = limit ? `?limit=${limit}` : "";
  return apiRequest(`/api/clients/${clientId}/boot-history${params}`);
}

// ============================================================================
// Image Commands
// ============================================================================

export async function listImages() {
  return invoke("/api/images");
}

export async function listMasters() {
  // return apiRequest("/api/masters");
  return invoke("get_images");
}

export async function getImage(id) {
  return apiRequest(`/api/images/${id}`);
}

export async function createImage(request) {
  return apiRequest("/api/images", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

// Note: importImage, deleteImage, cloneImage, createSnapshot, etc. might not be available through the API yet
export async function importImage(request) {
  return invoke("import_image", { request });
}

export async function deleteImage(id, force) {
  // API doesn't currently support force parameter
  return apiRequest(`/api/images/${id}`, { method: "DELETE" });
}

export async function cloneImage(sourceId, newName) {
  return invoke("clone_image", { sourceId, newName });
}

export async function createSnapshot(sourceId, snapshotName) {
  return invoke("create_snapshot_command", { sourceId, snapshotName });
}

export async function getImageInfo(id) {
  return invoke("get_image_info", { id });
}

export async function resizeImage(id, newSizeGb) {
  return invoke("resize_image", { id, newSizeGb });
}

export async function verifyImage(id) {
  return invoke("verify_image", { id });
}

// ============================================================================
// Version Commands
// ============================================================================

export async function listVersions(baseName) {
  return invoke("list_versions", { baseName });
}

export async function getVersionHistory(baseName) {
  return invoke("get_version_history", { baseName });
}

// ============================================================================
// Settings Commands
// ============================================================================

export async function getSettings() {
  return invoke("get_settings");
}

export async function saveSettings(settings) {
  return invoke("save_settings", { settings });
}
