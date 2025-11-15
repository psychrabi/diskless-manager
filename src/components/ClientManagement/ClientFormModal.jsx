import { useNotification } from '@/contexts/notification';
import { zodResolver } from '@hookform/resolvers/zod';
import { invoke } from '@tauri-apps/api/core';
import { Save } from 'lucide-react';
import { useEffect } from 'react';
import { useForm, useWatch } from 'react-hook-form';
import { z } from 'zod';
import { Input, Select } from '../ui';

const clientSchema = z.object({
  name: z.string().min(1, 'Client name is required'),
  mac: z.string().regex(/^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/, "Invalid MAC address format"),
  ip: z.string().regex(/^([\d]{1,3}\.){3}\d{1,3}$/, 'Invalid IP address format. Use X.X.X.X'),
  master: z.string().optional(),
  snapshot: z.string().optional().nullable(),
});


const ClientFormModal = ({ client, masters, isOpen, setIsOpen, refresh }) => {
  const { showNotification } = useNotification();

  const defaultValues = {
    name: client?.name || '',
    mac: client?.mac || '',
    ip: client?.ip || '',
    master: client?.master || '',
    snapshot: client?.snapshot || null,
    pxeMode: client?.pxeMode || 'uefi',
  };

  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    control
  } = useForm({
    resolver: zodResolver(clientSchema),
    defaultValues
  });

  const onSubmit = async (data) => {
    const token = localStorage.getItem('authToken') || '';
    if (!client.id) {
      showNotification(`Adding new client ${data.name}`, 'info');
      await invoke('add_client', { token, req: data });
      showNotification('success', 'Client Added', `Client ${data.name} added successfully.`);
    } else {
      showNotification(`Editing client ${data.name}`, 'info');
      await invoke('edit_client', {
        token,
        clientId: client.id,
        data: {
          name: data.name,
          mac: data.mac,
          ip: data.ip,
          master: data.master,
          snapshot: data.snapshot || null,
        }
      });
      showNotification('success', 'Client Updated', `Client ${data.name} updated successfully.`);
    }
    refresh();
  };

  const selectedMaster = useWatch({
    control,
    name: 'master'
  });

  useEffect(() => {
    if (!client?.id || selectedMaster !== client?.master) {
      setValue('snapshot', '');
    }
  }, [client?.id, client?.master, selectedMaster, setValue]);

  return (
    <Modal isOpen={isOpen} onClose={() => setIsOpen(false)} title="Create Client" size="xl">
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <Input
          label="Client Name"
          {...register('name')}
          type='text'
          placeholder="enter client name"
          error={errors.name?.message}
        />
        <Input
          label="MAC Address"
          {...register('mac')}
          type='text'
          placeholder="XX:XX:XX:XX:XX:XX"
          error={errors.mac?.message}
        />
        <Input
          label="IP Address"
          {...register('ip')}
          type='text'
          placeholder="X.X.X.X"
          error={errors.ip?.message}
        />
        <Select
          label="Select Image"
          {...register('master')}
          onChange={(e) => setValue('master', e.target.value)}
          error={errors.master?.message}
        >
          <option value="">Select image ...</option>
          {masters.map((master) => (
            <option key={master.name} value={master.name}>
              {master.name}
            </option>
          ))}
        </Select>
        <Select
          label="Select Snapshot"
          {...register('snapshot')}
          disabled={!selectedMaster}
          onChange={(e) => setValue('snapshot', e.target.value)}
          error={errors.snapshot?.message}
        >
          <option value="">Use master directly</option>
          {masters.find(m => m.name === selectedMaster)?.snapshots?.map((snap) => (
            <option key={snap.name} value={snap.name}>
              {snap.name} ({snap.created}, {snap.size})
            </option>
          ))}
        </Select>
        <div className="text-xs text-gray-500 mt-1">
          DEBUG: Current snapshot value: {useWatch({ control, name: 'snapshot' }) || 'empty'}<br />
          DEBUG: Available snapshots: {masters.find(m => m.name === selectedMaster)?.snapshots?.map(s => s.name).join(', ') || 'none'}
        </div>
        <div className="mt-6 flex justify-end space-x-3">
          <Button type="submit" variant="primary" icon={Save}>Create Master</Button>
          <Button type="button" variant="destructive" onClick={() => setIsOpen(false)}>Cancel</Button>
        </div>
      </form>
    </Modal>
  );
};

export default ClientFormModal;