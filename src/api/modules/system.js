import { apiRequest } from "../client";
import { checkZfsPoolExists } from "./disks";

/**
 * Run the setup preflight check shared by the public and admin layouts.
 * Returns the normalized dependency list and whether all services are
 * installed and the ZFS pool exists.
 */
export async function runPreflightCheck() {
  const res = await checkDependencies();
  const list = Array.isArray(res) ? res : res ? Object.values(res) : [];
  const allServicesInstalled = list.every((svc) => svc?.installed);
  const poolExists = await checkZfsPoolExists();
  return { list, allServicesInstalled, poolExists };
}

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

export async function getSettings() {
  return apiRequest("/api/system/settings");
}

export async function saveSettings(settings) {
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

export async function getRamUsage() {
  return apiRequest("/api/system/ram-usage");
}

export async function getZfsArcstat() {
  return apiRequest("/api/system/zfs-arcstat");
}
