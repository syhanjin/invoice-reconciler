# Order-Invoice Matching Contract

## 1. Scope / Trigger

This contract applies when changing order/invoice matching rules, matching settings, confirmed relationships, candidate enumeration, matching Tauri commands, or edits/deletes of matched records.

## 2. Signatures

```text
get_matching_page(query: MatchingQuery) -> MatchingPage
list_matchable_orders(query: MatchableOrdersQuery) -> MatchableOrdersPage
confirm_invoice_match(request: ConfirmInvoiceMatchRequest) -> ConfirmInvoiceMatchResult
confirm_invoice_matches(request: { matches: ConfirmInvoiceMatchRequest[] }) -> ConfirmInvoiceMatchResult[]
unconfirm_invoice_match(request: { id: string }) -> ()
retry_match_summary(request: { id: string }) -> ()
ledger_match_projection(maxOrdersPerCandidate, maxCandidatesPerInvoice) -> {
  invoices: Map<invoiceId, "unmatched" | "matched" | "confirmed">,
  orders: Map<orderId, "unmatched" | "matched" | "confirmed">
}
```

Durable schema:

```text
invoices 1 -- 0..1 invoice_matches 1 -- 1..* invoice_match_orders * -- 1 orders
```

`invoice_matches.invoice_id` is unique. `invoice_match_orders.order_id` is globally unique so an order cannot belong to two confirmed invoices. `invoice_matches` also stores nullable `display_name`, `summary_error`, provider/model/input-hash/generated-at audit, and `summary_status = pending | processing | succeeded | failed`.

## 3. Contracts

- Candidate combinations are computed from current SQLite data and are never persisted. Only explicit confirmation writes matching tables.
- Ledger match status is a shared projection of the complete candidate graph, never a page-local guess: `confirmed` means a persisted relationship; `matched` means one globally conflict-free automatic candidate; every other unconfirmed record, including ambiguous/conflicting/overflow cases, is `unmatched`. An order is `matched` only when it belongs to that unique candidate.
- Confirmed invoices and orders are excluded from candidate generation. Unconfirming deletes the relationship and releases all member orders.
- Invoices owned by archived projects are excluded from the matching workspace and candidate graph. Their confirmed orders remain durably occupied and are also hidden from the order ledger until unarchive; archiving never deletes or releases a match.
- Only CNY invoices with non-negative `total_fen` and orders with non-negative `paid_amount_fen` participate. Missing or invalid money never means zero and is also forbidden in custom matching.
- Candidate money is exact integer-fen equality. A single order needs no shop name. A multi-order combination requires every normalized shop name to exist and match after trimming, full-width-space conversion, and whitespace collapse.
- Reject an order for date only when both dates exist and its order date is later than the invoice date. A missing date on either side skips the comparison.
- A conflict exists when one invoice has multiple candidates or an order appears in candidates for multiple invoices. Candidate overflow exposes no truncated automatic combination and must use custom matching.
- Before classifying ordinary cross-invoice conflicts, suspected repurchases may disambiguate isolated single-order candidates. Orders group by exact amount plus normalized shop, while invoices group by exact amount plus normalized seller and subtype; unique identifiers, dates, and product summaries do not participate. Groups need at least two records on each side. Equal counts with complete natural dates pair ascending order dates to ascending invoice dates, and every pair must still pass the ordinary date rule. Repeated dates are valid; record IDs provide only a deterministic same-day tie-breaker. Unequal counts, missing dates, or a late chronological pair remain `conflict` with a reason beginning `疑似回购冲突`.
- Automatic candidates are limited by version-1 `settings.toml` `[matching]` values: `max_orders_per_candidate` defaults to `10` and validates to `2..=20`; `max_candidates_per_invoice` defaults to `100` and validates to `10..=500`. A deterministic search-node budget also prevents no-solution exponential traversal; exhausting either limit routes the invoice to custom matching. Missing sections in an older version-1 file receive defaults.
- Single automatic confirmation revalidates that the selected combination still exists; conflict combinations may be selected manually. Batch confirmation additionally requires every item to remain globally conflict-free and commits all items in one transaction.
- Custom matching may override money, date, and shop differences. The first request returns `confirmed: false` plus concrete `differences`; only a repeated request with `acceptMismatches: true` may persist it.
- Single and batch confirmation create `pending` summary state inside the existing confirmation transaction. After commit, a separate single-concurrency queue claims work, re-reads the current global LLM setting, and calls no external service inside the match transaction. Existing successful summaries remain unchanged when the configured model changes.
- Summary input is exactly the invoice seller and every invoice line item's name/spec/unit/quantity/unit-price/amount/tax fields plus every confirmed order's shop and all ordered product names. Internal IDs are excluded from the model payload. The prompt asks for a short Chinese product overview, does not require purpose, and forbids inferring purchase purpose, reimbursement context, project, user, merchant names, money, identifiers, and quantity lists. Output must be non-empty, single-line, and at most 60 Unicode characters.
- `displayName` is backend-owned: use a successful generated summary; otherwise join all non-empty confirmed-order product names in order; otherwise join external order numbers/internal IDs. Matching, project detail, XLSX, and ZIP folder naming must consume that projection rather than reimplement fallback rules.
- Startup migration backfills historical matches as `pending`; startup recovery changes interrupted `processing` work to `pending`. Failures remain `failed` with a bounded error and provider/model audit until `retry_match_summary` explicitly returns them to `pending`; they do not auto-loop.
- Updating an order's amount/date/shop or an invoice's total/date/currency deletes its confirmed relationship in the same transaction. Preserved-match changes to order product names or invoice seller reset summary state to `pending`; the claimed input hash prevents an older response from completing after a reset. Current invoice line items have no edit API, but any future line-item mutation must apply the same reset rule.
- Delete commands return `matching_locked` for confirmed orders or invoices. Bulk delete retains its per-item continuation behavior.
- The backend emits `match-summary-updated` as `{ invoiceId, status }` after durable state changes. It is an invalidation signal; consumers reload matching/project projections instead of deriving state from the event payload.

