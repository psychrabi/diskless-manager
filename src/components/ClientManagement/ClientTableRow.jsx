import { memo } from "react";
import { TableCell } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { formatUptime } from "@/utils/formatUptime";
import { Clock, Gamepad2, Monitor, MoveDown, MoveUp } from "lucide-react";
import ControlActionButtons from "./ControlActionButtons";
import { formatDiskBytes } from "@/utils/formatDiskBytes";

const ClientStatusBadge = memo(({ status }) => {
  const currentStatus = status || "Offline";
  const isOnline = currentStatus === "Online";
  const isLeased = currentStatus === "Leased";
  const variant = isOnline ? "default" : isLeased ? "secondary" : "destructive";

  return (
    <Badge variant={variant} className="mr-2" title={currentStatus}>
      <Monitor data-icon="inline-start" />
      {currentStatus}
    </Badge>
  );
});

const ClientModeBadge = memo(({ client }) => {
  const isUsingMasterDirectly = !client.snapshot;

  if (isUsingMasterDirectly) {
    return (
      <Badge variant="outline" title={`Super Client : ${client.master}`}>
        Super
      </Badge>
    );
  }

  if (client.keep_writeback === false) {
    return (
      <Badge variant="secondary" title="Non-Persistent">
        Non-Persistent
      </Badge>
    );
  }

  return (
    <Badge variant="default" title={`Persistent: ${client.block_device || client.block_store}`}>
      Persistent
    </Badge>
  );
});

const ClientGameBadge = memo(({ client }) => {
  if (!client.use_game_disk) {
    return null;
  }
  const selected = client.game_disks ?? [];
  const title =
    selected.length > 0
      ? `Game disks: ${selected.map((disk) => disk.split("/").pop()).join(", ")}`
      : "Game disks: all available game disks";
  return (
    <Badge variant="secondary" className="ml-2" title={title}>
      <Gamepad2 data-icon="inline-start" />
      {selected.length > 0 ? selected.length : "All"}
    </Badge>
  );
});

const SpeedCell = ({ metricValue, icon: Icon, iconClassName }) => {
  if (metricValue == null) {
    return <span className="text-muted-foreground/60">-</span>;
  }

  return (
    <span className="flex items-center justify-center gap-1">
      <Icon className={`size-3 ${iconClassName}`} />
      {metricValue.toFixed(2)}
    </span>
  );
};

const UptimeCell = ({ uptimeSeconds }) => {
  if (uptimeSeconds == null) {
    return <span className="text-muted-foreground/60">-</span>;
  }

  return (
    <span className="flex items-center gap-1">
      <Clock className="size-3 text-secondary" />
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
      <TableCell className="hidden text-xs font-mono md:table-cell">
        {client.mac}
      </TableCell>
      <TableCell className="text-center font-mono text-xs">{client.ip}</TableCell>
      <TableCell className="hidden font-mono lg:table-cell">
        <SpeedCell
          metricValue={clientMetrics?.iscsi?.read_speed_mbps}
          icon={MoveUp}
          iconClassName="text-primary"
        />
      </TableCell>
      <TableCell
        className="hidden text-center font-mono lg:table-cell"
        title="Total since the disk counters last restarted"
      >
        {formatDiskBytes(clientMetrics?.iscsi?.total_read_bytes)}
      </TableCell>
      <TableCell className="hidden font-mono lg:table-cell">
        <SpeedCell
          metricValue={clientMetrics?.iscsi?.write_speed_mbps}
          icon={MoveDown}
          iconClassName="text-secondary"
        />
      </TableCell>
      <TableCell
        className="hidden text-center font-mono lg:table-cell"
        title="Total since the disk counters last restarted"
      >
        {formatDiskBytes(clientMetrics?.iscsi?.total_write_bytes)}
      </TableCell>
      <TableCell className="hidden break-all text-center font-mono text-xs xl:table-cell">
        {client.master}
      </TableCell>
      <TableCell className="text-center">
        <ClientModeBadge client={client} />
        <ClientGameBadge client={client} />
      </TableCell>
      <TableCell className="hidden text-center font-mono text-xs lg:table-cell">
        <UptimeCell uptimeSeconds={clientMetrics?.uptime_seconds} />
      </TableCell>
      <TableCell>
        <ControlActionButtons client={client} onActionComplete={() => {}} />
      </TableCell>
    </>
  );
};

export default ClientTableRow;
