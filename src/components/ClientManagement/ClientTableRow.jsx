import { memo } from "react";
import { TableCell } from "@/components/ui";
import { formatUptime } from "@/utils/formatUptime";
import { Clock, Monitor, MoveDown, MoveUp } from "lucide-react";
import ControlActionButtons from "./ControlActionButtons";
import { formatDiskBytes } from "@/utils/formatDiskBytes";

const ClientStatusBadge = memo(({ status }) => {
  const currentStatus = status || "Offline";
  const isOnline = currentStatus === "Online";
  const isLeased = currentStatus === "Leased";
  const badgeClass = isOnline
    ? "text-success"
    : isLeased
      ? "text-warning"
      : "text-neutral";

  return <Monitor className={`inline mr-2 h-4 w-4 ${badgeClass}`} />;
});

const ClientModeBadge = memo(({ client }) => {
  const isUsingMasterDirectly = !client.snapshot;

  if (isUsingMasterDirectly) {
    return (
      <span
        className="status status-warning status-lg"
        title={`Super Client : ${client.master}`}
      />
    );
  }

  if (client.keep_writeback === false) {
    return (
      <span
        className="status status-secondary status-lg"
        title="Non-Persistent"
      />
    );
  }

  return (
    <span
      className="status status-info status-lg"
      title={`Persistent: ${client.block_store}`}
    />
  );
});

const SpeedCell = ({ metricValue, icon: Icon, iconClassName }) => {
  if (metricValue == null) {
    return <span className="text-base-content/40">-</span>;
  }

  return (
    <span className="flex items-center justify-center gap-1">
      <Icon className={`w-3 h-3 ${iconClassName}`} />
      {metricValue.toFixed(2)}
    </span>
  );
};

const UptimeCell = ({ uptimeSeconds }) => {
  if (uptimeSeconds == null) {
    return <span className="text-base-content/40">-</span>;
  }

  return (
    <span className="flex items-center gap-1">
      <Clock className="w-3 h-3 text-secondary" />
      {formatUptime(uptimeSeconds)}
    </span>
  );
};

const ClientTableRow = ({ client, clientMetrics }) => {
  return (
    <>
      <TableCell className="font-bold font-mono">
        <ClientStatusBadge status={client.status} />
        {client.name}
      </TableCell>
      <TableCell className="hidden md:table-cell text-xs font-mono">
        {client.mac}
      </TableCell>
      <TableCell className="font-mono text-xs text-center">{client.ip}</TableCell>
      <TableCell className="hidden lg:table-cell font-mono">
        <SpeedCell
          metricValue={clientMetrics?.iscsi?.read_speed_mbps}
          icon={MoveUp}
          iconClassName="text-primary"
        />
      </TableCell>
      <TableCell className="hidden lg:table-cell font-mono text-center" title="Total since the disk counters last restarted">
        {formatDiskBytes(clientMetrics?.iscsi?.total_read_bytes)}
      </TableCell>
      <TableCell className="hidden lg:table-cell font-mono">
        <SpeedCell
          metricValue={clientMetrics?.iscsi?.write_speed_mbps}
          icon={MoveDown}
          iconClassName="text-secondary"
        />
      </TableCell>
      <TableCell className="hidden lg:table-cell font-mono text-center" title="Total since the disk counters last restarted">
        {formatDiskBytes(clientMetrics?.iscsi?.total_write_bytes)}
      </TableCell>
      <TableCell className="hidden xl:table-cell text-xs font-mono break-all text-center">
        {client.master}
      </TableCell>
      <TableCell className="text-center">
        <ClientModeBadge client={client} />
      </TableCell>
      <TableCell className="hidden lg:table-cell text-xs font-mono text-center">
        <UptimeCell uptimeSeconds={clientMetrics?.uptime_seconds} />
      </TableCell>
      <TableCell >
        <ControlActionButtons client={client} onActionComplete={() => {}} />
      </TableCell>
    </>
  );
};

export default ClientTableRow;
