# Independent semantic review — Japanese

Reviewer: separate second-review executor run, not the first-pass researcher. Reviewed 2026-11-07 (Asia/Saigon). All inputs and outputs here are synthetic editing tests, not corpus quotations, executed experiments, current technical advice or Coppice behavior. A semantic pass does not validate an API or implementation.

## T01–T16: individual decisions

| Test | Decision on first-pass proposed rewrite | Independent audit and final disposition |
|---|---|---|
| T01 actor switch | Pass with input context | Client sends; server validates request and returns result. Retain the server noun at the switch and the validation-before-return ordering. In real material, `サーバーでは` alone need not identify the executing actor; ask if that is genuinely unresolved. Here the test explicitly presents the server as the second actor. Collapsing the verbs under one omitted subject remains rejected. |
| T02 OR/negation | Pass | Empty OR unequal remains rejection condition. Neither `空で不一致` nor acceptance merely because nonempty is licensed. No assumption about comparison algorithm is added. |
| T03 nested condition/timing | **Revise** | `場合に限り` adds a necessary/only condition not licensed by `場合は` alone. `その後も` can also invite an unstated interval anchor. Final conservative output: **「10秒後にTERMを送ります。まだ終了しておらず、かつ `KillAfter` が指定されている場合は、その秒数の経過後にKILLを送ります。」** Retain unchanged until timing origin is clarified; report that origin is unspecified. Do not silently insert “TERMから”, “起動から”, or a literal 5 seconds. Wrong-signal and omitted-option rejections remain valid. |
| T04 ability/permission | Pass | Input explicitly asserts an API capability; `取得できます` preserves that reading. It does not grant access or prove success in an unmentioned environment. If actual source says only `取得できる`, investigate context rather than choose a sense by editorial fiat. Permission variant in the first pass remains a separate input, not an inferred feature. |
| T05 recommendation | Pass | Preserve exactly recommended three retries and the purpose of reducing connection failures. Recommendation is not obligation or success guarantee. Do not reinterpret three retries as three total attempts. No claim of measured improvement. |
| T06 prerequisite | Pass | Administrator privilege is necessary for this operation, not recommended and not sufficient for every operation. Shortening removes only an empty action phrase. |
| T07 nominalization | Pass | Delete file versus confirm existing backup are different actions. Keep useful `こと` in the proposition being checked. Do not introduce backup creation or assume that a backup is usable. |
| T08 modifier ambiguity | **Hold is correct** | The source does not decide whether administrator created users or the list. Neither interpretation is an approved rewrite. Clarifying a missing fact is not prose editing. This is a successful ambiguity-detection test, not a failed source admission. |
| T09 observed result/uncertainty | Pass | Preserve tested environment, past shorter response time, and possible cache cause. “Cache speeds it up” would invent general causation and a guarantee. |
| T10 aspect/release | **Revise conservatively** | `現在、バックアップを実行しています` can shift a process-state statement toward an implied narrator/operator. Safer final: **「バックアップ処理は現在実行中です。差分保存には、次の版で対応する予定です。」** This preserves process, ongoing state, future version and plan. Neither successful completion nor shipped differential saving is claimed. `保存しています` is not universally wrong Japanese; it is rejected here for ambiguity/changed action, not by a categorical tense rule. |
| T11 OS/default/literal | Pass | Linux remains the platform, omitted `--dry-run` still applies changes. No “safe preview” is licensed. A flag must retain spelling, case and both hyphens. Explicit versus omitted flag cannot be conflated. |
| T12 sequence/cause | Pass | Saved setting precedes disconnect; cause remains uninvestigated. `ため` in the rejected output would assert causation absent from the input. The same actor need not be invented for both events. |
| T13 quantity | Pass within endpoint context | “At most ten” plus explicit nonreturn of excess supports at most ten returned results. Does not assert exactly ten, nonempty results or an exclusive upper bound. In a non-endpoint document, do not add “return” to a bare maximum merely by analogy. |
| T14 logging/blocking | **Revise** | `記録するだけ` can exclude other monitoring behavior beyond the input's nonblocking claim. Final: **「監視モードでは違反を記録します。要求は遮断しません。」** Keep mode scope for both sentences; no assertion that every recorded request is an attack. Nonblocking does not mean safe, unrestricted or zero side effects. |
| T15 register | Pass | In a polite instruction unit, changing `選択する` to `選択します` is stylistic only. Optional deletion remains optional. A nominal UI label is a separate unit and need not get `します`. |
| T16 politeness | Pass only with given context | In ordinary document introduction with no actual permission/benefit relation at issue, `説明します` preserves intent. Do not generalize to every acknowledgement, customer exchange or quotation. Neither excessive politeness nor a blanket ban follows from corpus evidence. |

