import { invoke } from '@tauri-apps/api/core';
import { useNotification } from '@/contexts/notification';
import { Button, Input, Modal, Select } from '../ui';
import { useForm } from 'react-hook-form';
import { z } from 'zod';
import { zodResolver } from '@hookform/resolvers/zod';
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

  const {
    register,
    handleSubmit,
    formState: { errors },
    setValue,
    reset,
    watch,
  } = useForm({
    resolver: zodResolver(clientSchema),
    defaultValues: {
      name: client?.name || '',
      mac: client?.mac || '',
      ip: client?.ip || '',
      master: client?.master || '',
      snapshot: client?.snapshot || '',
    },
  });

  // Keep form in sync with client prop
  useEffect(() => {    
    reset({
      name: client?.name || '',
      mac: client?.mac || '',
      ip: client?.ip || '',
      master: client?.master || '',
      snapshot: client?.snapshot || null,
      pxeMode: client?.pxeMode || 'uefi',
    });
  }, [client, reset]);

 

  const onSubmit = async (data) => {
    setIsOpen(false);

    if (!client.id) {
      showNotification(`Adding new client ${data.name}`, 'info');
      const token = localStorage.getItem('authToken') || '';
      await invoke('add_client', { token, req: data })
        .then((response) => {
          if (response.message) showNotification(response.message, 'success');
        })
        .catch((error) => {
          showNotification(error, 'error');
        })
        .finally(() => {
          refresh();
        });
    } else {
      showNotification(`Editing client ${data.name}`, 'info');
      const token = localStorage.getItem('authToken') || '';
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
      })
        .then((response) => {
          if (response.message) showNotification(response.message, 'success');
        })
        .catch((error) => {
          showNotification(error, 'error');
        })
        .finally(() => {
          refresh();
        });
    }
  };

  const selectedMaster = watch('master');

   // When master selection changes, clear snapshot to avoid stale selections
  useEffect(() => {
    // Only clear snapshot if we're not in edit mode or if the master actually changed
    if (!client?.id || selectedMaster !== client?.master) {
      setValue('snapshot', '');
    }
  }, [selectedMaster, setValue, client?.id, client?.master]);

  return (
    <Modal isOpen={isOpen} onClose={() => setIsOpen(false)} title={client.id ? 'Edit Client' : 'Add Client'}>
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-2">


        <fieldset className={`fieldset`}>
          <legend htmlFor='snapshotName' className='fieldset-legend'>Client Name</legend>
          <input {...register('name')} type='text' id='snapshotName' placeholder="enter client name" className='input w-full' />
          {errors.name && <div className="text-error text-xs">{errors.name.message}</div>}
        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor='mac' className='fieldset-legend'>MAC Address</legend>
          <input {...register('mac')} type='text' id='mac' placeholder="XX:XX:XX:XX:XX:XX" className='input w-full' />
          {errors.mac && <div className="text-error text-xs">{errors.mac.message}</div>}

        </fieldset>
        <fieldset className={`fieldset`}>
          <legend htmlFor='ip' className='fieldset-legend'>IP Address</legend>
          <input {...register('ip')} type='text' id='ip' placeholder="X.X.X.X" className='input w-full' />
          {errors.ip && <div className="text-error text-xs">{errors.ip.message}</div>}

        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor="master" className='fieldset-legend'>Select Image</legend>
          <select {...register('master')} id="master" className='select w-full' >
            <option value="">Select image ...</option>

            {masters.map((master) => (
              <option key={master.name} value={master.name}>
                {master.name}
              </option>
            ))}    </select>
          {errors.master && <div className="text-error text-xs">{errors.master.message}</div>}

        </fieldset>
        <fieldset className={`fieldset`}>
          <legend htmlFor="snapshot" className='fieldset-legend'>Select Snapshot</legend>
          <select
            {...register('snapshot')}
            id="snapshot" 
            className='select w-full' 
            disabled={!selectedMaster}
            onChange={(e) => {
              console.log('DEBUG: Snapshot changed to:', e.target.value);
              setValue('snapshot', e.target.value);
            }}
          >
            <option value="">Use master directly</option>
            {masters.find(m => m.name === selectedMaster)?.snapshots?.map((snap) => (
              <option key={snap.name} value={snap.name}>
                {snap.name} ({snap.created}, {snap.size})
              </option>
            ))}
          </select>
          <div className="text-xs text-gray-500 mt-1">
            DEBUG: Current snapshot value: {watch('snapshot') || 'empty'}<br/>
            DEBUG: Available snapshots: {masters.find(m => m.name === selectedMaster)?.snapshots?.map(s => s.name).join(', ') || 'none'}
          </div>
          {errors.snapshot && <div className="text-error text-xs">{errors.snapshot.message}</div>}
        </fieldset>
     

        <div className="flex justify-end space-x-3">
          <Button type="submit" variant="primary">{client.id ? 'Update Client' : 'Add Client'}</Button>
          <Button type="button" onClick={() => setIsOpen(false)} variant="destructive">Cancel</Button>
        </div>
      </form>
    </Modal>
  );
};

export default ClientFormModal;