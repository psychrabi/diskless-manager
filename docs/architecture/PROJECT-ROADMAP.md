# Nepal SME Billing — Build Roadmap

## Product definition

A Nepal-focused SME operating system combining POS, billing, inventory, purchasing, customers/suppliers, payments, reporting, and a regulatory compliance layer. Retail, restaurant/cafe, hotel, wholesale, and service workflows sit on top of the common core.

## Architecture

```text
Tauri Desktop / Web Clients
        |
        v
Application API / Domain Services
        |
   +----+----+--------------------+
   |         |                    |
   v         v                    v
Billing   Inventory            Accounting
Engine    Engine               Engine
   |         |                    |
   +---------+--------------------+
             |
             v
        Compliance Engine
        |      |       |
        |      |       +--> Audit Log
        |      +----------> Tax/Invoice Rules
        +-----------------> CBMS Adapter
             |
             v
       Local SQLite / PostgreSQL
             |
             v
        Sync/Backup Layer
```

## Core domain modules

- Business
- Branch
- Terminal/device
- User/role/permission
- Customer
- Supplier
- Product/category/unit
- Price list
- Tax profile
- Invoice
- Invoice line
- Payment
- Sales return
- Credit note
- Debit note
- Purchase
- Purchase return
- Inventory movement
- Stock count/adjustment
- Expense
- Cash drawer/account
- Fiscal year
- Audit event
- Compliance submission
- Compliance response

## Invoice lifecycle

```text
Draft
  -> Validated
  -> Issued
  -> Printed / Delivered
  -> Compliance Queue
  -> Submitted (when applicable)
  -> Accepted / Failed
  -> Reconciled
```

Corrections are represented as explicit business documents. The original issued invoice is never silently destroyed.

## Offline-first requirements

- POS can create an invoice without a live connection when the applicable operating mode permits it.
- Local database is the source of truth for the terminal's unsynchronized transactions.
- Every compliance submission has a durable queue record.
- Retry must be safe and idempotent.
- Sync conflicts must not overwrite financial records.
- Backup and restore must be testable.

## Compliance isolation

All IRD/CBMS behavior must be behind an adapter boundary:

```text
ComplianceService
  |
  +-- TaxRulesProvider
  +-- InvoiceComplianceValidator
  +-- ElectronicInvoiceFormatter
  +-- CBMSClient
  +-- SubmissionQueue
  +-- ReconciliationService
```

This prevents a future IRD procedure/API change from contaminating the core POS domain.

## Initial implementation stages

### Stage 0 — Requirements and regulatory baseline

- [x] Capture official IRD source register
- [x] Document compliance baseline
- [ ] Verify every production rule against current source before release
- [ ] Build requirements traceability matrix

### Stage 1 — Repository/application skeleton

- [ ] Cargo workspace
- [ ] Tauri desktop shell
- [ ] React frontend
- [ ] Shared TypeScript domain types
- [ ] SQLite migrations
- [ ] Error/result model
- [ ] Logging
- [ ] Configuration

### Stage 2 — Business core

- [ ] Business/branch setup
- [ ] Fiscal years
- [ ] Users/roles
- [ ] Products/categories/units
- [ ] Customers/suppliers

### Stage 3 — Sales/POS

- [ ] Cart
- [ ] Pricing
- [ ] Discounts
- [ ] Tax calculation
- [ ] Payment capture
- [ ] Invoice creation
- [ ] Receipt printing
- [ ] Returns/corrections

### Stage 4 — Inventory and purchasing

- [ ] Purchase orders
- [ ] Purchases
- [ ] Stock ledger
- [ ] Stock adjustments
- [ ] Stock counts
- [ ] Supplier returns
- [ ] Batch/expiry support
- [ ] HS code/product metadata support

### Stage 5 — Compliance

- [ ] Tax profile engine
- [ ] Invoice-format validator
- [ ] Audit log
- [ ] Immutable financial data model
- [ ] Sales/purchase register reports
- [ ] Backup/recovery workflows
- [ ] Electronic invoice format

### Stage 6 — CBMS

- [ ] Credential management
- [ ] Bill endpoint integration
- [ ] Bill-return endpoint integration
- [ ] Queue/retry
- [ ] Status reconciliation
- [ ] CBMS reporting

### Stage 7 — Restaurant

- [ ] Tables
- [ ] KOT
- [ ] Kitchen display
- [ ] Waiters
- [ ] Recipes/ingredients
- [ ] Table transfer/merge

### Stage 8 — Hotel

- [ ] Rooms
- [ ] Room types
- [ ] Reservations
- [ ] Guests
- [ ] Folios
- [ ] Room charges
- [ ] Night audit

### Stage 9 — Cloud/multi-branch

- [ ] PostgreSQL service
- [ ] Tenant isolation
- [ ] Synchronization
- [ ] Cloud backup
- [ ] Owner reporting
- [ ] Multi-branch inventory

## Quality gates

Each compliance-sensitive feature must have:

- Domain tests
- Database migration tests
- Tax calculation tests
- Fiscal-year tests
- Audit-log tests
- Serialization/API contract tests
- Failure/retry tests where network integration exists
- Regression fixtures derived from approved invoice examples

## Regulatory release gate

No release may claim IRD approval/enlistment/CBMS certification until the applicable IRD process has actually been completed and documented.
