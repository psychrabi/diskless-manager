import { clearRamCache } from "@/api/commands";
import { useAppStore } from "@/store/useAppStore";
import { RefreshCw } from "lucide-react";
import { Button, Card } from "../components/ui";
export const RAMUsage = () => {
  const { ramUsage, arcStat } = useAppStore();

  return (
    <Card
      title="RAM Usage"
      icon={RefreshCw}
      actions={
        <Button
          onClick={clearRamCache}
          variant="primary"
          className="w-full btn-xs"
        >
          Clear Cache
        </Button>
      }
    >
      <ul className="">
        <div className="grid grid-cols-2 gap-x-10 space-y-2">
          <li className="flex justify-between">
            <span className="font-semibold">Total:</span>{" "}
            {ramUsage.memory.total}
          </li>
          <li className="flex justify-between">
            <span className="font-semibold">Used:</span> {ramUsage.memory.used}
          </li>
          <li className="flex justify-between">
            <span className="font-semibold">Free:</span> {ramUsage.memory.free}
          </li>
          <li className="flex justify-between">
            <span className="font-semibold">Available:</span>{" "}
            {ramUsage.memory.available}
          </li>
          {arcStat && (
            <>
              <li className="flex justify-between">
                <span className="font-semibold">ZFS Cache:</span>{" "}
                {(arcStat.size / (1024 * 1024)).toFixed(1)} MB
              </li>
              <li className="flex justify-between">
                <span className="font-semibold">ZFS Cache Hit:</span>{" "}
                {arcStat.hit_percent.toFixed(2)}%
              </li>
            </>
          )}
        </div>
      </ul>
    </Card>
  );
};
