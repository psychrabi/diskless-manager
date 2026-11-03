import { apiRequest } from "../client";

export async function listImages() {
  return apiRequest("/api/images");
}

export async function listMasters() {
  return apiRequest("/api/masters");
}

/** Published image versions (ADR 0009), newest first per base name. */
export async function listImageVersions(baseName, all = false) {
  const params = new URLSearchParams();
  if (baseName) params.set("base_name", baseName);
  if (all) params.set("all", "true");
  const query = params.toString();
  return apiRequest(`/api/image-versions${query ? `?${query}` : ""}`);
}

export async function deleteImageVersion(id) {
  return apiRequest(`/api/image-versions/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

/** Publish a committed Windows servicing clone as a new image version. */
export async function publishImageVersion(request) {
  return apiRequest("/api/image-versions/post-commit", {
    method: "POST",
    body: JSON.stringify(request),
  });
}

export async function validateWindowsComputerName(computerName) {
  return apiRequest(
    `/api/image-versions/validate-computer-name?computer_name=${encodeURIComponent(computerName ?? "")}`
  );
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
  return apiRequest(`/api/images/${encodeURIComponent(sourceId)}/snapshots`, {
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

export async function rollbackImageSnapshot(masterName, snapshotName, newerSnapshotNames = []) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/snapshots/${encodeURIComponent(snapshotName)}/rollback`,
    {
      method: "POST",
      body: JSON.stringify({
        prune_newer_snapshots: newerSnapshotNames.length > 0,
        expected_newer_snapshots: newerSnapshotNames,
      }),
    },
  );
}

export async function applySnapshot(masterName, snapshotName, newName) {
  return apiRequest(
    `/api/images/${encodeURIComponent(masterName)}/snapshots/${encodeURIComponent(snapshotName)}/apply`,
    {
      method: "POST",
      body: JSON.stringify({ new_name: newName }),
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

export async function getDriverReadiness(imageId) {
  return apiRequest(`/api/images/${encodeURIComponent(imageId)}/driver-readiness`);
}

export async function getImageHealth(imageId) {
  return apiRequest(`/api/images/${encodeURIComponent(imageId)}/health`);
}
