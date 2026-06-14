import {
  HardDrive,
  History,
  Play,
  Power,
  RefreshCw,
  ScreenShare,
  Settings,
  ShieldAlert,
  Trash2,
} from "lucide-react";
import { useLayoutEffect, useRef, useState } from "react";
import { useOnClickOutside } from "../../hooks/useOnClickOutside";

const MenuItem = ({
  icon: Icon, // eslint-disable-line no-unused-vars
  label,
  onClick,
  variant = "default",
  className = "",
}) => {
  const variants = {
    default: "",
    success: "hover:bg-success/20 transition-colors text-success",
    warning: "hover:bg-warning/20 transition-colors text-warning",
    error: "hover:bg-error/20 transition-colors text-error",
    info: "hover:bg-info/20 transition-colors text-info",
    secondary: "hover:bg-secondary/20 transition-colors text-secondary",
    destructive:
      "hover:bg-error text-error hover:text-white transition-all group",
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
        className={`w-full text-left flex items-center gap-3 px-3 py-2 text-sm transition-all duration-200 ${variants[variant]} ${className}`}
      >
        <Icon
          className={`w-4 h-4 shrink-0 ${
            variant === "destructive"
              ? "group-hover:scale-110 transition-transform"
              : ""
          }`}
        />
        <span>{label}</span>
      </button>
    </li>
  );
};

const SectionHeader = ({ label }) => (
  <div className="px-3 py-1 font-bold bg-base-300 uppercase tracking-widest">
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
      className="fixed z-60 bg-base-100/80 backdrop-blur-md rounded-xl shadow-2xl min-w-55 border border-white/10 overflow-hidden animate-in fade-in zoom-in duration-150"
    >
      <div className="p-4 bg-base-300 border-b border-white/5">
        <p className="text-xs font-bold uppercase tracking-widest text-primary mb-2">
          {targetClient.name}
        </p>

        <p className="text-[10px] opacity-40 font-mono truncate text-base-content">
          {targetClient.ip} • {targetClient.mac}
        </p>
      </div>

      <ul className="menu w-full p-0" role="menu">
        <SectionHeader label="Control" />
        {!isOnline && (
          <MenuItem
            icon={Play}
            label="Power On"
            variant="success"
            onClick={() => handleAction(actions.wake)}
          />
        )}
        {isOnline && (
          <>
            <MenuItem
              icon={RefreshCw}
              label="Reboot"
              variant="warning"
              onClick={() => handleAction(actions.reboot)}
            />
            <MenuItem
              icon={Power}
              label="Shutdown"
              variant="error"
              onClick={() => handleAction(actions.shutdown)}
            />
            <MenuItem
              icon={ScreenShare}
              label="Remote Control"
              variant="info"
              onClick={() => handleAction(actions.remote)}
            />
          </>
        )}

        {/* Maintenance Group */}
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

        <SectionHeader label="Management" />
        {!isOnline && (
          <>
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
