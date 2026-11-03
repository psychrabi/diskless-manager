import { getClientBootDiagnostics } from "@/api/modules/clients";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { formatUptime } from "@/utils/formatUptime";
import { Activity, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";

function Row({ label, children, mono = false }) {
  return (
    <div className="flex items-baseline justify-between gap-4 py-0.5">
      <dt className="shrink-0 text-muted-foreground">{label}</dt>
      <dd className={mono ? "break-all text-right font-mono text-xs" : "text-right"}>
        {children}
      </dd>
    </div>
  );
}

function gatewayHint(gateway) {
  return gateway === "dhcp"
    ? "The script leaves the gateway to DHCP."
    : "Windows SAN boot overrides the DHCP gateway with 0.0.0.0 so iSCSI traffic stays on the boot subnet.";
}

const phaseLabels = {
  script_served: "Boot script served",
  session_observed: "OS session observed",
  expired: "Window expired",
};

/**
 * Read-only view of the boot inputs the server will use on this client's next
 * PXE request. It never boots, changes storage or edits the gateway; the
 * endpoint behind it is read-only for the same reason.
 */
export default function BootDiagnosticsSection({ clientId }) {
  const [data, setData] = useState(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [reloadKey, setReloadKey] = useState(0);

  // Reloading starts from the click, not from an effect body: the effect only
  // subscribes to the request it issued.
  const refresh = () => {
    setLoading(true);
    setError("");
    setReloadKey((key) => key + 1);
  };

  useEffect(() => {
    let cancelled = false;
    getClientBootDiagnostics(clientId).then(
      (next) => {
        if (cancelled) return;
        setData(next);
        setError("");
        setLoading(false);
      },
      (failure) => {
        if (cancelled) return;
        setData(null);
        setError(failure?.message || "Failed to load boot diagnostics");
        setLoading(false);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [clientId, reloadKey]);

  return (
    <div
      className="space-y-3 rounded-lg border border-border bg-muted/30 p-4"
      data-testid="boot-diagnostics"
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="flex items-center gap-2 font-semibold">
            <Activity className="size-4" />
            Boot diagnostics
            <Badge variant="outline">Read only</Badge>
          </div>
          <p className="mt-1 text-xs text-muted-foreground">
            What the server will send on this client's next PXE boot. Nothing
            here changes the boot script, storage or session state.
          </p>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="icon"
          title="Refresh boot diagnostics"
          aria-label="Refresh boot diagnostics"
          disabled={loading}
          onClick={refresh}
        >
          <RefreshCw />
        </Button>
      </div>

      {loading && !data ? (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner /> Loading boot diagnostics…
        </div>
      ) : error ? (
        <div className="space-y-2 text-sm text-destructive" role="alert">
          <p>{error}</p>
          <Button
            type="button"
            variant="outline"
            size="sm"
            onClick={refresh}
          >
            Retry
          </Button>
        </div>
      ) : data ? (
        <div className="space-y-3 text-sm">
          <dl className="grid gap-0.5">
            <Row label="PXE firmware">{data.firmware || "—"}</Row>
            <Row label="Assigned image">
              {data.assigned_image || "—"}
              {data.os_type ? ` (${data.os_type})` : ""}
            </Row>
            <Row label="iSCSI target" mono>
              {data.target_iqn || "Not assigned"}
            </Row>
            <Row label="Initiator IQN" mono>
              {data.initiator_iqn || "—"}
            </Row>
            <Row label="iSCSI login">
              {data.chap_enabled ? "CHAP required" : "No CHAP"}
            </Row>
            <Row label="Keep SAN">{data.keep_san ? "Yes" : "No"}</Row>
            <Row label="iBFT gateway" mono>
              {data.ibft_gateway}
            </Row>
          </dl>
          <p className="text-xs text-muted-foreground">{gatewayHint(data.ibft_gateway)}</p>

          <dl className="grid gap-0.5 border-t border-border/60 pt-3">
            <Row label="Boot NICs" mono>
              {data.boot_nic_ids?.length ? data.boot_nic_ids.join(", ") : "None recorded"}
            </Row>
            <Row label="Hardware">
              {[data.hardware_architecture, data.hardware_firmware]
                .filter(Boolean)
                .join(" / ") || "Not inventoried"}
            </Row>
          </dl>
          {data.hardware_source && (
            <p className="text-xs text-muted-foreground">
              From {data.hardware_source}
              {data.hardware_observed_at
                ? `, ${String(data.hardware_observed_at).slice(0, 10)}`
                : ""}
              .
            </p>
          )}

          <div className="border-t border-border/60 pt-3">
            <div className="mb-1 font-medium">Boot handoff</div>
            {data.handoff ? (
              <dl className="grid gap-0.5">
                <Row label="Phase">{phaseLabels[data.handoff.phase] || data.handoff.phase}</Row>
                <Row label="Attempt">{data.handoff.attempt_id}</Row>
                <Row label="Started">{formatUptime(data.handoff.age_seconds)} ago</Row>
                <Row label="Window left">
                  {data.handoff.active
                    ? formatUptime(data.handoff.expires_in_seconds)
                    : "Expired"}
                </Row>
              </dl>
            ) : (
              <p className="text-xs text-muted-foreground">
                No iPXE boot attempt is recorded for this client.
              </p>
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}
