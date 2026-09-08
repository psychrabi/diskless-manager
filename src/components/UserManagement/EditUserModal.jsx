import { Label } from "@/components/ui/label";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
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

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="role">
            <span className="text-sm font-medium">Role</span>
          </Label>
          <NativeSelect
            id="role"
            aria-invalid={!!errors.role}
            aria-describedby={errors.role ? "role-error" : undefined}
            {...register('role')}
          >
            <NativeSelectOption value="user">User</NativeSelectOption>
            <NativeSelectOption value="admin">Admin</NativeSelectOption>
          </NativeSelect>
          {errors.role && (
            <Label className="flex items-center gap-2 text-sm font-medium">
              <span id="role-error" role="alert" className="text-xs text-destructive">
                {errors.role.message}
              </span>
            </Label>
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
