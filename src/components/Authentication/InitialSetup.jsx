import { SignupForm } from "@/components/signup-form";
import { useAuth } from "@/contexts/auth";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { bootstrapAdmin, login } from "@/api/modules/auth";
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
      await bootstrapAdmin(data.username, data.password);

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
    <SignupForm
      onSubmit={handleSubmit(onSubmit)}
      register={register}
      errors={errors}
      isSubmitting={isSubmitting}
      onLogin={() => navigate("/login")}
    />
  );
};

export default InitialSetup;
