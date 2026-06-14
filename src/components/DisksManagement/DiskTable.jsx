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
      <Table className="bg-base-100 rounded-lg" aria-label="ZFS datasets list">
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
                <span className="badge badge-primary badge-sm">
                  {dataset.disk_type || "-"}
                </span>
              </TableCell>
              <TableCell>{dataset.used}</TableCell>
              <TableCell>{dataset.available}</TableCell>
              <TableCell>{dataset.referenced}</TableCell>
              <TableCell className="text-sm text-base-content/70">
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
