import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useForm } from "react-hook-form";
import { describe, expect, it, vi } from "vitest";
import { Input } from "./Input";
import { Select } from "./Select";
import { NativeSelectOption } from "./native-select";
import SambaForm from "../SettingsManagement/Forms/SambaForm";

function StorageForm({ onSubmit }) {
  const { register, handleSubmit, reset } = useForm({
    defaultValues: { name: "boot", disk: "pool-a" },
  });
  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <Input label="Pool name" register={register("name")} maxLength={12} />
      <Select label="Target disk" register={register("disk")} helperText="Choose a pool">
        <NativeSelectOption value="pool-a">Pool A</NativeSelectOption>
        <NativeSelectOption value="pool-b">Pool B</NativeSelectOption>
      </Select>
      <button type="button" onClick={() => reset({ name: "restored", disk: "pool-b" })}>Reset</button>
      <button type="submit">Save</button>
    </form>
  );
}

function SambaSettings({ onSubmit }) {
  const { register, control, handleSubmit, reset } = useForm({
    defaultValues: { enabled: true, guest_ok: true, read_only: false },
  });
  return (
    <form onSubmit={handleSubmit(onSubmit)}>
      <SambaForm register={register} control={control} errors={{}} />
      <button type="button" onClick={() => reset({ enabled: true, guest_ok: false, read_only: true })}>Load settings</button>
      <button type="submit">Save</button>
    </form>
  );
}

describe("shadcn form integration", () => {
  it("preserves registered input and select values across editing and reset", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<StorageForm onSubmit={onSubmit} />);

    const name = screen.getByLabelText("Pool name");
    const disk = screen.getByLabelText("Target disk");
    expect(name).toHaveValue("boot");
    expect(name).toHaveAttribute("maxlength", "12");
    expect(disk).toHaveValue("pool-a");
    expect(disk).toHaveAccessibleDescription("Choose a pool");

    await user.clear(name);
    await user.type(name, "games");
    await user.selectOptions(disk, "pool-b");
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSubmit.mock.calls[0][0]).toEqual({ name: "games", disk: "pool-b" }));

    await user.click(screen.getByRole("button", { name: "Reset" }));
    expect(name).toHaveValue("restored");
    expect(disk).toHaveValue("pool-b");
  });

  it("updates controlled selections when filters are cleared externally", () => {
    const renderFilter = (value) => (
      <Select label="Client" value={value} onChange={() => {}}>
        <NativeSelectOption value="">All clients</NativeSelectOption>
        <NativeSelectOption value="client-1">Client 1</NativeSelectOption>
      </Select>
    );
    const { rerender } = render(renderFilter("client-1"));
    expect(screen.getByLabelText("Client")).toHaveValue("client-1");
    rerender(renderFilter(""));
    expect(screen.getByLabelText("Client")).toHaveValue("");
  });

  it("submits boolean settings and restores checkbox states from loaded configuration", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn();
    render(<SambaSettings onSubmit={onSubmit} />);

    const enabled = screen.getByRole("checkbox", { name: "Samba Server (Start at boot)" });
    const guest = screen.getByRole("checkbox", { name: "Allow guest access" });
    const readOnly = screen.getByRole("checkbox", { name: "Read only" });
    expect(enabled).toBeChecked();
    expect(guest).toBeChecked();
    expect(readOnly).not.toBeChecked();

    await user.click(screen.getByText("Samba Server (Start at boot)"));
    await user.click(guest);
    await user.click(readOnly);
    await user.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(onSubmit.mock.calls[0][0]).toMatchObject({
      enabled: false, guest_ok: false, read_only: true,
    }));

    await user.click(screen.getByRole("button", { name: "Load settings" }));
    expect(enabled).toBeChecked();
    expect(guest).not.toBeChecked();
    expect(readOnly).toBeChecked();
  });
});
