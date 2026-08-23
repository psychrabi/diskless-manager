# IRD Compliance Baseline — Nepal SME Billing

> Research baseline for engineering. This document is not legal advice and does not establish that the software is IRD-enlisted or approved.

## 1. Scope

The application is intended for small and medium businesses in Nepal, including retail shops, restaurants, cafes, hotels, wholesale/distribution businesses, and service businesses.

The product must support ordinary PAN/taxpayer billing and accounting workflows for SMEs that are not currently subject to mandatory CBMS integration, while also providing an architecture that can support electronic invoicing and CBMS when applicable.

## 2. Important distinction: ordinary billing vs electronic invoicing/CBMS

IRD currently states that taxpayers with annual turnover above NPR 20 crore, excluding the categories stated in the notice, must issue invoices electronically and connect to CBMS at the time of issuing the invoice. The current notice is dated 4 Baisakh 2083.

This threshold must be treated as a regulatory rule that can change. It must not be hard-coded into the entire product architecture.

Electronic invoicing also has a separate regulatory framework. The Electronic Invoice Procedure 2074 requires electronic invoicing devices/software to satisfy the procedure's standards and states that only software/equipment listed by the Department may be used for electronic invoice issuance.

## 3. PAN requirements

The software must maintain the business taxpayer's PAN and use it consistently on relevant transaction, billing, accounting, banking, and tax records.

Business setup must therefore include:

- PAN
- Legal/business name
- Registered address
- Branches/locations
- Tax registration status
- VAT registration status
- Fiscal year

Customer/supplier PAN must be supported where applicable.

## 4. VAT invoice requirements

For VAT-registered businesses, the invoice subsystem must support the applicable tax-invoice format and data.

The electronic-invoice procedure's front-end invoice specification includes:

- Bill/invoice number
- Transaction date
- Invoice issue date
- Seller PAN
- Seller name
- Seller address
- Purchaser name
- Purchaser address
- Purchaser PAN
- Payment method
- Line-item description
- Quantity
- Unit price
- Line total
- Discount percentage
- Taxable amount
- VAT amount
- Total amount
- Amount in words
- Authorized signature

The system must also support the applicable non-VAT/PAN invoice form for businesses registered only for income tax.

## 5. VAT tax logic

The tax engine must not assume that every business is VAT-registered.

Business tax status must at minimum distinguish:

- PAN/income-tax only
- VAT registered
- Other applicable registration states as later required

For VAT businesses, the tax engine must support:

- Taxable sales
- Non-taxable/exempt treatment where legally applicable
- Input tax records
- Output tax records
- Discounts
- Returns
- Credit notes
- Tax-period reporting
- Purchase/sales register data

Current IRD guidance also requires records supporting VAT transactions, including issued and received tax invoices/abbreviated tax invoices, import/export documents, price-change evidence, debit/credit notes, VAT accounts, and purchase/sales accounts.

## 6. Sales and purchase records

The product must maintain structured sales and purchase records suitable for the required tax records.

For VAT businesses, support at least:

- Sales register
- Purchase register
- Customer/supplier details
- Invoice date
- Invoice number
- PAN where applicable
- Taxable sales
- Non-taxable/exempt sales where applicable
- Export sales where applicable
- Discount
- Tax amount
- Total amount
- Debit notes
- Credit notes
- Returns

Stock records should also be maintained for inventory businesses.

## 7. Invoice numbering

Invoice numbering must be controlled and auditable.

The IRD electronic billing procedure requires sequential invoice numbering for each fiscal year and a reportable structure for that sequence.

Application behavior:

- Invoice numbers must be sequential within the applicable series.
- A new fiscal year must start a new configured sequence according to the applicable IRD-approved numbering policy.
- The system must prevent silent renumbering.
- Deleted invoices must not be used as a mechanism to remove financial history.
- Voids/cancellations must remain auditable.

## 8. Immutability and audit trail

This is a core compliance requirement for the electronic billing path.

The electronic invoice procedure requires:

- Database-backed software
- The ability to process SQL queries at a basic level
- Data already entered into the database must not be deletable in a way that destroys the record
- Automatic logging/archiving of database activity
- Backup and recovery capability
- Each record to carry entry date/time and user identity information

Application design:

- Financial records are append-oriented.
- Corrections use reversal, void, return, debit-note, or credit-note workflows rather than destructive updates.
- Audit events are immutable.
- Every important financial mutation records timestamp, user, device/terminal where relevant, and reason/context.

## 9. Backup and recovery

The system must support:

- Local database backup
- Verified restore
- Scheduled backups
- Export of compliance/audit data
- Protection against accidental deletion
- Recovery testing

For hosted deployments, tenant isolation and backup policy must be explicit.

## 10. Fiscal-year handling

Fiscal year is a first-class domain object.

All financial documents and reports must be linked to a fiscal year.

The system must support fiscal-year close/open controls and prevent accidental cross-year invoice numbering.

## 11. Electronic billing software listing/enlistment

The Electronic Invoice Procedure 2074 states that persons issuing invoices electronically must use electronic devices/software that meet the prescribed standards and are listed by IRD.

The product must therefore maintain a dedicated regulatory release checklist before any production electronic-invoice deployment is marketed as compliant.

Required engineering documentation should include:

- Software name/version
- Front-end technology
- Back-end technology
- Database architecture
- System architecture
- Requirement document
- Design document
- User manual
- Data-modification integrity method
- Backup/recovery design
- Server/location design where applicable
- Terminal/device details where applicable

