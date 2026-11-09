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
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Edit,
  History,
  Layers,
  PlusCircle,
  Search,
  SearchX,
  ShieldCheck,
  Star,
  StarIcon,
  Terminal,
  Trash2,
  Monitor,
} from "lucide-react";
import { useCallback, useMemo, useState } from "react";
import CreateSnapshotModal from "./CreateSnapshotModal";
import ImageHealthDialog from "./ImageHealthDialog";
import RenameImageModal from "./RenameImageModal";
import { SnapshotsList } from "./SnapshotsList";
import ImageVersionModal from "./ImageVersionModal";
import { cn } from "@/lib/utils";

const isWindowsImage = (master) =>
  master.os_type === "windows" || /windows/i.test(`${master.name} ${master.path}`);

const isLinuxImage = (master) =>
  master.os_type === "linux" ||
  /linux|ubuntu|fedora|kubuntu|nobara|pop[_-]?os|debian|arch/i.test(
    `${master.name} ${master.path}`
  );

const FILTERS = [
  { id: "all", label: "All" },
  { id: "windows", label: "Windows" },
  { id: "linux", label: "Linux" },
];

export const ImagesList = ({ masters }) => {
  const { setDefaultMaster, handleDeleteImage } = useMasterManager();
  const [openSnapshotCreateModal, setOpenSnapshotCreateModal] = useState(false);
  const [selectedImage, setSelectedImage] = useState("");
  const [openRenameModal, setOpenRenameModal] = useState(false);
  const [recoveryImageName, setRecoveryImageName] = useState(null);
  const [healthImage, setHealthImage] = useState(null);
  const [openVersionModal, setOpenVersionModal] = useState(false);
  const [query, setQuery] = useState("");
  const [osFilter, setOsFilter] = useState("all");

  const handleCreateSnapshot = useCallback((image) => {
    setSelectedImage(image);
    setOpenSnapshotCreateModal(true);
  }, []);

  const handleRenameImage = useCallback((image) => {
    setSelectedImage(image);
    setOpenRenameModal(true);
  }, []);

  const filteredMasters = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return masters.filter((master) => {
      if (osFilter === "windows" && !isWindowsImage(master)) return false;
      if (osFilter === "linux" && !isLinuxImage(master)) return false;
      if (!needle) return true;
      return `${master.name} ${master.path}`.toLowerCase().includes(needle);
    });
  }, [masters, osFilter, query]);

  const recoveryImage = masters.find(
    (master) => master.name === recoveryImageName
  );

  return (
    <>
      <div className="overflow-hidden rounded-2xl border border-border/80 bg-card shadow-[0_1px_2px_rgb(0_0_0/0.035)]">
        <div className="flex flex-col gap-3 border-b border-border/70 p-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="relative w-full sm:max-w-xs">
            <Search className="pointer-events-none absolute top-1/2 left-3 size-3.5 -translate-y-1/2 text-muted-foreground" aria-hidden="true" />
            <Input
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search images or paths…"
              aria-label="Search images"
              className="h-9 rounded-lg border-border/80 bg-background pl-9 text-[12px] shadow-none"
            />
          </div>
          <div className="flex flex-wrap items-center gap-1">
            {FILTERS.map((filter) => (
              <button
                key={filter.id}
                type="button"
                onClick={() => setOsFilter(filter.id)}
                aria-pressed={osFilter === filter.id}
                className={cn(
                  "h-8 rounded-lg px-2.5 text-[11px] font-medium transition-colors",
                  osFilter === filter.id
                    ? "bg-primary/10 text-primary"
                    : "text-muted-foreground hover:bg-muted hover:text-foreground"
                )}
              >
                {filter.label}
              </button>
            ))}
            <span className="ml-1 text-[11px] tabular-nums text-muted-foreground sm:ml-2">
              {filteredMasters.length} of {masters.length}
            </span>
          </div>
        </div>

        {filteredMasters.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-center">
            <span className="flex size-11 items-center justify-center rounded-full border border-border/80 bg-muted/40">
              <SearchX className="size-5 text-muted-foreground" aria-hidden="true" />
            </span>
            <p className="mt-3 text-sm font-medium">No images match this view</p>
            <p className="mt-1 text-xs text-muted-foreground">Try a different OS filter or search term.</p>
          </div>
        ) : (
          <Table aria-label="Images">
            <TableHeader>
              <TableRow className="bg-muted/20 hover:bg-transparent">
                <TableHead className="h-9 px-4 text-[10px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">Image</TableHead>
                <TableHead className="h-9 text-[10px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">Size</TableHead>
                <TableHead className="h-9 text-[10px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">Status</TableHead>
                <TableHead className="h-9 px-4 text-right text-[10px] font-semibold tracking-[0.08em] uppercase text-muted-foreground">Actions</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filteredMasters.map((master) => {
                const windows = isWindowsImage(master);
                const OsIcon = windows ? Monitor : isLinuxImage(master) ? Terminal : Layers;
                return (
                  <TableRow key={master.id} className="group border-border/60 hover:bg-muted/25">
                    <TableCell className="min-w-64 whitespace-normal px-4 py-3">
                      <div className="flex items-start gap-3">
                        <span className={cn(
                          "flex size-9 shrink-0 items-center justify-center rounded-xl border",
                          windows
                            ? "border-info/15 bg-info/10 text-info"
                            : "border-primary/15 bg-primary/10 text-primary"
                        )}>
                          <OsIcon className="size-4" aria-hidden="true" />
                        </span>
                        <div className="min-w-0">
                          <div className="flex flex-wrap items-center gap-2">
                            <span className="text-[13px] font-semibold text-foreground">{master.name}</span>
                            {master.is_default && (
                              <Badge variant="warning" className="gap-1">
                                <Star className="size-3" />
                                Default
                              </Badge>
                            )}
                          </div>
                          <p className="mt-0.5 break-all font-mono-data text-[10px] text-muted-foreground">{master.path}</p>
                          <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
                            <span className="text-[10px] text-muted-foreground">
                              {master.snapshots?.length ?? 0} recovery point{(master.snapshots?.length ?? 0) === 1 ? "" : "s"}
                            </span>
                            {master.status === "missing" && (
                              <span className="text-[10px] text-destructive" role="status">
                                Missing from storage. Delete its snapshots first.
                              </span>
                            )}
                            {master.status === "failed" && (
                              <span className="text-[10px] text-destructive" role="status">
                                Linux image build failed. Delete this partial image before trying again.
                              </span>
                            )}
                          </div>
                        </div>
                      </div>
                    </TableCell>
                    <TableCell className="font-mono-data text-[11px] tabular-nums text-muted-foreground">
                      {master.size_gb} GB
                    </TableCell>
                    <TableCell>
                      <Badge variant={master.status === "ready" ? "success" : "destructive"} className="gap-1.5">
                        <span className="size-1.5 rounded-full bg-current" />
                        {master.status === "missing" ? "Missing" : master.status === "failed" ? "Build failed" : master.status === "ready" ? "Available" : master.status}
                      </Badge>
                    </TableCell>
                    <TableCell className="px-3">
                      <div className="flex flex-wrap items-center justify-end gap-1">
                        <Button
                          variant={master.is_default ? "warning" : "ghost"}
                          size="icon-sm"
                          icon={master.is_default ? Star : StarIcon}
                          onClick={() => setDefaultMaster(master.name)}
                          disabled={master.is_default || master.status !== "ready"}
                          title={!master.is_default ? "Set as Default" : "Default image"}
                        />
                        <Button
                          variant="outline"
                          size="sm"
                          icon={History}
                          onClick={() => setRecoveryImageName(master.name)}
                          title={`Recovery points for ${master.name}`}
                        >
                          Recovery ({master.snapshots?.length ?? 0})
                        </Button>
                        <Button
                          variant="outline"
                          size="sm"
                          icon={History}
                          onClick={() => {
                            setSelectedImage(master);
                            setOpenVersionModal(true);
                          }}
                          title={`Versions for ${master.name}`}
                        >
                          Versions
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          icon={ShieldCheck}
                          onClick={() => setHealthImage(master)}
                          title={`Check health of ${master.name}`}
                        />
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          icon={PlusCircle}
                          onClick={() => handleCreateSnapshot(master)}
                          title="Create Snapshot"
                          disabled={master.status !== "ready"}
                        />
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          icon={Edit}
                          onClick={() => handleRenameImage(master)}
                          title="Rename Image"
                        />
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          icon={Trash2}
                          onClick={() => handleDeleteImage(master)}
                          title="Delete Image"
                          className="text-muted-foreground hover:bg-destructive/10 hover:text-destructive"
                        />
                      </div>
                    </TableCell>
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
      </div>

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
      {openVersionModal && (
        <ImageVersionModal
          openVersionModal={openVersionModal}
          setOpenVersionModal={setOpenVersionModal}
          selectedImage={selectedImage}
        />
      )}
      <ImageHealthDialog
        open={Boolean(healthImage)}
        onOpenChange={(nextOpen) => !nextOpen && setHealthImage(null)}
        image={healthImage}
      />
    </>
  );
};
