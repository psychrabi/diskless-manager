import { NativeSelect, NativeSelectOption } from "@/components/ui/native-select";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Filter, X } from "lucide-react";

const AuditLogFilters = ({
  filters,
  clients,
  onFilterChange,
  onClearFilters,
}) => {
  return (
    <div className="rounded-lg border border-border bg-muted/30 p-4">
      <div className="mb-3 flex items-center gap-2">
        <Filter className="size-4" />
        <h3 className="text-sm font-semibold">Filters</h3>
      </div>

      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
        <div className="flex flex-col gap-1.5">
          <Label htmlFor="client-filter" className="text-sm">
            Client
          </Label>
          <NativeSelect
            id="client-filter"
            value={filters.client_id}
            onChange={(e) => onFilterChange("client_id", e.target.value)}
          >
            <NativeSelectOption value="">All Clients</NativeSelectOption>
            {clients.map((client) => (
              <NativeSelectOption key={client.id} value={client.id}>
                {client.name}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="operation-filter" className="text-sm">
            Operation Type
          </Label>
          <NativeSelect
            id="operation-filter"
            value={filters.operation_type}
            onChange={(e) => onFilterChange("operation_type", e.target.value)}
          >
            <NativeSelectOption value="">All Operations</NativeSelectOption>
            <NativeSelectOption value="shutdown">Shutdown</NativeSelectOption>
            <NativeSelectOption value="reboot">Reboot</NativeSelectOption>
            <NativeSelectOption value="remote">Remote Desktop</NativeSelectOption>
          </NativeSelect>
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="start-date" className="text-sm">
            Start Date
          </Label>
          <Input
            id="start-date"
            type="date"
            value={filters.start_date}
            onChange={(e) => onFilterChange("start_date", e.target.value)}
          />
        </div>

        <div className="flex flex-col gap-1.5">
          <Label htmlFor="end-date" className="text-sm">
            End Date
          </Label>
          <Input
            id="end-date"
            type="date"
            value={filters.end_date}
            onChange={(e) => onFilterChange("end_date", e.target.value)}
          />
        </div>
      </div>

      <div className="mt-3 flex justify-end">
        <Button variant="ghost" size="sm" onClick={onClearFilters}>
          <X data-icon="inline-start" />
          Clear Filters
        </Button>
      </div>
    </div>
  );
};

export default AuditLogFilters;
