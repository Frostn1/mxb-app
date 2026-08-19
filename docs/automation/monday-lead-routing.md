# Monday — Lead status → group routing

Board **מעקב לידים** (`5053409692`), status column **סטטוס** (`status_mkkswvrf`).

Every status label now routes the lead into a matching group. Rules live as native
monday automations on the board; there is no external dependency.

## Routing map

| Status | Label id | → Group | Group id | Rule |
|---|---|---|---|---|
| ליד חדש | 5 | לידים חדשים | `topics` | pre-existing |
| ליד חוזר | 12 | לידים חדשים | `topics` | pre-existing |
| בטיפול | 6 | בטיפול | `group_mkwnav4y` | pre-existing |
| אין מענה | 11 | בטיפול | `group_mkwnav4y` | pre-existing |
| לא הגיע לשיחה | 10 | בטיפול | `group_mkwnav4y` | added `1718661070` |
| שיחה בוצעה - לא עודכן סטטוס | 8 | בטיפול | `group_mkwnav4y` | added `1718661078` |
| נקבעה שיחה | 0 | קבע שיחה | `group_mkwr2a1d` | pre-existing |
| פולואפ | 3 | פולואפ | `duplicate_of_____________mkksgq2p` | added `1718661190` |
| פולואפ - לא דיברנו איתו | 14 | פולואפ | `duplicate_of_____________mkksgq2p` | added `1718661286` |
| שיחה בוטלה | 9 | פולואפ | `duplicate_of_____________mkksgq2p` | added `1718661059` |
| נסגר | 1 | לידים שנסגרו | `new_group_mkm8fwtq` | pre-existing |
| לא רלוונטי | 2 | לידים לא רלוונטיים | `duplicate_of_________________mkkswc8p` | pre-existing |
| לא מעוניין | 7 | לידים לא רלוונטיים | `duplicate_of_________________mkkswc8p` | added `1718661056` |
| לא נסגר | 4 | לא נסגרו | `group_mkwz89x7` | pre-existing |
| רשימת המתנה אוגוסט | 13 | *(deliberately unrouted)* | — | — |

`רשימת המתנה אוגוסט` is a seasonal label; it stays wherever the lead already is.

## Backfill (2026-08-19)

954 leads audited, 812 already correct, **132 moved** into the right group.
Post-run verification: 0 mismatches.

| Group | Before | After |
|---|---|---|
| לידים חדשים | 317 | 301 |
| בטיפול | 44 | 63 |
| קבע שיחה | 109 | 22 |
| פולואפ | 11 | 61 |
| לידים שנסגרו | 143 | 143 |
| לידים לא רלוונטיים | 140 | 176 |
| לא נסגרו | 190 | 188 |

## Known cleanup

Automation `1718661176` was created in error — its trigger resolved to
"when status changes **from** פולואפ **to** פולואפ", which can never fire. It is inert
but should be deleted by a board owner (the API token used here has create-only
rights on automations and cannot remove it). The working replacement is `1718661190`.

## Note on n8n

Routing is intentionally native to monday rather than n8n: monday already owned 8 of
the rules, and keeping all 15 in one place avoids splitting the logic across systems
and removes n8n uptime from the path of every status change.
