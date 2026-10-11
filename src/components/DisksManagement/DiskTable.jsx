import { Badge } from "@/components/ui/badge";
import { useConfirm } from "@/contexts/confirmDialog";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { useState } from "react";
import {
  Button,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui";
import RenameDiskModal from "./RenameDiskModal";

const DiskTable = ({ datasets, onRefresh }) => {
  const [selectedDisk, setSelectedDisk] = useState(null);
  const [isRenameModalOpen, setIsRenameModalOpen] = useState(false);
  const confirm = useConfirm();
  const deleteDataset = useAppStore((state) => state.deleteDataset);
  const { success, error } = useToastStore();

  const handleRenameDisk = (disk) => {
    setSelectedDisk(disk);
    setIsRenameModalOpen(true);
  };

  const handleDeleteDisk = async (disk) => {
    const ok = await confirm({
      title: "Delete disk",
      description: `Are you sure you want to delete disk "${disk.name}"? This action cannot be undone and might affect clones.`,
      confirmText: "Delete disk",
      cancelText: "Cancel",
      confirmVariant: "primary",
      size: "2xl",
    });
    if (ok) {
      const result = await deleteDataset(disk.name);
      if (result.success) {
        success(result.message);
        onRefresh();
      } else {
        error(result.error);
      }
    }
  };

  return (
    <>
      <div className="space-y-3 lg:hidden" aria-label="ZFS datasets list">
        {datasets.map((dataset) => (
          <article key={dataset.name} data-testid="disk-mobile-card" className="rounded-lg border bg-card p-4">
            <div className="flex min-w-0 items-start justify-between gap-3">
              <h2 className="break-all font-semibold">{dataset.name}</h2>
              <Badge variant="default">{dataset.disk_type || "-"}</Badge>
            </div>
            <dl className="mt-3 grid grid-cols-2 gap-x-4 gap-y-3 text-sm">
              <div><dt className="text-xs text-muted-foreground">Used</dt><dd className="mt-1">{dataset.used}</dd></div>
              <div><dt className="text-xs text-muted-foreground">Available</dt><dd className="mt-1">{dataset.available}</dd></div>
              <div><dt className="text-xs text-muted-foreground">Referenced</dt><dd className="mt-1">{dataset.referenced}</dd></div>
              <div className="col-span-2 min-w-0"><dt className="text-xs text-muted-foreground">Mount point</dt><dd className="mt-1 break-all text-muted-foreground">{dataset.mountpoint}</dd></div>
            </dl>
            <div className="mt-4 flex gap-2 border-t pt-3">
              <Button variant="info" className="flex-1" onClick={() => handleRenameDisk(dataset)}>Rename</Button>
              <Button variant="ghost" className="flex-1" onClick={() => handleDeleteDisk(dataset)}>Delete</Button>
            </div>
          </article>
        ))}
      </div>
      <div className="hidden overflow-x-auto lg:block">
      <Table className="rounded-lg bg-background" aria-label="ZFS datasets list">
        <TableHeader>
          <TableRow>
            <TableHead>Name</TableHead>
            <TableHead>Type</TableHead>
            <TableHead>Used</TableHead>
            <TableHead>Available</TableHead>
            <TableHead>Referenced</TableHead>
            <TableHead>Mount Point</TableHead>
            <TableHead>Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {datasets.map((dataset) => (
            <TableRow key={dataset.name}>
              <TableCell>{dataset.name}</TableCell>
              <TableCell>
                <Badge variant="default" className="">
                  {dataset.disk_type || "-"}
                </Badge>
              </TableCell>
              <TableCell>{dataset.used}</TableCell>
              <TableCell>{dataset.available}</TableCell>
              <TableCell>{dataset.referenced}</TableCell>
              <TableCell className="text-sm text-muted-foreground">
                {dataset.mountpoint}
              </TableCell>
              <TableCell>
                <div className="flex gap-2">
                  <Button
                    variant="info"
                    size="sm"
                    onClick={() => handleRenameDisk(dataset)}
                  >
                    Rename
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => handleDeleteDisk(dataset)}
                  >
                    Delete
                  </Button>
                </div>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      </div>
      {isRenameModalOpen && selectedDisk && (
        <RenameDiskModal
          openRenameModal={isRenameModalOpen}
          setOpenRenameModal={setIsRenameModalOpen}
          selectedDisk={selectedDisk}
          refresh={onRefresh}
        />
      )}
    </>
  );
};

export default DiskTable;
