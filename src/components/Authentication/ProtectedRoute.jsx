import { useEffect } from 'react';
import { useAuth } from '@/contexts/AuthContext';
import { useNavigate } from 'react-router-dom';
import { Loading } from '@/components/ui';

const ProtectedRoute = ({ children }) => {
  const { user, token, loading } = useAuth();
  const navigate = useNavigate();

  useEffect(() => {
    // If not loading and no user/token, redirect to login
    if (!loading && (!user || !token)) {
      navigate('/login');
    }
  }, [user, token, loading, navigate]);

  // Show loading spinner while checking auth state
  if (loading) {
    return <Loading />;
  }

  // If user and token exist, render children
  if (user && token) {
    return children;
  }

  // This shouldn't be reached due to the redirect, but just in case
  return null;
};

export default ProtectedRoute;