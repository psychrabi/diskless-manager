import { Eye } from "lucide-react";
import { Button, Card } from "../ui";

function BootScript({ onViewConfig }) {

  return (
    <Card title={"Boot IPXE Script"} className="flex-1" titleClassName="text-base md:text-lg">
      <div className="flex items-center justify-between">
        <span className="px-2 py-0.5 rounded-full bg-base-300 text-base-content text-xs font-semibold capitalize">
          {'Stopped'}
        </span>
        <div className="flex space-x-1">
          <Button
            onClick={() => {
              onViewConfig('tftp-autoexec', "Boot Script");
            }}
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title={`View Config for Boot IPXE Script`}
          >
            <Eye className="h-4 w-4 text-base-content" />
          </Button>
        </div>
      </div>
    </Card>
  );
}

export default BootScript;