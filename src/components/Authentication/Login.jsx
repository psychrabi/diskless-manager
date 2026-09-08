import { LoginForm } from "@/components/login-form";
import { useAuth } from "@/contexts/auth";
import { useToastStore } from "@/store/useToastStore";
import { zodResolver } from "@hookform/resolvers/zod";
import { checkAdminExists, login } from "@/api/modules/auth";
import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { z } from "zod";

const loginSchema = z.object({
  username: z.string().min(1, "Username is required"),
  password: z.string().min(6, "Password must be at least 6 characters"),
});

const Login = () => {
  const navigate = useNavigate();
  const { login: setAuth } = useAuth();
  const { error, success } = useToastStore();
  const [adminExists, setAdminExists] = useState(null);

  useEffect(() => {
    let cancelled = false;
    checkAdminExists()
      .then((response) => {
        if (!cancelled) {
          setAdminExists(response.exists ?? response.admin_exists ?? false);
        }
      })
      .catch(() => {
        if (!cancelled) setAdminExists(true);
      });
    return () => { cancelled = true; };
  }, []);

  const {
    register,
    handleSubmit,
    formState: { errors, isSubmitting },
    reset,
  } = useForm({
    resolver: zodResolver(loginSchema),
    defaultValues: { username: "", password: "" },
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
    <LoginForm
      onSubmit={handleSubmit(onSubmit)}
      register={register}
      errors={errors}
      isSubmitting={isSubmitting}
      canBootstrap={adminExists === false}
      onCreateAdmin={() => navigate("/initial-setup")}
    />
  );
};

export default Login;
