import { apiRequest, apiUpload } from "../client";

export function listInstallerMedia() {
  return apiRequest("/api/installer-media");
}

export function uploadInstallerMedia({ displayName, mediaType, file, onProgress }) {
  const body = new FormData();
  body.append("display_name", displayName);
  body.append("media_type", mediaType);
  body.append("file", file);
  return apiUpload("/api/installer-media/upload", body, { onProgress });
}

export function setDefaultInstallerMedia(id) {
  return apiRequest(`/api/installer-media/${encodeURIComponent(id)}/default`, { method: "POST" });
}

export function deleteInstallerMedia(id) {
  return apiRequest(`/api/installer-media/${encodeURIComponent(id)}`, { method: "DELETE" });
}

export function recordInstallerMediaVerification(id, verification) {
  return apiRequest(`/api/installer-media/${encodeURIComponent(id)}/verification`, {
    method: "POST",
    body: JSON.stringify(verification),
  });
}
