import { useConfirm } from '@/contexts/confirmDialog';
import { useNotification } from '@/contexts/notification';
import { useAppStore } from '@/store/useAppStore';
import { invoke } from '@tauri-apps/api/core';
import { useState } from 'react';
import RenameDiskModal from './RenameDiskModal';

const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full">{children}</table></div>;
const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-base-100 ${className}`}>{children}</thead>;
const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-base-300 transition-colors hover:bg-base-300 ${className}`}>{children}</tr>;
const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 align-middle font-bold text-base-content/60 ${className} `}>{children}</th>;
const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className} text-center`}>{children}</td>;

const DiskTable = ({ disks }) => {
	const { showNotification } = useNotification();
	const confirm = useConfirm();
	const [openRenameModal, setOpenRenameModal] = useState(false)
	const [selectedDisk, setSelectedDisk] = useState('')
	const { fetchData } = useAppStore();

	const handleRenameDisk = (disk) => {
		setSelectedDisk(disk)
		setOpenRenameModal(true)
	}

	const handleDeleteDisk = async (dataset) => {
		const ok = await confirm({
			title: 'Delete disk',
			description: `Are you sure you want to delete disk "${dataset.name}"? This action cannot be undone and might affect clones.`,
			confirmText: 'Delete disk',
			cancelText: 'Cancel',
			confirmVariant: 'primary',
			size: '2xl',
		});
		if (ok) {
			const token = localStorage.getItem('authToken') || '';
			await invoke('delete_zfs_dataset', { dataset: dataset.name, recursive: true })
				.then((response) => {
					if (response.message) showNotification(response.message, 'success');
				}).catch((error) => {
					showNotification(error.error, 'error',)
				})
		} else {
			showNotification("Disk deletion cancelled", 'error',)
		}
	}

	return (
		<>
			<Table className='bg-base-100 rounded-lg'>
				<TableHeader>
					<TableRow>
						<TableHead className='text-start'>Name</TableHead>
						<TableHead>Type</TableHead>
						<TableHead>Actions</TableHead>
					</TableRow>
				</TableHeader>
				<TableBody>
					{disks.map(ds => (
						<TableRow key={ds.name}>
							<TableCell className='text-start'>{ds.name}</TableCell>
							<TableCell>{ds.disk_type || (ds.type === 'volume' ? 'zvol' : '—')}</TableCell>
							<TableCell className="flex gap-2 justify-center">
								<button className="btn btn-sm" onClick={() => handleRenameDisk(ds)}>Rename</button>
								<button className="btn btn-sm btn-error" onClick={() => handleDeleteDisk(ds)}>Remove</button>
							</TableCell>
						</TableRow>
					))}
				</TableBody>
			</Table>
			{disks.length === 0 && <p className="text-center py-4 text-base-content/60">No disks added.</p>}
			{openRenameModal && <RenameDiskModal openRenameModal={openRenameModal} setOpenRenameModal={setOpenRenameModal} selectedDisk={selectedDisk} refresh={fetchData} />}
		</>
	)
};

export default DiskTable;