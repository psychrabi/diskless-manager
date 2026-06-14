import { GitPullRequestArrow } from "lucide-react";
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
      className="bg-base-100"
    >
      <ul className="steps steps-vertical lg:steps-horizontal w-full">
        {bootSteps.map((step, i) => {
          const isRunning = step.key ? runningServices.has(step.key) : true;
          const isComplete = i < bootSteps.length - 1;
          const completed = isComplete && isRunning;

          return (
            <li
              key={step.label}
              className={`step ${completed ? "step-primary" : ""}`}
              data-content={completed ? "\u2713" : undefined}
            >
              <div className="flex flex-col items-center mt-2">
                <div className="flex items-center gap-2">
                  <span className="font-bold">{step.label}</span>
                  <StatusBadge
                    status={isRunning ? "running" : "stopped"}
                    size="sm"
                    showIcon={false}
                  />
                </div>
                <span className="text-xs text-base-content/60 text-center max-w-[150px]">
                  {step.description}
                </span>
              </div>
            </li>
          );
        })}
      </ul>
    </Card>
  );
}
