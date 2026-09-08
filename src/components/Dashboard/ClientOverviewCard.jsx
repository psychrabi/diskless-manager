import { useToastStore } from "@/store/useToastStore";
import { getClientOverview } from "@/api/modules/dashboard";
import { Users } from "lucide-react";
import { useEffect, useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
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
    <Card>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Users className="size-4" />
          Client Overview
        </CardTitle>
      </CardHeader>
      <CardContent>
        {loading || !liveOverview ? (
          <div className="flex flex-col gap-3" aria-hidden="true">
            <Skeleton className="h-4 w-2/3" />
            <Skeleton className="h-4 w-1/2" />
            <Skeleton className="h-4 w-3/5" />
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            <div className="flex justify-between items-center">
              <span className="font-semibold">Total Clients:</span>
              <Badge variant="secondary" className="font-mono tabular-nums">
                {liveOverview.total}
              </Badge>
            </div>
            <div className="flex justify-between items-center">
              <span className="font-semibold">Online Clients:</span>
              <Badge variant="outline" className="font-mono tabular-nums">
                {liveOverview.online}
              </Badge>
            </div>
            <div className="flex justify-between items-center">
              <span className="font-semibold">Offline Clients:</span>
              <Badge variant="destructive" className="font-mono tabular-nums">
                {liveOverview.offline}
              </Badge>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
};

export default ClientOverviewCard;