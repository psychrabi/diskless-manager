import { Database, CheckCircle, AlertCircle } from "lucide-react";
import { Button, Card, Input, Select } from "../ui";
import { useForm } from "react-hook-form";

const StorageStep = ({
  disks,
  poolExists,
  poolName,
  onSubmit,
  isSubmitting,
}) => {
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm();

  return (
    <Card
      title="ZFS Storage Configuration"
      icon={Database}
      className="border-t-4 border-primary"
    >
      {poolExists ? (
        <div className="flex flex-col items-center py-8 space-y-6 text-center">
          <div className="w-20 h-20 bg-success/20 text-success rounded-full flex items-center justify-center">
            <CheckCircle size={48} />
          </div>
          <div className="space-y-2">
            <h3 className="text-xl font-bold">Storage is Ready</h3>
            <p className="text-base-content/60">
              ZFS pool &apos;{poolName || "zroot"}&apos; is initialized and
              ready for use.
            </p>
          </div>
        </div>
      ) : (
        <form onSubmit={handleSubmit(onSubmit)} className="space-y-6">
          <div className="grid md:grid-cols-2 gap-6">
            <Select
              label="Select Target Disk"
              register={register("disk", { required: true })}
              subtitle="Caution: All data on this disk will be erased"
              error={errors.disk}
            >
              <option value="">-- Select Disk --</option>
              {disks.map((disk) => (
                <option key={disk.name} value={disk.name}>
                  {disk.name} ({disk.size})
                </option>
              ))}
            </Select>
            <Input
              label="Pool Name"
              register={register("name", { required: true })}
              defaultValue="zroot"
              placeholder="e.g. zboot"
              error={errors.name}
            />
          </div>
          <div className="p-4 bg-warning/10 border border-warning/20 rounded-lg text-warning text-sm flex gap-3">
            <AlertCircle size={20} className="shrink-0" />
            <p>
              Creating a ZFS pool will format the selected disk. Ensure you have
              backups of any important data before proceeding.
            </p>
          </div>
          <Button
            variant="primary"
            className="w-full h-12 text-lg"
            type="submit"
            loading={isSubmitting}
          >
            Initialize Storage Pool
          </Button>
        </form>
      )}
    </Card>
  );
};

export default StorageStep;
