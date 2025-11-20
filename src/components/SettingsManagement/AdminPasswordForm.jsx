import { File } from 'lucide-react'
import { useForm } from 'react-hook-form';
import { useNotification } from '@/contexts/notification';
import { useSettings } from '@/hooks/useSettings';
import { Button, Card } from '../ui'

export default function AdminPasswordForm() {
  const { showNotification } = useNotification();
  const { updatePassword } = useSettings();

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    defaultValues: {
      old_password: '',
      new_password: '',
      confirm_new_password: '',
    }
  });

  const onSubmit = async (data) => {
    showNotification(`Updating Admin Password`, 'info');
    // Get token from localStorage
    if (!data.old_password === "admin123") {
      showNotification('Old password is incorrect', 'error');
      return;
    }
    if (data.new_password !== data.confirm_new_password) {
      showNotification('New password and confirm new password do not match', 'error');
      return;
    }
    if (data.new_password.length < 6) {
      showNotification('New password must be at least 6 characters long', 'error');
      return;
    }
    await updatePassword(data.old_password, data.new_password);
  };

  return (
    <Card title="Admin password" icon={File} >
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className='space-y-4'>
          <fieldset className='fieldset w-full'>
            <label className='fieldset-legend'>Old Password</label>
            <input type="password" className="input w-full" id="old_password" {...register('old_password')} placeholder="Old Password" />
            {errors.old_password && (
              <p className="mt-1 text-sm text-error">{errors.old_password.message}</p>
            )}
          </fieldset>
          <fieldset className='fieldset  w-full'>
            <label className='fieldset-legend'>New Password</label>
            <input type="password" className="input  w-full" id="new_password" {...register('new_password')} placeholder="New Password" />
            {errors.new_password && (
              <p className="mt-1 text-sm text-error">{errors.new_password.message}</p>
            )}
          </fieldset>
          <fieldset className='fieldset  w-full'>
            <label className='fieldset-legend'>Confirm New Password</label>
            <input type="password" className="input w-full" id="confirm_new_password" {...register('confirm_new_password')} placeholder="Confirm New Password" />
            {errors.confirm_new_password && (
              <p className="mt-1 text-sm text-error">{errors.confirm_new_password.message}</p>
            )}
          </fieldset>
          <Button variant="primary" type="submit">Update Password</Button>
        </div>
      </form>
    </Card>
  )
}
