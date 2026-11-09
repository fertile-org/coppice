# Vietnamese review evidence

Record of the independent second pass for [SKILL.md](SKILL.md). Citations and dates are in [repo-references.md](repo-references.md) and [repo-reference-manifest.json](repo-reference-manifest.json). This file does not reproduce the articles.

Review date: 2026-11-07, Asia/Saigon (UTC+7). The reviewer was a different reader from the first researcher.

## What was accepted

- The reviewer read **31** complete main textual articles: the 30 initial candidates plus replacement N02. Images themselves were not inspected. Text alternatives, captions, displayed code, output, endings, and references were read.
- **30** articles are retained. Their first-publication dates are strictly before 2020-11-07, from 2014-11-14 through 2019-02-23. That gate is original publication, not an archived snapshot of every current sentence. Live pages can still have later edits. Explicit 2022/2023 additions in E01 are excluded from historical evidence.
- The retained set spans **7** outlets and **8** credited writer identities. Article counts are not independent votes. Overlapping wording in C03 and C04 counts once.
- **29** retained articles have qualitative established-writer support. **E05** is an established-outlet guest article. Personal popularity for that byline was not verified. Do not describe all eight credited writers as independently popular.
- Originally authored Vietnamese here means the explanatory prose was composed in Vietnamese on the available evidence. It is not a mother-tongue or nationality finding. Borrowed definitions, code, diagrams, taxonomy, and closely translated passages are not independent Vietnamese style evidence.
- Historical commands and technical claims are not current Coppice advice. T05's description of `rm` is a known error and is not an approved instruction.

## E03 is not in the 30

