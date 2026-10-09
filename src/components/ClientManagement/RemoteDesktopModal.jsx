import { useEffect, useState } from "react";
import { useAppStore } from "@/store/useAppStore";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useToastStore } from "@/store/useToastStore";
import { remoteDesktopClient } from "@/api/modules/control";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Field,
  FieldContent,
  FieldError,
  FieldLabel,
} from "@/components/ui/field";
import { Spinner } from "@/components/ui/spinner";

const remoteDesktopSchema = z.object({
  username: z.string().min(1, "Username is required"),
  password: z.string().min(1, "Password is required"),
  port: z.coerce.number().int().min(1).max(65535),
});

const RemoteDesktopModal = ({ client, isOpen, onClose, onSuccess }) => {
  const { success, error: showError } = useToastStore();
  const [isLoading, setIsLoading] = useState(false);
  const imageOs = useAppStore((state) => state.masters.find((image) => image.name === client?.master || image.id === client?.master)?.os_type);
  const defaultUsername = (imageOs ? imageOs === "windows" : /windows/i.test(client?.master || "")) ? "Administrator" : "diskless";

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    resolver: zodResolver(remoteDesktopSchema),
    defaultValues: {
      username: defaultUsername,
      password: "",
      port: 3389,
    },
  });

  useEffect(() => {
    if (isOpen) reset({ username: defaultUsername, password: "", port: 3389 });
  }, [isOpen, client?.id, defaultUsername, reset]);

  const onSubmit = async (data) => {
    setIsLoading(true);
    try {
      const response = await remoteDesktopClient(client.id, {
        username: data.username,
        password: data.password,
        port: data.port,
      });
      if (response?.success === false) throw new Error(response.message);

      success(
        "Remote Desktop",
        response?.message || "Remote desktop connection initiated"
      );

      reset();
      onClose();

      if (onSuccess) {
        onSuccess();
      }
    } catch (error) {
      showError(
        "Remote Desktop",
        `Failed to connect: ${error.message || String(error)}`
      );
    } finally {
      setIsLoading(false);
    }
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) handleClose();
      }}
    >
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle>Remote Desktop Credentials</DialogTitle>
          <DialogDescription>Connect to {client?.name} using RDP. Remote Desktop must be enabled on the client.</DialogDescription>
        </DialogHeader>

        <form onSubmit={handleSubmit(onSubmit)} className="flex flex-col gap-4">
          <Field>
            <FieldLabel htmlFor="rd-username">Username</FieldLabel>
            <FieldContent>
              <Input
                id="rd-username"
                type="text"
                placeholder={defaultUsername}
                aria-invalid={!!errors.username}
                {...register("username")}
              />
              <FieldError>{errors.username?.message}</FieldError>
            </FieldContent>
          </Field>

          <Field>
            <FieldLabel htmlFor="rd-password">Password</FieldLabel>
            <FieldContent>
              <Input
                id="rd-password"
                type="password"
                placeholder="Enter password"
                aria-invalid={!!errors.password}
                {...register("password")}
              />
              <FieldError>{errors.password?.message}</FieldError>
            </FieldContent>
          </Field>

          <Field data-invalid={!!errors.port}>
            <FieldLabel htmlFor="rd-port">RDP port</FieldLabel>
            <FieldContent>
              <Input id="rd-port" type="number" min={1} max={65535} aria-invalid={!!errors.port} {...register("port")} />
              <FieldError>{errors.port?.message}</FieldError>
            </FieldContent>
          </Field>

          <DialogFooter className="pt-2">
            <Button
              type="button"
              variant="ghost"
              onClick={handleClose}
              disabled={isLoading}
            >
              Cancel
            </Button>
            <Button type="submit" disabled={isLoading}>
              {isLoading && <Spinner data-icon="inline-start" />}
              {isLoading ? "Connecting…" : "Connect"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
};

export default RemoteDesktopModal;
