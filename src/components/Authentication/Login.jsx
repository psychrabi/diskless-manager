import { Shield } from "lucide-react";
import { Button, Card, Input } from "@/components/ui";
import { useAuth } from "@/contexts/auth";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { z } from "zod";
import { login } from "@/api/modules/auth";

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
      const response = await login(data.username, data.password);

      setAuth(response.user, response.token);
      success("Authentication", "You have successfully logged in");
      localStorage.removeItem("last_path");
      navigate("/");
    } catch (e) {
      const errorMessage = e instanceof Error ? e.message : "An unknown error occurred";
      error("Login Failed", errorMessage);
      reset({ password: "" });
    }
  };

  return (
    <Card className="w-full max-w-md animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="text-center mb-8">
        <div className="w-16 h-16 bg-primary rounded-2xl flex items-center justify-center mx-auto mb-4 shadow-lg shadow-primary/20">
          <Shield className="h-8 w-8 text-primary-content" />
        </div>
        <h1 className="text-2xl font-bold text-base-content">Diskless Manager</h1>
        <p className="text-base-content/60 mt-1">Sign in to manage your boot server</p>
      </div>

      <form onSubmit={handleSubmit(onSubmit)} className="space-y-5">
        <Input
          id="username"
          type="text"
          label="Username"
          register={register("username")}
          placeholder="Enter your username"
          error={errors.username?.message}
        />
        <Input
          id="password"
          type="password"
          label="Password"
          register={register("password")}
          placeholder="Enter your password"
          error={errors.password?.message}
        />
        <Button
          type="submit"
          disabled={isSubmitting}
          className="w-full"
          variant="primary"
          size="lg"
        >
          {isSubmitting ? "Signing in\u2026" : "Sign in"}
        </Button>
      </form>
    </Card>
  );
};

export default Login;
