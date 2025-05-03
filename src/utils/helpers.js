export const formatBytes = (bytes) => {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
};

export const formatDate = (dateString) => {
  const date = new Date(dateString);
  return date.toLocaleString();
};

export const validateMacAddress = (mac) => {
  const regex = /^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/;
  return regex.test(mac);
};

export const validateIpAddress = (ip) => {
  const regex = /^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/;
  return regex.test(ip);
};

export const getSnapshotNameFromPath = (path) => {
  return path.split('@').pop();
};
