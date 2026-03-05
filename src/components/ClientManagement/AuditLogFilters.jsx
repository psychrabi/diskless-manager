import { Button } from "@/components/ui/Button";
import { Select } from "@/components/ui/Select";
import { Filter, X } from "lucide-react";

const AuditLogFilters = ({
  filters,
  clients,
  onFilterChange,
  onClearFilters,
}) => {
  return (
    <div className="bg-base-200/30 p-4 rounded-lg space-y-3">
      <div className="flex items-center gap-2 mb-3">
        <Filter className="h-4 w-4" />
        <h3 className="font-semibold text-sm">Filters</h3>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-3">
        <Select
          id="client-filter"
          label="Client"
          value={filters.client_id}
          onChange={(e) => onFilterChange("client_id", e.target.value)}
        >
          <option value="">All Clients</option>
          {clients.map((client) => (
            <option key={client.id} value={client.id}>
              {client.name}
            </option>
          ))}
        </Select>

        <Select
          id="operation-filter"
          label="Operation Type"
          value={filters.operation_type}
          onChange={(e) => onFilterChange("operation_type", e.target.value)}
        >
          <option value="">All Operations</option>
          <option value="shutdown">Shutdown</option>
          <option value="reboot">Reboot</option>
          <option value="remote">Remote Desktop</option>
        </Select>

        <div>
          <label htmlFor="start-date" className="form-label">
            Start Date
          </label>
          <input
            id="start-date"
            type="date"
            value={filters.start_date}
            onChange={(e) => onFilterChange("start_date", e.target.value)}
            className="input w-full"
          />
        </div>

        <div>
          <label htmlFor="end-date" className="form-label">
            End Date
          </label>
          <input
            id="end-date"
            type="date"
            value={filters.end_date}
            onChange={(e) => onFilterChange("end_date", e.target.value)}
            className="input w-full"
          />
        </div>
      </div>

      <div className="flex gap-2 justify-end">
        <Button variant="ghost" size="sm" onClick={onClearFilters} icon={X}>
          Clear Filters
        </Button>
      </div>
    </div>
  );
};

export default AuditLogFilters;
