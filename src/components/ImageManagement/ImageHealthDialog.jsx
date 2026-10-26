import { getImageHealth } from "@/api/modules/images";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Spinner } from "@/components/ui/spinner";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  AlertTriangle,
  CheckCircle2,
  RefreshCw,
  ShieldCheck,
  XCircle,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";

const CHECK_STYLES = {
  pass: { icon: CheckCircle2, badge: "default", label: "Pass" },
  warn: { icon: AlertTriangle, badge: "outline", label: "Warning" },
  fail: { icon: XCircle, badge: "destructive", label: "Failed" },
};

const formatSize = (bytes) => {
  if (!bytes) return "-";
  const gib = bytes / (1024 * 1024 * 1024);
  return gib >= 1 ? `${gib.toFixed(1)} GiB` : `${(bytes / (1024 * 1024)).toFixed(0)} MiB`;
};

const HealthCheckRow = ({ check }) => {
  const style = CHECK_STYLES[check.status] || CHECK_STYLES.warn;
  const Icon = style.icon;

  return (
    <li className="flex items-start gap-3 rounded-lg border border-border p-3">
      <Icon
        className={`mt-0.5 size-4 shrink-0 ${
          check.status === "fail"
            ? "text-destructive"
            : check.status === "warn"
              ? "text-amber-600 dark:text-amber-400"
              : "text-emerald-600 dark:text-emerald-400"
        }`}
        aria-hidden="true"
      />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="font-medium">{check.label}</p>
          <Badge variant={style.badge}>{style.label}</Badge>
        </div>
        <p className="mt-1 break-words text-sm text-muted-foreground">
          {check.detail}
        </p>
      </div>
    </li>
  );
};

const ImageHealthDialog = ({ open, onOpenChange, image }) => {
  const [health, setHealth] = useState(null);
  const [error, setError] = useState(null);
  const [loading, setLoading] = useState(false);

  const loadHealth = useCallback(async () => {
    if (!image?.id) return;
    setLoading(true);
    setError(null);
    try {
      setHealth(await getImageHealth(image.id));
    } catch (err) {
      setError(err?.message || String(err));
      setHealth(null);
    } finally {
      setLoading(false);
    }
  }, [image]);

  useEffect(() => {
    if (!open || !image?.id) return undefined;
    const timer = setTimeout(loadHealth, 0);
    return () => clearTimeout(timer);
  }, [open, image?.id, loadHealth]);

  const overall = health
    ? CHECK_STYLES[health.status] || CHECK_STYLES.warn
    : null;
  const OverallIcon = overall?.icon;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>
            Image Health{image?.name ? ` · ${image.name}` : ""}
          </DialogTitle>
          <DialogDescription>
            Read-only inspection of storage and boot metadata. The image is not
            mounted on the server.
          </DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="flex justify-center py-10">
            <Spinner />
          </div>
        ) : error ? (
          <div
            role="alert"
            className="flex items-center justify-between gap-4 rounded-lg border border-destructive/40 bg-destructive/5 p-4 text-sm"
          >
            <span>Failed to inspect image health: {error}</span>
            <Button variant="outline" onClick={loadHealth} icon={RefreshCw}>
              Retry
            </Button>
          </div>
        ) : health ? (
          <div className="flex flex-col gap-4">
            <div className="flex flex-wrap items-center justify-between gap-3 rounded-lg border border-border bg-muted/30 p-4">
              <div className="flex items-center gap-3">
                {OverallIcon ? (
                  <OverallIcon
                    className={`size-5 ${
                      health.status === "fail"
                        ? "text-destructive"
                        : health.status === "warn"
                          ? "text-amber-600 dark:text-amber-400"
                          : "text-emerald-600 dark:text-emerald-400"
                    }`}
                    aria-hidden="true"
                  />
                ) : (
                  <ShieldCheck className="size-5" aria-hidden="true" />
                )}
                <div>
                  <p className="font-medium">Overall status</p>
                  <p className="text-sm text-muted-foreground">
                    {health.status === "pass"
                      ? "All checks passed."
                      : health.status === "warn"
                        ? "Usable with warnings to review."
                        : "Boot-critical problems were detected."}
                  </p>
                </div>
              </div>
              <Badge variant={overall?.badge || "outline"}>{overall?.label}</Badge>
            </div>

            <ul aria-label="Health checks" className="flex flex-col gap-2">
              {health.checks?.map((check) => (
                <HealthCheckRow key={check.id} check={check} />
              ))}
            </ul>

            <div className="rounded-lg border border-border p-4">
              <h3 className="font-medium">Boot metadata</h3>
              <dl className="mt-3 grid gap-3 text-sm sm:grid-cols-2">
                <div>
                  <dt className="text-xs text-muted-foreground">
                    Partition table
                  </dt>
                  <dd className="mt-1">
                    {health.boot?.partition_table || "Not detected"}
                  </dd>
                </div>
                <div>
                  <dt className="text-xs text-muted-foreground">
                    Whole-device filesystem
                  </dt>
                  <dd className="mt-1">{health.boot?.filesystem || "-"}</dd>
                </div>
              </dl>

              {health.boot?.partitions?.length ? (
                <div className="mt-4 overflow-x-auto">
                  <Table aria-label="Detected partitions">
                    <TableHeader>
                      <TableRow>
                        <TableHead>Partition</TableHead>
                        <TableHead>Size</TableHead>
                        <TableHead>Filesystem</TableHead>
                        <TableHead>Type</TableHead>
                        <TableHead>Flags</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {health.boot.partitions.map((partition) => (
                        <TableRow key={partition.name}>
                          <TableCell className="font-medium">
                            {partition.name}
                          </TableCell>
                          <TableCell>{formatSize(partition.size)}</TableCell>
                          <TableCell>{partition.filesystem || "-"}</TableCell>
                          <TableCell>{partition.part_type || "-"}</TableCell>
                          <TableCell>{partition.part_flags || "-"}</TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>
              ) : null}
            </div>

            <div className="flex justify-end">
              <Button variant="outline" onClick={loadHealth} icon={RefreshCw}>
                Re-run checks
              </Button>
            </div>
          </div>
        ) : null}
      </DialogContent>
    </Dialog>
  );
};

export default ImageHealthDialog;
