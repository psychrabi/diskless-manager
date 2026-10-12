import { apiRequest } from "../client";

export async function listNetworkDrivers() {
  return apiRequest("/api/pxe/network-drivers");
}

export async function getNetworkDriverStatus() {
  return apiRequest("/api/pxe/network-drivers/status");
}

export async function listDriverRevisions(sourceImageId) {
  return apiRequest(
    `/api/pxe/windows/revisions?source_image_id=${encodeURIComponent(sourceImageId)}`,
  );
}

export async function listBootTests(revisionId) {
  return apiRequest(
    `/api/pxe/windows/boot-tests?revision_id=${encodeURIComponent(revisionId)}`,
  );
}

export async function startBootTest({ revisionId, testClientId, expectedNicId }) {
  return apiRequest("/api/pxe/windows/boot-tests", {
    method: "POST",
    body: JSON.stringify({
      revision_id: revisionId,
      test_client_id: testClientId,
      expected_nic_id: expectedNicId,
    }),
  });
}

export async function requestInventoryBoot(clientId) {
  return apiRequest(`/api/clients/${encodeURIComponent(clientId)}/inventory-boot`, {
    method: "POST",
  });
}
