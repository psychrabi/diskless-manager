export function clearSavedRoute() {
  try { localStorage.removeItem("last_path"); }
  catch { /* Recovery must still work when storage is unavailable. */ }
}

export function reloadApplication() {
  window.location.reload();
}

export function returnToHome() {
  clearSavedRoute();
  // Preserve the app's document URL, including in the Tauri webview.
  window.location.hash = "/";
  reloadApplication();
}

export function getErrorMessage(error) {
  try {
    if (typeof error?.message === "string") return error.message;
    if (error == null) return "Unknown error";
    return String(error);
  } catch {
    return "Unknown error";
  }
}
