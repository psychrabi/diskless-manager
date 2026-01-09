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
    key={snap.id || snap.name}
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
        className="h-7 w-7"
        title={`Rollback ${snap.name}`}
      >
        <RotateCcw className="h-4 w-4" />
      </Button>
      <Button
        onClick={() => handleDeleteSnapshot(snap.name, masterName)}
        variant="destructive"
        size="icon"
        className="h-7 w-7"
        title={`Delete ${snap.name}`}
      >
        <Trash2 className="h-4 w-4" />
      </Button>
    </div>
  </li>
);

export const SnapshotsList = ({ master }) => {
  const { handleDeleteSnapshot, handleRollbackSnapshot } = useMasterManager();

  return (
    <ul className="space-y-2 text-sm">
      {master.snapshots.map((snap) => (
        <SnapshotItem
          key={snap.id || snap.name}
          snap={snap}
          masterName={master.name}
          handleDeleteSnapshot={handleDeleteSnapshot}
          handleRollbackSnapshot={handleRollbackSnapshot}
        />
      ))}
    </ul>
  );
};
