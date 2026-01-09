/**
 * Format uptime in seconds to hh:mm format
 * @param {number} seconds - Uptime in seconds
 * @returns {string} Formatted uptime (e.g., "02:45" for 2 hours 45 minutes)
 */
export const formatUptime = (seconds) => {
  if (!seconds || seconds <= 0) return "Offline";

  const totalMinutes = Math.floor(seconds / 60);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
};
