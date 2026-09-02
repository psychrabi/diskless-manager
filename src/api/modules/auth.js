import { apiRequest, getAuthToken, setAuthToken } from "../client";

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

  setAuthToken(null);
  return data;
}

export async function validateAuthToken() {
  const token = getAuthToken();
  return apiRequest("/api/auth/validate", {
    method: "POST",
    body: JSON.stringify({ token }),
  });
}

export async function updateAdminPassword(passwordData) {
  const body =
    typeof passwordData === "string"
      ? { new_password: passwordData }
      : passwordData;

  return apiRequest("/api/auth/admin/password", {
    method: "PUT",
    body: JSON.stringify(body),
  });
}

export async function bootstrapAdmin(username, password) {
  return apiRequest("/api/auth/bootstrap", {
    method: "POST",
    body: JSON.stringify({ username, password }),
  });
}

export async function checkAdminExists() {
  return apiRequest("/api/auth/admin/exists");
}
