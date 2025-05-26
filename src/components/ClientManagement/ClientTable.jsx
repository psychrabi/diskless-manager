import { useAppStore } from '../../store/useAppStore';
import { Layers, Monitor, Power, PowerOff, Zap } from 'lucide-react';

const Table = ({ children, className = '' }) => <div className={`w-full overflow-x-auto ${className}`}><table className="min-w-full caption-bottom text-sm">{children}</table></div>;
const TableHeader = ({ children, className = '' }) => <thead className={`[&_tr]:border-b border-gray-200 dark:border-gray-700 ${className}`}>{children}</thead>;
const TableBody = ({ children, className = '' }) => <tbody className={`[&_tr:last-child]:border-0 ${className}`}>{children}</tbody>;
const TableRow = ({ children, className = '', onContextMenu }) => <tr onContextMenu={onContextMenu} className={`border-b border-gray-200 dark:border-gray-700 transition-colors hover:bg-gray-100/50 dark:hover:bg-gray-800/50 ${className}`}>{children}</tr>;
const TableHead = ({ children, className = '' }) => <th className={`h-12 px-4 text-left align-middle font-medium text-gray-500 dark:text-gray-400 ${className}`}>{children}</th>;
const TableCell = ({ children, className = '' }) => <td className={`p-4 align-middle ${className}`}>{children}</td>;

const ClientTable = ({ handleClientContextMenu }) => {
  const {clients} = useAppStore()

  return (
  <>
    <Table className='border border-gray-200 dark:border-gray-700'>
      <TableHeader>
        <TableRow>
          <TableHead>Name</TableHead>
          <TableHead className="hidden md:table-cell">MAC Address</TableHead>
          <TableHead>IP Address</TableHead>
          <TableHead className="hidden md:table-cell">Master</TableHead>
          <TableHead className="hidden xl:table-cell">Snapshot</TableHead>
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
              <span className={`inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium ${client.status === 'Online' ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-200' : 'bg-gray-100 text-gray-800 dark:bg-gray-700 dark:text-gray-300'}`}>
                {client.status === 'Online' ? <Power className="h-3 w-3 mr-1 text-green-500" /> : <PowerOff className="h-3 w-3 mr-1 text-gray-500" />}
                {client.status}
              </span>
            </TableCell>
            <TableCell>
              {!client.snapshot && (
                <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200" title="Changes persist directly to the clone">
                  <Zap className="h-3 w-3 mr-1 text-yellow-500" /> Master
                </span>
              )}
              {client.snapshot && (
                <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-yellow-100 text-yellow-800 dark:bg-yellow-900 dark:text-yellow-200" title="Changes persist directly to the clone">
                  <Layers className="h-3 w-3 mr-1" />Clone
                </span>
              )}
            </TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
    {clients.length === 0 && <p className="text-center py-4 text-gray-500">No clients configured.</p>}
  </>
)};

export default ClientTable;