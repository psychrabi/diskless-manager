import { Layers, Monitor, Power, PowerOff, Zap, RefreshCw } from 'lucide-react';
import { useAppStore } from '../../store/useAppStore';
import React from 'react';
import { TableVirtuoso } from 'react-virtuoso';
import { TableCell, TableHead } from '../ui';

const ClientStatusBadge = React.memo(({ status }) => {
  const currentStatus = status || 'Offline';
  const isOnline = currentStatus === 'Online';
  const isLeased = currentStatus === 'Leased';
  const badgeClass = isOnline
    ? 'badge-success'
    : isLeased
      ? 'badge-warning'
      : 'badge-neutral';
  const Icon = isOnline ? Power : PowerOff;
  return (
    <span className={`badge ${badgeClass} gap-1`}>
      <Icon className="h-3 w-3" />
      {currentStatus}
    </span>
  );
});

const ClientModeBadge = React.memo(({ client }) => {
  if (client.mode === 'super') {
    return (
      <span className="badge badge-warning gap-1" title="Using the image directly">
        <Zap className="h-3 w-3" /> Super Client
      </span>
    );
  }

  if (client.keep_writeback === false) {
    return (
      <span className="badge badge-secondary gap-1" title="Changes are lost on reset (Non-Persistent)">
        <RefreshCw className="h-3 w-3" /> Non-Persistent
      </span>
    );
  }

  return (
    <span className="badge badge-info gap-1" title="Client changes will be kept in the clone">
      <Layers className="h-3 w-3" /> Writeback
    </span>
  );
});

// Define Virtuoso components outside to prevent remounting
const VirtuosoTableComponents = {
  Table: ({ style, ...props }, ref) => (
    <table {...props} className="table w-full bg-base-100" style={{ ...style }} ref={ref} />
  ),
  TableBody: React.forwardRef((props, ref) => <tbody {...props} ref={ref} />),
  TableRow: React.forwardRef((props, ref) => {
    // Destructure to avoid passing non-DOM props to tr, but capture item and context
    // eslint-disable-next-line no-unused-vars
    const { item, itemProps, context, ...rest } = props;
    return (
      <tr
        {...rest}
        onContextMenu={(e) => {
          if (context?.handleClientContextMenu && item) {
            context.handleClientContextMenu(e, item);
          }
        }}
        className={`border-b border-base-200 transition-colors hover:bg-base-200/50 cursor-context-menu ${props.className || ''}`}
        ref={ref}
      />
    );
  }),
};

const ClientTable = ({ handleClientContextMenu }) => {
  const clients = useAppStore((state) => state.clients)

  return (
    <div className="bg-base-100 rounded-lg h-[calc(100vh-20rem)] w-full border border-base-200">
      {clients.length === 0 ? (
        <div className="p-4">
          <table className="table w-full">
            <thead>
              <tr className="border-b border-base-200">
                <TableHead>Name</TableHead>
                <TableHead className="hidden md:table-cell">MAC Address</TableHead>
                <TableHead>IP Address</TableHead>
                <TableHead className="hidden md:table-cell">Image</TableHead>
                <TableHead className="hidden xl:table-cell">Restore Point</TableHead>
                <TableHead className="hidden xl:table-cell">Boot disk</TableHead>
                <TableHead className='text-center'>Status</TableHead>
                <TableHead>Mode</TableHead>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td colSpan="8" className="text-center py-4 text-base-content/60">No clients configured.</td>
              </tr>
            </tbody>
          </table>
        </div>
      ) : (
        <TableVirtuoso
          data={clients}
          context={{ handleClientContextMenu }}
          components={VirtuosoTableComponents}
          fixedHeaderContent={() => (
            <tr className="bg-base-100 border-b border-base-200 shadow-sm z-10 w-full">
              <TableHead className="bg-base-100">Name</TableHead>
              <TableHead className="hidden md:table-cell bg-base-100">MAC Address</TableHead>
              <TableHead className="bg-base-100">IP Address</TableHead>
              <TableHead className="hidden md:table-cell bg-base-100">Image</TableHead>
              <TableHead className="hidden xl:table-cell bg-base-100">Restore Point</TableHead>
              <TableHead className="hidden xl:table-cell bg-base-100">Boot disk</TableHead>
              <TableHead className='text-center bg-base-100'>Status</TableHead>
              <TableHead className="bg-base-100">Mode</TableHead>
            </tr>
          )}
          itemContent={(index, client) => (
            <>
              <TableCell className="font-bold font-mono">
                <Monitor className="inline mr-2 h-4 w-4" />
                {client.name}
              </TableCell>
              <TableCell className="hidden md:table-cell text-xs font-mono">{client.mac}</TableCell>
              <TableCell className='font-mono text-xs'>{client.ip}</TableCell>
              <TableCell className="hidden md:table-cell text-xs font-mono break-all">{client.master}</TableCell>
              <TableCell className="hidden xl:table-cell text-xs font-mono break-all">{client.snapshot ?? "-"}</TableCell>
              <TableCell className="hidden xl:table-cell text-xs font-mono break-all">{client.block_device}</TableCell>
              <TableCell>
                <ClientStatusBadge status={client.status} />
              </TableCell>
              <TableCell>
                <ClientModeBadge client={client} />
              </TableCell>
            </>
          )}
          itemProps={(index) => ({
            'data-index': index,
          })}
        />
      )}
    </div>
  )
};

export default ClientTable;