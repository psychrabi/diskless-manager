import React from 'react';
import { Button, Modal } from '../ui';
import { invoke } from '@tauri-apps/api/core';
import { useNotification } from '@/contexts/NotificationContext';

const ClientDeleteConfirmModal = ({ openDeleteClientModal, setOpenDeleteClientModal, selectedClient }) => {
	const { showNotification } = useNotification();

	const confirmDeleteClient = async () => {
		if (!selectedClient) return;
		setOpenDeleteClientModal(false);
		showNotification(`Deleting ${selectedClient.name} from clients`, 'info')
		// Get token from localStorage
		const token = localStorage.getItem('authToken') || '';
		await invoke('delete_client', { token, clientId: selectedClient.id })
			.then((response) => {
				if (response.message) showNotification(response.message, 'success');
			}).catch((error) => showNotification(error, 'error'))
			.finally(() => {
				closeContextMenu();
				// Refresh the data after deletion
				fetchData();
			});
	};

	return (
		<Modal isOpen={openDeleteClientModal} onClose={() => setOpenDeleteClientModal(false)} title="Delete Client" size="2xl">
			<div className="space-y-4">
				<p>
					Are you sure you want to delete "{selectedClient.name}" image? <br />
					This action cannot be undone and might affect clones.
				</p>
				<div className="flex justify-end space-x-3">
					<Button variant="primary" onClick={() => confirmDeleteClient()} >
						Delete Client
					</Button>
					<Button variant="destructive" onClick={() => setOpenDeleteClientModal(false)} >
						Cancel
					</Button>
				</div>
			</div>
		</Modal>
	);
};

export default ClientDeleteConfirmModal;