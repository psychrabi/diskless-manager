import { apiRequest } from "../client";

export async function readConfig() {
  return apiRequest("/api/config");
}
