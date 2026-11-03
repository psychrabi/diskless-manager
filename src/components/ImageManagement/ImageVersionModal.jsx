import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { listImageVersions } from "@/api/modules/images";
import { useEffect, useState } from "react";

/**
 * Read-only browser for the versions published from verified servicing
 * clones (ADR 0009). Publishing itself happens in the Windows servicing
 * workflow; this view only reports what exists for one base image.
 */
export default function ImageVersionModal({
  openVersionModal,
  setOpenVersionModal,
  selectedImage,
}) {
  const baseName = selectedImage?.name ?? selectedImage ?? "";
  // The loaded list is tagged with the base name it belongs to, so "loading"
  // is derived during render instead of set synchronously from the effect.
  const [loaded, setLoaded] = useState({ baseName: "", versions: [], error: "" });
  const loading = Boolean(openVersionModal && baseName) && loaded.baseName !== baseName;
  const versions = loaded.versions;
  const error = loaded.error;

  useEffect(() => {
    if (!openVersionModal || !baseName) {
      return undefined;
    }
    let cancelled = false;
    listImageVersions(baseName)
      .then((published) => {
        if (!cancelled) {
          setLoaded({
            baseName,
            versions: Array.isArray(published) ? published : [],
            error: "",
          });
        }
      })
      .catch((cause) => {
        if (!cancelled) {
          setLoaded({
            baseName,
            versions: [],
            error: cause?.message || String(cause),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [openVersionModal, baseName]);

  return (
    <Dialog
      open={Boolean(openVersionModal)}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) {
          setOpenVersionModal(false);
        }
      }}
    >
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>
            Image versions{baseName ? ` · ${baseName}` : ""}
          </DialogTitle>
          <DialogDescription>
            Versions published from verified servicing clones. Publishing never
            renames the source image or moves an assigned client.
          </DialogDescription>
        </DialogHeader>
        {error ? (
          <p role="alert" className="py-6 text-center text-sm text-destructive">
            {error}
          </p>
        ) : loading ? (
          <p className="py-6 text-center text-sm text-muted-foreground">
            Loading versions…
          </p>
        ) : versions.length === 0 ? (
          <p className="py-6 text-center text-sm text-muted-foreground">
            No image versions are available yet.
          </p>
        ) : (
          <ul className="divide-y" data-testid="image-versions">
            {versions.map((version) => (
              <li key={version.id} className="py-3">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{version.version}</span>
                  {version.is_pinned && (
                    <span className="rounded border px-1.5 text-xs text-muted-foreground">
                      Pinned
                    </span>
                  )}
                  {version.is_locked && (
                    <span className="rounded border px-1.5 text-xs text-muted-foreground">
                      Locked
                    </span>
                  )}
                </div>
                <div className="text-sm text-muted-foreground">{version.name}</div>
                {version.source_description && (
                  <div className="text-xs text-muted-foreground">
                    {version.source_description}
                  </div>
                )}
                {(version.published_by || version.published_at) && (
                  <div className="text-xs text-muted-foreground">
                    Published{version.published_by ? ` by ${version.published_by}` : ""}
                    {version.published_at ? ` · ${version.published_at}` : ""}
                  </div>
                )}
              </li>
            ))}
          </ul>
        )}
      </DialogContent>
    </Dialog>
  );
}
