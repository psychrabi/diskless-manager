import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { invoke } from '@tauri-apps/api/core';
import { useNavigate } from 'react-router-dom';
import { Card, Input, Button, Loading } from '@/components/ui';
import { useEffect, useState } from 'react';
import { useAuth } from '@/contexts/auth';
import { useAppStore } from '@/store/useAppStore';

// Define validation schema
const loginSchema = z.object({
  username: z.string().min(1, 'Username is required'),
  password: z.string().min(6, 'Password must be at least 6 characters')
});

const Login = () => {
  const [error, setError] = useState('');
  const navigate = useNavigate();
  const { login: setAuth } = useAuth();
  const { setServices } = useAppStore();
  const [preflightLoading, setPreflightLoading] = useState(true);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset
  } = useForm({
    resolver: zodResolver(loginSchema),
    defaultValues: {
      username: '',
      password: ''
    }
  });

  // Preflight check before showing login
  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const res = await invoke('check_package_status');
        const list = Array.isArray(res) ? res : (res ? Object.values(res) : []);
        if (!cancelled) {
          setServices(list);
          const allServicesInstalled = list.every(svc => svc?.installed);
          const poolExists = await invoke('zfs_pool_exists', { poolName: null });

          // Only redirect to setup if services are not installed
          if (!allServicesInstalled || !poolExists) {
            navigate('/setup');
          }
        }
      } catch (e) {
        console.warn('Preflight check failed:', e);
        // Proceed to login UI even if preflight fails
      } finally {
        if (!cancelled) setPreflightLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [navigate, setServices]);


  if (preflightLoading) {
    return <Loading />;
  }

  const onSubmit = async (data) => {
    setError('');

    try {
      const response = await invoke('login', {
        request: { username: data.username, password: data.password }
      });

      // Set auth context immediately so ProtectedRoute sees it
      setAuth(response.user, response.token);
      navigate('/');
    } catch (err) {
      setError(err.message || 'Login failed');
      reset({ password: '' }); // Clear password field on error
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-base-200 text-base-content p-4">
      <Card className="w-full max-w-md">
        <div className="text-center mb-6">
          <h1 className="text-2xl font-bold text-base-content">Diskless Manager</h1>
          <p className="text-base-content/70 mt-2">Sign in to your account</p>
        </div>

        {error && (
          <div className="mb-4 p-3 rounded-md bg-error/10 text-error">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
          <div>
            <label htmlFor="username" className="block text-sm font-medium text-base-content mb-1">
              Username
            </label>
            <Input
              id="username"
              type="text"
              register={register('username')}
              placeholder="Enter your username"
              className="w-full"
              error={errors.username?.message}
            />
            {errors.username && (
              <p className="mt-1 text-sm text-error">{errors.username.message}</p>
            )}
          </div>

          <div>
            <label htmlFor="password" className="block text-sm font-medium text-base-content mb-1">
              Password
            </label>
            <Input
              id="password"
              type="password"
              register={register('password')}
              placeholder="Enter your password"
              className="w-full"
              error={errors.password?.message}
            />
            {errors.password && (
              <p className="mt-1 text-sm text-error">{errors.password.message}</p>
            )}
          </div>

          <Button
            type="submit"
            disabled={isSubmitting}
            className="w-full"
            variant="primary"
          >
            {isSubmitting ? 'Signing in...' : 'Sign in'}
          </Button>
        </form>

        <div className="mt-6 text-center text-sm text-base-content/70">
          <p>Default credentials: admin / admin123</p>
        </div>
      </Card>
    </div>
  );
};

export default Login;