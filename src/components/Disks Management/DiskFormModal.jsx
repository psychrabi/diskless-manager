import { useNotification } from '@/contexts/notification';
import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { Input, Select, FormModal } from '../ui';
import { useForm } from 'react-hook-form';

const diskSchema = z.object({
	zpool: z.string().min(1, 'Zpool is required'),
	name: z.string().min(4, 'Disk name is required'),
	usage_type: z.string().min(1, 'Disk type is required'),
	size: z.string().optional(),
});

const DiskFormModal = ({ zpools, isOpen, setIsOpen, refresh }) => {
  const onSubmit = async (data, showNotification) => {
    showNotification(`Adding new disk ${data.name}`, 'info');
    const token = localStorage.getItem('authToken') || '';
    await invoke('create_zfs_dataset', { token, zpool: data.zpool, name: data.name, usageType: data.usage_type, size: data.size ?? '' });
    showNotification('success', 'Dataset Created', `Dataset ${data.name} created successfully.`);
  };

  const defaultValues = { zpool: '', name: '', usage_type: 'image', size: '' };

  return (
    <FormModal
      isOpen={isOpen}
      setIsOpen={setIsOpen}
      title={'Add disk'}
      schema={diskSchema}
      defaultValues={defaultValues}
      onSubmit={onSubmit}
      submitButtonText={'Create disk'}
      refresh={refresh}
    >
      {({ register, errors, setValue, watch }) => {
        const usageType = watch('usage_type');
        return (
          <>
            <Select
              label="Select zpool"
              {...register('zpool')}
              onChange={(e) => setValue('zpool', e.target.value)}
              error={errors.zpool?.message}
            >
              <option value="">Select zpool</option>
              {zpools.map(p => <option key={p} value={p}>{p}</option>)}
            </Select>

            <Select
              label="Disk type"
              {...register('usage_type')}
              onChange={(e) => setValue('usage_type', e.target.value)}
              error={errors.usage_type?.message}
            >
              <option value="">Select disk type</option>
              <option value="image">Image (store images)</option>
              <option value="writeback">Writeback (store clones)</option>
              <option value="game">Game (Game Disks - creates zvol)</option>
            </Select>

            <Input
              label="Disk Name"
              {...register('name')}
              type='text'
              placeholder="Enter disk name"
              error={errors.name?.message}
            />

            {usageType === 'game' && (
              <Input
                label="Disk size"
                {...register('size')}
                type='text'
                placeholder="e.g. 50G"
                error={errors.size?.message}
              />
            )}
          </>
        );
      }}
    </FormModal>
  );
};

export default DiskFormModal;