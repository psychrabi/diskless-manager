import { useToastStore } from "@/store/useToastStore";
import { getClientOverview } from "@/api/modules/dashboard";
import { Users } from "lucide-react";
import { useEffect, useState } from "react";
import { Card, LoadingSkeleton } from "@/components/ui";

const ClientOverviewCard = () => {
  const [overview, setOverview] = useState(null);
  const [loading, setLoading] = useState(true);
  const { error } = useToastStore();

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

  return (
    <Card title="Client Overview" icon={Users}>
      {loading ? (
        <div className="space-y-3" aria-hidden="true">
          <LoadingSkeleton variant="text" width="2/3" />
          <LoadingSkeleton variant="text" width="1/2" />
          <LoadingSkeleton variant="text" width="3/5" />
        </div>
      ) : overview ? (
        <div className="space-y-2">
          <div className="flex justify-between items-center">
            <span className="font-semibold">Total Clients:</span>
            <span className="badge badge-ghost rounded-full font-mono tabular-nums">{overview.total}</span>
          </div>
          <div className="flex justify-between items-center">
            <span className="font-semibold">Online Clients:</span>
            <span className="badge badge-success rounded-full font-mono tabular-nums">{overview.online}</span>
          </div>
          <div className="flex justify-between items-center">
            <span className="font-semibold">Offline Clients:</span>
            <span className="badge badge-error rounded-full font-mono tabular-nums">{overview.offline}</span>
          </div>
        </div>
      ) : (
        <div className="text-error text-center py-4">Failed to load client overview.</div>
      )}
    </Card>
  );
};

export default ClientOverviewCard;
