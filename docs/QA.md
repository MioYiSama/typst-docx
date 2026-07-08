> 给typst加一个docx(microsoft word) renderer，底层直接操作ooxml，要求像素级还原。帮我分析一下可行性和方案。

```
分析完了。先说结论，再展开。

结论

"docx + 像素级还原"这两个要求本质上是矛盾的。 可以做到"正常阅读下视觉几乎不可区分"，但数学意义上的像素级一致做不到——除非放弃文本可编辑性，而那样 docx 就失去了存在意义（不如直接用已有的 PDF 导出）。下面解释为什么，以及三条可行路线的取舍。

根本矛盾：docx 不存储布局

PDF/SVG 存的是最终几何（每个字形的绝对坐标），而 OOXML 存的是排版意图（段落、样式、缩进）。docx 文件里没有换行位置、没有字形坐标——这些全部由打开文件的 Word 在渲染时用它自己的排版引擎现场计算。所以哪怕你把 Typst 的排版结果一字不差地写进 docx，最终像素仍由 Word 决定，且历史上 Word 的排版结果连自己不同版本、不同打印机驱动之间都不一致。

Typst 这边的架构对这个问题看得很清楚，代码库里恰好有两类导出器对应两条路线：

- typst-pdf / typst-svg / typst-render 消费排版完成的 PagedDocument，里面的 FrameItem::Text（crates/typst-library/src/layout/frame.rs:486）带有逐字形的 x_advance / x_offset（rustybuzz shaping 的结果）——几何信息完整。
- typst-html 走的是 realize/content 路线，导出语义结构，布局交给浏览器——和"交给 Word"是同一种哲学。

三条路线

A. 语义导出（HTML 路线）：把 heading/paragraph/table/list 映射到 Word 段落样式、OMML 公式等。产物是"正经的" docx，可编辑、可搜索，这也是 pandoc 目前做的事。但换行、连字、断字、对齐全由 Word 重算，与像素级还原无缘，通常连行数都对不上。

B. 绝对定位（Frame 路线）——最接近你的要求：消费 PagedDocument，把每一行文本放进页面锚定的 text box（wps:wsp，EMU 坐标定位，1pt = 12700 EMU，Abs 转换无损），页面尺寸/边距用 sectPr 精确设置。行的位置、行高、每个 run 的起点都能精确控制，可以做到行级完全精确。残余误差在字形层面：

- Word 用自己的 shaping 引擎（DirectWrite），和 Typst 的 rustybuzz 对 kerning、连字的处理不完全一致。可以关掉 Word 的 kerning、按 run 用 w:spacing（字符间距）补偿宽度差，把误差压到 run 内部的亚点级。
- Typst justification 产生的逐字形 x_offset 微调无法逐一还原——除非一个字形一个 run，那会让文件体积爆炸、Word 打开即卡死。
- 字体必须嵌入（fontTable + ODTTF 混淆格式），否则回退字体直接毁掉一切；Word 对 CFF/可变字体的嵌入支持较差，且要尊重字体的 fsType 嵌入许可位。

产物的代价：文档由几百上千个定位 text box 组成，在 Word 里基本不可编辑——挪一个字不会重排，只会破坏版面。本质是"穿着 docx 外衣的 PDF"。

C. 字形转轮廓（DrawingML custGeom 路径）：把每个字形画成矢量形状，几何上真正像素级。但产物不含文本——不可搜索、不可编辑、无障碍性为零、文件巨大。除非需求是"必须交付 .docx 后缀的文件且对方只看不改"，否则没有意义。

我的建议

先反问需求：要 docx 的人通常是为了能编辑。B 和 C 的产物都不可编辑，那么"像素级"这个指标值得重新审视——真需要像素级保真时 PDF 已经存在。

- 如果核心诉求是"交付物必须是 docx 且版面尽量还原"（如某些公文/投标场景）→ 做方案 B，目标定为"100% 缩放下视觉不可区分"，接受字形级亚点误差。
- 如果核心诉求其实是"对方要能改" → 做方案 A，接受重排，把精力花在样式映射的完整度上。
- 上游合并前景：Typst 官方对 docx 导出历来的态度是引导用户走 pandoc，官方精力在 HTML 导出上。这个功能建议做成独立 crate（仿 typst-svg 的结构，输入 PagedDocument），而不是奔着进上游主仓去做。

若做方案 B 的实现要点

- 新建 typst-docx crate，结构照抄 typst-svg（遍历 frame，分 text/shape/image 模块，自带一个轻量 XML writer，参考它的 write.rs）。OPC 打包用 zip crate 手写即可；不建议用 docx-rs 之类的库，它们的模型面向语义文档，对绝对定位和字体嵌入支持很弱。
- 减少对象数的关键技巧：不必一行一个 text box——每页（或每栏）一个大 text box，内部用"每行一个段落 + w:spacing 精确行高（lineRule="exact"）+ 精确缩进"来复现行位置，能把 drawing 对象数降一个数量级，Word 打开性能才可接受。
- Shape/Curve → DrawingML custGeom；Image → media part，SVG 需要 mc:AlternateContent 包 PNG fallback（Word 2016+ 才认原生 SVG）；数学公式在 frame 阶段已经变成定位好的字形和线条，方案 B 天然覆盖，反而是方案 A 里最难啃的部分（要转 OMML）。
- 验证闭环：CI 里用 LibreOffice headless 把产出 docx 转 PDF，和 typst-pdf 的输出做像素 diff 回归；但注意 LO 的排版 ≠ Word，最终验收必须开真 Word（可配合 Windows CI + Word 自动化）。

工作量估计：方案 B 的 MVP（文本、形状、图片、精确页面设置、字体嵌入）大约几千行 Rust、数周可见雏形；真正的大头在长尾——shaping 差异补偿、字体嵌入兼容性、不同 Word 版本行为差异的调试，这部分是按月计的。

需要的话我可以先做一个最小验证实验：写个几十行的原型，把一页 Typst 输出用"锚定 text box + 精确行高"塞进手工构造的 docx，在 Word 里量化实际偏差，用数据确认方案 B 的保真上限，再决定是否投入。
```

