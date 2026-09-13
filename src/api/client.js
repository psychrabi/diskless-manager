const API_BASE_URL = "http://127.0.0.1:8080";

let authToken = null;

export function setAuthToken(token) {
  authToken = token;
  if (token) {
    localStorage.setItem("authToken", token);
  } else {
    localStorage.removeItem("authToken");
  }
}

export function getAuthToken() {
  if (!authToken) {
    authToken = localStorage.getItem("authToken");
  }
  return authToken;
}

export async function apiRequest(endpoint, options = {}) {
  const { authToken, ...fetchOptions } = options;
  const url = `${API_BASE_URL}${endpoint}`;
  const headers = {
    "Content-Type": "application/json",
    ...options.headers,
  };

  const token = getAuthToken();
  const requestToken = authToken === undefined ? token : authToken;
  if (requestToken) {
    headers.Authorization = `Bearer ${requestToken}`;
  }

  const response = await fetch(url, {
    ...fetchOptions,
    headers,
  });

  if (!response.ok) {
    let errorMessage = `API request failed: ${response.status} ${response.statusText}`;
    try {
      const contentType = response.headers.get("content-type");
      if (contentType?.includes("application/json")) {
        const errorData = await response.json();
        if (errorData?.message || errorData?.error) {
          errorMessage = errorData.message || errorData.error;
        }
      }
    } catch {
      // Use fallback message when response body cannot be parsed.
    }
    const error = new Error(errorMessage);
    error.status = response.status;
    throw error;
  }

  const contentType = response.headers.get("content-type");
  if (contentType?.includes("application/json")) {
    return response.json();
  }
  return response.text();
}
