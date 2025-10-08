import { PlusCircle, Users } from 'lucide-react';
import { memo, useCallback, useEffect, useState } from 'react';
import { useAppStore } from '../../store/useAppStore';
import { useClientContextMenuActions } from '../../utils/contextMenuAction';
import { Button, Card } from '../ui';
import { ContextMenu } from '../ui/ContextMenu';
import ClientFormModal from './ClientFormModal';
import ClientTable from './ClientTable';
import DeprovisionModal from './DeprovisionModal';

const MemoizedClientTable = memo(ClientTable);
const MemoizedContextMenu = memo(ContextMenu);

const ClientManagement = () => {
  const { clients, fetchData, masters, startClientStatusPolling, stopClientStatusPolling } = useAppStore();
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
  const [deprovisionModal, setDeprovisionModal] = useState({ isOpen: false, client: null });
  const handleClientContextMenu = useCallback((event, client) => {
    event.preventDefault();
    setContextMenu({ isOpen: true, x: event.clientX, y: event.clientY, client: client });
  }, []);

  const closeContextMenu = useCallback(() => {
    setContextMenu(prev => ({ ...prev, isOpen: false }));
  }, []);

  const contextActions = useClientContextMenuActions(fetchData, closeContextMenu, setClient, setIsModalOpen, setDeprovisionModal);

  const handleClientFormModalOpen = useCallback(() => {
    let newName = 'pc000'
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

  useEffect(() => {
    // Start polling when this page mounts; stop when it unmounts
    startClientStatusPolling();
    return () => stopClientStatusPolling();
  }, [startClientStatusPolling, stopClientStatusPolling]);


  return (
    <Card title="Client Management" icon={Users} className="bg-base-300" actions={
      <Button variant="primary" onClick={handleClientFormModalOpen} icon={PlusCircle} disabled={masters.length === 0}>
        Add Client {masters.length === 0 && <span className="text-xs text-error ml-2 self-center">(Requires Master Image)</span>}
      </Button>
    } >
      <div className="min-h-[calc(100vh-15rem)]">
        <MemoizedClientTable handleClientContextMenu={handleClientContextMenu} />
        <MemoizedContextMenu isOpen={contextMenu.isOpen} xPos={contextMenu.x} yPos={contextMenu.y} targetClient={contextMenu.client} onClose={closeContextMenu} actions={contextActions} />
      </div>
      <ClientFormModal client={client} setClient={setClient} masters={masters} isOpen={isModalOpen} setIsOpen={setIsModalOpen} refresh={fetchData} />
      {deprovisionModal &&
        <DeprovisionModal
          isOpen={deprovisionModal.isOpen}
          onClose={() => setDeprovisionModal({ isOpen: false, client: null })}
          client={deprovisionModal.client}
          onSuccess={() => {
            fetchData();
            setDeprovisionModal({ isOpen: false, client: null });
          }}
        />
      }
    </Card>
  );
};

export default ClientManagement;
