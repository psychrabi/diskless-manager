import { File } from 'lucide-react'
import { Button, Card } from '../ui'
import { useForm } from 'react-hook-form';
import { useNotification } from '@/contexts/notification';
import { invoke } from '@tauri-apps/api/core';

export default function AdminPassword() {
  const { showNotification } = useNotification();

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
    if(!data.old_password === "admin123"){
      showNotification('Old password is incorrect', 'error');
      return;
    }
    if(data.new_password !== data.confirm_new_password){
      showNotification('New password and confirm new password do not match', 'error');
      return;
    }
    if(data.new_password.length < 6){
      showNotification('New password must be at least 6 characters long', 'error');
      return;
    }
    const token = localStorage.getItem('authToken') || '';
    await invoke('update_admin_password', { token, oldPassword:data.old_password,newPassword: data.new_password })
      .then((response) => {
        if (response) showNotification(response, 'success');
      })
      .catch((error) => {
        showNotification(error, 'error');
        console.log(error);
      });
  };

  return (
    <Card title="Admin password" icon={File} >
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className='space-y-4'>
          <fieldset className='fieldset flex-1'>
            <label className='fieldset-legend'>Old Password</label>
            <input type="password" className="input w-full" id="old_password" {...register('old_password')} placeholder="Old Password" />
            {errors.old_password && (
              <p className="mt-1 text-sm text-error">{errors.old_password.message}</p>
            )}
          </fieldset>
          <fieldset className='fieldset flex-1'>
            <label className='fieldset-legend'>New Password</label>
            <input type="password" className="input  w-full" id="new_password" {...register('new_password')} placeholder="New Password" />
            {errors.new_password && (
              <p className="mt-1 text-sm text-error">{errors.new_password.message}</p>
            )}
          </fieldset>
          <fieldset className='fieldset flex-1'>
            <label className='fieldset-legend'>Confirm New Password</label>
            <input type="password" className="input w-full" id="confirm_new_password" {...register('confirm_new_password')} placeholder="Confirm New Password" />
            {errors.confirm_new_password && (
              <p className="mt-1 text-sm text-error">{errors.confirm_new_password.message}</p>
            )}
          </fieldset>
          <Button variant="primary" type="submit">Save Admin Password</Button>
        </div>
      </form>
    </Card>
  )
}
