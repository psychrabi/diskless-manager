import { useContext } from "react";
import { MetricsContext } from "./metricsContext";

export const useMetrics = () => {
  const context = useContext(MetricsContext);
  if (!context) {
    throw new Error("useMetrics must be used within MetricsProvider");
  }
  return context;
};
