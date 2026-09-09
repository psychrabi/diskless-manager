import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { formatDiskBytes } from "@/utils/formatDiskBytes";
import { Gamepad2 } from "lucide-react";
import { Link } from "react-router-dom";

const masterBasename = (dataset) => dataset.split("/").pop();

const ClientGameDiskPicker = ({
  masters,
  loading,
  selected,
  onToggle,
  onNavigateAway,
}) => {
  if (loading) {
    return (
      <div className="flex items-center gap-2 rounded-lg border border-border bg-muted/30 p-3 text-sm text-muted-foreground">
        <Spinner data-icon="inline-start" />
        Loading game disks...
      </div>
    );
  }

  if (masters.length === 0) {
    return (
      <div className="rounded-lg border border-border bg-muted/30 p-3 text-sm text-muted-foreground">
        No game disks yet.{" "}
        <Link
          to="/disks"
          onClick={onNavigateAway}
          className="font-medium text-primary underline underline-offset-4 hover:text-primary/80"
        >
          Create one in Disks
        </Link>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-muted/30 p-3">
      <div className="flex items-center gap-2 text-sm font-medium">
        <Gamepad2 className="size-4 text-muted-foreground" />
        Game disks for this client
      </div>
      <p className="text-xs text-muted-foreground">
        Each selected disk gets a private writable clone for this client. The
        clone resets with the boot image unless the client is persistent.
      </p>
      {masters.map((disk) => {
        const checked = selected.includes(disk.dataset);
        const id = `game-disk-${disk.dataset}`;
        return (
          <div key={disk.dataset} className="flex items-start gap-3">
            <Checkbox
              id={id}
              checked={checked}
              onCheckedChange={() => onToggle(disk.dataset)}
            />
            <Label
              htmlFor={id}
              className="flex cursor-pointer flex-col items-start gap-0.5"
            >
              <span className="font-mono text-sm font-medium">
                {masterBasename(disk.dataset)}
              </span>
              <span className="text-xs text-muted-foreground">
                {formatDiskBytes(disk.size_bytes)}
                {disk.used_by?.length > 0 &&
                  ` · used by ${disk.used_by.length} client${disk.used_by.length === 1 ? "" : "s"}`}
              </span>
            </Label>
          </div>
        );
      })}
    </div>
  );
};

export default ClientGameDiskPicker;
