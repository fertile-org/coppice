# Reviewed rewrite tests

Manual proposition-by-proposition evaluation, not automated product tests. These are hypothetical language fixtures, **not Coppice capability assertions**. PASS means a rewrite preserves the stated fixture; it does not verify that the fixture describes the product. HOLD is a valid expected outcome for ambiguity or missing facts. Changing an erroneous fact requires a separately identified correction, not a silent “style” rewrite.

## Original eight tests — independently re-evaluated

### T01 — FAIL original proposed rewrite; corrected
Original: 「該軟體將會在您的檔案被上傳之後進行數據的存儲。」
A proposed: 「上載檔案後，軟件會儲存這些資料。」
Failure: loses ownership of **your** file, encourages an implied reader-uploader, and assumes the unspecified data are “these records”. No basis for the claimed record/measurement distinction. The stored data are not stated to be the file itself.
Conservative style-only candidate: **「你的檔案上載後，軟件會儲存數據。」**
Check: file ownership retained; uploader remains unspecified; software is storage actor; after-upload sequence and future storage retained; no invented records/file-content relation. PASS on supplied proposition.

### T02 — HOLD; original proposal invents modality
Original: 「你必須可以設定代理。」
A proposed: 「如有需要，你可以設定代理伺服器。」
Failure: turns a possible obligation to be capable into optional permission; invents a condition and narrows 代理 to a proxy server (could be an agent).
Expected: **request source clarification / retain ambiguity flagged**. If the fact is capability requirement: 「你必須能夠設定代理。」 If permission: 「你可以設定代理。」 These are alternatives, not interchangeable approved outputs. Do not choose one without evidence. This ambiguity gate PASSES; a single automatic rewrite FAILS.

### T03 — factual correction, not meaning-preserving rewrite
Original: 「此功能支援所有平台。」 Premise added: only macOS was tested.
A proposed: 「此功能已在 macOS 測試；Linux 及 Windows 尚未測試。」
Assessment: rejects the original all-platform assertion and substitutes test coverage. That is appropriate caution given the premise, but not unchanged meaning. Testing does not itself establish full support even on macOS.
Fact-repair candidate: **「此功能只曾在 macOS 測試；其他平台的支援情況尚未確認。」**
Check: testing and support remain distinct; no invented supported/unsupported platform list. PASS as explicitly labelled fact repair; FAIL as style-only equivalence.

### T04 — FAIL fabricated UI/status; corrected
Original: 「按下按鈕即可完成同步啦！」
A proposed: 「按「同步」。完成後，狀態列會顯示「已同步」。」
Failure: unknown button label, unknown status bar, unknown completion indicator. Calling labels “placeholders” does not excuse inventing the lifecycle/output.
Style-only candidate: **「按下按鈕即可完成同步。」**
Check: button and action remain generic, no new status. PASS for semantics. The original completion claim still needs product verification before actual documentation; do not infer an asynchronous polling flow either.

### T05 — FAIL invented purpose and weakened recommendation; corrected
Original: 「預設值是關閉，你應該開啟它。」 Premise: no reason provided.
A proposed: 「此選項預設為關閉。如需在離線時保留記錄，可開啟此選項。」
Failure: adds offline-record retention, new condition and capability; changes recommendation to optional possibility.
Candidate: **「此選項預設為關閉。建議開啟。」**
Check: same default and recommendation, no new reason. PASS. Mark the missing rationale as an editorial query, not a fabricated feature.

### T06 — conditional PASS / referent clarification
Original: 「run 完 command 之後 check 下個 log」
A proposed: 「執行指令後，檢查記錄檔（log）。」
Assessment: sequence retained, but log could mean stream, entry, file or literal identifier. Cantonese 「check 下個」 normally means check the log, not necessarily “the next log”; clarify if the source uses 下個 literally.
Conservative candidate: **「執行指令後，檢查記錄。」**
If a file is established, 記錄檔 is appropriate; if `log` is an exact identifier, keep `log` verbatim, not as an optional translated gloss. PASS with the referent established; otherwise HOLD the narrowing.

### T07 — FAIL invented sandbox specification; HOLD for facts
Original: 「沙盒令你的電腦完全安全。」
A proposed: 「沙盒會限制代理可存取的路徑；它不能阻止所有風險。」
Failure: changes a total safety claim into a limited guarantee and invents path-based agent access control. Both may sound sensible; neither is supplied product evidence.
Expected: **「原文的全面安全保證未經證實；需要核實隔離機制、限制及剩餘風險後再寫。」** This is an editorial hold, not final user-facing product copy. PASS for preventing unsupported claims, not for equivalence. No fabricated Coppice sandbox features.

### T08 — partial factual correction; remove invented future claim
Original: 「新版本已推出。」 Premise: it is a preview.
A proposed: 「預覽版現已提供；正式版推出前，功能及設定仍可能改變。」
Failure: first clause usefully supplies known lifecycle information; second adds a product-specific change/roadmap possibility absent from the premise. Generic plausibility is not evidence.
Fact-repair candidate: **「預覽版已推出。」**
Check: released state preserved, preview fact added explicitly from premise, no stable-release promise or settings-change claim. PASS as labelled lifecycle clarification, not strict semantic identity.

