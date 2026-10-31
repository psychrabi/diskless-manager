import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { zodResolver } from "@hookform/resolvers/zod";
import { useForm } from "react-hook-form";
import { describe, expect, it, vi } from "vitest";
import { storagePrewarmSchema } from "@/schema";
import StoragePrewarmForm from "./StoragePrewarmForm";

const GIB = 1024 ** 3;

const defaults = {
  spare_prewarm: {
    enabled: false,
    max_ready_spares: 2,
    ttl_hours: 24,
    min_free_bytes: 50 * GIB,
  },
};

function Harness({ onSave }) {
  const {
    control,
    register,
    handleSubmit,
    formState: { errors },
  } = useForm({
    resolver: zodResolver(storagePrewarmSchema),
    defaultValues: defaults,
  });
  return (
    <form onSubmit={handleSubmit(onSave)}>
      <StoragePrewarmForm
        control={control}
        register={register}
        errors={errors}
        config={defaults}
      />
      <button type="submit">Save</button>
    </form>
  );
}

describe("StoragePrewarmForm", () => {
  it("submits prewarming disabled by default", async () => {
    const onSave = vi.fn();
    render(<Harness onSave={onSave} />);

    expect(
      screen.getByRole("checkbox", { name: /prewarm replacement writebacks/i })
    ).not.toBeChecked();

    await userEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onSave.mock.calls[0][0]).toEqual({
      spare_prewarm: {
        enabled: false,
        max_ready_spares: 2,
        ttl_hours: 24,
        min_free_bytes: 50 * GIB,
      },
    });
  });

  it("edits limits and converts the free-space floor from GiB to bytes", async () => {
    const onSave = vi.fn();
    render(<Harness onSave={onSave} />);
    const user = userEvent.setup();

    await user.click(
      screen.getByRole("checkbox", { name: /prewarm replacement writebacks/i })
    );
    const spares = screen.getByLabelText("Maximum ready spares");
    await user.clear(spares);
    await user.type(spares, "4");

    const free = screen.getByLabelText("Minimum free pool space (GiB)");
    expect(free).toHaveValue(50);
    await user.clear(free);
    await user.type(free, "10");

    await user.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(onSave).toHaveBeenCalledTimes(1));
    expect(onSave.mock.calls[0][0]).toEqual({
      spare_prewarm: {
        enabled: true,
        max_ready_spares: 4,
        ttl_hours: 24,
        min_free_bytes: 10 * GIB,
      },
    });
  });

  it("refuses to submit a TTL outside the server bounds", async () => {
    const onSave = vi.fn();
    render(<Harness onSave={onSave} />);
    const user = userEvent.setup();

    // The input bounds mirror SparePrewarmConfig::MAX_TTL_HOURS, so the
    // browser blocks the submit before the resolver runs.
    const ttl = screen.getByLabelText("Discard unused spares after (hours)");
    expect(ttl).toHaveAttribute("max", "168");
    await user.clear(ttl);
    await user.type(ttl, "500");
    expect(ttl).toBeInvalid();

    await user.click(screen.getByRole("button", { name: "Save" }));
    expect(onSave).not.toHaveBeenCalled();
  });
});
