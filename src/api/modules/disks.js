import { apiRequest } from "../client";

export async function listDisks() {
  return apiRequest("/api/disks");
}

export async function renameDisk(diskName, newName) {
  return apiRequest(`/api/disks/${diskName}/rename`, {
    method: "PUT",
    body: JSON.stringify({ new_name: newName }),
  });
}

export async function createZfsPool(poolConfig) {
  return apiRequest("/api/disks/pool", {
    method: "POST",
    body: JSON.stringify(poolConfig),
  });
}

export async function checkZfsPoolExists() {
  return apiRequest("/api/disks/pool/exists");
}
