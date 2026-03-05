import { apiRequest } from "../client";

export async function testSshConnection(host, username, port = 22) {
  return apiRequest("/api/ssh/test-connection", {
    method: "POST",
    body: JSON.stringify({ host, username, port }),
  });
}

export async function executeSshCommand(host, username, command) {
  return apiRequest("/api/ssh/execute-command", {
    method: "POST",
    body: JSON.stringify({ host, username, command }),
  });
}

export async function getWindowsSystemInfo(host, username) {
  return apiRequest("/api/ssh/system-info", {
    method: "POST",
    body: JSON.stringify({ host, username }),
  });
}
