import { invoke } from "@tauri-apps/api/core";

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
// System Commands
// ============================================================================

export async function getSystemInfo() {
  return invoke("get_system_info");
}

export async function getServerStatus() {
  return invoke("get_server_status");
}

export async function initializeServer() {
  return invoke("initialize_server");
}

export async function checkDependencies() {
  return invoke("check_dependencies");
}

export async function clearRamCache() {
  await invoke("clear_ram_cache");
}

// ============================================================================
// Service Commands
// ============================================================================

export async function listServices() {
  return invoke("list_services");
}

export async function getServiceStatus(name) {
  return invoke("get_service_status", { name });
}

export async function startService(name) {
  return invoke("start_service", { name });
}

export async function stopService(name) {
  return invoke("stop_service", { name });
}

export async function restartService(name) {
  return invoke("restart_service", { name });
}

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
  return invoke("list_clients");
}

export async function getClient(id) {
  return invoke("get_client", { id });
}

export async function addClient(request) {
  return invoke("add_client", { request });
}

export async function updateClient(id, request) {
  return invoke("update_client", { id, request });
}

export async function deleteClient(id) {
  return invoke("delete_client", { id });
}

export async function getClientBootHistory(clientId, limit) {
  return invoke("get_client_boot_history", { clientId, limit });
}

// ============================================================================
// Image Commands
// ============================================================================

export async function listImages() {
  return invoke("list_images");
}

export async function getImage(id) {
  return invoke("get_image", { id });
}

export async function createImage(request) {
  return invoke("create_image_command", { request });
}

export async function importImage(request) {
  return invoke("import_image", { request });
}

export async function deleteImage(id, force) {
  return invoke("delete_image_command", { id, force: force ?? false });
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
