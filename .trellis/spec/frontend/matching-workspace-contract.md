# Matching Workspace Contract

## 1. Scope / Trigger

This contract applies to matching navigation, invoice/order comparison rows, candidate conflict selection, custom order selection, match confirmation, and matched-record edit/delete UX.

## 2. Signatures

Components use only the typed wrappers in `src/lib/api.ts`:

```text
api.getMatchingPage(query): Promise<MatchingPageData>
api.listMatchableOrders(query): Promise<MatchableOrdersPage>
api.confirmInvoiceMatch(request): Promise<ConfirmInvoiceMatchResult>
api.confirmInvoiceMatches(matches): Promise<ConfirmInvoiceMatchResult[]>
api.unconfirmInvoiceMatch(invoiceId): Promise<void>
```

## 3. Contracts

- Matching is a first-level operational workspace. It is invoice-driven and does not add a separate unmatched-order ledger.
- The server computes the global candidate graph before the UI applies invoice search, status filtering, or 50-row pagination.
- The comparison surface uses one vertical scroll container with paired two-column rows and sticky headers so invoice and order content remain aligned.
- Clicking invoice/order content opens the existing detail sheets. Checkbox and candidate controls must not activate a detail sheet.
- Only `suggested` rows are selectable for batch confirmation. Header selection applies only to suggested rows on the current page.
- Conflict candidates are selected explicitly and confirmed one invoice at a time. Candidate-overflow conflicts expose only custom selection.
- The custom picker uses server paging/search and preserves selected order summaries across pages and searches. Occupied or missing-money orders are disabled with a reason.
- A custom response with differences keeps the dialog open and requires a separate “accept differences” action. Any selection change clears the prior difference acceptance state.
- Opening the custom picker for a different invoice resets its page, search, selections, and differences.
- Confirmed rows show their orders and offer unconfirm. They do not offer in-place replacement.
- Detail sheets expose `isMatched`. Before saving changed matching fields they show an unmatch warning; matched delete confirmation explains that the relationship must be unconfirmed first and disables deletion.

## 4. Validation & Error Matrix

| Condition | Frontend behavior |
| --- | --- |
| Page load/reload fails | Preserve last successful rows and show a Sonner error |
| Page becomes invalid | Request the last valid page |
| Automatic suggestion expires | Show the backend error and reload the global graph |
| Batch confirmation fails | Reload; backend guarantees no partial batch |
| Custom selection is empty | Disable confirmation |
| Backend returns differences | Display each difference and require explicit acceptance |
| Invoice is ineligible | Explain money/currency reason and hide custom matching |
| Matched key fields change | Warn before save; successful save refreshes projections |
| Matched delete attempted | Do not invoke deletion from the detail sheet |

## 5. Good/Base/Bad Cases

- Good: checking three suggested rows sends one batch request, then reloads the page.
- Good: the user selects orders across searches, sees the running total/difference, changes selection, and must re-check the new combination.
- Base: an invoice with no automatic candidate opens the custom picker while an ineligible invoice directs the user to edit its data.
- Bad: independently scrolling left/right panes, persisting suggestions in Jotai, clearing cross-page custom selection on every query, or enabling batch confirmation for conflict rows.

## 6. Tests Required

- Component tests for query/paging, suggested-only selection, batch payloads, conflict candidate choice, custom cross-page retention, disabled orders, mismatch acceptance, unconfirm, and stale-data reload.
- Detail-sheet tests for matched badges, key-field warning, non-key edit preservation, and disabled deletion.
- Run `bun run lint`, `bun run test`, and `bun run build`; inspect the comparison surface at desktop and minimum supported window sizes.

## 7. Wrong vs Correct

Wrong: render two independent scroll areas and attempt to synchronize their `scrollTop` values with effects.

Correct: render each invoice and its candidate in one grid row inside one scroll container; alignment and scrolling then share one layout source of truth.
