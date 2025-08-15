import React from 'react';
import { Button, Modal } from '../ui';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import z from 'zod';
import { Save } from 'lucide-react';
import { useNotification } from '@/contexts/NotificationContext';
import { invoke } from '@tauri-apps/api/core';

const renameImageSchema = z.object({
  newName: z.string().min(1, 'New name is required').regex(/^[\w-]+$/, 'Name can only contain alphanumeric characters, underscores, and hyphens'),
});

const RenameImageModal = ({ openRenameModal, setOpenRenameModal, selectedImage, refresh }) => {
  const { showNotification } = useNotification();

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    resolver: zodResolver(renameImageSchema),
    defaultValues: {
      newName: "",
    },
  });

  const onSubmit = async (data) => {
    if (!selectedImage) return;

    // Extract the base name from the full ZFS path (e.g., "diskless/win11-master" -> "win11")
    const baseName = selectedImage.split('/').pop()?.replace('-master', '') || '';
    
    showNotification(`Renaming image from ${baseName} to ${data.newName}`, 'info');

    setOpenRenameModal(false);

    await invoke('rename_master', { 
      oldName: selectedImage, 
      newName: data.newName 
    })
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
  };

  // Extract the base name from the full ZFS path for display
  const displayName = selectedImage ? selectedImage.split('/').pop()?.replace('-master', '') || selectedImage : '';

  return (
    <Modal isOpen={openRenameModal} onClose={handleClose} title="Rename Master Image" size="xl">
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <div className="space-y-2">
          <p className="text-sm text-gray-600 dark:text-gray-400">
            Rename master image "{displayName}" to a new name.
          </p>
          <fieldset className={`fieldset`}>
            <legend htmlFor='newName' className='fieldset-legend'>New Name</legend>
            <input 
              {...register('newName')} 
              type='text' 
              id='newName' 
              placeholder="e.g., win11-enterprise (will create pool/name-master)"
              className='input w-full' 
            />
            {errors.newName && <div className="text-red-500 text-xs">{errors.newName.message}</div>}
          </fieldset>
        </div>

        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>Rename Image</Button>
          <Button type="button" variant="destructive" onClick={handleClose}>Cancel</Button>
        </div>
      </form>
    </Modal>
  );
};

export default RenameImageModal;
