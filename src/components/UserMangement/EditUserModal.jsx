import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { Modal, Input, Button } from '@/components/ui';
import { useUserManagement } from '@/hooks/useUserManagement';
import { useToastStore } from '@/store/useToastStore';

const editUserSchema = z.object({
  username: z
    .string()
    .min(3, 'Username must be at least 3 characters')
    .max(50, 'Username must be less than 50 characters')
    .regex(/^[a-zA-Z0-9_-]+$/, 'Username can only contain letters, numbers, underscores, and hyphens'),
  role: z.enum(['admin', 'user'], {
    errorMap: () => ({ message: 'Role must be either admin or user' }),
  }),
});

export default function EditUserModal({ isOpen, onClose, user }) {
  const { updateUser, loading } = useUserManagement();
  const { success, error } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    resolver: zodResolver(editUserSchema),
    defaultValues: {
      username: user.username,
      role: user.role,
    },
  });

  const onSubmit = async (data) => {
    try {
      await updateUser(user.id, {
        username: data.username,
        role: data.role,
      });
      success('Update User', `User "${data.username}" updated successfully`);
      onClose();
    } catch (err) {
      error('Update User', err.message || 'Failed to update user');
    }
  };

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="Edit User"
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
            {isSubmitting || loading ? 'Updating...' : 'Update User'}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
