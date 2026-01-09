import { useSettings } from "@/hooks/useSettings";
import { useAppStore } from "@/store/useAppStore";
import { useToastStore } from "@/store/useToastStore";
import { File } from "lucide-react";
import { useState } from "react";
import { useForm } from "react-hook-form";
import { Button, Card } from "@/components/ui";

export default function LicenseActivation() {
  const { activateLicense } = useSettings();
  const licenseInfo = useAppStore((state) => state.licenseInfo);
  const fetchLicenseInfo = useAppStore((state) => state.fetchLicenseInfo);
  const { error, info } = useToastStore();

  const { register, handleSubmit, reset } = useForm({
    defaultValues: { license_key: "" },
    values: { license_key: licenseInfo?.license_key ?? "" },
  });
  const [loading, setLoading] = useState(false);

  const onSubmit = async (data) => {
    if (!data.license_key || !data.license_key.trim()) {
      error("License Key Required: Please enter a license key");
      return;
    }
    setLoading(true);
    const success = await activateLicense(data.license_key.trim());
    if (success) {
      reset();
      await fetchLicenseInfo();
    }
    setLoading(false);
  };

  return (
    <Card title="License Activation" icon={File}>
      <form onSubmit={handleSubmit(onSubmit)} className="space-y-3">
        <label className="block">
          <span className="label-text">License</span>
          <input
            type="text"
            {...register("license_key")}
            placeholder="Enter license key"
            className="input input-bordered w-full mt-1"
            readOnly={!!licenseInfo?.license_key} // make read-only if already activated
          />
        </label>

        <div className="flex items-center gap-2">
          <Button
            type="submit"
            variant="primary"
            disabled={licenseInfo?.license_key || loading}
            loading={loading}
          >
            {loading ? "Activating…" : "Activate"}
          </Button>
          <Button
            type="button"
            variant="secondary"
            onClick={() => {
              reset();
              info("License activation form has been reset.");
            }}
            disabled={loading}
          >
            Reset
          </Button>
        </div>
      </form>
    </Card>
  );
}
