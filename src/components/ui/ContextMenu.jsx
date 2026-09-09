import {
  HardDrive,
  History,
  RefreshCw,
  Settings,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { useOnClickOutside } from "../../hooks/useOnClickOutside";
import { cn } from "@/lib/utils";

const MenuItem = ({
  icon: Icon,
  label,
  onClick,
  variant = "default",
  className = "",
}) => {
  const variants = {
    default: "hover:bg-accent hover:text-accent-foreground",
    success: "hover:bg-accent hover:text-accent-foreground",
    warning: "hover:bg-accent hover:text-accent-foreground",
    error: "hover:bg-accent hover:text-accent-foreground",
    info: "hover:bg-accent hover:text-accent-foreground",
    secondary: "hover:bg-accent hover:text-accent-foreground",
    destructive:
      "text-destructive hover:bg-destructive hover:text-destructive-foreground",
  };

  const handleKeyDown = (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onClick?.(e);
    }
  };

  return (
    <li role="none">
      <button
        type="button"
        onClick={onClick}
        onKeyDown={handleKeyDown}
        className={cn(
          "flex w-full items-center gap-3 rounded-md px-3 py-2 text-sm transition-colors duration-150",
          variants[variant],
          className
        )}
      >
        <Icon className="size-4 shrink-0" />
        <span>{label}</span>
      </button>
    </li>
  );
};

const SectionHeader = ({ label }) => (
  <div className="px-3 py-1.5 text-xs font-semibold text-muted-foreground uppercase tracking-widest">
    {label}
  </div>
);

export const ContextMenu = ({
  isOpen,
  xPos,
  yPos,
  targetClient,
  onClose,
  actions,
}) => {
  const menuRef = useRef(null);
  const [coords, setCoords] = useState({ x: xPos, y: yPos });
  useOnClickOutside(menuRef, onClose);

  useLayoutEffect(() => {
    if (isOpen && menuRef.current) {
      const { width, height } = menuRef.current.getBoundingClientRect();
      setCoords({
        x: Math.min(xPos, window.innerWidth - width - 10),
        y: Math.min(yPos, window.innerHeight - height - 10),
      });
    }
  }, [isOpen, xPos, yPos]);

  if (!isOpen || !targetClient) return null;

  const isOnline = targetClient?.status === "Online";
  const isSuper = targetClient?.mode === "super";
  const isPersistent =
    targetClient?.snapshot && targetClient?.keep_writeback && !isSuper;
  const isNonPersistent =
    targetClient?.snapshot && !targetClient?.keep_writeback && !isSuper;

  const handleAction = (cb) => {
    cb(targetClient);
    onClose();
  };

  return (
    <div
      ref={menuRef}
      style={{ top: coords.y, left: coords.x }}
      className="fixed z-50 min-w-55 overflow-hidden rounded-xl bg-popover p-1.5 text-popover-foreground shadow-2xl ring-1 ring-foreground/10 backdrop-blur-md"
    >
      <div className="border-b border-border px-3 py-2.5">
        <p className="text-xs font-bold uppercase tracking-widest text-primary mb-0.5">
          {targetClient.name}
        </p>

        <p className="text-[10px] text-muted-foreground  truncate">
          {targetClient.ip} • {targetClient.mac}
        </p>
      </div>

      <ul className="flex w-full flex-col p-0" role="menu">
        {!isOnline && (
          <>
            <SectionHeader label="Maintenance" />
            {isSuper ? (
              <>
                <MenuItem
                  icon={History}
                  label="Save Super Changes"
                  variant="secondary"
                  onClick={() => handleAction(actions.saveSuper)}
                />
                <MenuItem
                  icon={ShieldAlert}
                  label="Disable Super mode"
                  variant="error"
                  onClick={() => handleAction(actions.disableSuper)}
                />
              </>
            ) : (
              <MenuItem
                icon={HardDrive}
                label="Enable Super mode"
                variant="secondary"
                onClick={() => handleAction(actions.enableSuper)}
              />
            )}
          </>
        )}

        {!isOnline && (
          <>
            <SectionHeader label="Management" />
            <MenuItem
              icon={Settings}
              label="Edit Client"
              onClick={() => handleAction(actions.edit)}
            />

            {isPersistent && (
              <MenuItem
                icon={History}
                label="Reset Writeback"
                variant="warning"
                onClick={() => handleAction(actions.reset)}
              />
            )}
            {isNonPersistent && (
              <MenuItem
                icon={RefreshCw}
                label="Reset to Clean"
                variant="warning"
                onClick={() => handleAction(actions.resetToClean)}
              />
            )}

            <MenuItem
              icon={Trash2}
              label="Delete Client"
              variant="destructive"
              onClick={() => handleAction(actions.delete)}
            />
          </>
        )}
      </ul>
    </div>
  );
};
