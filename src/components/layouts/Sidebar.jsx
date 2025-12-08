import { Button } from '@/components/ui';
import { cn } from '@/lib/utils';
import { exit } from '@tauri-apps/plugin-process';

import {
  ComputerIcon,
  File,
  FilesIcon,
  Gamepad,
  HardDrive,
  KeyRound,
  Laptop2,
  LayoutDashboard,
  Monitor,
  MonitorStop,
  PanelLeftClose,
  PanelRightClose,
  Power,
  Settings,
  SquareLibrary
} from 'lucide-react';
import { Activity } from 'react';
import { NavLink } from 'react-router-dom';

const menuItems = [
  { id: 'dashboard', to: '/', label: 'Dashboard', icon: LayoutDashboard },
  { id: 'clients', to: '/clients', label: 'Clients', icon: Laptop2 },
  { id: 'disks', to: '/disks', label: 'Disks', icon: HardDrive },
  { id: 'masters', to: '/masters', label: 'Images', icon: FilesIcon },
  { id: 'settings', to: '/settings', label: 'Settings', icon: Settings },
  { id: 'services', to: '/services', label: 'Configuration', icon: Settings },
  { id: 'license', to: '/license', label: 'License', icon: KeyRound },
  { id: 'setup', to: '/setup', label: 'Setup', icon: SquareLibrary },
  { id: 'logs', to: '/logs', label: 'Logs', icon: File },
];

const handleExit = async () => {
  try {
    await exit(0);
  } catch (error) {
    console.error('Failed to exit application:', error);
  }
};

const Sidebar = ({ activeTab, onTabChange, isOpen = false, onClose, isCollapsed, onToggleCollapse }) => {
  return (
    <div
      className={cn(
        'fixed inset-y-0 left-0 z-40 flex h-full flex-col bg-base-100 text-base-content shadow-md transform transition-all duration-200 ease-in-out',
        'lg:static lg:translate-x-0',
        isOpen ? 'translate-x-0' : '-translate-x-full',
        isCollapsed ? 'w-20' : 'w-64'
      )}
      role="navigation"
      aria-label="Sidebar"
    >
      <div className={cn("flex h-16 items-center border-b border-base-300", isCollapsed ? 'justify-center px-2' : 'px-6 gap-2')}>
        <Gamepad />
        {!isCollapsed && <h1 className="text-xl font-bold">Hak3r'z Cafe</h1>}
      </div>
      <div className="flex-1 px-3">
        <nav className="space-y-2 py-4">
          {menuItems.map((item) => {
            const Icon = item.icon;
            return (
              <NavLink
                to={item.to}
                key={item.id}
                variant={activeTab === item.id ? 'secondary' : 'ghost'}
                className={({ isActive }) => cn(
                  'group relative flex items-center p-2 rounded-md overflow-hidden whitespace-nowrap',
                  isActive
                    ? 'bg-base-200 text-base-content'
                    : 'text-base-content/70 hover:bg-base-200 hover:text-base-content',
                  isCollapsed ? 'justify-center w-10 h-10' : 'w-full justify-start gap-3'
                )}
                onClick={() => {
                  onTabChange(item.id);
                  // Auto-close on small screens after navigation
                  if (onClose) onClose();
                }}
              >
                <Icon className="h-5 w-5" />
                <Activity mode={!isCollapsed ? 'visible' : 'hidden'}>
                  {item.label}
                </Activity>

                <Activity mode={isCollapsed ? 'visible' : 'hidden'}>

                  <span className="absolute left-full ml-3 px-2 py-1 bg-base-300 text-base-content rounded shadow-lg opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none z-50">
                    {item.label}
                  </span>
                </Activity>
              </NavLink>
            );
          })}
        </nav>
      </div>
      <div className="p-3 border-t border-base-300 space-y-2">
        <Button
          variant="ghost"
          className={cn(
            "w-full text-base-content/70 hover:bg-base-200 hover:text-base-content",
            isCollapsed ? 'justify-center' : 'justify-start gap-3',
            'group relative'
          )}
          onClick={onToggleCollapse}
        >
          <Activity mode={!isCollapsed ? 'visible' : 'hidden'}>
            <PanelLeftClose className="h-5 w-5" />
            Minimize
          </Activity>
          <Activity mode={isCollapsed ? 'visible' : 'hidden'}>
            <PanelRightClose className="h-5 w-5" />
            <span className="absolute left-full ml-3 px-2 py-1 bg-base-300 text-base-content rounded shadow-lg opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none z-50">
              {isCollapsed ? 'Expand' : 'Minimize'}
            </span>
          </Activity>
        </Button>
        <Button
          variant="ghost"
          className={cn(
            "w-full text-base-content/70 hover:bg-base-200 hover:text-base-content",
            isCollapsed ? 'justify-center' : 'justify-start gap-3',
            'group relative'
          )}
          onClick={() => {
            if (onClose) onClose();
            handleExit();
          }}
        >
          <Power className="h-5 w-5" />
          {!isCollapsed && 'Exit'}
          {isCollapsed && (
            <span className="absolute left-full ml-3 px-2 py-1 bg-base-300 text-base-content rounded shadow-lg opacity-0 group-hover:opacity-100 transition-opacity duration-200 pointer-events-none z-50">
              Exit
            </span>
          )}
        </Button>
      </div>
    </div>
  );
}

export default Sidebar;