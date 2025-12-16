import { GitPullRequestArrow } from "lucide-react";
import { Card } from "../ui";

export default function BootProcessOverview() {
  return (
    <Card
      title="Boot Process Overview"
      icon={GitPullRequestArrow}
      className="bg-base-100"
    >
      <ul className="steps steps-vertical lg:steps-horizontal w-full">
        <li className="step step-primary">
          <div className="flex flex-col items-center mt-2">
            <span className="font-bold">DHCP</span>
            <span className="text-xs text-base-content/60 text-center max-w-[150px]">
              Client requests IP and boot server info
            </span>
          </div>
        </li>
        <li className="step step-primary">
          <div className="flex flex-col items-center mt-2">
            <span className="font-bold">TFTP</span>
            <span className="text-xs text-base-content/60 text-center max-w-[150px]">
              Client downloads bootloader and kernel
            </span>
          </div>
        </li>
        <li className="step step-primary">
          <div className="flex flex-col items-center mt-2">
            <span className="font-bold">iSCSI</span>
            <span className="text-xs text-base-content/60 text-center max-w-[150px]">
              Client connects to disk image
            </span>
          </div>
        </li>
        <li className="step step-primary">
          <div className="flex flex-col items-center mt-2">
            <span className="font-bold">Boot</span>
            <span className="text-xs text-base-content/60 text-center max-w-[150px]">
              OS boots from network storage
            </span>
          </div>
        </li>
      </ul>
    </Card>
  );
}
