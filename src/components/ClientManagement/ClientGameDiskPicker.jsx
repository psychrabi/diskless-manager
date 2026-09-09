import { Checkbox } from "@/components/ui/checkbox";
import { Spinner } from "@/components/ui/spinner";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Field, FieldContent, FieldDescription, FieldLabel, FieldTitle } from "@/components/ui/field";
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
      <Alert>
        <Spinner />
        <AlertDescription>Loading game disks...</AlertDescription>
      </Alert>
    );
  }

  if (masters.length === 0) {
    return (
      <Alert>
        <AlertDescription>
          No game disks yet.{" "}
          <Link to="/disks" onClick={onNavigateAway}>
            Create one in Disks
          </Link>
        </AlertDescription>
      </Alert>
    );
  }

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border bg-muted/30 p-3">
      <FieldTitle>
        <Gamepad2 className="size-4 text-muted-foreground" />
        Game disks for this client
      </FieldTitle>
      <FieldDescription className="text-xs">
        Each selected disk gets a private writable clone for this client. The
        clone resets with the boot image unless the client is persistent.
      </FieldDescription>
      {masters.map((disk) => {
        const checked = selected.includes(disk.dataset);
        const id = `game-disk-${disk.dataset}`;
        return (
          <Field key={disk.dataset} orientation="horizontal">
            <Checkbox
              id={id}
              checked={checked}
              onCheckedChange={() => onToggle(disk.dataset)}
            />
            <FieldContent>
              <FieldLabel htmlFor={id} className="cursor-pointer ">
                {masterBasename(disk.dataset)}
              </FieldLabel>
              <FieldDescription className="text-xs">
                {formatDiskBytes(disk.size_bytes)}
                {disk.used_by?.length > 0 &&
                  ` · used by ${disk.used_by.length} client${disk.used_by.length === 1 ? "" : "s"}`}
              </FieldDescription>
            </FieldContent>
          </Field>
        );
      })}
    </div>
  );
};

export default ClientGameDiskPicker;
