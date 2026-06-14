

export const Activity = ({ mode, children }) => {
  if (mode === "hidden") {
    return null;
  }
  return <>{children}</>;
};
