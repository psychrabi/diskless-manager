import { memo } from "react";
import { TableCell } from "@/components/ui/table";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { formatUptime } from "@/utils/formatUptime";
import { Clock, Gamepad2, Monitor, MoveDown, MoveUp, MoreHorizontal } from "lucide-react";
import ControlActionButtons from "./ControlActionButtons";
import { formatDiskBytes } from "@/utils/formatDiskBytes";

const ClientStatusIcon = memo(({ status }) => {
  const currentStatus = status || "Offline";
  const isOnline = currentStatus === "Online";
  const isLeased = currentStatus === "Leased";
  const color = isOnline
    ? "text-emerald-500"
    : isLeased
      ? "text-amber-500"
      : "text-destructive";

  return (
    <span
      role="img"
      aria-label={`Status: ${currentStatus}`}
      title={currentStatus}
      className="flex shrink-0"
    >
      <Monitor className={`size-4 ${color}`} />
    </span>
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
    <Badge variant="secondary" title={title}>
      <Gamepad2 data-icon="inline-start" />
      {selected.length > 0 ? selected.length : "All"}
    </Badge>
  );
});

const RateLine = ({ metricValue, icon: Icon, iconClassName, label }) => (
  <span className="flex items-center gap-1" title={label}>
    <Icon className={`size-3 shrink-0 ${iconClassName}`} />
    {metricValue == null ? (
      <span className="text-muted-foreground/60">-</span>
    ) : (
      metricValue.toFixed(2)
    )}
  </span>
);

const TotalLine = ({ byteValue, icon: Icon, iconClassName, label }) => (
  <span className="flex items-center gap-1" title={`${label} since the disk counters last restarted`}>
    <Icon className={`size-3 shrink-0 ${iconClassName}`} />
    {formatDiskBytes(byteValue)}
  </span>
);

const UptimeCell = ({ uptimeSeconds }) => {
  if (uptimeSeconds == null) {
    return <span className="text-muted-foreground/60">-</span>;
  }

  return (
    <span className="flex items-center gap-1">
      <Clock className="size-3 text-muted-foreground" />
      {formatUptime(uptimeSeconds)}
    </span>
  );
};

const Detail = ({ label, children }) => (
  <div className="min-w-0">
    <dt className="text-xs text-muted-foreground">{label}</dt>
    <dd className="mt-0.5 truncate text-sm">{children}</dd>
  </div>
);

const ClientTableRow = ({ client, clientMetrics, onMoreActions }) => {
  return (
    <>
      <TableCell colSpan={9} className="p-3 xl:hidden">
        <article data-testid="client-mobile-card" className="rounded-lg border bg-background p-3 text-left">
          <div className="flex items-start justify-between gap-3">
            <div className="flex min-w-0 items-center gap-2">
              <ClientStatusIcon status={client.status} />
              <span className="truncate font-semibold">{client.name}</span>
            </div>
            <div className="flex shrink-0 flex-col items-end gap-1">
              <ClientModeBadge client={client} />
              <ClientGameBadge client={client} />
            </div>
          </div>
          <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-3">
            <Detail label="MAC address">{client.mac}</Detail>
            <Detail label="IP address">{client.ip}</Detail>
            <Detail label="Image">{client.master}</Detail>
            <Detail label="Uptime"><UptimeCell uptimeSeconds={clientMetrics?.uptime_seconds} /></Detail>
            <Detail label="Read / write (MB/s)">
              <span className="tabular-nums">{clientMetrics?.iscsi?.read_speed_mbps?.toFixed(2) ?? "-"} / {clientMetrics?.iscsi?.write_speed_mbps?.toFixed(2) ?? "-"}</span>
            </Detail>
            <Detail label="Total read / written">
              {formatDiskBytes(clientMetrics?.iscsi?.total_read_bytes)} / {formatDiskBytes(clientMetrics?.iscsi?.total_write_bytes)}
            </Detail>
          </dl>
          <div className="mt-3 flex flex-wrap items-center justify-between gap-2 border-t pt-2">
            <Button variant="outline" size="sm" onClick={(event) => onMoreActions?.(event, client)}>
              <MoreHorizontal data-icon="inline-start" />More actions
            </Button>
            <ControlActionButtons client={client} onActionComplete={() => {}} />
          </div>
        </article>
      </TableCell>
      <TableCell className="hidden min-w-0 max-w-56 xl:table-cell">
        <div className="flex min-w-0 items-center gap-2">
          <ClientStatusIcon status={client.status} />
          <span className="truncate  font-bold mt-0.5">{client.name}</span>
        </div>
      </TableCell>
      <TableCell className="hidden text-center text-sm xl:table-cell">
        {client.mac}
      </TableCell>
      <TableCell className="hidden text-center text-sm xl:table-cell">
        {client.ip}</TableCell>
      <TableCell className="hidden text-center text-xs xl:table-cell">
        <div className="flex gap-1 justify-evenly">
          <RateLine
            metricValue={clientMetrics?.iscsi?.read_speed_mbps}
            icon={MoveUp}
            iconClassName="text-primary"
            label="Read speed (MB/s)"
          />
          <RateLine
            metricValue={clientMetrics?.iscsi?.write_speed_mbps}
            icon={MoveDown}
            iconClassName="text-muted-foreground"
            label="Write speed (MB/s)"
          />
        </div>
      </TableCell>
      <TableCell className="hidden text-center text-xs xl:table-cell">
        <div className="flex gap-1 justify-evenly">
          <TotalLine icon={MoveUp}
          iconClassName="text-primary" byteValue={clientMetrics?.iscsi?.total_read_bytes} label="Total read" />
          <TotalLine icon={MoveDown}
          iconClassName="text-primary" byteValue={clientMetrics?.iscsi?.total_write_bytes} label="Total written" />
        </div>
      </TableCell>
      <TableCell className="hidden max-w-36 xl:table-cell">
        <span className="block truncate text-center  text-xs">{client.master}</span>
      </TableCell>
      <TableCell className="hidden text-center xl:table-cell">
        <div className="flex flex-col items-center gap-1">
          <ClientModeBadge client={client} />
          <ClientGameBadge client={client} />
        </div>
      </TableCell>
      <TableCell className="hidden text-center text-xs xl:table-cell">
        <div className="flex justify-center">
          <UptimeCell uptimeSeconds={clientMetrics?.uptime_seconds} />
        </div>
      </TableCell>
      <TableCell className="hidden xl:table-cell">
        <ControlActionButtons client={client} onActionComplete={() => {}} />
      </TableCell>
    </>
  );
};

export default ClientTableRow;
