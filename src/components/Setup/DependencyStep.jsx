import { Check, Package, RefreshCw } from "lucide-react";
import {
  Button,
  Card,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui";

const DependencyStep = ({
  dependencies,
  checking,
  onRefresh,
  onInstall,
  installing,
}) => {
  return (
    <Card
      title="System Dependencies"
      subtitle="The following packages are required for the system to function
          correctly."
      icon={Package}
      className="border-t-4 border-primary overflow-hidden"
      actions={
        <Button
          variant="ghost"
          size="icon"
          onClick={onRefresh}
          disabled={checking}
        >
          <RefreshCw className={checking ? "animate-spin" : ""} size={16} />
        </Button>
      }
    >

      <div className="border rounded-xl overflow-hidden bg-base-200/50 backdrop-blur-sm">
        <Table>
          <TableHeader>
            <TableRow className="bg-base-300/50">
              <TableHead>Package</TableHead>
              <TableHead>Version</TableHead>
              <TableHead className="text-center">Status</TableHead>
              <TableHead className="text-right">Action</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {dependencies.map((svc) => (
              <TableRow
                key={svc.name}
                className="hover:bg-base-300/20 transition-colors"
              >
                <TableCell className="font-medium">{svc.name}</TableCell>
                <TableCell className="font-mono text-xs">
                  {svc.version || "---"}
                </TableCell>
                <TableCell className="text-center">
                  {svc.installed ? (
                    <span className="badge badge-success badge-sm gap-1">
                      <Check size={12} /> Installed
                    </span>
                  ) : (
                    <span className="badge badge-warning badge-sm gap-1">
                      Missing
                    </span>
                  )}
                </TableCell>
                <TableCell className="text-right">
                  {!svc.installed && (
                    <Button
                      variant="success"
                      size="xs"
                      loading={installing === svc.name}
                      onClick={() => onInstall(svc.name)}
                    >
                      Install
                    </Button>
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

    </Card>
  );
};

export default DependencyStep;
