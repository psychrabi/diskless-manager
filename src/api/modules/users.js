import { apiRequest } from "../client";

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
