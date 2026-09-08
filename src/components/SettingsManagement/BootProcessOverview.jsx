import { Check, GitPullRequestArrow } from "lucide-react";
import { useShallow } from "zustand/shallow";
import { useAppStore } from "@/store/useAppStore";
import { Card, StatusBadge } from "@/components/ui";

const bootSteps = [
  { key: "dhcp", label: "DHCP", description: "Client requests IP and boot server info" },
  { key: "tftp", label: "TFTP", description: "Client downloads bootloader and kernel" },
  { key: "iscsi", label: "iSCSI", description: "Client connects to disk image" },
  { key: null, label: "Boot", description: "OS boots from network storage" },
];

export default function BootProcessOverview() {
  const services = useAppStore(
    useShallow((state) => state.services || [])
  );

  const runningServices = new Set(
    services.filter((s) => s.running).map((s) => s.name)
  );

  return (
    <Card
      title="Boot Process Overview"
      icon={GitPullRequestArrow}
      className="bg-background"
    >
      <ol aria-label="Network boot sequence" className="grid gap-4 sm:grid-cols-2 lg:grid-cols-4">
        {bootSteps.map((step, i) => {
          const isRunning = step.key ? runningServices.has(step.key) : true;
          const isComplete = i < bootSteps.length - 1;
          const completed = isComplete && isRunning;

          return (
            <li
              key={step.label}
              className="flex items-start gap-3 rounded-lg border bg-muted/30 p-4"
            >
              <span className={`flex size-8 shrink-0 items-center justify-center rounded-full text-sm font-medium ${completed ? "bg-primary text-primary-foreground" : "bg-muted text-muted-foreground"}`}>
                {completed ? <Check aria-label="Complete" className="size-4" /> : i + 1}
              </span>
              <div className="flex flex-col gap-2">
                <div className="flex items-center gap-2">
                  <span className="font-bold">{step.label}</span>
                  <StatusBadge
                    status={isRunning ? "running" : "stopped"}
                    size="sm"
                    showIcon={false}
                  />
                </div>
                <span className="text-xs text-muted-foreground">
                  {step.description}
                </span>
              </div>
            </li>
          );
        })}
      </ol>
    </Card>
  );
}
