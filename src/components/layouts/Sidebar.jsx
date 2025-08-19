import { Button } from '@/components/ui';
import { cn } from '@/lib/utils';
import { exit } from '@tauri-apps/plugin-process';

import {
  Gamepad,
  HardDrive,
  LayoutDashboard,
  Monitor,
  Power,
  Settings,
  Wrench
} from 'lucide-react';
import { NavLink } from 'react-router-dom';

const menuItems = [
  { id: 'dashboard', to: '/', label: 'Dashboard', icon: LayoutDashboard },
  { id: 'clients', to: '/clients', label: 'Client Management', icon: Monitor },
  { id: 'masters', to: '/masters', label: 'Image Management', icon: HardDrive },
  { id: 'settings', to: '/settings', label: 'System Settings', icon: Settings },
  { id: 'setup', to: '/setup', label: 'Setup', icon: Wrench },
];

const handleExit = async () => {
  try {
    await exit(0);
  } catch (error) {
    console.error('Failed to exit application:', error);
  }
};

const Sidebar = ({ activeTab, onTabChange, isOpen = false, onClose }) => {
  return (
    <div
      className={cn(
        'fixed inset-y-0 left-0 z-40 w-64 flex h-full flex-col bg-base-100 text-base-content shadow-md transform transition-transform duration-200 ease-in-out',
        'lg:static lg:translate-x-0',
        isOpen ? 'translate-x-0' : '-translate-x-full'
      )}
      role="navigation"
      aria-label="Sidebar"
    >
      <div className="flex h-16 items-center px-6 gap-2 border-b border-base-300">
        <Gamepad /><h1 className="text-xl font-bold">Hak3r'z Cafe</h1>
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
                  'w-full justify-start gap-3 text-left flex items-center p-2 rounded-md',
                  isActive
                    ? 'bg-base-200 text-base-content'
                    : 'text-base-content/70 hover:bg-base-200 hover:text-base-content'
                )}
                onClick={() => {
                  onTabChange(item.id);
                  // Auto-close on small screens after navigation
                  if (onClose) onClose();
                }}

              >
                <Icon className="h-5 w-5" />
                {item.label}
              </NavLink>
            );
          })}
        </nav>
      </div>
      <div className="p-3 border-t border-base-300">
        <Button
          variant="ghost"
          className="w-full justify-start gap-3 text-base-content/70 hover:bg-base-200 hover:text-base-content"
          onClick={() => {
            if (onClose) onClose();
            handleExit();
          }}
        >
          <Power className="h-5 w-5" />
          Exit
        </Button>
      </div>
    </div>
  );
}

export default Sidebar;