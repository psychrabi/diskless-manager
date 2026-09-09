import { PageHeader } from "@/components/ui";
import LicenseActivation from "./LicenseActivation";
import LicenseCard from "./LicenseCard";

const LicenseManagement = () => {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="License Management"
        description="Activate and review your server license."
      />
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 items-stretch">
        {/* License activation */}
        <LicenseActivation />
        <LicenseCard />
      </div>
    </div>
  );
};
export default LicenseManagement;
