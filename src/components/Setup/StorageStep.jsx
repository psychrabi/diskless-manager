import { NativeSelectOption } from "@/components/ui/native-select";
import { AlertCircle, CheckCircle, Database } from "lucide-react";
import { useForm } from "react-hook-form";
import { Button, Card, Input, Select } from "@/components/ui";

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
          <div className="w-20 h-20 bg-emerald-600/20 text-emerald-600 rounded-full flex items-center justify-center">
            <CheckCircle size={48} />
          </div>
          <div className="space-y-2">
            <h3 className="text-xl font-bold">Storage is Ready</h3>
            <p className="text-muted-foreground">
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
              <NativeSelectOption value="">-- Select Disk --</NativeSelectOption>
              {disks.map((disk) => (
                <NativeSelectOption key={disk} value={disk}>
                  {disk}
                </NativeSelectOption>
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
          <div className="p-4 bg-amber-600/10 border border-amber-600/20 rounded-lg text-amber-600 text-sm flex gap-3">
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
