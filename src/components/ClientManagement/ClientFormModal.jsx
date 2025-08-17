import { invoke } from '@tauri-apps/api/core';
import { useNotification } from '../../contexts/NotificationContext';
import { Button, Input, Modal, Select } from '../ui';
import { useForm } from 'react-hook-form';
import { z } from 'zod';
import { zodResolver } from '@hookform/resolvers/zod';
import { useEffect } from 'react';

const clientSchema = z.object({
  name: z.string().min(1, 'Client name is required'),
  mac: z.string().regex(/^([0-9A-Fa-f]{2}[:-]){5}([0-9A-Fa-f]{2})$/, "Invalid MAC address format"),
  ip: z.string().regex(/^([\d]{1,3}\.){3}\d{1,3}$/, 'Invalid IP address format. Use X.X.X.X'),
  master: z.string().min(1, 'Master image is required'),
  snapshot: z.string().optional().nullable(),
  pxeMode: z.enum(['legacy', 'uefi', 'secureboot'], {
    errorMap: () => ({ message: 'Please select a PXE boot mode' })
  })
});

const ClientFormModal = ({ client, setClient, masters, isOpen, setIsOpen, refresh }) => {
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
      pxeMode: client?.pxeMode || 'uefi',
    },
  });

  // Keep form in sync with client prop
  useEffect(() => {
    reset({
      name: client?.name || '',
      mac: client?.mac || '',
      ip: client?.ip || '',
      master: client?.master || '',
      snapshot: client?.snapshot || '',
      pxeMode: client?.pxeMode || 'uefi',
    });
  }, [client, reset]);

  const onSubmit = async (data) => {
    setIsOpen(false);
    const clientData = {
      ...data,
      pxeMode: data.pxeMode || 'uefi' // Ensure pxeMode is always set
    };

    if (!client.id) {
      showNotification(`Adding new client ${data.name}`, 'info');
      const token = localStorage.getItem('authToken') || '';
      await invoke('add_client', { token, req: clientData })
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
          name: clientData.name,
          mac: clientData.mac,
          ip: clientData.ip,
          master: clientData.master,
          snapshot: clientData.snapshot || null,
          pxeMode: clientData.pxeMode
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
  
  return (
    <Modal isOpen={isOpen} onClose={() => setIsOpen(false)} title={client.id ? 'Edit Client' : 'Add Client'}>
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-2">


        <fieldset className={`fieldset`}>
          <legend htmlFor='snapshotName' className='fieldset-legend'>Client Name</legend>
          <input {...register('name')} type='text' id='snapshotName' placeholder="enter client name" className='input w-full' />
          {errors.name && <div className="text-red-500 text-xs">{errors.name.message}</div>}
        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor='mac' className='fieldset-legend'>MAC Address</legend>
          <input {...register('mac')} type='text' id='mac' placeholder="XX:XX:XX:XX:XX:XX" className='input w-full' />
          {errors.mac && <div className="text-red-500 text-xs">{errors.mac.message}</div>}

        </fieldset>
        <fieldset className={`fieldset`}>
          <legend htmlFor='ip' className='fieldset-legend'>IP Address</legend>
          <input {...register('ip')} type='text' id='ip' placeholder="X.X.X.X" className='input w-full' />
          {errors.ip && <div className="text-red-500 text-xs">{errors.ip.message}</div>}

        </fieldset>

        <fieldset className={`fieldset`}>
          <legend htmlFor="master" className='fieldset-legend'>Select Image</legend>
          <select   {...register('master')} id="master" defaultValue={client.master || ''} className='select w-full' >
            <option value="">Select image ...</option>

            {masters.map((master) => (
              <option key={master.name} value={master.name}>
                {master.name}
              </option>
            ))}    </select>
          {errors.master && <div className="text-red-500 text-xs">{errors.master.message}</div>}

        </fieldset>
        <fieldset className={`fieldset`}>
          <legend htmlFor="snapshot" className='fieldset-legend'>Select Snapshot</legend>
          <select   {...register('snapshot')} value={watch('snapshot')} onChange={e => setValue('snapshot', e.target.value)}
            id="snapshot" defaultValue={client.master || ''} className='select w-full' disabled={!selectedMaster}
          >
            <option value="">Use master directly</option>
            {masters.find(m => m.name === selectedMaster)?.snapshots?.map((snap) => (
              <option key={snap.name} value={snap.name}>
                {snap.name} ({snap.created}, {snap.size})
              </option>
            ))}
          </select>
          {errors.snapshot && <div className="text-red-500 text-xs">{errors.snapshot.message}</div>}
        </fieldset>
        <fieldset className={`fieldset`}>
          <legend htmlFor="pxeMode" className='fieldset-legend'>PXE Boot Mode</legend>
          <select 
            {...register('pxeMode')} 
            id="pxeMode" 
            className='select w-full'
          >
            <option value="legacy">Legacy BIOS (undionly.kpxe)</option>
            <option value="uefi">UEFI (ipxe.efi)</option>
            <option value="secureboot">Secure Boot (secureboot.efi)</option>
          </select>
          {errors.pxeMode && <div className="text-red-500 text-xs">{errors.pxeMode.message}</div>}
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