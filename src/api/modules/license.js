import { apiRequest } from "../client";

export async function getLicenseInfo() {
  return apiRequest("/api/license/info");
}

export async function activateLicense(key) {
  return apiRequest("/api/license/activate", {
    method: "POST",
    body: JSON.stringify({ key }),
  });
}
