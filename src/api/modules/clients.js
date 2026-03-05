import { apiRequest } from "../client";

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
