import { Edit, History, Play, Power, RefreshCw, ScreenShare, Trash2 } from "lucide-react";
import { useRef } from "react";
import { useOnClickOutside } from "../../hooks/useOnClickOutside";

export const ContextMenu = ({ isOpen, xPos, yPos, targetClient, onClose, actions }) => {
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
      className="fixed z-[60] bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg min-w-[180px] animate-fade-in"
    >
      <ul className="menu bg-base-400 rounded-box w-48">
        <li><a onClick={() => { actions.edit(targetClient); onClose(); }}><Edit className="w-4 h-4" />Edit Client</a></li>
        <li><a onClick={() => { actions.reset(targetClient); onClose(); }}><History className="w-4 h-4" />Reset Client</a></li>
        <div className="divider my-0"></div>
        <li><a onClick={() => { actions.reboot(targetClient); onClose(); }}><RefreshCw className="w-4 h-4" />Reboot</a></li>
        <li><a onClick={() => { actions.shutdown(targetClient); onClose(); }} ><Power className="w-4 h-4" />Shutdown</a></li>
        <li><a onClick={() => { actions.wake(targetClient); onClose(); }}><Play className="w-4 h-4" />Wake Up</a></li>
        <li><a onClick={() => { actions.remote(targetClient); onClose(); }}><ScreenShare className="w-4 h-4" />Remote Control</a></li>
        <div className="divider my-0"></div>
        <li><a onClick={() => { actions.delete(targetClient); onClose(); }}><Trash2 className="w-4 h-4" />Delete Client</a></li>
      </ul>
      <style jsx="true">{`
        @keyframes fade-in {
          from { opacity: 0; }
          to { opacity: 1; }
        }
        .animate-fade-in { animation: fade-in 0.1s ease-out forwards; }
      `}</style>
    </div>
  );
};