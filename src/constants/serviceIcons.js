import { Folder, FolderOpen, FolderOpenDot, Globe, Network, Save, Settings } from "lucide-react";

export const serviceIcons = {
  dhcp: Network,
  tftp: FolderOpen,
  iscsi: Save,
  nfs: FolderOpenDot,
  samba: Folder,
  http: Globe,
};

export const getServiceIcon = (name) => serviceIcons[name] || Settings;