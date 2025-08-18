import { Button } from '@/components/ui';
import { Shield, User, LogOut, Menu } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';

const Header = ({ onToggleSidebar }) => {
  const navigate = useNavigate();
  const { logout, user } = useAuth();

  return (
    <div className="bg-white dark:bg-gray-900 dark:border-gray-700 px-6 py-4 shadow-xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          {/* Hamburger only on small screens */}
          <button
            type="button"
            className="lg:hidden inline-flex items-center justify-center rounded-md p-2 text-gray-500 hover:text-white hover:bg-gray-800 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-indigo-500"
            aria-label="Open sidebar"
            onClick={onToggleSidebar}
          >
            <Menu className="h-6 w-6" />
          </button>
          <h1 className="text-2xl dark:text-gray-100 font-bold">Diskless Boot Server Manager</h1>
          <p className="text-sm dark:text-gray-200">Administrator Dashboard</p>
        </div>
        <div className="flex items-center gap-4">
          <Button variant="success" className="flex items-center gap-2 capitalize">
            <Shield className="h-4 w-4" />{user.username}
          </Button>
          <Button
            variant="destructive"
            onClick={() => {
              logout();
              navigate('/login');
            }}
            className="flex items-center gap-2"
          >
            <LogOut className="h-4 w-4" />
            Logout
          </Button>
        </div>
      </div>
    </div>
  );
}

export default Header;