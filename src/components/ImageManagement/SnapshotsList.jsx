import { Button } from "@/components/ui";
import { Badge } from "@/components/ui/badge";
import { useMasterManager } from "@/hooks/useMasterManager";
import { ChevronDown, ChevronRight, CopyPlus, History } from "lucide-react";
import { RotateCcw, Trash2 } from "lucide-react";
import { useMemo, useState } from "react";

const getShortName = (name) => {
  if (!name) return "";
  return name.includes("@") ? name.split("@").pop() : name;
};

const sortByCreated = (list) =>
  [...list].sort((a, b) => {
    const ta = Date.parse(a?.created);
    const tb = Date.parse(b?.created);
    if (!Number.isNaN(ta) && !Number.isNaN(tb) && ta !== tb) return ta - tb;
    return String(a?.name ?? "").localeCompare(String(b?.name ?? ""));
  });

const ChainNode = ({
  snap,
  index,
  isLast,
  masterName,
  newerSnapshots,
  handleDeleteSnapshot,
  handleRollbackSnapshot,
  handleApplySnapshot,
}) => (
  <li
    role="treeitem"
    aria-label={`Snapshot ${index + 1}: ${snap.name}${isLast ? ", latest" : ""}`}
    className="relative flex gap-3"
  >
    <div aria-hidden="true" className="flex flex-col items-center">
      <span className="flex size-6 shrink-0 items-center justify-center rounded-full border border-border bg-muted text-[11px] font-semibold text-muted-foreground">
        {index + 1}
      </span>
      {!isLast && <span className="w-px flex-1 bg-border" />}
    </div>
    <div className="flex-1 min-w-0 pb-3">
      <div data-testid="snapshot-actions" className="flex flex-col items-stretch gap-2 rounded-lg border border-border/60 bg-background px-3 py-2 hover:bg-muted/40 sm:flex-row sm:items-center sm:justify-between">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className=" text-xs break-all">
              {getShortName(snap.name)}
            </span>
            {isLast && (
              <Badge variant="default" className="bg-primary/10 text-primary hover:bg-primary/10 text-[11px]">
                latest
              </Badge>
            )}
          </div>
          <div className="text-muted-foreground mt-0.5 break-words text-xs sm:whitespace-nowrap">
            #{index + 1} &middot; {snap.created} &middot; {snap.used}
          </div>
        </div>
        <div className="flex shrink-0 justify-end gap-1 sm:space-x-1">
          <Button
            onClick={() => handleApplySnapshot(snap.name, masterName)}
            variant="secondary"
            size="icon"
            className="min-h-[36px] min-w-[36px]"
            title={`Apply ${snap.name} as new image`}
            aria-label={`Apply ${snap.name} as new image`}
          >
            <CopyPlus className="h-4 w-4" />
          </Button>
          <Button
            onClick={() =>
              handleRollbackSnapshot(snap.name, masterName, newerSnapshots)
            }
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
      </div>
    </div>
  </li>
);

export const SnapshotsList = ({ master }) => {
  const {
    handleDeleteSnapshot,
    handleRollbackSnapshot,
    handleApplySnapshot,
  } = useMasterManager();
  const [expanded, setExpanded] = useState(true);

  const snapshots = useMemo(
    () => sortByCreated(master?.snapshots || []),
    [master?.snapshots]
  );

  if (!master || snapshots.length === 0) return null;

  return (
    <div role="tree" aria-label={`Snapshot chain for ${master.name}`}>
      <div
        role="treeitem"
        aria-expanded={expanded}
        aria-label={`Snapshots for ${master.name}, ${snapshots.length} snapshots in chain`}
        className="flex items-center gap-2 rounded p-2 hover:bg-background"
      >
        <Button
          type="button"
          variant="ghost"
          size="icon"
          onClick={() => setExpanded((v) => !v)}
          aria-expanded={expanded}
          aria-label={
            expanded
              ? `Collapse snapshot chain for ${master.name}`
              : `Expand snapshot chain for ${master.name}`
          }
          title={expanded ? "Collapse chain" : "Expand chain"}
          className="size-6 shrink-0"
        >
          {expanded ? (
            <ChevronDown className="h-4 w-4" />
          ) : (
            <ChevronRight className="h-4 w-4" />
          )}
        </Button>
        <History className="h-4 w-4 shrink-0 text-primary" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          Restore Points
        </span>
        <Badge variant="secondary" className="shrink-0">
          {snapshots.length} snapshot{snapshots.length === 1 ? "" : "s"}
        </Badge>
      </div>

      {expanded && (
          <ol
            role="group"
            aria-label="Snapshots in creation order"
            className="ml-5 mt-1"
          >
            {snapshots.map((snap, index) => (
              <ChainNode
                key={snap.name}
                snap={snap}
                index={index}
                isLast={index === snapshots.length - 1}
                masterName={master.name}
                newerSnapshots={snapshots.slice(index + 1)}
                handleDeleteSnapshot={handleDeleteSnapshot}
                handleRollbackSnapshot={handleRollbackSnapshot}
                handleApplySnapshot={handleApplySnapshot}
              />
            ))}
          </ol>
        )}
    </div>
  );
};