## 4. Validation & Error Matrix

| Condition | Required result |
| --- | --- |
| Missing/negative money or non-CNY invoice | Exclude from candidates and reject confirmation with `invalid_input` |
| Both dates exist and order is later | Exclude automatic candidate; report a custom difference |
| Either date is missing | Skip date comparison |
| Multi-order shop missing/different | Exclude automatic candidate; report a custom difference |
| Candidate changed since page load | Reject automatic confirmation and require refresh |
| Suspected repurchase counts differ, dates are missing, or a chronological pair is late | Preserve the candidates as `conflict` and return a dedicated `疑似回购冲突` reason |
| Order already belongs to another invoice | Return `matching_locked`; unique index remains the final guard |
| Custom differences not accepted | Return `confirmed: false`; write no rows |
| One batch item fails | Roll back every batch item |
| Delete a confirmed record | Return `matching_locked` before filesystem compensation starts |
| Invoice belongs to an archived project | Omit it from matching candidates/page; preserve any confirmed relationship and order occupancy |
| Automatic graph has ambiguity, conflict, or overflow | Project the unconfirmed invoice/orders as `unmatched`, not `matched` |
| Summary request/protocol/output fails | Keep the match confirmed, mark summary `failed`, expose deterministic fallback, and allow explicit retry |
| App exits while summary is `processing` | Recover it to `pending` after migrations and process it on startup |
| Summary input changes during an in-flight request | Reset to `pending`; reject the stale completion by status/input hash |

## 5. Good/Base/Bad Cases

- Good: two same-shop orders sum exactly to one CNY invoice, both dates are absent, and the combination remains eligible.
- Good: equal-count suspected-repurchase groups with repeated complete dates still produce non-overlapping suggestions; IDs make same-day pairing deterministic without becoming business grouping fields.
- Good: a custom mismatch preview writes nothing; explicit acceptance then stores one relationship transactionally.
- Good: the order and invoice ledgers call the same global candidate projection, so a cross-invoice order conflict is `unmatched` on both pages.
- Good: confirming a match returns immediately; the summary later becomes `succeeded` and replaces the fallback product name without changing the relationship.
- Base: the model is offline, so the match stays confirmed/exportable, the UI shows all product names as `displayName`, and the failed item offers retry.
- Base: candidate enumeration reaches its configured limit and returns a conflict reason with no confirmable truncated combination.
- Bad: persisting suggestions, computing match state from only the current page, treating missing money as zero, enforcing date comparison when one date is absent, confirming batch items one transaction at a time, or relying only on React to prevent order reuse.

## 6. Tests Required

- Pure algorithm tests for one-to-one, same-shop many-to-one, normalization, missing-date behavior, late-order rejection, limit overflow, and deterministic output.
- Repository tests for cross-invoice conflicts, chronological repurchase pairing, repeated-date tie-breaking, missing-date conflicts, custom two-step confirmation, unique occupancy, unconfirm/reuse, batch rollback, edit-driven release, and delete locking.
- Projection tests must assert unique suggestions as `matched`, persisted relationships as `confirmed`, cross-conflicts as `unmatched`, and archived project members absent from matching while their confirmed orders remain occupied.
- Migration tests for fresh files, m0003 upgrade with seeded orders/invoices, interrupted m0004 recovery, cascade behavior, and the unique order constraint.
- Settings tests for defaults, ranges, old version-1 files, atomic replacement, permissions, and credential-free public responses.
- Summary tests for complete model input, no-purpose prompt, Ollama/OpenAI strict schemas, single-line/length validation, migration/backfill/restart recovery, explicit retry, product/seller edit reset, stale-hash discard, deterministic fallback, and invalidation events.

## 7. Wrong vs Correct

Wrong:

```rust
for request in requests {
    database.confirm_invoice_match(request).await?;
}
```

This can leave a partially confirmed batch.

Correct: validate current automatic suggestions, then insert every relationship through one `DatabaseTransaction`; commit only after all items succeed.

Wrong: derive ledger state from whether a record happens to have any candidate on the current page.

Correct: build the complete shared candidate graph once, apply global conflict classification, then map invoice and order IDs to the three ledger states.

Wrong: call the LLM before committing a confirmed match or make summary success an export prerequisite.

Correct: commit `pending` atomically with the match, generate afterward through the durable queue, and always expose a deterministic non-blocking `displayName` fallback.
