import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { invoke } from '@tauri-apps/api/core';
import { useNavigate } from 'react-router-dom';
import { Card, Input, Button } from '@/components/ui';
import { useState } from 'react';

// Define validation schema
const loginSchema = z.object({
  username: z.string().min(1, 'Username is required'),
  password: z.string().min(6, 'Password must be at least 6 characters')
});

const Login = () => {
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const navigate = useNavigate();

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset
  } = useForm({
    resolver: zodResolver(loginSchema),
    defaultValues: {
      username: '',
      password: ''
    }
  });

  const onSubmit = async (data) => {
    setLoading(true);
    setError('');

    try {
      const response = await invoke('login', {
        request: { username: data.username, password: data.password }
      });

      localStorage.setItem('authToken', response.token);
      localStorage.setItem('user', JSON.stringify(response.user));
      navigate('/');
    } catch (err) {
      setError(err.message || 'Login failed');
      reset({ password: '' }); // Clear password field on error
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50 dark:bg-gray-900 p-4">
      <Card className="w-full max-w-md">
        <div className="text-center mb-6">
          <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Diskless Manager</h1>
          <p className="text-gray-600 dark:text-gray-400 mt-2">Sign in to your account</p>
        </div>

        {error && (
          <div className="mb-4 p-3 bg-red-100 text-red-700 rounded-md dark:bg-red-900 dark:text-red-200">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
          <div>
            <label htmlFor="username" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
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
              <p className="mt-1 text-sm text-red-600 dark:text-red-400">{errors.username.message}</p>
            )}
          </div>

          <div>
            <label htmlFor="password" className="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-1">
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
              <p className="mt-1 text-sm text-red-600 dark:text-red-400">{errors.password.message}</p>
            )}
          </div>

          <Button
            type="submit"
            disabled={loading}
            className="w-full"
            variant="primary"
          >
            {loading ? 'Signing in...' : 'Sign in'}
          </Button>
        </form>

        <div className="mt-6 text-center text-sm text-gray-600 dark:text-gray-400">
          <p>Default credentials: admin / admin123</p>
        </div>
      </Card>
    </div>
  );
};

export default Login;