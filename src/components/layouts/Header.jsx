import { Button } from '@/components/ui';
import { Shield, User, LogOut } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import { useAuth } from '@/contexts/AuthContext';

const Header = () => {
  const navigate = useNavigate();
  const { logout, user } = useAuth();

  return (
    <div className="bg-white dark:bg-gray-900 dark:border-gray-700 px-6 py-4 shadow-xl">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl darK:text-gray-100 font-bold">Diskless Boot Server Manager</h1>
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