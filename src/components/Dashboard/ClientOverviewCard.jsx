import { useToastStore } from "@/store/useToastStore";
import { getClientOverview } from "@/api/modules/dashboard";
import { Users } from "lucide-react";
import { useEffect, useState } from "react";
import { Card } from "@/components/ui";

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
        <div>Loading...</div>
      ) : overview ? (
        <ul className="space-y-2 ">
          <li className="flex justify-between">
            <span className="font-semibold">Total Clients:</span>
            {overview.total}
          </li>
          <li className="flex justify-between">
            <span className="font-semibold">Online Clients:</span>
            {overview.online}
          </li>
          <li className="flex justify-between">
            <span className="font-semibold">Offline Clients:</span>
            {overview.offline}
          </li>
        </ul>
      ) : (
        <div className="text-red-500">Failed to load client overview.</div>
      )}
    </Card>
  );
};

export default ClientOverviewCard;
