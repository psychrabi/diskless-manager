import { useNotification } from '@/contexts/notification';
import { zodResolver } from '@hookform/resolvers/zod';
import { invoke } from '@tauri-apps/api/core';
import { useForm } from 'react-hook-form';
import { z } from 'zod';
import { Button, Modal } from '../ui';

const diskSchema = z.object({
	zpool: z.string().min(1, 'Zpool is required'),
	name: z.string().min(4, 'Disk name is required'),
	usage_type: z.string().min(1, 'Disk type is required'),
	size: z.string().optional(),
});

const DiskFormModal = ({ zpools, isOpen, setIsOpen, refresh }) => {
	const { showNotification } = useNotification();

	const {
		register,
		handleSubmit,
		formState: { errors, isSubmitting },
		setValue,
		reset,
		watch,
	} = useForm({
		resolver: zodResolver(diskSchema),
		defaultValues: { zpool: '', name: '', usage_type: 'image', size: '' },

	});

	const onSubmit = async (data) => {
		setIsOpen(false);
		showNotification(`Adding new disk ${data.name}`, 'info');
		const token = localStorage.getItem('authToken') || '';
		await invoke('create_zfs_dataset', { zpool: data.zpool, name: data.name, usageType: data.usage_type })
			.then((response) => {
				if (response.message) showNotification(response.message, 'success');
				reset({ zpool: data.zpool, name: '', usage_type: 'image', size: '' });

			})
			.catch((error) => {
				showNotification(error, 'error');
			})
			.finally(() => {
				refresh();
			});
	};

	const usageType = watch('usage_type');

	return (
		<Modal isOpen={isOpen} onClose={() => setIsOpen(false)} title={'Add disk'}>
			<form onSubmit={handleSubmit(onSubmit)} className="space-y-2">
				<fieldset className={`fieldset`}>
					<legend htmlFor="zpool" className='fieldset-legend'>Select zpool</legend>
					<select
						{...register('zpool')}
						id="zpool"
						className='select w-full'
						onChange={(e) => {
							console.log('DEBUG: zpool changed to:', e.target.value);
							setValue('zpool', e.target.value);
						}}
					>
						<option value="">Select zpool</option>
						{zpools.map(p => <option key={p} value={p}>{p}</option>)}
					</select>
					{errors.zpool && <div className="text-error text-xs">{errors.zpool.message}</div>}
				</fieldset>

				<fieldset className={`fieldset`}>
					<legend htmlFor="usage_type" className='fieldset-legend'>Disk type</legend>
					<select
						{...register('usage_type')}
						id="usage_type"
						className='select w-full'
						onChange={(e) => {
							console.log('DEBUG: usage_type changed to:', e.target.value);
							setValue('usage_type', e.target.value);
						}}
					>
						<option value="">Select disk type</option>
						<option value="image">Image (store images)</option>
						<option value="writeback">Writeback (store clones)</option>
						<option value="game">Game (Game Disks - creates zvol)</option>
					</select>
					{errors.usage_type && <div className="text-error text-xs">{errors.usage_type.message}</div>}
				</fieldset>

				<fieldset className={`fieldset`}>
					<legend htmlFor='name' className='fieldset-legend'>Disk Name</legend>
					<input {...register('name')} type='text' id='name' placeholder="Enter disk name" className='input w-full' />
					{errors.name && <div className="text-error text-xs">{errors.name.message}</div>}
				</fieldset>

				{usageType === 'game' && (
					<fieldset className={`fieldset`}>
						<legend htmlFor='size' className='fieldset-legend'>Disk size</legend>
						<input {...register('size')} type='text' id='size' placeholder="e.g. 50G" className='input w-full' />
						{errors.size && <div className="text-error text-xs">{errors.size.message}</div>}
					</fieldset>
				)}

				<div className="flex justify-end space-x-3">
					<Button type="submit" variant="primary">
						{isSubmitting ? (usageType === 'game' ? 'Creating game disk' : 'Creating disk...') : (usageType === 'game' ? 'Create game disk' : 'Create disk')}
					</Button>
					<Button type="button" onClick={() => { reset(); setIsOpen(false) }} variant="destructive">Cancel</Button>
				</div>
			</form>
		</Modal>
	);
};

export default DiskFormModal;