The procedure's application materials also reference PAN registration, company/firm registration, tax clearance, user manual, specifications, and documentation concerning integrity of data modifications.

## 12. Hosted/server deployment requirements

Where the electronic billing procedure applies to hosted/server-based operation, the architecture must be able to support the procedure's requirements, including:

- Server/operator registered in Nepal where required by the procedure
- Server located in Nepal where required
- Department/office access where required
- Multi-tenancy isolation
- Contractual/data-transfer arrangements among software producer/distributor, server operator, and invoice-issuing taxpayer where required

The final production deployment model must be validated against the exact current IRD procedure before enrolment/application.

## 13. CBMS integration

IRD's developer documentation and current billing guidance define CBMS endpoints for billing integration.

Current documented endpoints include:

- Sales invoice: `https://cbapi.ird.gov.np/api/bill`
- Sales return / credit note: `https://cbapi.ird.gov.np/api/billreturn`

The guidance states that SellerPAN is the taxpayer PAN and the API credentials use the taxpayer login credentials. A successful submission returns response code 200, and synchronized sales can be checked in the CBMS External Portal's Sales Register Sync report.

The application must therefore provide:

- CBMS configuration
- Secure credential storage
- Bill submission
- Sales return/credit-note submission
- Request/response logging
- Retry queue
- Idempotency protection
- Submission status
- Failure diagnostics
- Reconciliation against CBMS status

CBMS submission must be decoupled from the ability to create an offline local invoice.

## 14. Offline-first compliance architecture

A POS transaction must be persistable locally even if the Internet is unavailable, subject to the rules applicable to the taxpayer's mandated submission mode.

Architecture:

```text
Local Invoice
    -> Immutable Local Record
    -> Print/Receipt
    -> Compliance Queue
    -> CBMS Submission
    -> Accepted / Failed / Retry / Reconcile
```

No network outage should cause a silent loss of the local transaction record.

## 15. Returns, cancellations, debit notes, credit notes

The domain model must represent corrections as explicit documents rather than destructive updates.

Required capabilities:

- Sales return
- Purchase return
- Invoice cancellation/void subject to applicable rule
- Credit note
- Debit note
- Link correction documents to the original invoice
- Record reason/user/date/time
- Preserve original financial history
- Support CBMS return/credit-note submission where applicable

## 16. Abbreviated tax invoices / retail billing

Retail businesses may use abbreviated tax invoices only where the applicable IRD permission/rules allow it.

The system should therefore implement invoice-type configuration and enforce the relevant limits/rules instead of treating abbreviated invoices as universally available.

Current IRD guidance states that abbreviated tax invoices are for permitted retail sales, have specific required handling, and carry a limit under the cited rules; the exact threshold must be sourced from the current legal/procedural version used for production.

## 17. HS code support

For VAT-registered taxpayers dealing with imported goods, the invoice system must support HS code information as required by the current IRD rules/guidance.

Product master should therefore support:

- HS code
- Country of origin where relevant
- Product description
- Unit
- Brand
- Model
- Size

The product should not force HS codes for every domestic product unless the current rule requires them for that transaction type.

## 18. Foreign-currency transactions

Where a taxable supply is settled in convertible foreign currency, the invoice engine must be able to convert the transaction to NPR using the applicable Nepal Rastra Bank exchange-rate basis required by the VAT rules.

The original currency and converted NPR amounts should both be retained for auditability.

## 19. Tax-period and reporting support

The reporting engine must be able to produce the structured data required for:

- Sales reports
- Purchase reports
- VAT reports
- Tax-period transaction summaries
- Customer/supplier ledgers
- Stock reports
- Audit/export reports
- CBMS reconciliation reports

The application should not claim that it replaces filing responsibilities unless the required filing integration is explicitly implemented and verified.

## 20. Compliance status model

Every business should have a compliance profile:

```text
Tax profile
- PAN
- VAT status
- Fiscal year
- Invoice mode
- Electronic billing eligibility/requirement
- CBMS enabled
- CBMS connection status
- Last successful sync
- Pending submissions
- Failed submissions
```

## 21. Compliance release gates

Before production electronic invoicing:

- [ ] Verify current IRD Electronic Invoice Procedure version
- [ ] Verify current VAT Act/Rules invoice format
- [ ] Verify current IRD notices affecting target taxpayer class
- [ ] Verify current CBMS API contract
- [ ] Verify current CBMS authentication method
- [ ] Verify current electronic-invoice software listing/enlistment process
- [ ] Complete required software documentation
- [ ] Complete required organization/company/PAN documentation
- [ ] Complete IRD listing/enlistment/approval process as applicable
- [ ] Validate invoice samples
- [ ] Validate sales/purchase registers
- [ ] Validate correction documents
- [ ] Validate numbering and fiscal-year transitions
- [ ] Validate audit log and data immutability
- [ ] Validate backup/restore
- [ ] Validate CBMS submissions and reconciliation

## 22. What the product must NOT claim yet

Until the applicable IRD process is completed, the product must not claim:

- IRD approved
- IRD enlisted
- CBMS certified
- Government certified

The correct product wording during development is:

> Designed to support applicable Nepal IRD billing and electronic invoicing requirements.

## 23. Regulatory source baseline

See `docs/compliance/SOURCES.md` for the official source register and version/date tracking.
