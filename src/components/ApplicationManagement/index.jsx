import { Wrench } from "lucide-react";
import { Card } from "@/components/ui";
import AdminPasswordForm from "./AdminPasswordForm";
import PrivilegeManagementForm from "./PrivilegeManagementForm";
import UserManagement from "../UserManagement";

const ApplicationSettings = () => {
  return (
    <div className="space-y-6">
      <Card
        title="Application Settings"
        subtitle="Set your application settings"
        icon={Wrench}
        className="bg-base-300"
      >
        <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
          <AdminPasswordForm />
          <PrivilegeManagementForm />
        </div>
      </Card>
    </div>
  );
};
export default ApplicationSettings;
