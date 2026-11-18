import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Field, FieldDescription, FieldLabel } from "@/components/ui/field";
import {
  cancelImageCopy,
  copyImage,
  listImageCopies,
} from "@/api/modules/images";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { Copy, X } from "lucide-react";
import { useEffect, useState } from "react";

const STATE_LABEL = {
  running: "Copying",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
};

function percent(job) {
  if (job.state === "completed") return 100;
  if (!job.total_bytes) return 0;
  return Math.min(100, Math.round((job.bytes_copied / job.total_bytes) * 100));
}

/**
 * Start and follow an independent image copy (Phase 4C).
 *
 * The server returns a job and streams `zfs send` into a new dataset; the copy
 * only appears in the image list once the transfer is verified, so a partial
 * or cancelled copy can never be assigned to a client.
 */
export default function CopyImageModal({
  openCopyModal,
  setOpenCopyModal,
  selectedImage,
}) {
  const fetchMasters = useAppStore((state) => state.fetchMasters);
  const { success, error } = useToastStore();
  const [name, setName] = useState("");
  const [starting, setStarting] = useState(false);
  const [jobs, setJobs] = useState([]);
  const [jobsError, setJobsError] = useState("");
  const running = jobs.some((job) => job.state === "running");

  useEffect(() => {
    if (!openCopyModal) {
      return undefined;
    }
    let cancelled = false;
    let completed = new Set();

    const load = async () => {
      try {
        const loaded = await listImageCopies();
        if (cancelled) return;
        const list = Array.isArray(loaded) ? loaded : [];
        setJobs(list);
        setJobsError("");
        for (const job of list) {
          if (job.state === "completed" && !completed.has(job.id)) {
            completed.add(job.id);
            await fetchMasters();
          }
        }
      } catch (cause) {
        if (!cancelled) {
          setJobsError(cause?.message || String(cause));
        }
      }
    };

    load();
    const timer = setInterval(load, 2000);
    return () => {
      cancelled = true;
      clearInterval(timer);
    };
  }, [openCopyModal, fetchMasters]);

  const selectedId = selectedImage?.id ?? "";
  const visibleJobs = selectedId
    ? jobs.filter((job) => job.source_image_id === selectedId)
    : jobs;

  const handleStart = async () => {
    if (!selectedImage) return;
    setStarting(true);
    try {
      const job = await copyImage(selectedImage.id, name.trim() || null);
      success("Image copy", `Copy to ${job.destination_name} started.`);
      setName("");
      const refreshed = await listImageCopies();
      setJobs(Array.isArray(refreshed) ? refreshed : []);
    } catch (cause) {
      error(`Failed to start the copy: ${cause?.message || cause}`);
    } finally {
      setStarting(false);
    }
  };

  const handleCancel = async (job) => {
    try {
      await cancelImageCopy(job.id);
      success("Image copy", "Cancellation requested.");
    } catch (cause) {
      error(`Failed to cancel the copy: ${cause?.message || cause}`);
    }
  };

  return (
    <Dialog
      open={Boolean(openCopyModal)}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) setOpenCopyModal(false);
      }}
    >
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            Copy image{selectedImage ? ` · ${selectedImage.name}` : ""}
          </DialogTitle>
          <DialogDescription>
            A copy is an independent master. The source image, its snapshots and
            its clients are left untouched.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4">
          <Field>
            <FieldLabel htmlFor="copy-name">Copy name</FieldLabel>
            <Input
              id="copy-name"
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Leave empty for <image>-copy"
              disabled={starting}
            />
            <FieldDescription>
              Created inside the image dataset; it must not already exist.
            </FieldDescription>
          </Field>

          <div className="flex items-center justify-between gap-3">
            <h3 className="text-sm font-semibold">Recent copies</h3>
            <Button
              type="button"
              variant="primary"
              icon={Copy}
              onClick={handleStart}
              disabled={starting || running || selectedImage?.status !== "ready"}
            >
              {starting ? "Starting…" : "Start copy"}
            </Button>
          </div>
          {running && (
            <p className="text-xs text-muted-foreground">
              A copy is already running. Wait for it to finish or cancel it
              before starting another.
            </p>
          )}

          {jobsError ? (
            <p role="alert" className="text-sm text-destructive">
              {jobsError}
            </p>
          ) : visibleJobs.length === 0 ? (
            <p className="py-4 text-center text-sm text-muted-foreground">
              No copies have been started for this image.
            </p>
          ) : (
            <ul className="divide-y" data-testid="image-copies">
              {visibleJobs.map((job) => (
                <li key={job.id} className="space-y-1.5 py-3">
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span className="break-all font-mono-data text-xs">
                      {job.destination_name}
                    </span>
                    <span className="flex items-center gap-2">
                      <span className="text-xs text-muted-foreground">
                        {STATE_LABEL[job.state] ?? job.state}
                      </span>
                      {job.state === "running" && (
                        <Button
                          type="button"
                          variant="outline"
                          size="sm"
                          icon={X}
                          onClick={() => handleCancel(job)}
                        >
                          Cancel
                        </Button>
                      )}
                    </span>
                  </div>
                  <div
                    className="h-1.5 overflow-hidden rounded-full bg-muted"
                    role="progressbar"
                    aria-label={`Copy progress for ${job.destination_name}`}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={percent(job)}
                  >
                    <div
                      className="h-full rounded-full bg-primary transition-all"
                      style={{ width: `${percent(job)}%` }}
                    />
                  </div>
                  {job.error && (
                    <p role="alert" className="text-xs text-destructive">
                      {job.error}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        <DialogFooter className="mt-4">
          <Button
            type="button"
            variant="outline"
            onClick={() => setOpenCopyModal(false)}
          >
            Close
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
