import { Code, Eye } from "lucide-react";
import { Button, Card } from "@/components/ui";

function BootScript({ onViewConfig }) {
  return (
    <Card
      title={"Boot Script"}
      subtitle={"Boot menu for PXE clients"}
      icon={Code}
      actions={
          <Button
            icon={Eye}
            variant="info"
            onClick={() => onViewConfig("tftp-autoexec", "Boot Script")}
            title={`View Config for Boot Script`}
          >
            View Config
          </Button>
      }
    />
  );
}

export default BootScript;