E03, "Tìm hiểu Map và Set trong JavaScript" (https://ehkoo.com/bai-viet/map-set-javascript-es6-es2015, first published 2018-03-25), was read and then **excluded**. A full public mirror of the cited English article showed closely translated and adapted explanatory prose, including the size/length FAQ and the WeakMap explanation. Shared API names alone would not have been enough. Local quiz and code changes did not supply enough clearly independent prose for this original-Vietnamese sample. This is a sampling exclusion, not an allegation of misconduct. The canonical English URL was unavailable, and the mirror was not authenticated against it.

**N02**, "Nói chuyện về Kotlin" (https://codeaholicguy.com/2017/06/28/noi-chuyen-ve-kotlin/, 2017-06-28), replaces E03. N02 was fully read and is one of the 30. E03 is not rule support. The retained count is 30, not 31, and E03 plus N02 are not both counted.

E03 remains only in `screened_out_during_independent_review` in the manifest. Its passive sentence that reverses old and new data is a diagnostic of a technical defect, not positive style evidence.

## Other provenance limits that stay attached

- E01 is a Vietnamese synthesis informed by Nolan Lawson. Keep the authored exposition. Do not treat the shared lesson sequence as a separate cross-author consensus. Exclude the dated 2022/2023 insertions.
- M04 borrows a learning-style taxonomy and algorithm lists. Keep the authored Vietnamese explanation. Do not treat list order or translated labels as independent style evidence.
- G04 is the author's own Vietnamese republication of a magazine piece, with fiction. The exact earlier magazine date is unverified. The blog date passes the cutoff. Fiction and celebrity claims are not product facts.
- No UI, error-message, or API-manual corpus was sampled. README, reference, landing, and UI shapes in the skill are editorial transfers.

Comparison pages used only to screen provenance, not added to the 30:

- https://pouchdb.com/2015/05/18/we-have-a-problem-with-promises.html
- https://www.stevebrownlee.com/weakmap-javascript-private-data/
- https://machinelearningmastery.com/a-tour-of-machine-learning-algorithms/
- https://ongxuanhong.wordpress.com/2015/10/22/diem-qua-cac-thuat-toan-machine-learning-hien-dai/
- http://psychologydector.blogspot.com/2015/01/ecmascript-6-maps-and-sets.html

## Rule bounds

Support is qualitative. A counterexample limits a rule. Anchors are source IDs in the retained catalog.

| Rule | Decision | Retained anchors | Counterexamples |
| --- | --- | --- | --- |
| 1 Purpose early enough for the task | Kept. Answer-first README shape is an editorial choice. | H01, H04, M03, T04 | M02 and T02 open with a story. |
| 2 Recoverable actor | Kept. Passive and topic-comment sentences can stand. Do not invent an actor. | H03, G02, T05 | H01 uses an object-centered passive. E03 is not evidence. |
| 3 Conditions and modals | Kept. A missing guarantee is not proof that a failure occurred. | E02, E04, M01 | T05 states strong obligations that are technically wrong. |
| 4 Address | Kept as an editorial choice. Product docs default to neutral prose. Do not invent firsthand experience. | M01, M02, C03, N01 | H02 shifts address. N01 also uses mình briefly. |
| 5 Omission | Kept when the actor is recoverable. Name the actor when it changes. | E02, G03, T01 | E02 also repeats bạn. T05 needs the responsible actor. |
| 6 Information load | Kept. No whitespace word quota. Vietnamese spaces also separate syllables. | M01, H06, N01 | T05's ending is overloaded. M02's story is long and useful. |
| 7 Connectors | Kept as a meaning test. Do not add or delete them mechanically. | H01, E05, M02 | G01 uses a broad Tuy nhiên. N01 can start with Và. |
| 8 Stable terms | Kept. Equivalents are allowed after they are identified. | H03, G02, T03 | The corpus varies casing. M03 allows an English lookup term. |
| 9 Verbs and terminology | Kept. Do not ban Hán–Việt or every việc/sự. One writer's dislike is not the project glossary. | T01, T03, M03 | M04 dislikes one standard translation. G03 code-switches heavily. |
| 10 Literals | Kept as a safety rule. Preserve supplied literals and flag defects. Do not silently repair them. | E02, E05, T05 | E02, T03, T04, and T05 contain typos or a wrong command claim. |
| 11 Examples | Kept. A labeled failing example may teach. An invented product behavior may not. | H02, E05, M02, N02 | H02 also claims the code has no bugs. G01 closes as if finished. G04's analogy dominates. |
| 12 Tone and warnings | Calm Coppice voice is an editorial selection. Putting a warning next to the risky step is a safety choice. Many sources put the limit at the end. | C04, H01, H05, M01 | Jokes and exaggeration appear. E04's caveat is late. |

## Rewrite tests

These are synthetic editing contracts, not Coppice facts and not instructions to run. Eight supplied pairs were audited. Four passed as first worded. Four were tightened. Test 6 is the uncertainty-preservation fix, and the skill uses that corrected wording.

| Test | Result | Wording that passed |
| --- | --- | --- |
| 1 Actor and condition | Tightened | "Ứng dụng sẽ chỉ lưu tệp `config.json` khi bạn bấm Lưu." Keep sẽ. Do not drop tense only to shorten the sentence. |
| 2 Omitted address | Pass | "Chạy `git status`, rồi đọc danh sách tệp đã thay đổi." The reader does both actions. |
| 3 Cache repetition | Pass | "Bộ nhớ đệm giữ kết quả truy vấn nhưng không tự làm mới kết quả đó." No new invalidation mechanism. |
| 4 Uncertainty | Pass | "Nếu kết nối bị gián đoạn, quá trình đồng bộ có thể chưa hoàn tất." chưa is not không bao giờ. |
| 5 Warning | Tightened for a standalone warning | "Chỉ xóa thư mục `cache` sau khi đã sao lưu các tệp cần giữ. Không xóa thư mục `cache` khi chưa sao lưu." Not an order to delete now. |
| 6 Linux and non-guarantee | Semantic failure, then corrected | "Trên Linux, tính năng này có thể giảm thời gian khởi động. Tuy nhiên, kết quả trên Linux còn phụ thuộc vào dữ liệu nên không có bảo đảm rằng lần chạy nào cũng nhanh hơn." "Không bảo đảm" does not mean "không phải lần chạy nào cũng nhanh hơn." The second sentence asserts that an exception has occurred. |
| 7 Advice versus obligation | Pass | "Bạn phải bấm Lưu để lưu thay đổi. Nên kiểm tra lại nội dung trước khi bấm." phải and nên stay distinct. |
| 8 Unknown save state | Tightened | "Không kết nối được. Chúng tôi không biết chắc yêu cầu đã được lưu hay chưa." Do not replace that with "chưa được lưu", "không có dữ liệu nào bị mất", or a safe retry. |

Further pressure checks, also hypothetical:

- **Several actors.** After the user selects `input.csv`, the server checks the header. A valid header leads the worker to process rows. An invalid header leads the server to report an error. Do not collapse those actors into nó, and do not add a repair.
- **One scoped condition.** If `USE_CACHE=1` and the key is present, `readValue(key)` returns the stored value; otherwise it returns `null`. `null` does not identify the cause. Do not drop the key-presence condition when splitting the sentence.
- **Unknown save.** A connection can fail after submission while the client still lacks confirmation. That is not proof the save failed, and it is not permission to invent a safe retry.
- **Unspecified actor.** Files in `out/` will be overwritten on export. Files that must be kept have to be backed up first. Keep the passive and the obligation. Do not weaken sẽ bị to có thể bị.
- **Modals.** "Có thể giúp" plus "không có bảo đảm" must not become "luôn nhanh hơn", "chắc chắn chậm hơn", or a phải where the brief only has nên.
- **Gloss.** Give idempotency and tính lũy đẳng once, and keep the statement that the definition does not say the current API can be retried safely.

No SEO, detector, or technical-correctness result is claimed from this research.
