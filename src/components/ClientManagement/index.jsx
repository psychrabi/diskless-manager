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
    let newName = 'pc002'
    let newIp = '192.168.1.101'
    if (clients.length > 0) {
      const lastClient = clients[clients.length - 1]
      // Increment name (e.g., pc002 -> pc003)
      const nameMatch = lastClient.name.match(/(.*?)(\d+)$/)
      if (nameMatch) {
        const prefix = nameMatch[1]
        const num = parseInt(nameMatch[2], 10) + 1
        newName = `${prefix}${num.toString().padStart(nameMatch[2].length, '0')}`
      }
      // Increment IP last octet
      const ipParts = lastClient.ip.split('.')
      if (ipParts.length === 4) {
        const lastOctet = parseInt(ipParts[3], 10) + 1
        ipParts[3] = lastOctet.toString()
        newIp = ipParts.join('.')
      }
    }
    setClient({
      name: newName,
      mac: '',
      ip: newIp,
      master: masters[0]?.name || '',
      snapshot: '',
      clone: ''
    })
    setIsModalOpen(true)
  }, [clients, masters])

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
