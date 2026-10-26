import { apiRequest } from "../client";

export async function listClients() {
  return apiRequest("/api/clients");
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

export function completeClientInstallation(id) {
  return apiRequest(`/api/clients/${encodeURIComponent(id)}/installation/complete`, {
    method: "POST",
  });
}

export async function exitLegacySuper(id, snapshot) {
  return apiRequest(`/api/clients/${encodeURIComponent(id)}/legacy-super/exit`, {
    method: "POST",
    body: JSON.stringify({ snapshot }),
  });
}

export function getSuperStatus(id) {
  return apiRequest(`/api/clients/${encodeURIComponent(id)}/super/status`);
}

export function enterSuper(id, options) {
  return apiRequest(`/api/clients/${encodeURIComponent(id)}/super/enter`, {
    method: "POST",
    body: JSON.stringify(options),
  });
}

export function saveSuper(id) {
  return apiRequest(`/api/clients/${encodeURIComponent(id)}/super/save`, { method: "POST" });
}

export function discardSuper(id) {
  return apiRequest(`/api/clients/${encodeURIComponent(id)}/super/discard`, { method: "POST" });
}

export async function deleteClient(id) {
  return apiRequest(`/api/clients/${id}`, {
    method: "DELETE",
  });
}

export async function deleteClientWriteback(id) {
  return apiRequest(`/api/clients/${id}/writeback/delete`, { method: "POST" });
}

export async function rotateChapSecret(id) {
  return apiRequest(`/api/clients/${id}/chap/rotate`, {
    method: "POST",
  });
}

export async function getClientNvmeOfStatus(clientId) {
  return apiRequest(`/api/clients/${clientId}/nvmeof`);
}

export async function prepareClientNvmeOf(clientId, serverIp) {
  return apiRequest(`/api/clients/${clientId}/nvmeof/prepare`, {
    method: "POST",
    body: JSON.stringify({ server_ip: serverIp }),
  });
}

export async function removeClientNvmeOf(clientId) {
  return apiRequest(`/api/clients/${clientId}/nvmeof`, {
    method: "DELETE",
  });
}