Summary: 12 proposed pairs pass, T03/T10/T14 need conservative correction, and T08 correctly stays on hold rather than resolving an unknown fact. The first-pass rejected outputs remain rejected, but T10’s reason must be contextual ambiguity, not a universal ban on the phrase. The edited finals above are the approved forms of these tests.

## Standalone reviewed pairs (synthetic inputs)

These restate every original input with the reviewed disposition; the audit above controls context and rejection reasons. R numbers refer to the private research rule IDs, not an implementation contract.

| ID・論点 | 前 → 採用する後 | 不採用例と理由 |
|---|---|---|
| T01 行為者交代 | 「クライアントは要求を送信します。サーバーでは、要求を検証してから結果の返却を行います。」→「クライアントは要求を送信します。サーバーは要求を検証してから結果を返します。」 | 「要求を送信し、検証して結果を返します。」は三動作の行為者を同じに読ませる。R2/R4。 |
| T02 OR・否定 | 「トークンが空である場合、または保存値と一致しない場合は、要求を拒否します。」→「トークンが空、または保存値と不一致の場合は、要求を拒否します。」 | 「空で、保存値と不一致の場合」はOR→AND。「空でなければ受理」は不一致検査を落とす。R8。 |
| T03 入れ子の条件 | 「10秒後にTERMを送ります。まだ終了しておらず、かつ `KillAfter` が指定されている場合は、その秒数の経過後にKILLを送ります。」→**変更せず起点を確認する**。前の二文はそのまま保持し、時間の起点・必要条件は補わない。 | 「10秒後にKILLを送る」はsignalと順序を変える。「終了しない場合は必ずKILL」は設定条件を落とす。R3/R8。 |
| T04 能力≠許可 | 「このAPIには、履歴を取得する機能があります。」→「このAPIで履歴を取得できます。」 | 「履歴を取得してよい」は許可の捏造。入力が「管理者には履歴取得が許可されています」なら「管理者は履歴を取得してよい」が許可として成立するが、実装成功までは保証しない。R6。 |
| T05 推奨≠必須 | 「接続失敗を減らすため、再試行を3回に設定することを推奨します。」→「接続失敗を減らすため、再試行回数には3回を推奨します。」 | 「3回に設定する必要があります」は拘束を追加。「3回なら必ず接続できます」は成功保証を追加。R7。 |
| T06 必須・目的 | 「この操作を行うには管理者権限が必要です。」→「この操作には管理者権限が必要です。」 | 「管理者権限を推奨します」は成立条件を弱める。「管理者ならすべての操作が可能」は適用範囲を拡大。R7/R8。 |
| T07 ことの必要性 | 「ファイルの削除の実行を行います。」→「ファイルを削除します。」／「削除前に、バックアップが存在することを確認します。」→変更しない。 | 後者を「削除前にバックアップします」にすると確認から新規作成へ行為が変わる。「こと」を一律削除しない。R4。 |
| T08 係り先未確定 | 「管理者が作成した利用者の一覧を更新します。」→**保留して確認する**。「管理者が作成した」の対象が利用者か一覧か、原文だけで決めない。 | 勝手に「管理者が作った一覧」を採ると利用者作成の意味を消す。事実確定後にのみ二文へ。R2/R3。 |
| T09 時制・観測 | 「検証環境では応答時間が短くなりました。原因はキャッシュかもしれません。」→「検証環境では、応答時間が短くなりました。キャッシュが原因の可能性があります。」 | 「キャッシュにより高速化します」は環境・過去観測・推量を一般因果へ変える。R5/R6。 |
| T10 相とrelease | 「バックアップ処理は現在実行中です。差分保存は次の版で対応予定です。」→「バックアップ処理は現在実行中です。差分保存には、次の版で対応する予定です。」 | 「バックアップを保存しています」は進行中か保存状態か不明瞭。「差分保存に対応しています」は予定を実装済みにする。R5。 |
| T11 識別子とdefault | 「Linuxでは、`--dry-run` を指定しない場合、変更を適用します。」→「Linuxでは、`--dry-run` を省略すると変更を適用します。」 | 「通常は安全に確認できます」はOS・省略時の破壊的動作を落とす。フラグの翻訳や綴り変更は不可。R9/R12。 |
| T12 因果の新設 | 「設定を保存しました。その後、接続が切れました。原因は未調査です。」→「設定を保存した後、接続が切れました。原因は未調査です。」 | 「設定を保存したため、接続が切れました」は時系列を因果へ強化。R3/R5。 |
| T13 範囲・数量 | 「結果は最大10件です。10件を超える分は返しません。」→「結果を最大10件返します。10件を超える分は返しません。」 | 「10件未満を返す」は10を除く。「10件返す」は不足時にも10件を保証。R8。 |
| T14 報告≠遮断 | 「監視モードでは違反を記録しますが、要求は遮断しません。」→「監視モードでは違反を記録します。要求は遮断しません。」 | 「違反を防ぎます」は機構を変える。「記録された要求は攻撃です」は観測を断定へ。R6/R8。 |
| T15 文体の範囲 | 敬体の手順中「一覧を開きます。対象を選択する。削除は任意です。」→「一覧を開きます。対象を選択します。削除は任意です。」 | 「対象を選び、必ず削除しましょう」は任意を必須へ。「対象を選択」等のUIラベルはこの文末統一の対象外。R1/R7。 |
| T16 丁寧さ | 「ここでは、ログの見方を説明させていただきます。」→「ここでは、ログの見方を説明します。」（通常の文書紹介で実際の許可関係がないという入力条件） | 対人の許可・恩恵が主題の文まで一律に置換するのは不採用。礼儀と冗長の判断は文脈依存。R11。 |


