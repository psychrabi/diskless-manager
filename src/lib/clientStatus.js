export const isOnlineStatus = (status) =>
  typeof status === "string" && status.toLowerCase() === "online";
