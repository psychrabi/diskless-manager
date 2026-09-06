import { apiRequest } from "../client";

export async function testSshConnection(host, username, port = 22, password) {
  return apiRequest("/api/ssh/test-connection", {
    method: "POST",
    body: JSON.stringify({ host, username, port, password: password || null }),
  });
}

export async function executeSshCommand(host, username, command, password) {
  return apiRequest("/api/ssh/execute-command", {
    method: "POST",
    body: JSON.stringify({ host, username, command, password: password || null }),
  });
}

export async function getWindowsSystemInfo(host, username, password) {
  return apiRequest("/api/ssh/system-info", {
    method: "POST",
    body: JSON.stringify({ host, username, password: password || null }),
  });
}
