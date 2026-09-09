import { Label } from "@/components/ui/label";
import { FieldError } from "@/components/ui/field";
import { DialogFooter } from "@/components/ui/dialog";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
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
            <FieldError id="role-error">{errors.role.message}</FieldError>
          )}
        </div>

        <DialogFooter className="pt-4">
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
        </DialogFooter>
      </form>
    </Modal>
  );
}
