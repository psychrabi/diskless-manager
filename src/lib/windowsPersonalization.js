// Shared validation for the per-client Windows computer name.
//
// The server enforces the same NetBIOS rules; this copy only exists so a typo
// is caught before a dialog submits. Keep the two in sync.

export function validateComputerName(value) {
  const name = String(value ?? "").trim();
  if (!name) return "";
  if (!/^[A-Za-z0-9-]+$/.test(name)) return "Use letters, digits and hyphens only.";
  if (name.startsWith("-") || name.endsWith("-")) {
    return "Do not start or end with a hyphen.";
  }
  if (/^[0-9]+$/.test(name)) return "Do not use digits only.";
  const upper = name.toUpperCase();
  const reserved = new Set([
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5",
    "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4",
    "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
  ]);
  if (reserved.has(upper)) return `Computer name '${upper}' is reserved by Windows.`;
  if (name.length > 15) return "Use at most 15 characters.";
  return "";
}

/** Normalize a plan-shaped object for comparison and submission. */
export function normalizePersonalizationPlan(plan) {
  return {
    computer_name: String(plan?.computer_name ?? "").trim(),
    convert_static_ipv4_to_dhcp: Boolean(plan?.convert_static_ipv4_to_dhcp),
    clear_stale_drive_letters: Boolean(plan?.clear_stale_drive_letters),
  };
}

export function personalizationPlanIsEmpty(plan) {
  const normalized = normalizePersonalizationPlan(plan);
  return (
    !normalized.computer_name &&
    !normalized.convert_static_ipv4_to_dhcp &&
    !normalized.clear_stale_drive_letters
  );
}

export function personalizationPlansMatch(a, b) {
  const left = normalizePersonalizationPlan(a);
  const right = normalizePersonalizationPlan(b);
  return (
    left.computer_name.toLowerCase() === right.computer_name.toLowerCase() &&
    left.convert_static_ipv4_to_dhcp === right.convert_static_ipv4_to_dhcp &&
    left.clear_stale_drive_letters === right.clear_stale_drive_letters
  );
}
