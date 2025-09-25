import { Button } from '@/components/ui';
import { Shield, User, LogOut, Menu, Sun, Moon } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/auth';
import { useTheme } from '@/contexts/theme';

const Header = ({ onToggleSidebar }) => {
  const navigate = useNavigate();
  const { logout, user } = useAuth();
  const { theme, setTheme } = useTheme();

  const toggleTheme = () => setTheme(theme === 'dark' ? 'light' : 'dark');

  return (
    <div className="bg-base-100 border-b border-base-300 px-6 py-3 shadow-2xl">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          {/* Hamburger only on small screens */}
          <button
            type="button"
            className="lg:hidden inline-flex items-center justify-center rounded-md p-2 text-base-content/70 hover:text-base-content hover:bg-base-200 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-primary"
            aria-label="Open sidebar"
            onClick={onToggleSidebar}
          >
            <Menu className="h-6 w-6" />
          </button>
          <h1 className="text-2xl text-base-content font-bold">Diskless Boot Server Manager</h1>
          <p className="text-sm text-base-content/70">Administrator Dashboard</p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="ghost" className="flex items-center gap-2" onClick={toggleTheme} title={`${theme === 'dark' ? 'Light' : 'Dark'} Mode`}>
            {theme === 'dark' ? <Sun className="h-5 w-5" /> : <Moon className="h-5 w-5" />}            
          </Button>
          <Button variant="success" className="flex items-center gap-2 capitalize">
            <Shield className="h-4 w-4" />{user.role}
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