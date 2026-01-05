import { Button, Card, Input } from "@/components/ui";
import { useAuth } from "@/contexts/auth";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { z } from "zod";
import * as api from "@/api/commands";

// Define validation schema
const loginSchema = z.object({
  username: z.string().min(1, "Username is required"),
  password: z.string().min(6, "Password must be at least 6 characters"),
});

const Login = () => {
  const navigate = useNavigate();
  const { login: setAuth } = useAuth();
  const { error, success } = useToastStore();

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(loginSchema),
    defaultValues: {
      username: "",
      password: "",
    },
  });

  const onSubmit = async (data) => {
    try {
      const response = await api.login(data.username, data.password);

      // Set auth context immediately so ProtectedRoute sees it
      // For now, we'll simulate a user object since the API returns just the token
      setAuth({ username: data.username }, response.token);
      success("Login Successful", "You have successfully logged in");
      navigate("/");
    } catch (e) {
      error("Login Failed", e.message || "An unknown error occurred");
      reset({ password: "" }); // Clear password field on error
    }
  };

  return (
    <Card className="w-[24rem]">
      <div className="text-center mb-6">
        <h1 className="text-2xl font-bold text-primary">Diskless Manager</h1>
        <p className="text-base-content/70 mt-2">Sign in to Diskless Manager</p>
      </div>

      <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
        <Input
          id="username"
          type="text"
          label="Username"
          register={register("username")}
          placeholder="Enter your username"
          className="w-full"
          error={errors.username?.message}
        />
        <Input
          id="password"
          type="password"
          label="Password"
          register={register("password")}
          placeholder="Enter your password"
          className="w-full"
          error={errors.password?.message}
        />
        <Button
          type="submit"
          disabled={isSubmitting}
          className="w-full"
          variant="primary"
        >
          {isSubmitting ? "Signing in..." : "Sign in"}
        </Button>
      </form>
    </Card>
  );
};

export default Login;
