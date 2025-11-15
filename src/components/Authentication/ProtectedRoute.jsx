import { useEffect } from 'react';
import { useAuth } from '@/contexts/auth';
import { useNavigate } from 'react-router-dom';
import { Loading } from '@/components/ui';

const ProtectedRoute = ({ children }) => {
  const { user, token } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    if (!user || !token) {
      navigate('/login');
    }
  }, [user, token, navigate]);


  if (user && token) {
    return children;
  }

  return null;
};

export default ProtectedRoute;