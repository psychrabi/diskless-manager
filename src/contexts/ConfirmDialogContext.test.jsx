import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ConfirmDialogProvider } from "./ConfirmDialogContext";
import { useConfirm } from "./confirmDialog";

const Trigger = ({ onResult }) => {
  const confirm = useConfirm();

  return (
    <button
      type="button"
      onClick={async () => {
        const answer = await confirm({
          title: "Delete disk",
          description: "This cannot be undone.",
          confirmText: "Delete disk",
          confirmVariant: "destructive",
          showInput: true,
          inputLabel: "Dataset name",
          requireTyping: true,
          typingValue: "tank/win11",
        });
        onResult(answer);
      }}
    >
      Delete
    </button>
  );
};

/**
 * A destructive confirmation asks the operator to retype the object name.
 * The gate has to hold: a wrong or partial value must not resolve the
 * promise that the caller awaits before deleting anything.
 */
it("blocks confirmation until the exact value is typed", async () => {
  const onResult = vi.fn();
  render(
    <ConfirmDialogProvider>
      <Trigger onResult={onResult} />
    </ConfirmDialogProvider>,
  );

  await userEvent.click(screen.getByRole("button", { name: "Delete" }));

  const confirmButton = await screen.findByRole("button", { name: "Delete disk" });
  expect(confirmButton).toBeDisabled();

  const input = screen.getByLabelText("Dataset name");
  await userEvent.type(input, "tank/win");
  expect(confirmButton).toBeDisabled();

  await userEvent.type(input, "11");
  await waitFor(() => expect(confirmButton).toBeEnabled());

  await userEvent.click(confirmButton);
  await waitFor(() => expect(onResult).toHaveBeenCalledWith("tank/win11"));
});

it("resolves false when the operator cancels", async () => {
  const onResult = vi.fn();
  render(
    <ConfirmDialogProvider>
      <Trigger onResult={onResult} />
    </ConfirmDialogProvider>,
  );

  await userEvent.click(screen.getByRole("button", { name: "Delete" }));
  await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));

  await waitFor(() => expect(onResult).toHaveBeenCalledWith(false));
});
