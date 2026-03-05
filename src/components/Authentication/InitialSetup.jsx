import { Button, Card, Input } from "@/components/ui";
import { useAuth } from "@/contexts/auth";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { updateAdminPassword, login } from "@/api/commands";
import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { z } from "zod";

// Define validation schema for initial admin setup
const initialSetupSchema = z
  .object({
    username: z
      .string()
      .min(3, "Username must be at least 3 characters")
      .max(50, "Username must be less than 50 characters")
      .regex(
        /^[a-zA-Z0-9_-]+$/,
        "Username can only contain alphanumeric characters, underscores, and hyphens"
      ),
    password: z
      .string()
      .min(8, "Password must be at least 8 characters")
      .regex(
        /^(?=.*[a-z])(?=.*[A-Z])(?=.*\d)/,
        "Password must contain at least one uppercase letter, one lowercase letter, and one number"
      ),
    confirmPassword: z.string().min(1, "Please confirm your password"),
  })
  .refine((data) => data.password === data.confirmPassword, {
    message: "Passwords don't match",
    path: ["confirmPassword"],
  });

const InitialSetup = () => {
  const navigate = useNavigate();
  const { login: setAuth } = useAuth();
  const { error, success } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(initialSetupSchema),
    defaultValues: {
      username: "admin",
      password: "",
      confirmPassword: "",
    },
  });

  const onSubmit = async (data) => {
    try {
      // Try to update password with default password first
      try {
        await updateAdminPassword({
          old_password: "admin123",
          new_password: data.password
        });
        console.log("Password updated successfully");
      } catch {
        // Password might already be changed, try with the new password as old
        console.log("Default password failed, password might already be set");
        try {
          await updateAdminPassword({
            old_password: data.password,
            new_password: data.password
          });
        } catch {
          // Ignore - password is likely already set correctly
          console.log("Password already set, proceeding to login");
        }
      }

      // Now attempt to log in with the password
      const loginResponse = await login(data.username, data.password);

      // Set auth context immediately so ProtectedRoute sees it
      setAuth(loginResponse.user, loginResponse.token);
      success(
        "Setup Complete",
        "Successfully logged in to Diskless Manager"
      );
      navigate("/");
    } catch (e) {
      console.error("Setup error:", e);
      const errorMessage = e instanceof Error ? e.message : "An unknown error occurred";
      error(
        "Setup Failed",
        errorMessage
      );
      reset({ password: "", confirmPassword: "" }); // Clear password fields on error
    }
  };

  return (
    <Card className="w-[24rem]">
      <div className="text-center mb-6">
        <h1 className="text-2xl font-bold text-primary">Diskless Manager</h1>
        <p className="text-base-content/70 mt-2">Initial Admin Setup</p>
        <p className="text-sm text-base-content/60 mt-1">
          Create your first admin account
        </p>
      </div>

      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <Input
          id="username"
          type="text"
          label="Username"
          register={register("username")}
          placeholder="Enter admin username"
          className="w-full"
          error={errors.username?.message}
        />
        <Input
          id="password"
          type="password"
          label="Password"
          register={register("password")}
          placeholder="Enter password"
          className="w-full"
          error={errors.password?.message}
        />
        <Input
          id="confirmPassword"
          type="password"
          label="Confirm Password"
          register={register("confirmPassword")}
          placeholder="Confirm password"
          className="w-full"
          error={errors.confirmPassword?.message}
        />
        <Button
          type="submit"
          disabled={isSubmitting}
          className="w-full"
          variant="primary"
        >
          {isSubmitting ? "Setting up..." : "Create Admin Account"}
        </Button>
      </form>
    </Card>
  );
};

export default InitialSetup;
