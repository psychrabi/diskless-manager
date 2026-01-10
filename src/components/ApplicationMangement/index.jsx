import { Wrench } from "lucide-react";
import { Card } from "@/components/ui";
import AdminPasswordForm from "./AdminPasswordForm";
import PrivilegeManagementForm from "./PrivilegeManagementForm";

const ApplicationSettings = () => {
  return (
    <Card title="Application Settings" subtitle="Set your application settings" icon={Wrench}  className="bg-base-300">
     

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <AdminPasswordForm />
        <PrivilegeManagementForm />
      </div>
    </Card>
  );
};
export default ApplicationSettings;
