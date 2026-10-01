import { apiRequest } from "../client";

export async function getWindowsServicingCapabilities() {
  return apiRequest("/api/pxe/windows/servicing");
}

export async function getRemoteWindowsServicingCapabilities(request) {
  return apiRequest("/api/pxe/windows/servicing/remote", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function prepareWindowsImage(request) {
  return apiRequest("/api/pxe/windows/prepare-image", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function prepareWindowsImageRemote(request) {
  return apiRequest("/api/pxe/windows/prepare-image/remote", {
    method: "POST",
    body: JSON.stringify(request),
  });
}


export async function prepareWindowsImageRemoteCatalog(request) {
  return apiRequest("/api/pxe/windows/prepare-image/remote/catalog", {
    method: "POST",
    body: JSON.stringify(request),
  });
}
