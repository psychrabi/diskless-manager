# Table search, sorting, and notification layering

## Status

Design approved by the user on 2026-10-18. Awaiting spec review before implementation planning.

## Goal

Make the application's user-facing data tables searchable and sortable, and ensure notifications remain visible above open dialogs.

## Existing context

- Most screens use the shared table primitives in `src/components/ui/table.jsx`, but the primitives render markup only and do not own row data.
- The client list uses `react-virtuoso` with custom row and header components.
- Several tables have responsive card layouts. They must show the same filtered and sorted rows as the desktop table.
- Audit logs are fetched as a complete filtered result set and paginated locally in `AuditLogViewer`.
- Toasts are currently mounted inside `AdminLayout` and `PublicLayout` with `z-50`; dialogs portal their overlays and content with the same z-index. The portal stacking context can place dialog layers above the in-layout toast container.

## Agreed behavior

1. Each user-facing data table has its own search field and sort state.
2. Search matches displayed data fields case-insensitively and excludes action controls.
3. Non-action data columns are sortable. Activating a sortable column toggles ascending and descending order; values sort by their data type where applicable (numbers and timestamps), otherwise case-insensitive text.
4. Filtering and sorting apply to the source rows before responsive cards, virtualization, or client-side pagination render them.
5. Each table keeps its current default row order until the user chooses a sort column.
6. Notifications remain in their existing screen position but render in a body-level portal above dialog overlays and content.

## Design

Add a small shared table-data hook that accepts rows, searchable values, and sortable column accessors from each caller. It returns filtered and sorted rows while keeping screen-specific row rendering and action handling in the existing component. Add a shared search control and sortable header control so labels, accessible names, focus behavior, and direction indicators stay consistent. The client list applies the returned rows to `TableVirtuoso` and preserves its virtualized rendering.

Integrate the behavior into these user-facing tables and their responsive equivalents:

- Client Management, including its virtualized table and responsive rows.
- Image Management.
- Disk Management, including mobile cards.
- Audit Logs, including mobile cards and pagination.
- Scheduled Operations, including mobile cards.
- User Management, including mobile cards.
- Pending Recovery.
- Setup dependency results, including mobile cards.
- Windows servicing registry-change results.

The notification container will use React's existing portal support to render under `document.body` and a z-index above the dialog layer. Toast dismissal, lifetime, styling, and bottom-right placement remain unchanged.

## Data and interaction details

- Search is local to each table and does not issue requests or change server-side data.
- Search text remains while the table component remains mounted; it is not persisted across navigation or reloads.
- Existing server/data filters, such as audit-log date and client filters, continue to work. Search and sorting apply to the returned result set. Audit-log pagination is recalculated after search and sorting so pages contain the expected rows.
- Action columns are neither searchable nor sortable. Data columns, including dates, status, counters, IDs, and computed values shown by a table, are searchable; non-action columns are sortable.
- Empty states distinguish an empty source dataset from no rows matching the current search.

## Error handling and accessibility

- Search controls have a visible label or accessible name and a clear affordance.
- Sort controls are keyboard-operable buttons in table headers and expose `aria-sort` on the relevant column header.
- Sorting missing values is deterministic; empty values remain grouped at the end for either direction.
- The toast portal retains alert/status roles and dismissal behavior. A notification displayed while a dialog is open must layer above the dialog without changing dialog focus handling.

## Acceptance criteria

1. Every listed table can find rows by any displayed non-action field and immediately filters its matching desktop/mobile representation.
2. Every listed table can sort each non-action column ascending and descending using the underlying value type, not formatted display text where a raw value is available.
3. Audit-log paging remains correct after filtering or sorting and resets/clamps to a valid page when the result set changes.
4. Client virtualization continues to render only visible rows after filtering/sorting.
5. Empty, loading, and error states retain their existing behavior.
6. Success, error, warning, and info notifications remain visible above dialogs in both admin and public layouts.
7. Existing row actions continue to target the same underlying record after filtering and sorting.

## Scope exclusions

- No server-side search, API changes, persisted table preferences, column visibility controls, multi-column sorting, or bulk actions.
- No change to toast placement or notification lifetime.
- No change to decorative or layout-only HTML tables used for non-data presentation.
