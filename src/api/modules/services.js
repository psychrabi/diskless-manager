import { apiRequest } from "../client";

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

export async function getServiceConfig(serviceName) {
  return apiRequest(`/api/services/${encodeURIComponent(serviceName)}/config`);
}

export async function saveServiceConfig(serviceName, config) {
  return apiRequest(`/api/services/${encodeURIComponent(serviceName)}/configure`, {
    method: "POST",
    body: JSON.stringify(config),
  });
}

export async function configureService(serviceName) {
  return apiRequest(`/api/services/${encodeURIComponent(serviceName)}/configure`, {
    method: "POST",
  });
}
