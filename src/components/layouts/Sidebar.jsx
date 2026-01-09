import { Button } from "@/components/ui";
import { cn } from "@/lib/utils";
import { exit } from "@tauri-apps/plugin-process";

import {
  File,
  FilesIcon,
  HardDrive,
  KeyRound,
  Laptop2,
  LayoutDashboard,
  PanelLeftClose,
  PanelRightClose,
  Power,
  Settings,
  Shield,
  SquareLibrary,
} from "lucide-react";
import { NavLink, useNavigate } from "react-router-dom";

const menuItems = [
  { id: "dashboard", to: "/", label: "Dashboard", icon: LayoutDashboard },
  { id: "clients", to: "/clients", label: "Clients", icon: Laptop2 },
  { id: "disks", to: "/disks", label: "Disks", icon: HardDrive },
  { id: "images", to: "/images", label: "Images", icon: FilesIcon },
  { id: "setup", to: "/setup", label: "Setup", icon: SquareLibrary },
  { id: "services", to: "/services", label: "Services", icon: Settings },
  { id: "settings", to: "/settings", label: "System Settings", icon: Settings },
  {
    id: "application-settings",
    to: "/application-settings",
    label: "Application Settings",
    icon: Settings,
  },
  { id: "logs", to: "/logs", label: "Logs", icon: File },
];

const handleExit = async () => {
  try {
    await exit(0);
  } catch (error) {
    console.error("Failed to exit application:", error);
  }
};

const Sidebar = ({
  onTabChange,
  isOpen = false,
  onClose,
  isCollapsed,
  onToggleCollapse,
}) => {
  const navigate = useNavigate();

  return (
    <aside
      className={cn(
        "fixed inset-y-0 left-0 z-40 flex h-full flex-col bg-base-100/95 backdrop-blur-sm text-base-content border-r border-base-200/50 transform transition-all duration-300 ease-in-out overflow-hidden",
        "lg:static lg:translate-x-0 lg:flex-shrink-0",
        isOpen ? "translate-x-0" : "-translate-x-full",
        isCollapsed ? "w-20" : "w-64"
      )}
      role="navigation"
      aria-label="Main navigation"
    >
      {/* Header */}
      <div
        className={cn(
          "flex h-16 items-center border-b border-base-200/30 bg-base-100/50",
          isCollapsed ? "justify-center px-2" : "px-6 gap-3"
        )}
      >
        <div className="w-8 h-8 bg-primary rounded-lg flex items-center justify-center flex-shrink-0">
          <Shield className="h-5 w-5 text-primary-content" />
        </div>
        {!isCollapsed && (
          <div>
            <h1 className="text-heading-sm font-bold text-base-content">
              Diskless Manager
            </h1>
            <p className="text-caption text-base-content/60">
              Boot Server Control
            </p>
          </div>
        )}
      </div>

      {/* Navigation */}
      <nav className="flex-1 px-3 py-4 space-y-1 overflow-y-auto overflow-x-hidden">
        {menuItems.map((item) => {
          const Icon = item.icon;
          return (
            <NavLink
              key={item.id}
              to={item.to}
              className={({ isActive }) =>
                cn(
                  "flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-all duration-200 group relative",
                  isActive
                    ? "bg-primary text-primary-content shadow-sm"
                    : "text-base-content/70 hover:text-base-content hover:bg-base-200/50",
                  isCollapsed ? "justify-center" : ""
                )
              }
              onClick={() => {
                onTabChange?.(item.id);
                if (window.innerWidth < 1024) onClose?.();
              }}
              title={isCollapsed ? item.label : undefined}
            >
              <Icon className={cn("flex-shrink-0", isCollapsed ? "h-5 w-5" : "h-4 w-4")} />
              {!isCollapsed && (
                <span className="truncate">{item.label}</span>
              )}

              {/* Tooltip for collapsed state */}
              {isCollapsed && (
                <div className="absolute left-full ml-2 px-2 py-1 bg-base-content text-base-100 text-xs rounded opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none whitespace-nowrap z-50">
                  {item.label}
                </div>
              )}
            </NavLink>
          );
        })}
      </nav>

      {/* Footer */}
      <div className={cn("p-3 border-t border-base-200/30 space-y-2")}>
        {/* Collapse toggle */}
        <Button
          variant="ghost"
          size={isCollapsed ? "icon" : "sm"}
          onClick={onToggleCollapse}
          className={cn(
            "w-full justify-start gap-3 text-base-content/70 hover:text-base-content",
            isCollapsed ? "justify-center" : ""
          )}
          title={isCollapsed ? "Expand sidebar" : "Collapse sidebar"}
        >
          {isCollapsed ? (
            <PanelRightClose className="h-4 w-4" />
          ) : (
            <>
              <PanelLeftClose className="h-4 w-4" />
              <span className="text-sm">Collapse</span>
            </>
          )}
        </Button>

        {/* License button */}
        <Button
          variant="ghost"
          size={isCollapsed ? "icon" : "sm"}
          onClick={() => {
            navigate("/license");
            if (window.innerWidth < 1024) onClose?.();
          }}
          className={cn(
            "w-full justify-start gap-3 text-base-content/70 hover:text-base-content",
            isCollapsed ? "justify-center" : ""
          )}
          title={isCollapsed ? "License" : undefined}
        >
          <KeyRound className="h-4 w-4" />
          {!isCollapsed && <span className="text-sm">License</span>}
        </Button>

        {/* Exit button */}
        <Button
          variant="ghost"
          size={isCollapsed ? "icon" : "sm"}
          onClick={handleExit}
          className={cn(
            "w-full justify-start gap-3 text-error/70 hover:text-error hover:bg-error/10",
            isCollapsed ? "justify-center" : ""
          )}
          title={isCollapsed ? "Exit application" : undefined}
        >
          <Power className="h-4 w-4" />
          {!isCollapsed && <span className="text-sm">Exit</span>}
        </Button>
      </div>

      {/* Mobile overlay */}
      {isOpen && (
        <div
          className="fixed inset-0 bg-black/20 backdrop-blur-sm lg:hidden"
          onClick={onClose}
          aria-hidden="true"
        />
      )}
    </aside>
  );
};

export default Sidebar;
