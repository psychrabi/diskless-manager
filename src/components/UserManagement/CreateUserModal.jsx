import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { Modal, Input, Button } from '@/components/ui';
import { useUserManagement } from '@/hooks/useUserManagement';
import { useToastStore } from '@/store/useToastStore';

const createUserSchema = z.object({
  username: z
    .string()
    .min(3, 'Username must be at least 3 characters')
    .max(50, 'Username must be less than 50 characters')
    .regex(/^[a-zA-Z0-9_-]+$/, 'Username can only contain letters, numbers, underscores, and hyphens'),
  password: z
    .string()
    .min(6, 'Password must be at least 6 characters')
    .max(100, 'Password must be less than 100 characters'),
  confirmPassword: z.string(),
  role: z.enum(['admin', 'user'], {
    errorMap: () => ({ message: 'Role must be either admin or user' }),
  }),
}).refine((data) => data.password === data.confirmPassword, {
  message: "Passwords don't match",
  path: ['confirmPassword'],
});

export default function CreateUserModal({ isOpen, onClose }) {
  const { createUser, loading } = useUserManagement();
  const { success, error } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    resolver: zodResolver(createUserSchema),
    defaultValues: {
      username: '',
      password: '',
      confirmPassword: '',
      role: 'user',
    },
  });

  const onSubmit = async (data) => {
    try {
      await createUser(data.username, data.password, data.role);
      success('Create User', `User "${data.username}" created successfully`);
      onClose();
    } catch (err) {
      error('Create User', err.message || 'Failed to create user');
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Create New User"
      size="md"
    >
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <Input
          label="Username"
          id="username"
          register={register('username')}
          error={errors.username?.message}
          placeholder="Enter username"
          autoFocus
        />

        <Input
          label="Password"
          id="password"
          type="password"
          register={register('password')}
          error={errors.password?.message}
          placeholder="Enter password"
        />

        <Input
          label="Confirm Password"
          id="confirmPassword"
          type="password"
          register={register('confirmPassword')}
          error={errors.confirmPassword?.message}
          placeholder="Confirm password"
        />

        <div className="form-control">
          <label className="label">
            <span className="label-text">Role</span>
          </label>
          <select
            className={`select select-bordered w-full ${
              errors.role ? 'select-error' : ''
            }`}
            {...register('role')}
          >
            <option value="user">User</option>
            <option value="admin">Admin</option>
          </select>
          {errors.role && (
            <label className="label">
              <span className="label-text-alt text-error">
                {errors.role.message}
              </span>
            </label>
          )}
        </div>

        <div className="flex justify-end gap-2 pt-4">
          <Button
            type="button"
            variant="ghost"
            onClick={onClose}
            disabled={isSubmitting || loading}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            disabled={isSubmitting || loading}
          >
            {isSubmitting || loading ? 'Creating...' : 'Create User'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
