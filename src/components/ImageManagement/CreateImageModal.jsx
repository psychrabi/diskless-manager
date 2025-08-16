import React from 'react';
import { Button, Modal } from '../ui';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import z from 'zod';
import { Save } from 'lucide-react';
import { useNotification } from '@/contexts/NotificationContext';
import { invoke } from '@tauri-apps/api/core';

const imageSchema = z.object({
  name: z.string().min(1, 'Image name is required'),
  size: z.string().min(1, 'Image Size is required'),
  imageType: z.enum(['zfs', 'fileio'], {
    required_error: 'Please select an image type',
  }),
});

const CreateImageModal = ({ openImageCreateModal, setOpenImageCreateModal }) => {
    const { showNotification } = useNotification();
  
  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    reset,
    watch,
  } = useForm({
    resolver: zodResolver(imageSchema),
    defaultValues: {
      name: "",
      size: "50G",
      imageType: "zfs"
    },
  });

  const watchedImageType = watch('imageType');

  const onSubmit = async (data) => {
    showNotification(`Adding new ${data.imageType.toUpperCase()} image ${data.name}`, 'info');

    setOpenImageCreateModal(false);

    if (data.imageType === 'zfs') {
      await invoke('create_master', { name: data.name, size: data.size })
        .then((response) => {
          console.log(response)
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => {
          showNotification(error, 'error',)
        })
    } else {
      await invoke('create_fileio_master', { name: data.name, size: data.size })
        .then((response) => {
          console.log(response)
          if (response.message) showNotification(response.message, 'success');
        }).catch((error) => {
          showNotification(error, 'error',)
        })
    }
  };

  return (
    <Modal isOpen={openImageCreateModal} onClose={() => setOpenImageCreateModal(false)} title="Create Master Image" size="xl">
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <fieldset className={`fieldset`}>
          <legend className='fieldset-legend'>Image Type</legend>
          <div className="flex gap-4">
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="radio"
                value="zfs"
                {...register('imageType')}
                className="w-4 h-4 text-blue-600 bg-gray-100 border-gray-300 focus:ring-blue-500"
              />
              <span>ZFS Volume (Recommended)</span>
            </label>
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="radio"
                value="fileio"
                {...register('imageType')}
                className="w-4 h-4 text-blue-600 bg-gray-100 border-gray-300 focus:ring-blue-500"
              />
              <span>FileIO (File-based)</span>
            </label>
          </div>
          {errors.imageType && <div className="text-red-500 text-xs">{errors.imageType.message}</div>}
        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor='name' className='fieldset-legend'>Master Name</legend>
          <input {...register('name')} type='text' id='name' placeholder="e.g., win11-enterprise (will create pool/name-master)"
            className='input w-full' />
          {errors.name && <div className="text-red-500 text-xs">{errors.name.message}</div>}
        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor='size' className='fieldset-legend'>Image Size</legend>
          <input {...register('size')} type='text' id='size' placeholder="e.g., 50G, 1T"
            className='input w-full' title="Enter size (e.g., 50G, 100G, 1T)"
          />
          {errors.size && <div className="text-red-500 text-xs">{errors.size.message}</div>}
        </fieldset>

        {watchedImageType === 'fileio' && (
          <div className="p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-md">
            <p className="text-sm text-blue-800 dark:text-blue-200">
              <strong>FileIO Note:</strong> File-based images are stored as regular files and may be slower than ZFS volumes. 
              They are useful when ZFS is not available or for testing purposes.
            </p>
          </div>
        )}

        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>Create Master</Button>
          <Button type="button" variant="destructive" onClick={() => setOpenImageCreateModal(false)}>Cancel</Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateImageModal;