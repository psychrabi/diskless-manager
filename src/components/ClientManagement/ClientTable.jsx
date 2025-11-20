import { Layers, Monitor, Power, PowerOff, Zap } from 'lucide-react';
import { useAppStore } from '../../store/useAppStore';

import React from 'react';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '../ui';

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
  return client.super ? (
    <span className="badge badge-warning gap-1" title="Using the image directly">
      <Zap className="h-3 w-3" /> Super Client
    </span>
  ) : (
    <span className="badge badge-info gap-1" title="Client changes will be kept in the clone">
      <Layers className="h-3 w-3" />Writeback
    </span>
  )
});

const ClientRow = React.memo(({ client, handleClientContextMenu }) => (
  <TableRow key={client.id} onContextMenu={(e) => handleClientContextMenu(e, client)} className="cursor-context-menu">
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
  </TableRow>
));

const ClientTable = ({ handleClientContextMenu }) => {
  const clients = useAppStore((state) => state.clients)

  return (
    <>
      <Table className='bg-base-100 rounded-lg'>
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead className="hidden md:table-cell">MAC Address</TableHead>
            <TableHead>IP Address</TableHead>
            <TableHead className="hidden md:table-cell">Image</TableHead>
            <TableHead className="hidden xl:table-cell">Restore Point</TableHead>
            <TableHead className="hidden xl:table-cell">Boot disk</TableHead>
            <TableHead className='text-center'>Status</TableHead>
            <TableHead>Mode</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {clients.map((client) => (
            <ClientRow key={client.id} client={client} handleClientContextMenu={handleClientContextMenu} />
          ))}
        </TableBody>
      </Table>
      {clients.length === 0 && <p className="text-center py-4 text-base-content/60">No clients configured.</p>}
    </>
  )
};

export default ClientTable;