## The eight small tests in the draft skill

1. Actor sequence: pass on the explicit test premise; the reviewed skill makes the synthetic input say `サーバーは` rather than relying on locative `サーバーでは`, so the source actor is explicit; no permission to infer a server actor in genuinely ambiguous documentation. The returned result still follows validation.
2. Empty OR unequal: pass; do not replace OR with AND.
3. API capability: pass; permission is not introduced. No observed execution claimed.
4. Existing backup check: pass unchanged; confirmation is not creation.
5. Tested environment and possible cause: pass; observation stays past and inference stays tentative.
6. Linux omitted dry-run applies changes: pass; identifier and default behavior unchanged.
7. Administrator modifier: correct hold; neither users nor list creation selected without evidence.
8. Three-retry recommendation: pass; do not convert recommendation to obligation or retries to total attempts.

All eight are suitable with those context limits; no source quote or actual product fact is implied.

## Added adversarial tests

These deliberately challenge overbroad applications of the rules. “Keep” is a valid editing outcome; no quota demands a visibly different rewrite.

| ID | Synthetic input and acceptable handling | Adversarial output to reject / reason |
|---|---|---|
| A01 useful こと | `終了したことを確認してから、ファイルを削除します。` Keep. | `終了してから削除します。` drops the verification step; `ファイルを終了します` changes the object/action. |
| A02 legitimate long modifier | `管理者が作成し、監査担当者が承認した利用者の一覧を表示します。` Keep or ask whether both clauses modify users or list; do not split until scope is clear. | `管理者が一覧を作成し、監査担当者が承認します。` invents target and changes display to creation/approval. Length alone does not settle attachment. |
| A03 nominal heading | Heading `認証情報の有効期限` may stay nominal. | `認証情報を失効させます。` converts an attribute heading into an operation and actor implication. |
| A04 local register | Design paragraph `この制約は既存データを守るためである。` followed by a separate instruction panel `移行前にバックアップを確認してください。` Keep both unit-appropriate registers. | Global replacement into a single register, or labeling the mix a grammar error, is unjustified. Do not weaken the pre-migration safety check. |
| A05 omitted versus changed actor | `利用者は要求を送ります。管理者が承認すると、サーバーが処理します。` Keep actors and condition. | `要求を送り、承認して処理します。` collapses three actors and erases the condition. |
| A06 ability ambiguity | `このネットワークでは履歴を取得できません。原因は未調査です。` Keep unknown cause. | `履歴の取得は禁止されています。` invents policy; `ネットワーク障害です` invents diagnosis. |
| A07 non-universal negation | `すべての要求が成功するとは限りません。` Keep. | `すべての要求が失敗します。` swaps not-all for none; `要求は失敗しません` reverses the warning. |
| A08 sufficient/necessary | `Aが有効な場合は、Bを記録します。` Keep as sufficient condition. | `Aが有効な場合に限り、Bを記録します。` asserts B cannot occur otherwise; converse and only-if are not given. |
| A09 uncertainty about necessity | `この環境では、追加の権限が必要だと思われます。` Keep qualification. | `追加権限は必須です。` invents certainty and widens environment scope; `追加権限を推奨します` substitutes preference for a tentative prerequisite. |
| A10 retry/default | `設定を省略した場合は再試行しません。再試行回数には3回を推奨します。` Keep both default and advice. | `既定で3回再試行します。` makes recommendation the default; `合計3回試みます` changes retry counting. |
| A11 release/conditional | `次の版で対応する案がありますが、採用は未定です。` Keep. | `次の版で対応予定です。` advances proposal to plan; `対応しました` advances to shipped. |
| A12 security fail-closed | `sandboxを適用できない場合は、実行を開始しません。` Keep. | `sandboxを適用できなければ、警告して続行します。` invents warning and reverses safety condition. No claim this is Coppice’s actual behavior. |
| A13 literal/platform | `Windowsでは、` followed by literal command `tool --name "A B" --dry-run` must keep literal and platform. | Translating `--name`, changing `A B` to `A_B`, stripping quotes, or substituting `—dry-run` changes contract. Typography normalization never touches code automatically. |
| A14 real acknowledgement | `資料をご提供いただいた皆様に感謝します。` Keep if source supports it; a quotation also stays quoted. | `資料を提供しました。` changes giver to narrator; deleting every polite form loses attribution/thanks. Do not invent permission or thanks to humanize a manual. |
| A15 timing origin | `設定値の秒数が経過したら停止します。` Keep pending clarification of interval start. | `要求を受信してから設定値の秒数後に停止します。` adds unstated origin. A clearer sentence can still be technically false. |
| A16 causal scope | `LinuxではAを有効にした後でBが失敗しました。原因は未調査です。` Keep Linux, sequence and unknown cause. | `AのためBが失敗します。` drops platform/past and invents general causation. |

## How to use this audit

Compare actor, object, release state, condition, default, quantification, causal scope, negation, literal, uncertainty and warning after every edit. Resolve factual ambiguity from project evidence or leave a question. No test licenses inventing features, fixes, benchmarks or observations. For UI/reference and typography, treat the advice as editorial transfer, not corpus-wide linguistic law.
