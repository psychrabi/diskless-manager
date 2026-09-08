import { File, FilesIcon, HardDrive, KeyRound, Laptop2, LayoutDashboard, Server, Settings, SlidersHorizontal, SquareLibrary, Terminal, Users2 } from "lucide-react";

export const navSections = [
  {
    label: "Overview",
    items: [
      { id: "dashboard", to: "/", label: "Dashboard", icon: LayoutDashboard },
    ],
  },
  {
    label: "Infrastructure",
    items: [
      { id: "clients", to: "/clients", label: "Clients", icon: Laptop2 },
      { id: "disks", to: "/disks", label: "Disks", icon: HardDrive },
      { id: "images", to: "/images", label: "Images", icon: FilesIcon },
      { id: "services", to: "/services", label: "Services", icon: Server },
    ],
  },
  {
    label: "Configuration",
    items: [
      {
        id: "settings",
        to: "/settings",
        label: "System Settings",
        icon: Settings,
      },
      {
        id: "application-settings",
        to: "/application-settings",
        label: "Application Settings",
        icon: SlidersHorizontal,
      },
      { id: "setup", to: "/setup", label: "Setup Wizard", icon: SquareLibrary },
    ],
  },
  {
    label: "Management",
    items: [
      { id: "users", to: "/users", label: "Users", icon: Users2 },
      {
        id: "ssh-tester",
        to: "/ssh-tester",
        label: "SSH Tester",
        icon: Terminal,
      },
      { id: "logs", to: "/logs", label: "Logs", icon: File },
      { id: "license", to: "/license", label: "License", icon: KeyRound },
    ],
  },
];


export function getNavigationItem(pathname) {
  for (const section of navSections) {
    const item = section.items.find((item) => item.to === pathname);
    if (item) return { ...item, section: section.label };
  }
  return { label: "Diskless Manager", section: "Overview" };
}
