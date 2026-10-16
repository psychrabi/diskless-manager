import { useState } from "react";
import { Button } from "@/components/ui/button";
import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useAuth } from "@/contexts/auth";
import { useConfirm } from "@/contexts/confirmDialog";
import { exitLegacySuper } from "@/api/modules/clients";
import { useToastStore } from "@/store/useToastStore";

const LegacySuperExitDialog = ({ client, masters, isOpen, onClose, refresh }) => {
  const { user } = useAuth();
  const confirm = useConfirm();
  const { success, error } = useToastStore();
  const [selection, setSelection] = useState({ clientId: null, value: "" });

  if (user?.role !== "admin") return null;

  const master = masters?.find((item) => item.name === client?.master);
  const snapshots = master?.snapshots || [];
  const isRecovery = client?.legacy_super_state === "recovery_required";
  const journalSnapshot = client?.legacy_super_snapshot;
  const snapshot = isRecovery
    ? journalSnapshot
    : selection.clientId === client?.id ? selection.value : "";
  const belongsToMaster = snapshot?.startsWith(`${client?.master}@`);

  const handleExit = async () => {
    if (!belongsToMaster || client?.status !== "Offline") return;
    const approved = await confirm({
      title: "Exit Legacy Super",
      description: `Confirm the client is offline before exiting. The direct-master legacy mapping for ${client.name} will be replaced with the selected snapshot.`,
      confirmText: "Exit Legacy Super",
      cancelText: "Cancel",
      confirmVariant: "primary",
    });
    if (!approved) return;

    try {
      const response = await exitLegacySuper(client.id, snapshot);
      success("Client Management", response?.message || "Legacy Super exited successfully.");
      await refresh?.();
      onClose();
    } catch (err) {
      await refresh?.();
      error("Client Management", `Failed to exit Legacy Super: ${err.message || String(err)}`);
    }
  };

  return (
    <Dialog open={isOpen} onOpenChange={(open) => !open && onClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Exit Legacy Super</DialogTitle>
          <DialogDescription>
            Select a snapshot belonging to {client?.master}. The client must be offline before its direct-master legacy mapping is replaced.
          </DialogDescription>
        </DialogHeader>
        <label className="space-y-2 text-sm font-medium" htmlFor="legacy-super-snapshot">
          Master snapshot
          <NativeSelect
            id="legacy-super-snapshot"
            value={snapshot}
            disabled={isRecovery}
            onChange={(event) => setSelection({ clientId: client?.id, value: event.target.value })}
          >
            <NativeSelectOption value="">Select snapshot…</NativeSelectOption>
            {isRecovery && belongsToMaster && !snapshots.some((item) =>
              (item.name.includes("@") ? item.name : `${client.master}@${item.name}`) === journalSnapshot
            ) && <NativeSelectOption value={journalSnapshot}>{journalSnapshot}</NativeSelectOption>}
            {snapshots.map((item) => {
              const value = item.name.includes("@") ? item.name : `${client.master}@${item.name}`;
              return <NativeSelectOption key={item.name} value={value}>{value}</NativeSelectOption>;
            })}
          </NativeSelect>
        </label>
        <DialogFooter>
          <Button type="button" variant="ghost" onClick={onClose}>Cancel</Button>
          <Button type="button" disabled={!belongsToMaster || client?.status !== "Offline"} onClick={handleExit}>
            Exit Legacy Super
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
};

export default LegacySuperExitDialog;
