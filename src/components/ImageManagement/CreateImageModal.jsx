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
      size: "50G"
    },
  });

  const onSubmit = async (data) => {
    showNotification(`Adding new image ${data.name}`, 'info');

    setOpenImageCreateModal(false);

    await invoke('create_master', { name: data.name, size: data.size })
      .then((response) => {
        console.log(response)
        if (response.message) showNotification(response.message, 'success');
      }).catch((error) => {
        showNotification(error, 'error',)
      })
  };


  return (
    <Modal isOpen={openImageCreateModal} onClose={() => setOpenImageCreateModal(false)} title="Create Master Image" size="xl">
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-2">
        <fieldset className={`fieldset`}>
          <legend htmlFor='name' className='fieldset-legend'>Master Name</legend>
          <input {...register('name')} type='text' id='name' placeholder="e.g., win11-enterprise (will create pool/name-master)"
            className='input w-full' />
          {errors.name && <div className="text-red-500 text-xs">{errors.name.message}</div>}
        </fieldset>
        <fieldset className={`fieldset`}>
          <legend htmlFor='size' className='fieldset-legend'>Master Name</legend>
          <input {...register('size')} type='text' id='size' placeholder="e.g., 50G, 1T"

            className='input w-full' title="Enter size (e.g., 50G, 100G, 1T)"
          />
          {errors.size && <div className="text-red-500 text-xs">{errors.size.message}</div>}
        </fieldset>

        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>Create Master</Button>
          <Button type="button" variant="destructive" onClick={() => setOpenImageCreateModal(false)}>Cancel</Button>
        </div>
      </form>
    </Modal>
  );
};

export default CreateImageModal;