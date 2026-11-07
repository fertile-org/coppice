# 简体中文写作研究：审后来源

供 [SKILL.md](../SKILL.md) 引用。逐条元数据在 [repo-reference-manifest.json](repo-reference-manifest.json)。审查与语义测试在 [independent-review.md](independent-review.md)。

独立审查：2026-11-07，Asia/Saigon（UTC+7）。原30篇全文均重读；另全文读 M6。审后接收30篇（8渠道、17个个人/笔名署名），R2由M6替换。R2不在下面的30篇里，也不作规则依据；不把 R2 和 M6 同时计入30。

## 资格与证据边界

初发必须早于2020-11-07；原作中文，不以作者母语或国籍判断。原页发布字段属于初发证据，不声称已复原不可变旧版。正文当前可能有未标明修订。下列定位是本轮私有全文档案的行号，供复核；仓库不附全文档案，公共URL与分节足以寻找上下文。图片、视频和代码执行未验证。

“受欢迎”采用成熟作者/渠道的定性选择：张鑫旭原站自报读者/评论，Phodal与Draveness著作及长期系列，廖雪峰长期教程；这些不是独立流量测量。美团仅为机构渠道证据，不把每个署名说成热门博主。

wklken补得[独立推荐](https://github.com/yihong0618/gitblog/issues/195)第12项，明确提及Python源码系列与重构文章；这是2020-11-20创建、2021-01-21更新的推荐，不是截止日前人气证明。王垠的[InfoQ授权专栏说明](https://www.infoq.cn/article/wisdom-of-programming)可核实传播。均不推断粉丝或阅读规模。

## 审后30篇

<a id="source-d2"></a>
### D2 — 为什么 Redis 选择单线程模型
- 作者：Draveness；渠道：面向信仰编程；原站：[全文](https://draveness.me/whys-the-design-redis-single-thread/)。
- 初发：2019-10-18。证据：文章标题下 2019-10-18。
- 原作判断：接受中文综合论证。两个版本问题→可维护性/并发/瓶颈→删除操作的全篇组织并不复现 Kyle Davis 的 UNLINK 使用指南。后者有 INFO 检查、ZRANGE、MULTI/EXEC、替换 DEL 的例外；D2 未逐段译写这些内容。英文 FAQ 引句及其中文释义不抽取句式。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 排除正文之后纸质书推广、二维码、许可/配图/推荐模块。

<a id="source-d3"></a>
### D3 — 为什么 TCP 建立连接需要三次握手
- 作者：Draveness；渠道：面向信仰编程；原站：[全文](https://draveness.me/whys-the-design-tcp-three-way-handshake/)。
- 初发：2019-10-28。证据：文章标题下 2019-10-28。
- 原作判断：接受中文综合解释。与 StackExchange 全页答案对读，初始序列号与中间两步合并属共同协议解释；D3 用错误听力类比、连接定义、历史连接为主论点、握手次数讨论重新组织，并明确引用 RFC。RFC 原句及其译述不作作者原创句式证据。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 排除正文之后纸质书推广、二维码、许可/配图/推荐模块。

<a id="source-d4"></a>
### D4 — 为什么 MySQL 使用 B+ 树
- 作者：Draveness；渠道：面向信仰编程；原站：[全文](https://draveness.me/whys-the-design-mysql-b-plus-tree/)。
- 初发：2019-12-11。证据：文章标题下 2019-12-11。
- 原作判断：接受中文综合解释。对读 Medium 及 Callibrity 后，基础树定义与磁盘局部性相通，但 D4 的 InnoDB 层级澄清、作者 posts SQL、macOS getconf、先哈希后 B 树的取舍链并非二者逐段转写。Ayende 返回405未读，其余所有参考未穷尽；这是有依据的判断，不是查重认证。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 排除正文之后纸质书推广、二维码、许可/配图/推荐模块。

<a id="source-z1"></a>
### Z1 — CSS backdrop-filter简介与苹果iOS毛玻璃效果
- 作者：张鑫旭（zhangxinxu）；渠道：鑫空间-鑫生活；原站：[全文](https://www.zhangxinxu.com/wordpress/2019/11/css-backdrop-filter/)。
- 初发：2019-11-15。证据：这篇文章发布于 2019年11月15日。
- 原作判断：首段本人经历、本人 demo 与原作声明相互印证；取两个滤镜作用对象的对照，不取绝对化预测及末尾生活闲谈。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 原站声明会更新知识点/纠错，故保留 live 修订不确定性；相关文章与个人闲谈不作为规范依据。

<a id="source-z2"></a>
### Z2 — CSS flex属性深入理解
- 作者：张鑫旭（zhangxinxu）；渠道：鑫空间-鑫生活；原站：[全文](https://www.zhangxinxu.com/wordpress/2019/12/css-flex-deep/)。
- 初发：2019-12-22。证据：这篇文章发布于 2019年12月22日。
- 原作判断：本人五兄弟例子贯穿语法与实现，原作声明；115–129 的反馈/更正段排除，不能把错字或代码中的全角分号当自然风格。
- 修订边界：排除 //zxx 评论反馈补记及“更正为”/删除线段（article.md 115–129 行）作为历史写作证据；日期不明。末尾动态相关文章全部排除。 原站声明会更新知识点/纠错，故保留 live 修订不确定性；相关文章与个人闲谈不作为规范依据。

<a id="source-z3"></a>
### Z3 — 常见纯CSS图标的代码分离与整理（持续更新）
- 作者：张鑫旭（zhangxinxu）；渠道：鑫空间-鑫生活；原站：[全文](https://www.zhangxinxu.com/wordpress/2019/08/pure-css-icon/)。
- 初发：2019-08-13。证据：这篇文章发布于 2019年08月13日。
- 原作判断：本人整理工具的使用说明，承认图标代码参考 icons.css 不等于外文文章译写。持续更新，无逐段历史版本；只作初发合格的现存文本，不能说43图标及每句话均是2019初版。
- 修订边界：标题明确“持续更新”，正文无逐段修订日期；允许初发日期合格的 live 页，但不声称所有句子为2019年原样；动态相关文章排除。 原站声明会更新知识点/纠错，故保留 live 修订不确定性；相关文章与个人闲谈不作为规范依据。

<a id="source-z4"></a>
### Z4 — 让CSS flex布局最后一行列表左对齐的N种方法
- 作者：张鑫旭（zhangxinxu）；渠道：鑫空间-鑫生活；原站：[全文](https://www.zhangxinxu.com/wordpress/2019/08/css-flex-last-align/)。
- 初发：2019-08-15。证据：这篇文章发布于 2019年08月15日。
- 原作判断：同一布局问题分条件给多个本人 demo，结尾还有兼容限制；不把旧 IE 建议用于现在的产品。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 原站声明会更新知识点/纠错，故保留 live 修订不确定性；相关文章与个人闲谈不作为规范依据。

<a id="source-z5"></a>
### Z5 — 给IE/Edge写了个textarea resize拉伸的polyfill
- 作者：张鑫旭（zhangxinxu）；渠道：鑫空间-鑫生活；原站：[全文](https://www.zhangxinxu.com/wordpress/2019/08/ie-edge-textarea-resize-polyfill/)。
- 初发：2019-08-07。证据：这篇文章发布于 2019年08月7日。
- 原作判断：本人 polyfill 的接口和兼容说明；只支持 textarea 与不能覆盖所有场景的边界清楚。医疗推论、个人夸赞、代码未执行不采纳。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 原站声明会更新知识点/纠错，故保留 live 修订不确定性；相关文章与个人闲谈不作为规范依据。

<a id="source-w1"></a>
### W1 — Better Code: 使用property
- 作者：wklken；渠道：wklken；原站：[全文](https://wklken.me/posts/2019/10/07/better-code-1-use-property.html)。
- 初发：2019-10-07。证据：文章开头 Oct 7, 2019。
- 原作判断：中文重构说明可收，但 Student property 代码与常见廖雪峰教程示例高度相似，不能把这些代码当独创凭据。本文 Django 字段与调用者认知负担的说明才是中文写作观察。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-w2"></a>
### W2 — Python源码阅读-闭包的实现
- 作者：wklken；渠道：wklken；原站：[全文](https://wklken.me/posts/2015/09/04/python-source-closure.html)。
- 初发：2015-09-04。证据：文章开头 Sep 4, 2015。
- 原作判断：同一 add/do_add 实验到 cell/frame/字节码的中文追踪完整；英文 CPython 注释是引用，不是中文写作证据。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-w3"></a>
### W3 — Python源码阅读-内存管理机制(一)
- 作者：wklken；渠道：wklken；原站：[全文](https://wklken.me/posts/2015/08/29/python-source-memory-1.html)。
- 初发：2015-08-29。证据：文章开头 Aug 29, 2015。
- 原作判断：作者阅读笔记，层级→block→pool→分配阶段；长源码不是优质句子的计量单位，中文解释和源码引用分开。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-p1"></a>
### P1 — 架构金字塔
- 作者：Phodal Huang；渠道：Phodal；原站：[全文](https://www.phodal.com/blog/architecture-pyramid/)。
- 初发：2019-10-28。证据：作者行 2019年10月28日 18:26。
- 原作判断：本人提出金字塔及本书框架，同时承认演进式架构/DDD思想来源；不是翻译这些书。保留其概念密度作为反例，不提炼“完美”宣称。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 排除相关链接、关于作者、招徕等外围；P4引述React官方句子/P2外部引用不是原创风格证据。

<a id="source-p2"></a>
### P2 — Clean Architecture 实施指南
- 作者：Phodal Huang；渠道：Phodal；原站：[全文](https://www.phodal.com/blog/clean-frontend-architecture-in-action/)。
- 初发：2019-07-22。证据：作者行 2019年7月22日 20:29。
- 原作判断：团队上下文、目录、请求链、尚未解决问题构成第一手实践；DDD 引述另排。反复“嗯”与项目拼写错不进入规则。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 排除相关链接、关于作者、招徕等外围；P4引述React官方句子/P2外部引用不是原创风格证据。

<a id="source-p4"></a>
### P4 — 【架构拾集】组件设计原则（Stage1）
- 作者：Phodal Huang；渠道：Phodal；原站：[全文](https://www.phodal.com/blog/architecture-in-realworld-design-component-based-architecture/)。
- 初发：2019-07-16。证据：作者行 2019年7月16日 18:00。
- 原作判断：本人遗留项目与组件拆合反思；React 官方译引及 WelcomeDialog 示例不是作者原生中文证据。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 排除相关链接、关于作者、招徕等外围；P4引述React官方句子/P2外部引用不是原创风格证据。

<a id="source-p5"></a>
### P5 — 前端下半场：构建跨框架的 UI 库
- 作者：Phodal Huang；渠道：Phodal；原站：[全文](https://www.phodal.com/blog/build-cross-framework-ui-library/)。
- 初发：2018-08-13。证据：作者行 2018年8月13日 20:09。
- 原作判断：Phodit、Angular/React 的本人实验；filename 成功与 action 似乎不行分别报告。未来必然可用的预判是反面教材。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。 排除相关链接、关于作者、招徕等外围；P4引述React官方句子/P2外部引用不是原创风格证据。

<a id="source-f1"></a>
### F1 — 对 skynet 的 gate 服务的重构
- 作者：云风；渠道：云风的 BLOG；原站：[全文](https://blog.codingnow.com/2014/04/skynet_gate_lua_version.html)。
- 初发：2014-04-15。证据：云风 提交于 April 15, 2014。
- 原作判断：本人 skynet 重构，C分包/Lua转发的取舍、现状与拟议队列分开。只看正文不能验证代码当前实现。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-f2"></a>
### F2 — skynet 里的 coroutine
- 作者：云风；渠道：云风的 BLOG；原站：[全文](https://blog.codingnow.com/2015/12/skynet_coroutine.html)。
- 初发：2015-12-18。证据：云风 提交于 December 18, 2015。
- 原作判断：本人框架与日志流转案例；框架和应用层 coroutine 的执行者变化不能混淆。PIL链接不是本篇整体改编证据。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-f4"></a>
### F4 — 在线调试 Lua 代码
- 作者：云风；渠道：云风的 BLOG；原站：[全文](https://blog.codingnow.com/2015/02/skynet_debugger.html)。
- 初发：2015-02-11。证据：云风 提交于 February 11, 2015。
- 原作判断：本人调试器实现与使用，提示符到 patch 的完整叙述；bug 从可能到调查的认识过程不能压成一开始已证实。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-f5"></a>
### F5 — STM 的简单实现
- 作者：云风；渠道：云风的 BLOG；原站：[全文](https://blog.codingnow.com/2014/08/stm.html)。
- 初发：2014-08-12。证据：云风 提交于 August 12, 2014。
- 原作判断：本人简化设计与 stm API；写方/读方、false 的两个原因适合作语义保护证据。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-f6"></a>
### F6 — 在移动网络上创建更稳定的连接
- 作者：云风；渠道：云风的 BLOG；原站：[全文](https://blog.codingnow.com/2014/02/connection_reuse.html)。
- 初发：2014-02-08。证据：云风 提交于 February 8, 2014。
- 原作判断：主体为本人测试、设计协议与拟议接口；英文 MPTCP 引句、ps与2月14补记排除。旧 RC4/MD5 不作安全依据。
- 修订边界：排除“ps. 经过这两天的测试”补记及末尾“2月14日补充”（年份未写）作为本次风格规则依据；正文协议涉及过时密码算法，不作当前安全建议。

<a id="source-l1"></a>
### L1 — Java Map的正确使用方式
- 作者：廖雪峰；渠道：廖雪峰的官方网站；原站：[全文](https://liaoxuefeng.com/blogs/all/2018-12-26-java-map/)。
- 初发：2018-12-26。证据：当前文章 #gsi-blog-info 后的 time[datetime="2018-12-26"]。
- 原作判断：本人 Student 的 HashMap/TreeMap 对照，原站当前文章 time 已定位，非侧栏日期。Java21链接是明确较晚元素，JDK译引不抽样。
- 修订边界：JDK文档链接现指向 Java21，明显晚于初发；排除该链接年代作为技术/风格证据。不据此推断整篇无效。

<a id="source-l2"></a>
### L2 — 如何正确缓存按需创建的记录
- 作者：廖雪峰；渠道：廖雪峰的官方网站；原站：[全文](https://liaoxuefeng.com/blogs/all/2018-12-27-cache-insert/)。
- 初发：2018-12-27。证据：当前文章 #gsi-blog-info 后的 time[datetime="2018-12-27"]。
- 原作判断：本人 Account 事务/缓存案例，考虑嵌套事务；末尾可靠性广泛宣称不能直接升级为产品保证。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-l3"></a>
### L3 — Java的Fork/Join任务，你写对了吗？
- 作者：廖雪峰；渠道：廖雪峰的官方网站；原站：[全文](https://liaoxuefeng.com/blogs/all/2017-05-03-java-fork-join/)。
- 初发：2017-05-03。证据：当前文章 #gsi-blog-info 后的 time[datetime="2017-05-03"]。
- 原作判断：本人 SumTask 与清洁工比喻，原站 time 已定位；Fork/Join 技术解释未复现，不能背书其绝对错误及耗时断言。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-m2"></a>
### M2 — LruCache在美团DSP系统中的应用演进
- 作者：王粲、崔涛、霜霜；渠道：美团技术团队；原站：[全文](https://tech.meituan.com/2018/12/20/lrucache-practice-dsp.html)。
- 初发：2018-12-20。证据：标题下 王粲 崔涛 霜霜 2018-12-20。
- 原作判断：具名本团队 DSP 实践，16核16G、95%命中率、分片与长尾指标明确；属于成熟机构渠道，不声称三位均热门博主。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-m4"></a>
### M4 — 美团点评旅游搜索召回策略的演进
- 作者：郑刚；渠道：美团技术团队；原站：[全文](https://tech.meituan.com/2017/06/16/travel-search-strategy.html)。
- 初发：2017-06-16。证据：标题下 郑刚 2017-06-16。
- 原作判断：具名旅游搜索建设记录和本团队具体查询；同作者演讲一致不是外文改编。搜索丢词不可外推写作删“的”。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-m5"></a>
### M5 — 美团外卖前端可视化界面组装平台 —— 乐高
- 作者：徐楷、冰冰、东亮；渠道：美团技术团队；原站：[全文](https://tech.meituan.com/2017/03/09/waimai-lego.html)。
- 初发：2017-03-09。证据：标题下 徐楷 冰冰 东亮 2017-03-09。
- 原作判断：具名本团队乐高平台，用途、使用、SDK、实现、现状；有参考文档形态但不是足量UI微文案语料。示例代码和界面未执行/逐图核验。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-y1"></a>
### Y1 — 解密"设计模式"
- 作者：王垠；渠道：yinwang.org；原站：[全文](https://yinwang.org/posts/design-patterns)。
- 初发：2013-03-06。证据：Independent WebFetch original non-www rendered title/date on review day; 原站渲染页标题下 2013年3月6日。
- 原作判断：逐段对照 non-www 原站 WebFetch，标题、2013年3月6日、全部论证及结尾匹配人工稿；引用 Norvig 后有本人的反驳/限定，非其演讲译文。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-y2"></a>
### Y2 — 编辑器与IDE
- 作者：王垠；渠道：yinwang.org；原站：[全文](https://yinwang.org/posts/editor-ide)。
- 初发：2013-04-19。证据：Independent WebFetch original non-www rendered title/date on review day; 原站渲染页标题下 2013年4月19日。
- 原作判断：逐段对照 non-www 原站 WebFetch，2013年4月19日、三节正文及末尾四条匹配；本人工具经历和AST论述，英文动词与个人优越性评价非规范。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-y3"></a>
### Y3 — 关于语言的思考
- 作者：王垠；渠道：yinwang.org；原站：[全文](https://yinwang.org/posts/languages)。
- 初发：2013-04-16。证据：Independent WebFetch original non-www rendered title/date on review day; 原站渲染页标题下 2013年4月16日。
- 原作判断：逐段及代码对照 non-www 原站 WebFetch，2013年4月16日、三个小节、全部代码和结尾匹配；本人的语言比较非译文，不引入营养类比和贬称。
- 修订边界：未发现可定年的后置正文修订标记；live 页未与历史快照比对，不能保证逐字保持初版。

<a id="source-m6"></a>
### M6 — WebView性能、体验分析与优化
- 作者：育新、徐宏、嘉洁；渠道：美团技术团队；原站：[全文](https://tech.meituan.com/2017/06/09/webviewperf.html)。
- 初发：2017-06-09。证据：Original HTML article title followed by 育新 徐宏 嘉洁 2017-06-09; independently extracted and checked.。
- 原作判断：替换 R2；独立完整阅读543行。具名本团队测试，设备/版本/测试方式与指标定义明确；同时吸收其他中文团队实践并标出处，不是外文文稿结构性改编。旧安全结论尤其 Socket 绕过劫持与 HTTPS 取舍不背书。
- 修订边界：Current page, no historical snapshot; no dated post-cutoff main-text additions identified; navigation/media excluded.

## 不再计数的 R2

[JSON Web Token 入门教程](https://www.ruanyifeng.com/blog/2018/07/json_web_token-tutorial.html)，阮一峰，2018-07-23。不计入严格原创语料。与 Auth0 Introduction 对读后发现，3.1–3.3 的分节顺序、两个 JSON 示例、签名公式及紧接的保密警告均与该参考高度对应，使用段也共享 Bearer 示例。开头跨域例子及末尾补充体现中文综合，但不能据此排除结构性外文改编。是保守资格排除，不是抄袭指控。
对照：[Auth0 Introduction](https://jwt.io/introduction/)。不作全篇抄译指控，也不以它支撑审后写作规则。

## 进一步比对与未覆盖

D2 对读[UNLINK说明](https://www.odbms.org/2018/03/the-little-known-feature-of-redis-4-0-that-will-speed-up-your-applications/)；旧 Redis FAQ 已改跳开发首页，不冒称读取原FAQ。D3 对读[StackExchange问题与全部答案](https://networkengineering.stackexchange.com/questions/24068/why-do-we-need-a-3-way-handshake-why-not-just-2-way)。D4 对读[Medium树索引比较](https://medium.com/@mena.meseha/what-is-the-difference-between-mysql-innodb-b-tree-index-and-hash-index-ed8f2ce66d69)和[Callibrity](https://www.callibrity.com/blog/database-index-a-re-visit-to-b-tree-part-1)；Ayende返回405，未穷尽每条参考资料。此为有限原创判断，不是全网查重。

Y1–Y3通过无www原站渲染全文逐段/逐代码核对；www版本轮只返回空壳，旧blog-cn路径超时。正文和显示日期匹配私有人工稿；链接地址扁平化、媒体未核验，未恢复历史时区。

## 文件与安装范围

这些文件在 `.cursor/skills/zh-cn-technical-writing/`：`SKILL.md`，以及本目录的 `repo-references.md`、`repo-reference-manifest.json`、`independent-review.md`。仓库不附 sources/、原始 HTML、article.md 或抓取脚本，也不附未经审后的 analysis.md、source-index.md、provenance-notes.md。全文不入库。
