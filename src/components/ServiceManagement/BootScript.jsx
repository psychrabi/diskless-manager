import { Code, Eye } from "lucide-react";
import { Button, Card } from "@/components/ui";

function BootScript({ onViewConfig }) {
  return (
    <Card
      title={"Boot Script"}
      subtitle={"Boot menu for PXE clients"}
      icon={Code}
    >
      <div className="flex items-center justify-between">
        <div className="flex w-full space-x-1">
          <Button
            icon={Eye}
            variant="info"
            className="flex-1"
            onClick={() => onViewConfig("tftp-autoexec", "Boot Script")}
            title={`View Config for Boot Script`}
          >
            View Config
          </Button>
        </div>
      </div>
    </Card>
  );
}

export default BootScript;
