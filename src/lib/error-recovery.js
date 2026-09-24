export function clearSavedRoute() {
  try { localStorage.removeItem("last_path"); }
  catch { /* Recovery must still work when storage is unavailable. */ }
}

export function reloadApplication() {
  window.location.reload();
}

export function returnToHome() {
  clearSavedRoute();
  window.location.pathname = "/";
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
