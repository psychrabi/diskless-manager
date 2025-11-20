import { useConfirm } from '@/contexts/confirmDialog';
import { useZfs } from '@/hooks/useZfs';
import { useState } from 'react';
import { Button, Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui';
import RenameDiskModal from './RenameDiskModal';

const DiskTable = ({ datasets, onRefresh }) => {
	const [selectedDisk, setSelectedDisk] = useState(null);
	const [isRenameModalOpen, setIsRenameModalOpen] = useState(false);
	const confirm = useConfirm();
	const { deleteDataset } = useZfs();

	const handleRenameDisk = (disk) => {
		setSelectedDisk(disk);
		setIsRenameModalOpen(true);
	};

	const handleDeleteDisk = async (disk) => {
		const ok = await confirm({
			title: 'Delete disk',
			description: `Are you sure you want to delete disk "${disk.name}"? This action cannot be undone and might affect clones.`,
			confirmText: 'Delete disk',
			cancelText: 'Cancel',
			confirmVariant: 'primary',
			size: '2xl',
		});
		if (ok) {
			const success = await deleteDataset(disk.name);
			if (success) {
				onRefresh();
			}
		}
	};

	return (
		<>
			<div className="overflow-x-auto">
				<Table className='bg-base-100 rounded-lg'>
					<TableHeader>
						<TableRow>
							<TableHead className='text-start'>Name</TableHead>
							<TableHead>Type</TableHead>
							<TableHead>Actions</TableHead>
						</TableRow>
					</TableHeader>
					<TableBody>
						{datasets.map((ds) => (
							<TableRow key={ds.name}>
								<TableCell className='text-start'>{ds.name}</TableCell>
								<TableCell>{ds.disk_type || (ds.type === 'volume' ? 'zvol' : '—')}</TableCell>
								<TableCell className="flex gap-2 justify-center">
									<Button size="sm" onClick={() => handleRenameDisk(ds)}>Rename</Button>
									<Button size="sm" variant="destructive" onClick={() => handleDeleteDisk(ds)}>Remove</Button>
								</TableCell>
							</TableRow>
						))}
						{datasets.length === 0 && (
							<TableRow>
								<TableCell colSpan={3} className="text-center py-4 text-base-content/70">
									No disks found
								</TableCell>
							</TableRow>
						)}
					</TableBody>
				</Table>
			</div>
			{isRenameModalOpen && <RenameDiskModal openRenameModal={isRenameModalOpen} setOpenRenameModal={setIsRenameModalOpen} selectedDisk={selectedDisk} refresh={onRefresh} />}
		</>
	);
};

export default DiskTable;