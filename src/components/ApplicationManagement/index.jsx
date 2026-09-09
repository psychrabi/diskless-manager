import { PageHeader } from "@/components/ui";
import AdminPasswordForm from "./AdminPasswordForm";
import PrivilegeManagementForm from "./PrivilegeManagementForm";

const ApplicationSettings = () => {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Application Settings"
        description="Set your application settings, admin access, and user accounts."
      />
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2 items-stretch">
        <AdminPasswordForm />
        <PrivilegeManagementForm />
      </div>
    </div>
  );
};
export default ApplicationSettings;
