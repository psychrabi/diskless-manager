const API_BASE_URL = "";

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

function uploadError(status, statusText, responseText) {
  let message = `API request failed: ${status} ${statusText}`;
  try {
    const errorData = JSON.parse(responseText);
    if (errorData?.message || errorData?.error) {
      message = errorData.message || errorData.error;
    }
  } catch {
    // Use fallback message when response body cannot be parsed.
  }
  const error = new Error(message);
  error.status = status;
  return error;
}

/**
 * Uploads a request body with XMLHttpRequest so callers can report transfer
 * progress, which fetch does not expose. The request is owned by this module
 * and is not tied to a React component lifecycle.
 *
 * @param {string} endpoint API path.
 * @param {BodyInit} body Request body, typically FormData.
 * @param {{ onProgress?: (fraction: number) => void }} [options]
 *   `onProgress` receives a 0..1 fraction while the request body is sent.
 */
export function apiUpload(endpoint, body, { onProgress } = {}) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", `${API_BASE_URL}${endpoint}`);

    const token = getAuthToken();
    if (token) {
      xhr.setRequestHeader("Authorization", `Bearer ${token}`);
    }

    if (onProgress) {
      xhr.upload.onprogress = (event) => {
        if (event.lengthComputable && event.total > 0) {
          onProgress(event.loaded / event.total);
        }
      };
      // Some browsers do not deliver a final 100% progress event.
      xhr.upload.onload = () => onProgress(1);
    }

    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        const contentType = xhr.getResponseHeader("content-type") || "";
        if (contentType.includes("application/json")) {
          try {
            resolve(JSON.parse(xhr.responseText));
          } catch {
            reject(new Error("The server returned an invalid JSON response."));
          }
        } else {
          resolve(xhr.responseText);
        }
        return;
      }
      reject(uploadError(xhr.status, xhr.statusText, xhr.responseText));
    };
    xhr.onerror = () => reject(new Error("Network error during upload."));
    xhr.ontimeout = () => reject(new Error("The upload timed out."));

    xhr.send(body);
  });
}

export async function apiRequest(endpoint, options = {}) {
  const { authToken, ...fetchOptions } = options;
  const url = `${API_BASE_URL}${endpoint}`;
  const headers = {
    ...(options.body instanceof FormData ? {} : { "Content-Type": "application/json" }),
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
