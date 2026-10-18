import { useMasterManager } from "@/hooks/useMasterManager";
import {
  Button,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Edit,
  History,
  PlusCircle,
  Star,
  StarIcon,
  Trash2,
} from "lucide-react";
import { useCallback, useState } from "react";
import CreateSnapshotModal from "./CreateSnapshotModal";
import RenameImageModal from "./RenameImageModal";
import { SnapshotsList } from "./SnapshotsList";

export const ImagesList = ({ masters }) => {
  const { setDefaultMaster, handleDeleteImage } = useMasterManager();
  const [openSnapshotCreateModal, setOpenSnapshotCreateModal] = useState(false);
  const [selectedImage, setSelectedImage] = useState("");
  const [openRenameModal, setOpenRenameModal] = useState(false);
  const [recoveryImageName, setRecoveryImageName] = useState(null);

  const handleCreateSnapshot = useCallback((image) => {
    setSelectedImage(image);
    setOpenSnapshotCreateModal(true);
  }, []);

  const handleRenameImage = useCallback((image) => {
    setSelectedImage(image);
    setOpenRenameModal(true);
  }, []);

  const recoveryImage = masters.find(
    (master) => master.name === recoveryImageName
  );

  return (
    <>
      <Table aria-label="Images">
        <TableHeader>
          <TableRow>
            <TableHead>Image</TableHead>
            <TableHead>Size</TableHead>
            <TableHead>Status</TableHead>
            <TableHead className="text-right">Actions</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {masters.map((master) => (
            <TableRow key={master.id}>
              <TableCell className="min-w-48 whitespace-normal">
                <div className="font-medium">{master.name}</div>
                <div className="text-xs text-muted-foreground">{master.path}</div>
                {master.is_default && (
                  <div className="text-xs text-muted-foreground">Default image</div>
                )}
                {master.status === "missing" && (
                  <div className="mt-1 text-xs text-destructive" role="status">
                    Image missing from storage. Delete its snapshots first, then
                    delete this entry to remove the saved record.
                  </div>
                )}
              </TableCell>
              <TableCell>{master.size_gb} GB</TableCell>
              <TableCell>
                {master.status === "missing" ? "Missing" : "Available"}
              </TableCell>
              <TableCell>
                <div className="flex flex-wrap justify-end gap-1">
                  <Button
                    variant={master.is_default ? "success" : "default"}
                    size="icon"
                    icon={master.is_default ? Star : StarIcon}
                    onClick={() => setDefaultMaster(master.name)}
                    disabled={master.is_default || master.status === "missing"}
                    title={!master.is_default ? "Set as Default" : "Default image"}
                  />
                  <Button
                    variant="outline"
                    size="sm"
                    icon={History}
                    onClick={() => setRecoveryImageName(master.name)}
                    title={`Recovery points for ${master.name}`}
                  >
                    Recovery points ({master.snapshots?.length ?? 0})
                  </Button>
                  <Button
                    variant="primary"
                    size="icon"
                    icon={PlusCircle}
                    onClick={() => handleCreateSnapshot(master)}
                    title="Create Snapshot"
                    disabled={master.status === "missing"}
                  />
                  <Button
                    variant="info"
                    size="icon"
                    icon={Edit}
                    onClick={() => handleRenameImage(master)}
                    title="Rename Image"
                  />
                  <Button
                    variant="destructive"
                    size="icon"
                    icon={Trash2}
                    onClick={() => handleDeleteImage(master)}
                    title="Delete Image"
                  />
                </div>
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>

      <Dialog
        open={Boolean(recoveryImageName)}
        onOpenChange={(open) => !open && setRecoveryImageName(null)}
      >
        <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
          <DialogHeader>
            <DialogTitle>
              Recovery points{recoveryImage ? ` · ${recoveryImage.name}` : ""}
            </DialogTitle>
            <DialogDescription>
              Review snapshots and restore this image to an earlier point.
            </DialogDescription>
          </DialogHeader>
          {recoveryImage?.snapshots?.length ? (
            <SnapshotsList master={recoveryImage} />
          ) : (
            <p className="py-6 text-center text-sm text-muted-foreground">
              No recovery points are available for this image.
            </p>
          )}
        </DialogContent>
      </Dialog>

      {openSnapshotCreateModal && (
        <CreateSnapshotModal
          openSnapshotCreateModal={openSnapshotCreateModal}
          setOpenSnapshotCreateModal={setOpenSnapshotCreateModal}
          selectedImage={selectedImage}
        />
      )}
      {openRenameModal && (
        <RenameImageModal
          openRenameModal={openRenameModal}
          setOpenRenameModal={setOpenRenameModal}
          selectedImage={selectedImage}
        />
      )}
    </>
  );
};
