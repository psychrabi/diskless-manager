import { apiRequest } from "../client";

export function preflightLinuxImageBuild(request) {
  return apiRequest("/api/images/linux-builder/preflight", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export function buildLinuxImage(request) {
  return apiRequest("/api/images/linux-builder/build", {
    method: "POST",
    body: JSON.stringify(request),
  });
}
