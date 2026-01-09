import { useAppStore } from "@/store/useAppStore";
import { Card } from "@/components/ui";

const ZfsPoolCard = ({ loading }) => {
  const zpoolStats = useAppStore((state) => state.zpoolStats);
  return (
    <Card title="Disk Usage">
      {loading ? (
        <div>Loading...</div>
      ) : zpoolStats ? (
        <ul className="">
          <div className="grid grid-cols-2 gap-x-10 space-y-2">
            <li className="flex justify-between">
              <span className="font-semibold">Pool:</span> {zpoolStats.name}
            </li>
            <li className="flex justify-between">
              <span className="font-semibold">Size:</span> {zpoolStats.size}
            </li>
            <li className="flex justify-between">
              <span className="font-semibold">Used:</span> {zpoolStats.allocated}
            </li>
            <li className="flex justify-between">
              <span className="font-semibold">Available:</span>
              {zpoolStats.free}
            </li>
            <li className="flex justify-between">
              <span className="font-semibold">Health:</span>
              <span className="badge badge-success rounded-full">
                {zpoolStats.health == "-" ? "Good" : zpoolStats.health}
              </span>
            </li>
          </div>
        </ul>
      ) : (
        <div className="text-red-500">Failed to load ZFS pool info.</div>
      )}
    </Card>
  );
};

export default ZfsPoolCard;
