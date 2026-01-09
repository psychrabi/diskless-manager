import { Copy } from "lucide-react";
import { Card } from "@/components/ui";
import LicenseActivation from "./LicenseActivation";
import LicenseCard from "./LicenseCard";

const LicenseManagement = () => {
  return (
    <Card title="License Management" icon={Copy} className="bg-base-300">
      <div className="grid grid-cols-2 gap-4">
        {/* License activation */}
        <LicenseActivation />
        <LicenseCard />
      </div>
    </Card>
  );
};
export default LicenseManagement;
