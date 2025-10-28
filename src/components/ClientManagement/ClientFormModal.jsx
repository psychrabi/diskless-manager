import { invoke } from '@tauri-apps/api/core';
import { useNotification } from '@/contexts/notification';
import { Input, Select, FormModal } from '../ui';
import { z } from 'zod';
import { useEffect } from 'react';

const clientSchema = z.object({
  name: z.string().min(1, 'Client name is required'),
  mac: z.string().regex(/^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/, "Invalid MAC address format"),
  ip: z.string().regex(/^([\d]{1,3}\.){3}\d{1,3}$/, 'Invalid IP address format. Use X.X.X.X'),
  master: z.string().optional(),
  snapshot: z.string().optional().nullable(), 
});

const ClientFormModal = ({ client, masters, isOpen, setIsOpen, refresh }) => {
  const { showNotification } = useNotification();

  const onSubmit = async (data, showNotification) => {
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
  };

  const defaultValues = {
    name: client?.name || '',
    mac: client?.mac || '',
    ip: client?.ip || '',
    master: client?.master || '',
    snapshot: client?.snapshot || null,
    pxeMode: client?.pxeMode || 'uefi',
  };

  return (
    <FormModal
      isOpen={isOpen}
      setIsOpen={setIsOpen}
      title={client.id ? 'Edit Client' : 'Add Client'}
      schema={clientSchema}
      defaultValues={defaultValues}
      onSubmit={onSubmit}
      submitButtonText={client.id ? 'Update Client' : 'Add Client'}
      refresh={refresh}
    >
      {({ register, errors, setValue, watch }) => {
        const selectedMaster = watch('master');

        useEffect(() => {
          if (!client?.id || selectedMaster !== client?.master) {
            setValue('snapshot', '');
          }
        }, [selectedMaster, setValue, client?.id, client?.master]);

        return (
          <>
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
              DEBUG: Current snapshot value: {watch('snapshot') || 'empty'}<br />
              DEBUG: Available snapshots: {masters.find(m => m.name === selectedMaster)?.snapshots?.map(s => s.name).join(', ') || 'none'}
            </div>
          </>
        );
      }}
    </FormModal>
  );
};

export default ClientFormModal;