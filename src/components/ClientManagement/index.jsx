import { PlusCircle, Users } from 'lucide-react';
import { useCallback, useState, memo } from 'react';
import { Button, Card } from '../ui';
import { clientContextMenuActions } from '../../utils/contextMenuAction';
import ClientFormModal from './ClientFormModal';
import ClientTable from './ClientTable';
import {ContextMenu} from '../ui/ContextMenu';
import { useAppStore } from '../../store/useAppStore';

const MemoizedClientTable = memo(ClientTable);
const MemoizedContextMenu = memo(ContextMenu);

export const ClientManagement = () => {
  const { clients, masters, fetchData } = useAppStore();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [client, setClient] = useState({
    name: '',
    mac: '',
    ip: '',
    master: '',
    snapshot: '',
    clone: ''
  });
  const [contextMenu, setContextMenu] = useState({ isOpen: false, x: 0, y: 0, client: null });

  const handleClientContextMenu = useCallback((event, client) => {
    event.preventDefault();
    setContextMenu({ isOpen: true, x: event.clientX, y: event.clientY, client: client });
  }, []);

  const closeContextMenu = useCallback(() => {
    setContextMenu(prev => ({ ...prev, isOpen: false }));
  }, []);

  const contextActions = clientContextMenuActions(fetchData, closeContextMenu, setClient, setIsModalOpen);

  const handleClientFormModalOpen = useCallback(() => {
    setClient({
      name: 'pc002',
      mac: 'd8:43:ae:a7:8e:a8',
      ip: '192.168.1.101',
      master: 'nsboot0/Windows11-24h2-master',
      snapshot: ''
    });
    setIsModalOpen(true);
  }, []);

  return (
    <div className="mb-2 md:mb-4">
      <Card title="Client Management" icon={Users} actions={
        <Button onClick={handleClientFormModalOpen} icon={PlusCircle} disabled={masters.length === 0}>
          Add Client {masters.length === 0 && <span className="text-xs text-red-500 ml-2 self-center">(Requires Master Image)</span>}
        </Button>
      }>
        <MemoizedClientTable clients={clients} handleClientContextMenu={handleClientContextMenu} />
        <MemoizedContextMenu isOpen={contextMenu.isOpen} xPos={contextMenu.x} yPos={contextMenu.y} targetClient={contextMenu.client} onClose={closeContextMenu} actions={contextActions} />
      </Card>
      <ClientFormModal client={client} setClient={setClient} masters={masters} isOpen={isModalOpen} setIsOpen={setIsModalOpen} refresh={fetchData} />
    </div>
  );
};

export default ClientManagement;
