import { RefreshCcw, Network } from "lucide-react";
import { Button } from "@/components/ui";

const NetworkInterfaceSelector = ({
  loading,
  interfaces,
  selectedInterfaces,
  onRefresh,
  onToggle,
  errorMessage,
}) => {
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <label className="text-sm font-semibold text-base-content/70 uppercase tracking-tight flex items-center gap-2">
          <Network size={14} /> Network Interfaces
        </label>
        <Button
          type="button"
          onClick={onRefresh}
          variant="ghost"
          size="xs"
          className="gap-1 opacity-70 hover:opacity-100"
          disabled={loading}
        >
          <RefreshCcw size={12} className={loading ? "animate-spin" : ""} />
          Refresh
        </Button>
      </div>
      <div className="border border-base-300 rounded-xl bg-base-200/30 overflow-hidden">
        <div className="max-h-[200px] overflow-y-auto p-2 space-y-1">
          {loading ? (
            <div className="flex flex-col items-center justify-center py-8 gap-2 opacity-50">
              <span className="loading loading-spinner loading-sm"></span>
              <span className="text-xs">Detecting interfaces...</span>
            </div>
          ) : interfaces.length === 0 ? (
            <div className="py-8 text-center text-sm text-error/70 italic">
              No active network interfaces detected.
            </div>
          ) : (
            interfaces.map((iface) => {
              const isSelected = selectedInterfaces?.includes(iface);
              return (
                <label
                  key={iface}
                  className={`flex items-center justify-between p-2 rounded-lg cursor-pointer transition-all border ${
                    isSelected
                      ? "bg-primary/10 border-primary/30 text-primary shadow-sm"
                      : "bg-base-100 border-transparent hover:border-base-300 hover:bg-base-200"
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <input
                      type="checkbox"
                      className="checkbox checkbox-primary checkbox-sm rounded"
                      checked={isSelected}
                      onChange={() => onToggle(iface)}
                    />
                    <span className="font-mono text-sm font-bold">{iface}</span>
                  </div>
                  {isSelected && (
                    <span className="badge badge-primary badge-xs py-2 px-2 font-bold uppercase tracking-widest text-[10px]">
                      Active
                    </span>
                  )}
                </label>
              );
            })
          )}
        </div>
      </div>
      {errorMessage && (
        <p className="text-xs text-error font-medium flex items-center gap-1 mt-1">
          <span>⚠️</span> {errorMessage}
        </p>
      )}
    </div>
  );
};

export default NetworkInterfaceSelector;
