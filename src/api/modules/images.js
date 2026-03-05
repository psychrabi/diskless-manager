import { apiRequest } from "../client";

export async function listImages() {
  return apiRequest("/api/images");
}

export async function listMasters() {
  return apiRequest("/api/masters");
}

export async function getImage(id) {
  return apiRequest(`/api/images/${id}`);
}

export async function renameImage(id, newName) {
  return apiRequest(`/api/images/${id}/rename`, {
    method: "PUT",
    body: JSON.stringify({ new_name: newName }),
  });
}

export async function createImage(request) {
  return apiRequest("/api/images", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function importImage(request) {
  return apiRequest("/api/images/import", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function deleteImage(id) {
  return apiRequest(`/api/images/${id}`, { method: "DELETE" });
}

export async function cloneImage(sourceId, newName) {
  return apiRequest(`/api/images/${sourceId}/clone`, {
    method: "POST",
    body: JSON.stringify({ new_name: newName }),
  });
}

export async function createSnapshot(sourceId, snapshotName) {
  return apiRequest(`/api/images/${sourceId}/snapshots`, {
    method: "POST",
    body: JSON.stringify({ snapshot_name: snapshotName }),
  });
}

export async function getSnapshots(imageId) {
  return apiRequest(`/api/images/${imageId}/snapshots`, {
    method: "GET",
  });
}

export async function getImageInfo(id) {
  return apiRequest(`/api/images/${id}/info`);
}

export async function resizeImage(id, newSizeGb) {
  return apiRequest(`/api/images/${id}/resize`, {
    method: "POST",
    body: JSON.stringify({ new_size_gb: newSizeGb }),
  });
}

export async function verifyImage(id) {
  return apiRequest(`/api/images/${id}/verify`, { method: "POST" });
}

export async function listVersions(baseName) {
  return apiRequest(`/api/images/${baseName}/versions`);
}

export async function getVersionHistory(baseName) {
  return apiRequest(`/api/images/${baseName}/version-history`);
}

export async function deleteSnapshot(masterName, snapshotName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/snapshots/${encodeURIComponent(snapshotName)}`,
    {
      method: "DELETE",
    }
  );
}

export async function rollbackImageSnapshot(masterName, snapshotName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/snapshots/${encodeURIComponent(snapshotName)}/rollback`,
    {
      method: "POST",
    }
  );
}

export async function setDefaultImage(masterName) {
  return apiRequest(`/api/images/${encodeURIComponent(masterName)}/set-default`, {
    method: "POST",
  });
}
