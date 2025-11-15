import { useAuth } from '@/contexts/auth';
import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';

const PublicRoute = ({ children }) => {
    const { user, token } = useAuth();
    const navigate = useNavigate();

    useEffect(() => {
        if (token && user) {
            navigate('/');
        }
    }, [user, token, navigate]);


    if (!user && !token) {
        return children;
    }

    return null;
};

export default PublicRoute;