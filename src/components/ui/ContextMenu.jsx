import {
  Edit,
  History,
  Play,
  Power,
  RefreshCw,
  ScreenShare,
  Star,
  Trash2,
  XCircle,
} from "lucide-react";
import { useRef } from "react";
import { useOnClickOutside } from "../../hooks/useOnClickOutside";

export const ContextMenu = ({
  isOpen,
  xPos,
  yPos,
  targetClient,
  onClose,
  actions,
}) => {
  const menuRef = useRef(null);
  useOnClickOutside(menuRef, onClose);

  if (!isOpen || !targetClient) return null;

  const menuStyle = {
    top: `${yPos}px`,
    left: `${xPos}px`,
  };

  return (
    <div
      ref={menuRef}
      style={menuStyle}
      className="fixed z-[60] bg-base-100 rounded-md shadow-lg min-w-[180px] animate-fade-in"
    >
      <ul className="menu rounded-box w-52">
        <li>
          <a
            onClick={() => {
              actions.wake(targetClient);
              onClose();
            }}
          >
            <Play className="w-4 h-4" />
            Power On
          </a>
        </li>
        <li>
          <a
            onClick={() => {
              actions.reboot(targetClient);
              onClose();
            }}
          >
            <RefreshCw className="w-4 h-4" />
            Reboot
          </a>
        </li>
        <li>
          <a
            onClick={() => {
              actions.shutdown(targetClient);
              onClose();
            }}
          >
            <Power className="w-4 h-4" />
            Shutdown
          </a>
        </li>
        <li>
          <a
            onClick={() => {
              actions.remote(targetClient);
              onClose();
            }}
          >
            <ScreenShare className="w-4 h-4" />
            Remote Control
          </a>
        </li>
        {/* Super Client items */}
        {targetClient?.status === "Offline" ? (
          <>
            <div className="divider my-0"></div>
            {targetClient?.mode === "super" ? (
              <>
                <li>
                  <a
                    onClick={() => {
                      actions.saveSuper(targetClient);
                      onClose();
                    }}
                  >
                    {/* using History icon for snapshot save; replace if you prefer a different icon */}
                    <History className="w-4 h-4" />
                    Save Super
                  </a>
                </li>
                <li>
                  <a
                    onClick={() => {
                      actions.disableSuper(targetClient);
                      onClose();
                    }}
                  >
                    <RefreshCw className="w-4 h-4" />
                    Disable Super Client
                  </a>
                </li>
              </>
            ) : (
              <li>
                <a
                  onClick={() => {
                    actions.enableSuper(targetClient);
                    onClose();
                  }}
                >
                  {/* using RefreshCw icon for toggle; replace if you prefer a star/wand */}
                  <RefreshCw className="w-4 h-4" />
                  Enable Super Client
                </a>
              </li>
            )}
          </>
        ) : null}
        <div className="divider my-0"></div>
        <li>
          <a
            onClick={() => {
              actions.edit(targetClient);
              onClose();
            }}
          >
            <Edit className="w-4 h-4" />
            Edit Client
          </a>
        </li>
        <li>
          <a
            onClick={() => {
              actions.reset(targetClient);
              onClose();
            }}
          >
            <History className="w-4 h-4" />
            Reset Writeback
          </a>
        </li>
        {targetClient?.keep_writeback === false && (
          <li>
            <a
              onClick={() => {
                actions.resetToClean(targetClient);
                onClose();
              }}
            >
              <RefreshCw className="w-4 h-4" />
              Reset to Clean
            </a>
          </li>
        )}
        <li>
          <a
            onClick={() => {
              actions.delete(targetClient);
              onClose();
            }}
          >
            <Trash2 className="w-4 h-4" />
            Delete Client
          </a>
        </li>
      </ul>
      <style jsx="true">{`
        @keyframes fade-in {
          from {
            opacity: 0;
          }
          to {
            opacity: 1;
          }
        }
        .animate-fade-in {
          animation: fade-in 0.1s ease-out forwards;
        }
      `}</style>
    </div>
  );
};
