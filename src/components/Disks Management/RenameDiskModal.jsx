import { useNotification } from '@/contexts/notification';
import { zodResolver } from '@hookform/resolvers/zod';
import { invoke } from '@tauri-apps/api/core';
import { Save } from 'lucide-react';
import { useForm } from 'react-hook-form';
import z from 'zod';
import { Button, Modal } from '../ui';

const renameDiskSchema = z.object({
  newName: z.string().min(1, 'New name is required').regex(/^[\w-]+$/, 'Name can only contain alphanumeric characters, underscores, and hyphens'),
});

const RenameDiskModal = ({ openRenameModal, setOpenRenameModal, selectedDisk, refresh }) => {
  const { showNotification } = useNotification();
  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    resolver: zodResolver(renameDiskSchema),
    defaultValues: {
      newName: "",
    },
  });

  const onSubmit = async (data) => {
    if (!selectedDisk) return;

    //TODO: Fix the name extraction logic
    const baseName = selectedDisk.split('/').pop() || '';
    showNotification(`Renaming disk from ${baseName} to ${data.newName}`, 'info');
    setOpenRenameModal(false);

    // Get token from localStorage
    const token = localStorage.getItem('authToken') || '';
    		await invoke('rename_zfs_dataset', { token, old: selectedDisk.name, new: data.newName })
      .then((response) => {
        if (response.message) showNotification(response.message, 'success');
        reset();
      }).catch((error) => {
        showNotification(error, 'error');
      }).finally(() => {
        refresh && refresh();
      });
  };

  const handleClose = () => {
    setOpenRenameModal(false);
    reset();
    showNotification("Disk rename cancelled", 'info',)
  };

  //TODO: Fix the name extraction logic
  const displayName = selectedDisk.name;

  return (
    <Modal isOpen={openRenameModal} onClose={handleClose} title="Rename disk" size="xl">
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <div className="space-y-2">
          <p className="text-sm text-gray-600 dark:text-gray-400">
            Rename disk "{displayName}" to a new name.
          </p>
          <fieldset className={`fieldset`}>
            <legend htmlFor='newName' className='fieldset-legend'>New Name</legend>
            <input
              {...register('newName')}
              type='text'
              id='newName'
              placeholder="e.g., boot-disk, writeback-disk"
              className='input w-full'
            />
            {errors.newName && <div className="text-red-500 text-xs">{errors.newName.message}</div>}
          </fieldset>
        </div>
        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>Rename disk</Button>
          <Button type="button" variant="destructive" onClick={handleClose}>Cancel</Button>
        </div>
      </form>
    </Modal>
  );
};

export default RenameDiskModal;
