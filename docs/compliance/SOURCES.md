# IRD Compliance Sources

This register records the official IRD sources used for the Nepal SME Billing compliance baseline.

| Source | Current reference | Purpose |
|---|---|---|
| Electronic Invoice Procedure 2074 (fourth amendment) | IRD PDF, `1958456687.pdf` | Electronic billing software/device standards, listing, data integrity, audit/logging, backup/recovery, numbering, invoice and sales-book formats, hosted/server conditions |
| CBMS API Technical Document for Software Developers | IRD publication dated 16 Jestha 2080 | CBMS developer integration reference |
| CBMS billing guidance | IRD PDF, `1589285873.pdf` / later copy `1773880257.pdf` | Current published integration guidance, endpoints, credentials, response handling |
| Electronic invoice notice | IRD notice dated 4 Baisakh 2083 | Mandatory electronic invoicing + CBMS for taxpayers with annual turnover above NPR 20 crore, subject to stated exclusions |
| Enlisted electronic invoicing software list | IRD notice dated 22 Ashoj 2082 | Confirms IRD publishes a list of software enlisted under Electronic Invoice Procedure 2074 |
| VAT Act / Rules tax invoice requirements | IRD VAT materials including current Rules/translated guidance | Invoice structure and tax-document requirements |
| IRD FAQ | Current IRD FAQ | PAN display, computer billing approval/listing, fiscal-year invoice sequence, VAT records, sales/purchase records, abbreviated tax invoice guidance |
| HS code tax invoice FAQ | IRD publication dated 24 Shrawan 2081 | HS-code requirements for imported goods and tax invoices |

## Important version-control policy

Regulatory requirements can change. Before each production compliance release, re-check the official IRD website and replace/annotate this source register with the current version/date of every applicable document.

## Primary URLs

- https://ird.gov.np/category/electronic-invoice/
- https://ird.gov.np/content/9052/cbmsapitechnicaldocumentfor/
- https://ird.gov.np/content/13488/notice-regarding-electrical-appliances/
- https://ird.gov.np/content/9368/notice-17599213073/
- https://ird.gov.np/faq/

## Engineering interpretation rule

The software must distinguish:

1. Requirements that apply to ordinary taxpayer/PAN billing and records.
2. Requirements that apply to VAT-registered taxpayers.
3. Requirements that apply to electronic invoicing under the Electronic Invoice Procedure 2074.
4. Requirements that apply to taxpayers currently mandated to connect electronic invoices to CBMS.

Never assume a single rule applies to every SME.
