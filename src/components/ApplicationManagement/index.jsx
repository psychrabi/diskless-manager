import { PageHeader } from "@/components/ui";
import AdminPasswordForm from "./AdminPasswordForm";
import PrivilegeManagementForm from "./PrivilegeManagementForm";
import BackupManagement from "./BackupManagement";

const ApplicationSettings = () => {
  return (
    <div className="flex flex-col gap-4">
      <PageHeader
        title="Application Settings"
        description="Set your application settings, admin access, and user accounts."
      />
      <nav aria-label="Application settings sections" className="flex flex-wrap gap-x-4 gap-y-2 border-y border-border py-3 text-sm md:hidden">
        <a className="text-primary underline-offset-4 hover:underline" href="#admin-password">Admin password</a>
        <a className="text-primary underline-offset-4 hover:underline" href="#privileges">Privileges</a>
        <a className="text-primary underline-offset-4 hover:underline" href="#backup">Backup and restore</a>
      </nav>
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <section id="admin-password" className="scroll-mt-4"><AdminPasswordForm /></section>
        <section id="privileges" className="scroll-mt-4"><PrivilegeManagementForm /></section>
      </div>
      <section id="backup" className="scroll-mt-4"><BackupManagement /></section>
    </div>
  );
};
export default ApplicationSettings;
