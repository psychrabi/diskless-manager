import { Badge } from "@/components/ui/badge";
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
          title="Refresh dependencies"
          onClick={onRefresh}
          disabled={checking}
        >
          <RefreshCw className={checking ? "animate-spin" : ""} size={16} />
        </Button>
      }
    >

      <div className="border rounded-xl overflow-hidden bg-muted/50 backdrop-blur-sm">
        <Table>
          <TableHeader>
            <TableRow className="bg-muted/50">
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
                className="hover:bg-muted/20 transition-colors"
              >
                <TableCell className="font-medium">{svc.name}</TableCell>
                <TableCell className=" text-xs">
                  {svc.version || "---"}
                </TableCell>
                <TableCell className="text-center">
                  {svc.installed ? (
                    <Badge variant="outline" className="gap-1 border-emerald-600/30 text-emerald-600">
                      <Check size={12} /> Installed
                    </Badge>
                  ) : (
                    <Badge variant="outline" className="gap-1 border-amber-600/30 text-amber-600">
                      Missing
                    </Badge>
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