## Twelve adversarial fixtures

For each, reject the tempting rewrite even if it sounds more fluent. The safe version must satisfy the stated invariants.

| ID / focus | Source fixture | Safe candidate / expected action | Tempting FAIL and semantic oracle |
|---|---|---|---|
| A01 actors/ownership | 伺服器接收用戶端上載的檔案後，會通知檔案擁有人。 | 用戶端上載的檔案送達後，伺服器會通知檔案擁有人。 | 「上載後，你會通知用戶端」 changes receiver/notifier/owner. Server receives then notifies owner; client and owner need not be the same person. |
| A02 ability ≠ permission | 系統能匯出報告，但訪客不得執行匯出。 | 系統支援匯出報告，但不允許訪客執行這項操作。 | 「訪客無法匯出」 may erase the difference between policy denial and technical inability. Capability belongs to system; prohibition to guests. |
| A03 recommendation ≠ obligation | 建議啟用驗證，但現有連線不必重新建立。 | 建議啟用驗證；現有連線無須重新建立。 | 「必須啟用並重新連線」 upgrades advice and reverses a negation. No forced reconnection. |
| A04 uncertainty | 記錄可能因網絡中斷而延遲；原因尚未確認。 | 網絡中斷可能令記錄延遲，但原因尚未確認。 | 「網絡中斷導致記錄延遲」 makes the suspected cause certain. Both possibility and unconfirmed status must survive. |
| A05 only-if and causality | 只有金鑰有效時才會開始傳送；有效不保證傳送成功。 | 傳送只會在金鑰有效時開始；金鑰有效也不保證傳送成功。 | 「金鑰有效便會成功傳送」 treats necessary as sufficient and start as successful completion. |
| A06 negation scope | 不是所有失敗的工作都會重試。 | 並非每項失敗的工作都會重試。 | 「所有失敗工作都不會重試」 changes not-all into none. Do not infer which failures retry. |
| A07 default/platform scope | 只在 Linux 上預設啟用；Windows 可手動啟用。 | 只有 Linux 預設啟用這項設定；Windows 則可手動啟用。 | 「所有平台預設啟用」 removes only-Linux default; 「Windows 預設停用」 invents an exact default not expressly specified. Only default-enable scope and Windows manual capability are established. |
| A08 literals | 在 `/tmp/a b.json` 執行 `tool --dry-run=false --retries=0`；鍵名是 `userID`。 | 原樣保留三個字面值；如無執行目錄／檔案之別的資料，另行釐清「在」的含義。 | `userId`, `--dry-run=true`, `--retries=3`, unquoted path expansion or localisation of tokens FAIL. No inference that 0 means default, unlimited, or disabled. |
| A09 lifecycle/time | 這是預覽版，將於下月供受邀帳戶測試。 | 這個預覽版將於下月開放予受邀帳戶測試。 | 「正式版現已向所有人推出」 changes phase, tense and audience. Month remains relative unless an anchored date is supplied. |
| A10 safety/lifecycle | 刪除後無法復原。先確認備份成功，再刪除原檔。 | 原檔刪除後無法復原。請先確認備份成功，才刪除原檔。 | 「備份後可安全刪除，稍後亦可還原」 invents restoration/safety. Successful verification precedes deletion; irreversibility survives. |
| A11 conjunctive scope | 如服務已停止且沒有待處理工作，可移除暫存檔。 | 只有在服務已停止、而且沒有待處理工作時，才可移除暫存檔。 **HOLD this candidate:** it strengthens a sufficient condition into a necessary condition. Safer:「若服務已停止，而且沒有待處理工作，就可移除暫存檔。」 | 「服務停止或沒有工作便可移除」 changes AND to OR. The safer sentence retains the conditional permission without silently asserting exclusivity. |
| A12 obligation/exception | 管理員必須保留稽核記錄至少 30 日；測試帳戶不適用。 | 除測試帳戶外，管理員必須保留稽核記錄至少 30 日。 | 「用戶可保留記錄 30 日」 changes actor, duty, lower bound and exception. Clarify if the exception refers to account records rather than administrator accounts; do not narrow it unasked. |

## Review result
All original 8 evaluated; all 12 adversarial cases manually checked. T02/T07 deliberately end in HOLD; T03/T08 explicitly change facts using supplied premises; T06 remains referent-dependent. A08/A12 preserve or flag ambiguity rather than invent its resolution; A11 illustrates that adding 只有…才 can itself strengthen the original. This is a semantic regression suite, not evidence of actual product tests or completed source-count gates.


## Final corpus repair: applicability unchanged

The final positive corpus contains exactly 30 substantive sources, four qualified reserves and one additional originality hold (HK62). Those counts do not change the semantic oracles above. The newly read sources reinforce these checks: HK64 distinguishes application identity from user permission; HK59 keeps missing-data/threshold/time scope; HK67 distinguishes personal impressions from confirmed capability; HK50 demonstrates why prose/code numeric ranges must be checked rather than “smoothed”. No source example is asserted to be a Coppice feature.

The 8 original + 12 adversarial fixtures remain manual language tests, not execution of the historical code, a product security audit, or automated repository tests.
