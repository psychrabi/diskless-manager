import { LoginForm } from "@/components/login-form";
import { useAuth } from "@/contexts/auth";
import { useToastStore } from "@/store/useToastStore";
import Brand from "@/components/layouts/Brand";
import { zodResolver } from "@hookform/resolvers/zod";
import { checkAdminExists, login } from "@/api/modules/auth";
import { CheckCircle2, LockKeyhole } from "lucide-react";
import { useEffect, useState } from "react";
import { useForm } from "react-hook-form";
import { useNavigate } from "react-router-dom";
import { z } from "zod";

const loginSchema = z.object({
  username: z.string().min(1, "Username is required"),
  password: z.string().min(6, "Password must be at least 6 characters"),
});

const bootLines = [
  { label: "PXE", detail: "DHCP lease acquired", meta: "192.168.1.121" },
  { label: "iSCSI", detail: "Writeback attached", meta: "LUN 0" },
  { label: "Windows", detail: "Image verified", meta: "00:01:42" },
];

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
    <div className="grid min-h-svh lg:grid-cols-[1.06fr_0.94fr]">
      <section className="relative hidden overflow-hidden bg-[oklch(0.155_0.018_230)] p-10 text-white lg:flex lg:flex-col lg:justify-between xl:p-14">
        <div className="hero-grid pointer-events-none absolute inset-0 opacity-70" aria-hidden="true" />
        <div className="pointer-events-none absolute -top-24 -left-28 size-[460px] rounded-full bg-primary/15 blur-3xl" aria-hidden="true" />
        <div className="relative">
          <Brand subtitle="Boot server control" titleClassName="text-white" subtitleClassName="text-white/45" />
        </div>

        <div className="relative max-w-xl py-10">
          <span className="inline-flex items-center gap-2 rounded-full border border-white/10 bg-white/[0.045] px-3 py-1 text-[11px] font-medium tracking-[0.08em] uppercase text-white/60">
            <span className="size-1.5 rounded-full bg-primary" />
            Diskless boot control plane
          </span>
          <h2 className="mt-6 text-[clamp(2.5rem,4.4vw,4rem)] leading-[1.02] font-semibold tracking-[-0.055em]">
            One image.
            <br />
            Every machine.
            <br />
            <span className="text-teal-300">Zero local disks.</span>
          </h2>
          <p className="mt-5 max-w-md text-sm leading-6 text-white/55">
            Provision clients, stream their storage, and recover a machine without touching a single local drive.
          </p>

          <div className="mt-8 max-w-md overflow-hidden rounded-2xl border border-white/10 bg-white/[0.035] shadow-2xl shadow-black/20 backdrop-blur-sm">
            <div className="flex items-center justify-between border-b border-white/8 px-4 py-3">
              <div className="flex items-center gap-1.5" aria-hidden="true">
                <span className="size-1.5 rounded-full bg-white/20" />
                <span className="size-1.5 rounded-full bg-white/20" />
                <span className="size-1.5 rounded-full bg-primary/70" />
              </div>
              <span className="font-mono-data text-[10px] tracking-[0.14em] text-white/55 uppercase">
                boot sequence
              </span>
            </div>
            <div className="divide-y divide-white/6 px-4">
              {bootLines.map((line) => (
                <div key={line.label} className="flex items-center gap-3 py-3 font-mono-data text-[11px]">
                  <CheckCircle2 className="size-3.5 shrink-0 text-teal-300" aria-hidden="true" />
                  <span className="w-14 shrink-0 text-white/55">{line.label}</span>
                  <span className="min-w-0 flex-1 truncate text-white/80">{line.detail}</span>
                  <span className="shrink-0 text-white/55">{line.meta}</span>
                </div>
              ))}
            </div>
          </div>
        </div>

        <div className="relative flex gap-10 text-[11px] text-white/40">
          <div>
            <p className="font-mono-data text-[12px] font-medium text-white/80">PXE</p>
            <p className="mt-1">network boot</p>
          </div>
          <div>
            <p className="font-mono-data text-[12px] font-medium text-white/80">iSCSI</p>
            <p className="mt-1">block storage</p>
          </div>
          <div>
            <p className="font-mono-data text-[12px] font-medium text-white/80">ZFS</p>
            <p className="mt-1">copy-on-write</p>
          </div>
        </div>
      </section>

      <section className="flex min-h-svh items-center justify-center px-5 py-14 sm:px-10">
        <div className="w-full max-w-[400px]">
          <div className="mb-9 lg:hidden">
            <Brand subtitle="Boot server control" />
          </div>
          <LoginForm
            className="border-0 bg-transparent p-0 shadow-none ring-0"
            onSubmit={handleSubmit(onSubmit)}
            register={register}
            errors={errors}
            isSubmitting={isSubmitting}
            canBootstrap={adminExists === false}
            onCreateAdmin={() => navigate("/initial-setup")}
          />
          <p className="mt-8 flex items-center gap-2 text-[11px] leading-5 text-muted-foreground">
            <LockKeyhole className="size-3.5 shrink-0" aria-hidden="true" />
            Server-side authentication. Sign-in is required for every management action.
          </p>
        </div>
      </section>
    </div>
  );
};

export default Login;
