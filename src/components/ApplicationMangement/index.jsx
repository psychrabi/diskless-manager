import { Wrench } from "lucide-react";
import { Card } from "../ui";
import AdminPasswordForm from "./AdminPasswordForm";
import PrivilegeManagementForm from "./PrivilegeManagementForm";

const ApplicationSettings = () => {
  return (
    <div className="space-y-6">
      <div className="flex items-center gap-3 mb-2">
        <div className="p-2 bg-primary/10 text-primary rounded-lg">
          <Wrench size={24} />
        </div>
        <h1 className="text-2xl font-bold">Application settings</h1>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <AdminPasswordForm />
        <PrivilegeManagementForm />
      </div>
    </div>
  );
};
export default ApplicationSettings;
