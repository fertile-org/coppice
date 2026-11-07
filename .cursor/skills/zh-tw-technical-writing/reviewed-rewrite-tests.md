# Second-review canonical tests

Original analysis unchanged. T04 precision, T07 completion and T11 explicit causality corrected. See [independent-review.md](independent-review.md) for judgments and additional A01–A08.

## 13. Meaning-preserving rewrite tests (all invented, generic—not Coppice behavior)

These are fresh exercises, not rewrites of copyrighted passages or descriptions of actual features. The “acceptable” column changes expression, not truth conditions. A reviewer should attempt to falsify equivalence.

| Test | Deliberately clumsy baseline | Acceptable revision | Reject / invariant |
|---|---|---|---|
| T01 no implicit action | 當設定檔不存在的情況發生時，工具不會進行資料夾建立，也不會進行連線。 | 設定檔不存在時，工具不會建立資料夾，也不會連線。 | “找不到設定檔時” can include permission errors; do not broaden nonexistent into unfindable. Both negated actions survive. |
| T02 platform + default | 在 Linux 平台上，此選項在預設的情況之下為停用。 | 在 Linux 上，此選項預設停用。 | Not “所有平台預設停用”; not “建議停用”. |
| T03 possibility | 如果伺服器仍在處理中，回應可能會延遲。 | 伺服器仍在處理時，回應可能延遲。 | Not “會延遲”, “一定失敗”, or invented timeout value. |
| T04 actor/access | 瀏覽器會阻止網頁程式對這個回應內容進行讀取，但請求已經送出。 | 瀏覽器會阻止網頁程式讀取這個回應內容，但請求已送出。 | Not “伺服器不接受請求” or “請求未送出”. No generic claim about every CORS branch. |
| T05 interval | 此範例允許的版本大於或等於 1.4，且小於 2.0。 | 此範例允許 `>= 1.4` 且 `< 2.0` 的版本。 | Equality at 2.0 remains excluded. Keep conjunction, not “1.4 到 2.0” ambiguity. |
| T06 necessary branch | 工具僅在內容有變更時才寫入檔案。 | 只有內容有變更時，工具才會寫入檔案。 | Not “每次變更都會寫入”; necessary condition is not made sufficient. |
| T07 uncertain remedy | 重新啟動可能可以排除此問題，但目前尚未完成確認。 | 重新啟動可能排除此問題，但目前尚未確認完成。 | Not “重新啟動即可修復”. Uncertainty and verification status survive. |
| T08 operation identity | 本函式會回傳新的陣列，不會對原陣列進行修改。 | 本函式會回傳新陣列，不會修改原陣列。 | Not “更新陣列”; return/new/nonmutation all survive. |
| T09 irreversible consequence | 此操作會刪除 `/tmp/demo-cache` 內的所有檔案，無法復原。 | 此操作會刪除 `/tmp/demo-cache` 內的所有檔案，且無法復原。 | No shortened “清理快取即可”; exact path, all files and irreversible warning retained. Generic example only. |
| T10 evidence scope | 在我這次使用版本 1.6 的測試中，沒有觀察到重試。 | 這次使用 1.6 版測試時，我沒有觀察到重試。 | Not “1.6 版不會重試”. Observer/version/test scope remain. |
| T11 UI hypothetical | 因為目前帳號沒有寫入權限，這個檔案尚未被更新。 | 因為目前帳號沒有寫入權限，這個檔案尚未更新。 | Not “請重新登入即可”; no invented fix, no success claim. This is UI extrapolation. |
| T12 temporal checkpoint | 驗證成功之後，工具才會傳送這筆資料。 | 只有驗證成功後，工具才會傳送這筆資料。 | Not “傳送後進行驗證”; do not claim success always causes transmission. |

