import { useToastStore } from "@/store/useToastStore";
import { getDefaultImageOverview } from "@/api/modules/dashboard";
import { HardDrive, RefreshCw } from "lucide-react"; // Add Refresh icon
import { useCallback, useEffect, useState } from "react";
import { Button, Card } from "@/components/ui"; // Assume Button component

const MasterImageOverviewCard = () => {
  const [overview, setOverview] = useState(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const { error: showError } = useToastStore();

  const fetchMasterImageOverview = useCallback(
    async (showErrorToast = true) => {
      setError("");
      setLoading(true);
      try {
        const data = await getDefaultImageOverview();
        setOverview(data);
      } catch (err) {
        console.error(err);
        let errorMsg = err || "An unknown error occurred";
        // Map specific errors
        if (
          errorMsg.includes("not set in config") ||
          errorMsg.includes("please set a new one")
        ) {
          errorMsg = "Set a default image first.";
        } else if (errorMsg.includes("deleted or not present")) {
          errorMsg = "Master image is deleted or not present.";
        }
        const errorText =
          typeof errorMsg === "string"
            ? errorMsg
            : errorMsg?.message || String(errorMsg);
        setError(errorText);
        if (showErrorToast) {
          showError(`Failed to load master image overview: ${errorText}`);
        }
        setOverview(null);
      } finally {
        setLoading(false);
      }
    },
    [showError],
  );

  useEffect(() => {
    // Defer so setState inside the fetch is not synchronous within
    // the effect body (react-hooks/set-state-in-effect).
    const timer = setTimeout(fetchMasterImageOverview, 0);
    return () => clearTimeout(timer);
  }, [fetchMasterImageOverview]); // Run once on mount

  const handleRetry = () => fetchMasterImageOverview(false); // No duplicate toast

  return (
    <Card title="Default Image Overview" icon={HardDrive}>
      {loading ? (
        <div className="space-y-3" aria-hidden="true">
          <div className="h-5 bg-base-200 rounded animate-pulse w-3/4" />
          <div className="h-5 bg-base-200 rounded animate-pulse w-1/2" />
          <div className="h-5 bg-base-200 rounded animate-pulse w-2/3" />
        </div>
      ) : error ? (
        <div className="text-center py-4 space-y-2">
          <div className="text-error">{error}</div>
          <Button onClick={handleRetry} variant="outline" size="sm">
            <RefreshCw className="w-4 h-4 mr-2" />
            Retry
          </Button>
        </div>
      ) : overview ? (
        <div className="space-y-2">
          <div className="flex justify-between">
            <span className="font-semibold">Name:</span>
            <span className="text-right">{overview.name}</span>
          </div>
          <div className="flex justify-between">
            <span className="font-semibold">Created:</span>
            <span className="text-right">{overview.creation_date}</span>
          </div>
          {overview.clones && overview.clones !== "-" && (
            <div className="flex justify-between">
              <span className="font-semibold">Clones:</span>
              <span className="text-right">{overview.clones}</span>
            </div>
          )}
        </div>
      ) : (
        <div className="text-error text-center py-4">
          Set a default image first.
        </div>
      )}
    </Card>
  );
};

export default MasterImageOverviewCard;
