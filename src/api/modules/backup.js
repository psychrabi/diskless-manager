import { apiRequest, getAuthToken } from "../client";

export async function downloadBackup() {
  const token = getAuthToken();
  const response = await fetch("/api/system/backup", {
    headers: token ? { Authorization: `Bearer ${token}` } : {},
  });
  if (!response.ok) {
    let message = `API request failed: ${response.status} ${response.statusText}`;
    try {
      const body = await response.json();
      message = body.message || body.error || message;
    } catch { /* Keep the HTTP error if its body is not JSON. */ }
    const error = new Error(message);
    error.status = response.status;
    throw error;
  }

  const url = URL.createObjectURL(await response.blob());
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = response.headers.get("content-disposition")?.match(/filename="?([^";]+)"?/i)?.[1] || "diskless-backup.json";
  document.body.appendChild(anchor);
  try {
    anchor.click();
  } finally {
    anchor.remove();
    URL.revokeObjectURL(url);
  }
}

export function restoreBackup(file) {
  return apiRequest("/api/system/restore", { method: "POST", body: file });
}
