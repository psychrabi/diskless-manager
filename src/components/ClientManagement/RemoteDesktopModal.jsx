import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useToastStore } from "@/store/useToastStore";
import { remoteDesktopClient } from "@/api/modules/control";

const remoteDesktopSchema = z.object({
  username: z.string().min(1, "Username is required"),
  password: z.string().min(1, "Password is required"),
});

const RemoteDesktopModal = ({ client, isOpen, onClose, onSuccess }) => {
  const { success, error: showError } = useToastStore();
  const [isLoading, setIsLoading] = useState(false);

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm({
    resolver: zodResolver(remoteDesktopSchema),
    defaultValues: {
      username: "Administrator",
      password: "1",
    },
  });

  const onSubmit = async (data) => {
    setIsLoading(true);
    try {
      const response = await remoteDesktopClient(client.id, {
        username: data.username,
        password: data.password,
      });

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

  if (!isOpen) return null;

  return (
    <div className="modal modal-open">
      <div className="modal-box w-full max-w-md">
        <h3 className="font-bold text-lg mb-4">Remote Desktop Credentials</h3>

        <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
          <div>
            <label className="label">
              <span className="label-text">Client: {client?.name}</span>
            </label>
          </div>

          <div>
            <label className="label">
              <span className="label-text">Username</span>
            </label>
            <input
              type="text"
              placeholder="Administrator"
              className="input input-bordered w-full"
              {...register("username")}
            />
            {errors.username && (
              <span className="text-error text-sm">{errors.username.message}</span>
            )}
          </div>

          <div>
            <label className="label">
              <span className="label-text">Password</span>
            </label>
            <input
              type="password"
              placeholder="Enter password"
              className="input input-bordered w-full"
              {...register("password")}
            />
            {errors.password && (
              <span className="text-error text-sm">{errors.password.message}</span>
            )}
          </div>

          <div className="modal-action">
            <button
              type="button"
              className="btn btn-ghost"
              onClick={handleClose}
              disabled={isLoading}
            >
              Cancel
            </button>
            <button
              type="submit"
              className="btn btn-primary"
              disabled={isLoading}
            >
              {isLoading ? (
                <>
                  <span className="loading loading-spinner loading-sm"></span>
                  Connecting...
                </>
              ) : (
                "Connect"
              )}
            </button>
          </div>
        </form>
      </div>

      <div className="modal-backdrop" onClick={handleClose} />
    </div>
  );
};

export default RemoteDesktopModal;
