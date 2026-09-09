import { apiRequest } from "../client";

export async function listImages() {
  return apiRequest("/api/images");
}

export async function listMasters() {
  return apiRequest("/api/masters");
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

export async function scanAndImportImages() {
  return apiRequest("/api/images/import-scan", { method: "POST" });
}

export async function deleteImage(id) {
  return apiRequest(`/api/images/${id}`, { method: "DELETE" });
}

export async function createSnapshot(sourceId, snapshotName) {
  return apiRequest(`/api/images/${sourceId}/snapshots`, {
    method: "POST",
    body: JSON.stringify({ snapshot_name: snapshotName }),
  });
}

export async function deleteSnapshot(masterName, snapshotName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/snapshots/${encodeURIComponent(snapshotName)}`,
    {
      method: "DELETE",
    },
  );
}

export async function rollbackImageSnapshot(masterName, snapshotName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/snapshots/${encodeURIComponent(snapshotName)}/rollback`,
    {
      method: "POST",
    },
  );
}

export async function setDefaultImage(masterName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/set-default`,
    {
      method: "POST",
    },
  );
}
