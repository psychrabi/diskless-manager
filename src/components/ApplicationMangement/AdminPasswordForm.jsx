import { useSettings } from "@/hooks/useSettings";
import { zodResolver } from "@hookform/resolvers/zod";
import { LockKeyhole } from "lucide-react";
import { useForm } from "react-hook-form";
import { z } from "zod";
import { Button, Card, Input } from "../ui";

const adminSchema = z.object({
  old_password: z.string().min(1, "Old password is required"),
  new_password: z.string().min(1, "New password is required"),
  confirm_new_password: z.string().min(1, "Confirm new password is required"),
});

export default function AdminPasswordForm() {
  const { updatePassword } = useSettings();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
  } = useForm({
    resolver: zodResolver(adminSchema),
    defaultValues: {
      old_password: "",
      new_password: "",
      confirm_new_password: "",
    },
  });

  const onSubmit = async (data) => {
    await updatePassword(data.old_password, data.new_password);
  };

  return (
    <Card title="Admin password" icon={LockKeyhole}>
      <form onSubmit={handleSubmit(onSubmit)}>
        <div className="space-y-4">
          <Input
            type="password"
            id="old_password"
            label="Old Password"
            register={register("old_password")}
            placeholder="Old Password"
            error={errors.old_password?.message}
          />
          <Input
            type="password"
            id="new_password"
            label="New Password"
            register={register("new_password")}
            placeholder="New Password"
            error={errors.new_password?.message}
          />
          <Input
            type="password"
            id="confirm_new_password"
            label="Confirm New Password"
            register={register("confirm_new_password")}
            placeholder="Confirm New Password"
            error={errors.confirm_new_password?.message}
          />
          <Button variant="primary" type="submit">
            {isSubmitting ? "Updating password" : "Update password"}
          </Button>
        </div>
      </form>
    </Card>
  );
}
