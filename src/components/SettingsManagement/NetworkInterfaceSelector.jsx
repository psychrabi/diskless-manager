import { Label } from "@/components/ui/label";
import { Spinner } from "@/components/ui/spinner";
import { Badge } from "@/components/ui/badge";
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
        <Label className="text-sm font-semibold text-muted-foreground uppercase tracking-tight flex items-center gap-2">
          <Network size={14} /> Network Interfaces
        </Label>
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
      <div className="border border-border rounded-xl bg-muted/30 overflow-hidden">
        <div className="max-h-[200px] overflow-y-auto p-2 space-y-1">
          {loading ? (
            <div className="flex flex-col items-center justify-center py-8 gap-2 opacity-50">
              <Spinner className=""></Spinner>
              <span className="text-xs">Detecting interfaces...</span>
            </div>
          ) : interfaces.length === 0 ? (
            <div className="py-8 text-center text-sm text-destructive/70 italic">
              No active network interfaces detected.
            </div>
          ) : (
            interfaces.map((iface) => {
              const isSelected = selectedInterfaces?.includes(iface);
              return (
                <Label
                  key={iface}
                  className={`flex items-center justify-between p-2 rounded-lg cursor-pointer transition-all border ${
                    isSelected
                      ? "bg-primary/10 border-primary/30 text-primary shadow-sm"
                      : "bg-background border-transparent hover:border-border hover:bg-muted"
                  }`}
                >
                  <div className="flex items-center gap-3">
                    <Checkbox
                      checked={Boolean(isSelected)}
                      onCheckedChange={() => onToggle(iface)}
                    />
                    <span className="font-mono text-sm font-bold">{iface}</span>
                  </div>
                  {isSelected && (
                    <Badge variant="default" className="py-2 px-2 font-bold uppercase tracking-widest text-[10px]">
                      Active
                    </Badge>
                  )}
                </Label>
              );
            })
          )}
        </div>
      </div>
      {errorMessage && (
        <p className="text-xs text-destructive font-medium flex items-center gap-1 mt-1">
          <span>⚠️</span> {errorMessage}
        </p>
      )}
    </div>
  );
};

export default NetworkInterfaceSelector;
import { Checkbox } from "@/components/ui/checkbox";
