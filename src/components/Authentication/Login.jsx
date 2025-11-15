import { Button, Card, Input } from '@/components/ui';
import { useAuth } from '@/contexts/auth';
import { useNotification } from '@/contexts/notification';
import { zodResolver } from '@hookform/resolvers/zod';
import { invoke } from '@tauri-apps/api/core';
import { useForm } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import { z } from 'zod';

// Define validation schema
const loginSchema = z.object({
  username: z.string().min(1, 'Username is required'),
  password: z.string().min(6, 'Password must be at least 6 characters')
});

const Login = () => {
  const navigate = useNavigate();
  const { login: setAuth } = useAuth();
  const { showNotification } = useNotification();

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

  const onSubmit = async (data) => {

    try {
      const response = await invoke('login', {
        request: { username: data.username, password: data.password }
      });

      console.log(response);

      // Set auth context immediately so ProtectedRoute sees it
      setAuth(response.user, response.token);
      navigate('/');
    } catch (e) {
      showNotification('error', 'Login Failed', e.message || 'An unknown error occurred');
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