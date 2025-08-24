import { useEffect } from 'react';
import { useAuth } from '@/contexts/AuthContext';
import { useNavigate } from 'react-router-dom';
import { Loading } from '@/components/ui';

const ProtectedRoute = ({ children }) => {
  const { user, token, loading: authLoading } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    if (!authLoading && (!user || !token)) {
      navigate('/login');
    }
  }, [user, token, authLoading, navigate]);

  if (authLoading) {
    return <Loading />;
  }

  if (user && token) {
    return children;
  }

  return null;
};

export default ProtectedRoute;