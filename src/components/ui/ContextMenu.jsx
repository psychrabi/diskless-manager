import { useOnClickOutside } from "../../hooks/useOnClickOutside";
import { useRef } from "react";
import { Button } from "../ui/index.js";
import { Edit, Zap, RefreshCw, PowerSquare, Sunrise, Trash2, ScreenShare } from "lucide-react";

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
       className="fixed z-[60] bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-md shadow-lg py-2 min-w-[180px] animate-fade-in"      
    >
      <ul>
        <li><Button onClick={() => { actions.edit(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={Edit}>Edit Client</Button></li>
        <li><Button onClick={() => { actions.toggleSuper(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={Zap}>{targetClient.isSuperClient ? 'Disable' : 'Enable'} Super Client</Button></li>
        <hr className="my-1 border-gray-200 dark:border-gray-700" />
        <li><Button onClick={() => { actions.reboot(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={RefreshCw}>Reboot</Button></li>
        <li><Button onClick={() => { actions.shutdown(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={PowerSquare}>Shutdown</Button></li>
        <li><Button onClick={() => { actions.wake(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={Sunrise}>Wake Up</Button></li>
        <li><Button onClick={() => { actions.remote(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm" icon={ScreenShare}>Remote Control</Button></li>
        <hr className="my-1 border-gray-200 dark:border-gray-700" />
        <li><Button onClick={() => { actions.delete(targetClient); onClose(); }} variant="ghost" className="w-full justify-start px-3 py-1.5 text-sm text-red-600 dark:text-red-400 hover:bg-red-100 dark:hover:bg-red-900/50" icon={Trash2}>Delete Client</Button></li>
      </ul>
       <style jsx>{`
        @keyframes fade-in {
          from { opacity: 0; }
          to { opacity: 1; }
        }
        .animate-fade-in { animation: fade-in 0.1s ease-out forwards; }
      `}</style>
    </div>
  );
};