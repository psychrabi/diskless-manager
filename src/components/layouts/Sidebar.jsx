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

const Sidebar = ({ activeTab, onTabChange }) => {
  return (
    <div className="flex h-full w-64 flex-col bg-gray-800 text-gray-100 shadow-mg">
      <div className="flex h-16 items-center px-6 gap-2">
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
                    ? 'bg-gray-800 text-white'
                    : 'text-gray-300 hover:bg-gray-800 hover:text-white'
                )}
                onClick={() => onTabChange(item.id)}

              >
                <Icon className="h-5 w-5" />
                {item.label}
              </NavLink>
            );
          })}
        </nav>
      </div>
      <div className="p-3">
        <Button variant="ghost" className="w-full justify-start gap-3 text-gray-300 hover:bg-gray-800 hover:text-white" onClick={handleExit}>
          <Power className="h-5 w-5" />
          Exit
        </Button>
      </div>
    </div>
  );
}

export default Sidebar;