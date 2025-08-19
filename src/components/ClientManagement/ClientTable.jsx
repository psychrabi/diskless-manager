import { Layers, Monitor, Power, PowerOff, Zap } from 'lucide-react';
import { useAppStore } from '../../store/useAppStore';

const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full caption-bottom text-sm">{children}</table></div>;
const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-base-300 ${className}`}>{children}</thead>;
const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-base-300 transition-colors hover:bg-base-200 ${className}`}>{children}</tr>;
const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 text-left align-middle font-medium text-base-content/60 ${className}`}>{children}</th>;
const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className}`}>{children}</td>;

const ClientTable = ({ handleClientContextMenu }) => {
  const { clients } = useAppStore()

  return (
  <>
    <Table className='border border-base-300'>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead className="hidden md:table-cell">MAC Address</TableHead>
          <TableHead>IP Address</TableHead>
          <TableHead className="hidden md:table-cell">Image</TableHead>
          <TableHead className="hidden xl:table-cell">Restore Point</TableHead>
          <TableHead className="hidden xl:table-cell">Writeback</TableHead>
          <TableHead>Status</TableHead>
          <TableHead>Mode</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {clients.map((client) => (
          <TableRow key={client.id} onContextMenu={(e) => handleClientContextMenu(e, client)} className="cursor-context-menu">
            <TableCell className="font-medium">
              <Monitor className="inline mr-2 h-4 w-4" />
              {client.name}
            </TableCell>
            <TableCell className="hidden md:table-cell text-xs font-mono">{client.mac}</TableCell>
            <TableCell>{client.ip}</TableCell>
            <TableCell className="hidden md:table-cell text-xs font-mono break-all">{client.master}</TableCell>
            <TableCell className="hidden xl:table-cell text-xs font-mono break-all">{client.snapshot}</TableCell>
            <TableCell className="hidden xl:table-cell text-xs font-mono break-all">{client.block_device}</TableCell>
            <TableCell>
              {(() => {
                const status = client.status || 'Offline';
                const isOnline = status === 'Online';
                const isLeased = status === 'Leased';
                const badgeClass = isOnline
                  ? 'badge-success'
                  : isLeased
                  ? 'badge-warning'
                  : 'badge-neutral';
                const Icon = isOnline ? Power : PowerOff;
                return (
                  <span className={`badge ${badgeClass} gap-1`}>
                    <Icon className={`h-3 w-3`} />
                    {status}
                  </span>
                );
              })()}
            </TableCell>
            <TableCell>
              {client.mode === 'super' || client.super_client ? (
                <span className="badge badge-error gap-1" title="Using the image directly">
                  <Zap className="h-3 w-3" /> Super Client
                </span>
              ) : client.keep_writeback ? (
                <span className="badge badge-success gap-1" title="Client changes will be kept in the clone">
                  <Layers className="h-3 w-3" />Keep Writeback
                </span>
              ) : (
              <span className="badge badge-info gap-1" title="Clone will be reset on reboot">
                <Layers className="h-3 w-3" />Reset on reboot
              </span>
              )}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
    {clients.length === 0 && <p className="text-center py-4 text-base-content/60">No clients configured.</p>}
  </>
)
};

export default ClientTable;