import { useMasterManager } from "@/hooks/useMasterManager";
import { RotateCcw, Trash2 } from "lucide-react";
import { Button } from "@/components/ui";

const SnapshotItem = ({
  snap,
  masterName,
  handleDeleteSnapshot,
  handleRollbackSnapshot,
}) => (
  <li
    key={snap.name}
    className="flex flex-wrap justify-between items-center gap-2 p-2 rounded hover:bg-base-100"
  >
    <div className="flex-1 min-w-0">
      <span className="font-mono text-xs break-all ">{snap.name}</span>
      <span className="text-base-content/60 text-xs ml-2 whitespace-nowrap">
        ({snap.created}, {snap.used})
      </span>
    </div>
    <div className="flex space-x-1 flex-shrink-0">
      <Button
        onClick={() => handleRollbackSnapshot(snap.name, masterName)}
        variant="info"
        size="icon"
        className="min-h-[36px] min-w-[36px]"
        title={`Rollback ${snap.name}`}
      >
        <RotateCcw className="h-4 w-4" />
      </Button>
      <Button
        onClick={() => handleDeleteSnapshot(snap.name, masterName)}
        variant="destructive"
        size="icon"
        className="min-h-[36px] min-w-[36px]"
        title={`Delete ${snap.name}`}
      >
        <Trash2 className="h-4 w-4" />
      </Button>
    </div>
  </li>
);

export const SnapshotsList = ({ master }) => {
  const { handleDeleteSnapshot, handleRollbackSnapshot } = useMasterManager();
  
  // Use snapshots from master prop if available, otherwise empty array
  const snapshots = master.snapshots || [];

  if (!snapshots || snapshots.length === 0) {
    return (
      <p className="text-sm text-base-content/60">
        No snapshots found for this image.
      </p>
    );
  }

  return (
    <ul className="space-y-2 text-sm">
      {snapshots.map((snap) => (
        <SnapshotItem
          key={snap.name}
          snap={snap}
          masterName={master.name}
          handleDeleteSnapshot={handleDeleteSnapshot}
          handleRollbackSnapshot={handleRollbackSnapshot}
        />
      ))}
    </ul>
  );
};
