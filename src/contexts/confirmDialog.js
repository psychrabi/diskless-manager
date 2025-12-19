import { createContext, useContext } from "react";

export const ConfirmDialogContext = createContext();

export const useConfirm = () => {
  const ctx = useContext(ConfirmDialogContext);
  if (!ctx)
    throw new Error("useConfirm must be used within ConfirmDialogProvider");
  return ctx.confirm;
};
