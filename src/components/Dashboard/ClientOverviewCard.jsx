import { useToastStore } from "@/store/useToastStore";
import { getClientOverview } from "@/api/modules/dashboard";
import { Users } from "lucide-react";
import { useEffect, useState } from "react";
import { Card, LoadingSkeleton } from "@/components/ui";
import { useMetrics } from "@/contexts/useMetrics";

const ClientOverviewCard = () => {
  const [overview, setOverview] = useState(null);
  const [loading, setLoading] = useState(true);
  const { error } = useToastStore();
  const { metrics } = useMetrics();

  useEffect(() => {
    const fetchClientOverview = async () => {
      try {
        const data = await getClientOverview();
        setOverview(data);
      } catch (err) {
        error(
          `Failed to load client overview: ${err.message || "An unknown error occurred"
          }`,
        );
        console.error(err);
        setOverview(null);
      } finally {
        setLoading(false);
      }
    };
    fetchClientOverview();
  }, [error]);

  const liveClients = metrics?.clients;
  const liveOverview = overview && liveClients
    ? {
        total: overview.total,
        online: liveClients.filter((client) => client.status === "Online").length,
      }
    : null;
  if (liveOverview) {
    liveOverview.offline = Math.max(0, liveOverview.total - liveOverview.online);
  }

  return (
    <Card title="Client Overview" icon={Users}>
      {loading || !liveOverview ? (
        <div className="space-y-3" aria-hidden="true">
          <LoadingSkeleton variant="text" width="2/3" />
          <LoadingSkeleton variant="text" width="1/2" />
          <LoadingSkeleton variant="text" width="3/5" />
        </div>
      ) : liveOverview ? (
        <div className="space-y-2">
          <div className="flex justify-between items-center">
            <span className="font-semibold">Total Clients:</span>
            <span className="badge badge-ghost rounded-full font-mono tabular-nums">{liveOverview.total}</span>
          </div>
          <div className="flex justify-between items-center">
            <span className="font-semibold">Online Clients:</span>
            <span className="badge badge-success rounded-full font-mono tabular-nums">{liveOverview.online}</span>
          </div>
          <div className="flex justify-between items-center">
            <span className="font-semibold">Offline Clients:</span>
            <span className="badge badge-error rounded-full font-mono tabular-nums">{liveOverview.offline}</span>
          </div>
        </div>
      ) : (
        <div className="text-error text-center py-4">Failed to load client overview.</div>
      )}
    </Card>
  );
};

export default ClientOverviewCard;
