import { apiRequest } from "../client";

export async function listDatasets(zpool) {
  return apiRequest(`/api/zfs/datasets?zpool=${zpool}`);
}

export async function createZfsDataset(req) {
  return apiRequest("/api/zfs/datasets", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function deleteZfsDataset(dataset, recursive = true) {
  return apiRequest(`/api/zfs/datasets/${dataset}`, {
    method: "DELETE",
    body: JSON.stringify({ recursive }),
  });
}

export async function getZpoolList() {
  return apiRequest("/api/zfs/pools/stats");
}

export async function listZpools() {
  return apiRequest("/api/zfs/pools");
}
