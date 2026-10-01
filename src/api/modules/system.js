import { apiRequest } from "../client";
export async function getSetupStatus() {
  return apiRequest("/api/system/setup");
}

export async function completeSetup() {
  return apiRequest("/api/system/setup/complete", { method: "POST" });
}

export async function getSystemInfo() {
  return apiRequest("/api/system/info");
}

export async function getServerStatus() {
  return apiRequest("/api/system/status");
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

export async function openEnrollment(minutes) {
  return apiRequest("/api/system/enrollment/open", {
    method: "POST",
    body: JSON.stringify({ minutes }),
  });
}

export async function closeEnrollment() {
  return apiRequest("/api/system/enrollment/close", {
    method: "POST",
  });
}

export async function checkPrivilegedAccess() {
  return apiRequest("/api/system/privileged-access");
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