> 这个问题的痛点就是学校强制交docx，不能交pdf，但是docx太难用了。我觉得两条路都可以搞。你觉得假如要开发，是直接fork源码改仓库还是开一个新项目作为第三方crate引入typst做开发。

```
明确建议：开新项目做第三方 crate，不要 fork。理由如下。

为什么不 fork

- 上游合并无望，fork 就意味着永久维护分叉。 Typst 官方对 docx 的态度是"用 pandoc"，这个 feature 进不了主仓。fork 后每次上游发版（typst 迭代很快，内部 API 每个 0.x 版本都有破坏性变更）你都要 rebase 自己的改动，成本随时间只增不减。
- 你根本不需要改 typst 内部。 你要的所有东西都是已发布的公开 API：typst、typst-library、typst-kit 都在 crates.io 上。编译拿到 PagedDocument，Frame / FrameItem / TextItem（连逐字形的 x_advance/x_offset）全是 pub 的——typst-pdf 自己也只是这些 API 的消费者，和你的 crate 地位平等。生态里已有先例：typst-as-lib、typst.ts 都是这么在外部驱动 typst 编译的。
- fork 唯一的好处是能做出 typst compile thesis.docx 这种原生 CLI 体验，但这用一个自己的薄 CLI 就能等价实现，不值得背整个仓库。

建议的项目结构

新仓库，一个 workspace：

typst-docx/
├── crates/
│   ├── typst-docx/        # 核心库：PagedDocument -> .docx（方案B，帧路线）
│   ├── typst-docx-flow/   # 可选：语义路线（方案A），后期再做
│   └── typst-docx-cli/    # 薄 CLI：用 typst-kit 搭 World（字体发现、包下载），
│                          #   typst::compile，然后调上面的库导出

CLI 部分抄 typst-cli 的骨架即可，它本身就只是对库 crate 的薄包装，World 的搭建（字体、包管理）typst-kit 都封装好了，百来行就能跑通 typst-docx compile main.typ -o out.docx。用户体验上和原生命令没有区别。

依赖上游的唯一代价是跟版本：typst 各 crate 锁版本齐步发布，每次升级（0.14 → 0.15…）你要适配一次 API 变更。但这比维护 fork 轻得多——你只碰公开接口，变更面小且有 changelog。

两条路的开发顺序

既然你说两条路都可以搞，建议先做方案 B（帧路线）：

1. 它直接解决你的痛点（交 docx、版面还原），且实现路径最短——遍历 Frame 写 XML，不用理解 typst 的 realize/样式系统；
2. 方案 A（语义路线）工程量大好几倍（样式映射、表格、列表、OMML 公式……），而且 pandoc 已经覆盖了它 70% 的场景，边际价值低。

另外针对"学校交 docx"这个场景提醒两点，做方案 B 时要设计进去：

- 查重系统要能抽出文本。定位 text box 里的文字是可提取的，但提取顺序取决于文档中 drawing 对象的出现顺序——写入时严格按阅读顺序排列对象，避免查重时文本乱序。
- 老师可能会点开看甚至想批注。批注（comment）锚在 text box 文字上是可用的，但要有心理预期：对方一旦尝试编辑正文，版面会碎。如果学校有"必须用某某模板、宋体小四、行距 22 磅"这类格式审查，审查者在 Word 里看到的段落面板信息会和常规文档不一样——这是方案 B 无法掩盖的特征。

要不要我先起个原型？最小闭环是：typst-kit 搭 World 编译一个单页文档 → 手写 OPC 打包 + 每行一个精确定位段落 → 在 Word/LibreOffice 里打开量偏差。这一步能用真实数据验证保真上限，再决定投入多深。
```
