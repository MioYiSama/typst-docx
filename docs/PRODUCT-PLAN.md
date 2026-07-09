# typst-docx 产品计划

> 2026-07-09 制定。前置:MVP(M0–M4)已完成并验证可行 —— 文本/形状/图片/页面/字体嵌入
> 全链路通过 officecli 结构校验与内容渲染检查,基线规则 R1 在 Word 中人工确认成立。

## 0. 产品定位

**一句话**:`typst compile` 的 DOCX 后端 —— 输出在 Word 中 100% 缩放下与 PDF 视觉一致的
`.docx`,服务"对方只收 Word 文档"的投稿/公文/合同场景。

**非目标**(写进 README,永不摇摆):
- 不做可编辑的语义转换(段落流、样式表、目录域)。那是 pandoc 的领域。
- 不支持 Word 2013 以前的版本(不做 VML/mc:AlternateContent 回退,风险 R3 保持接受)。
- 不做 Word → Typst 反向转换。

**成功指标**:
- 视觉:视觉回归基线上,150ppi 渲染逐像素差异(|Δ|>24)占比 < 0.5%/页。
- 兼容:Word 365(mac/win)、WPS、LibreOffice 7.6+ 打开零修复对话框。
- 性能:100 页文档(~3×10⁵ glyph)导出 < 2s(不含 typst 编译),内存 < 1GB。

## 1. 现状基线(已完成,commit 起点)

| 能力 | 实现位置 | 状态 |
|---|---|---|
| 帧展平 + 变换分类(Simple/Rotated/Skewed) | `frame.rs` | ✅ |
| 行段切分/合并(width_word 连续性判据)、R1 基线规则 | `text.rs` | ✅ |
| prstGeom/custGeom、虚线 custDash、线帽/连接 | `shape.rs` | ✅ |
| PNG/JPG/GIF/WebP 嵌入去重 | `image.rs` | ✅ |
| 每页一节、sectPr(type→pgSz→pgMar)、页面底色 | `page.rs` | ✅ |
| RIBBI 槽位、ODTTF 混淆嵌入、确定性 GUID | `font.rs` | ✅ |
| OPC 打包(zip 8, deflate/stored) | `package.rs` | ✅ |
| CLI(typst-kit: SystemFiles/FontStore/诊断) | `typst-docx-cli` | ✅ |
| 10 个测试(单元 + roxmltree 端到端不变量) | `tests/export.rs` | ✅ |

已知降级(全部有警告):渐变→首色标、平铺→黑、裁剪忽略、链接丢弃、skew 仅平移、
SVG/PDF/像素图跳过、文本描边丢弃、奇偶填充、虚线相位、RTL 整框。

---

## 2. 阶段总览

| 阶段 | 主题 | 周期 | 核心交付物 | 质量门 |
|---|---|---|---|---|
| P0 | 正确性加固 + 视觉回归流水线 | 1 周 | visreg 脚本、Word 度量开关修正 | visreg 全绿 + Word 人工冒烟 |
| P1 | 视觉保真补全 | 3–4 周 | 渐变/裁剪/SVG/数学/RTL | 扩展 fixture 全绿 |
| P2 | 字体工程 + 兼容矩阵 + 性能 | 3 周 | TTC 提取、子集化、兼容报告 | 兼容矩阵 4 平台通过 |
| P3 | 发布与产品化 | 2 周 | crates.io、二进制分发、文档 | 0.1.0 发布 |
| P4 | 远期扩展 | 持续 | 字形轮廓模式、PPTX、WASM | — |

依赖关系:P0 是所有后续阶段的回归安全网,必须最先做;P1/P2 内部条目可并行。

---

## 3. P0 —— 正确性加固(1 周)

> **进度(2026-07-09)**:3.1 已落地(`scripts/visreg.sh` + `imgdiff.py` +
> `posdiff.py`,LibreOffice 26.2 实测:blank 0.000% / shapes 0.133% /
> images 0.096% **PASS**,calibration 0.733%,残差为 LO 对 Libertinus 的
> exact 行高基线分配差异 ≤0.43pt@24pt 与 CJK 回退字体不一致,Word 侧 R1
> 已人工确认,记为已知 LO 残差)。3.2.1 USE_TYPO_METRICS 已实现。
> 另修复一个 visreg 逼出的真 bug:**片段内累积字距漂移**——原先只在单
> glyph 偏差 >0.2pt 时拆分,长单词内 <0.2pt 的 kern 会累加(两端对齐行实测
> 最大 +0.53pt);已改为累积偏差 >0.1pt 即拆分,漂移全局有界(实测降到
> ≤0.16pt)。
>
> **进度(2026-07-09 续)**:P0 收尾。3.2.2(autoSpaceDE/DN=0)、3.2.3
> (`w:noProof` + `w14:ligatures=none`)、3.4(`emu/twips/twips_ceil` 的
> `is_finite` 门、页宽 >31680 twips 警告、`w:t` C0 控制字符过滤+警告、
> 零尺寸/非有限 shape 丢弃)全部落地并测试。visreg 四 fixture 零回归
> (数值与上批一致),确定性 sha256 复现,CJK+math 导出 XML 良构。
> **3.3 重要修正认知**:实测 typst **不产生 per-glyph `y_offset`**——
> 上下标/重音/组合符/阿拉伯文全部走独立平移 TextItem(不同字号),故原
> `y_offset≠0→atomic` 分支在真实输出中从不触发,plan 描述的"y_offset 框
> 爆炸"机制不成立。已按 plan 重构 `fragments`(y_offset 相等则合并、仅
> 大 x_offset 保持 atomic),对现网输出为**正确的 no-op 安全网**(未来字体/
> typst 若产出 y_offset run 才生效)。真实框数由 TextItem 数 + `render_texts`
> 段合并决定,**中等而非上千**(实测:单式 9、密集单行 19、四式半页 51),
> 多 glyph 同基线 run(如 `n+1` 上标)已正确合并为单框。若要进一步降框数,
> 应改进 `render_texts` 跨 TextItem 合并(与 R10/5.4 锚点上限相关),另行立项。

### 3.1 视觉回归流水线(最高优先级)

officecli 在 macOS 上不渲染 `wp:anchor` 绝对定位,不能做位置回归。改用 LibreOffice
headless 作为自动化渲染器(对 wps/anchor 支持完整):

```
scripts/visreg.sh:
  typst compile --format png --ppi 150 f.typ ref-{n}.png     # 基准
  target/release/typst-docx f.typ -o f.docx
  soffice --headless --convert-to pdf f.docx                  # 待测
  pdftoppm -r 150 -png f.pdf got
  python3 scripts/imgdiff.py ref-*.png got-*.png              # 判定
```

- `imgdiff.py`(PIL):统一到相同像素尺寸;逐像素通道差 |Δ|>24 记为坏点;
  坏点占比 < 0.5% 判过;失败时输出红色热力图 diff-{n}.png。
- **关键坑:LibreOffice 不读 ODTTF 嵌入字体**。visreg fixture 一律使用
  `fixtures/fonts/` 内自带的 OFL 字体(DejaVu Sans/Serif/Mono + Noto Sans SC),
  运行前安装到 `~/Library/Fonts`(CI 上装到 `~/.fonts`),typst 侧用 `--font-path`
  指向同一目录,保证两端字形一致。
- CI:GitHub Actions ubuntu-latest,`apt install libreoffice-writer poppler-utils
  fonts-dejavu`,阈值同上;macOS Word 冒烟保留为发版前人工 checklist
  (打开 5 个 fixture:无修复对话框 + 抽查叠加)。

### 3.2 Word 排版行为的三个精确修正(`text.rs`)

1. **USE_TYPO_METRICS**:OS/2 `fsSelection` bit 7 置位时,Word 用
   sTypoAscender/sTypoDescender 而非 win 度量定位基线。`win_metrics()` 改为:
   `if os2.use_typographic_metrics() { (typo_ascender, -typo_descender) } else
   { (win_ascent, -win_descender) }`(ttf-parser 0.25 暴露该 flag;若无对应
   方法则直接读 fsSelection 位)。calibration fixture 补一个置位该 bit 的字体
   (Noto Sans SC 即是)。
2. **东亚自动加间距**:Word 默认在 CJK/拉丁边界注入 1/8 em(`w:autoSpaceDE/DN`
   默认开),破坏自然步进假设。textbox 段落 pPr 中显式写入
   `<w:autoSpaceDE w:val="0"/><w:autoSpaceDN w:val="0"/>`
   (pPr 子元素顺序:spacing 之前,按 CT_PPrBase 序列 autoSpaceDE→autoSpaceDN→…→spacing)。
3. **连字**:Word 365 对部分字体默认启用 OpenType 标准连字,重新连字会改变步进。
   rPr 尾部(w:lang 之后)追加 `<w14:ligatures w14:val="none"/>`;w14 已声明为
   mc:Ignorable,旧版本忽略仅损失该保险。同位置加 `<w:noProof/>`(rPr 序列中
   位于 w:i 之后、w:color 之前)关闭拼写检查红线(纯观感)。

### 3.3 数学/上下标的框爆炸修复(`text.rs::fragments`)

现状:每个 y_offset≠0 的 glyph 单独成 atomic 框,一页数学公式会产生数千锚点。
修正:相邻 glyph 若 `y_offset` 相等且 `x_offset` 均为 0,合并进同一个带 `dy` 的
fragment(fragment 增加当前 dy 状态,dy 变化时 close);atomic 语义只保留给
x_offset 超阈值的孤立 glyph。验收:`$x^2 + y_i^2 = z^{n+1}$` 一行 ≤ 8 个框。

### 3.4 边界防御

- 页面尺寸 > 31680 twips(Word 上限 22in)→ 警告并照写,visreg 记录 Word 实际行为后再定 clamp 策略。
- `w:t` 中的 `\u{0000}`–`\u{001F}`(除 tab)过滤 + 警告(Word 修复触发源)。
- 空 frame、零尺寸 shape、NaN 变换 guard(`emu()` 里 `is_finite()` 检查)。

---

## 4. P1 —— 视觉保真补全(3–4 周)

按用户可感知度排序,每项含:OOXML 方案 / 代码落点 / 验收。

### 4.1 渐变填充(`paint.rs` 重构 + `shape.rs`)

`paint::solid()` 拆为 `paint::fill(xml, exporter, paint, bbox)`,直接写 fill 元素:

- **线性** `Gradient::Linear` → `<a:gradFill rotWithShape="1"><a:gsLst>` +
  每个 stop `<a:gs pos="{offset×100000}">` + srgbClr(带 a:alpha),
  `<a:lin ang="{θ}" scaled="1"/>`。角度换算:typst 角度(数学系,起点在渐变方向)
  → DrawingML ST_PositiveFixedAngle(顺时针,自 x 正轴,1/60000 度),
  `ang = deg × 60000`(2026-07-09 visreg 定标)。
- **径向** `Gradient::Radial` → `<a:gradFill><a:gsLst>…<a:path path="circle">
  <a:fillToRect l/t/r/b/>`,center/radius 换算为相对 bbox 的千分比。
  焦点偏移(focal ≠ center)DrawingML 无法表达 → 警告 + 忽略焦点。
- **锥形** `Gradient::Conic` → 无对应物,走 4.3 的栅格化兜底。
- 文本填充为渐变:保持首色标 + 警告(Word 文本渐变要 w14:textFill,收益低)。

验收 fixture:`gradients.typ`(线性 0/45/90°、径向居中/偏心、锥形、带透明度)。

> **进度(2026-07-09)**:4.1 已落地形状填充的 DrawingML 原生线性/径向
> 渐变。线性写 `<a:gradFill rotWithShape="1">`、`gsLst`、`a:lin scaled="1"`,
> 角度按 visreg 校准为 `deg × 60000`;
> 径向写 `a:path path="circle"` + `a:fillToRect`，按 shape 自身 bbox 近似。
> parent-relative、非默认径向半径、不可表达 focal 参数会警告。锥形渐变、
> 文本渐变、平铺填充仍保持显式降级，等待 4.3 栅格化兜底。新增
> `fixtures/gradients.typ` 只覆盖当前应视觉通过的 supported cases，并已加入
> `scripts/visreg.sh` 默认列表。

### 4.2 平铺(Tiling)

`typst-render`(新增可选依赖,feature `raster-fallback`)把单个 tile 渲染为
PNG(2× 目标尺寸),`<a:blipFill><a:tile tx="0" ty="0" sx="100000" sy="100000"
flip="none" algn="tl"/>`,blip 走现有 media 管线。

### 4.3 栅格化兜底通道(基础设施,`raster.rs` 新模块)

统一入口 `rasterize(exporter, frame: &Frame, transform, ppi) -> Option<PicAnchor>`:
构造临时 `Frame` → `typst_render::render()` → tiny-skia Pixmap → png crate 编码
→ 走 image.rs 嵌入。默认 ppi=192,CLI `--raster-ppi` 可调。消费方:

- **裁剪组**(Group.clip = Some):
  - 快路径:clip 曲线是轴对齐矩形(4 条 Line + Close 判定)且组内单个 Image →
    `pic:blipFill` 加 `<a:srcRect l/t/r/b>`(千分比裁剪),零质量损失;
  - 慢路径:整组栅格化,按组 bbox 放置;
  - CLI `--clip=raster|ignore`(默认 raster)。
- **锥形渐变、奇偶填充曲线**(自相交时)。
- **SVG 图片**:`ImageKind::Svg` → typst-render 内部走 resvg,直接复用;
- **PDF 图片**:feature `pdf-images`(依赖 hayro,较重,默认开);
- **像素图片** `RasterFormat::Pixel` → png crate 直接编码,不走渲染。

### 4.4 形状任意变换烘焙(`shape.rs`)

Skewed 分支不再只取平移:把完整 2×3 矩阵直接作用到路径坐标
(Rect 先转 4 点 custGeom 路径),extent 取变换后 bbox,xfrm 不写 rot。
非等比缩放下描边宽度取 `sqrt(|det|)` 近似 + 警告。Rotated 分支保持现有
中心定位方案(Word 端语义更好、XML 更小)。图片的非等比缩放:直接烘焙进
cx/cy(无需警告);skew+rot 混合的图片走栅格化。

### 4.5 文本描边

rPr 尾部 `<w14:textOutline w14:w="{emu}" w14:cap="rnd" w14:cmpd="sng"
w14:algn="ctr"><w14:solidFill>…`。旧 Word 忽略(降级为无描边)。虚线描边文本
不支持 → 警告。

### 4.6 RTL 精确定位(`text.rs`)

glyph range 非单调时,按脚本分流:
- **非连写 RTL**(希伯来文等):按 cluster 逆序拆 atomic fragment,逐 cluster
  精确定位,rPr 加 `<w:rtl/>`;
- **连写脚本**(阿拉伯文系,Script ∈ {Arab, Syrc, Nkoo…}):孤立 cluster 会渲染
  成孤立形,必须整 run 一框(现状),并保留字体 GSUB(见 5.2 子集化例外),
  接受 justify 场景下的行内漂移 + 警告。
脚本判定用 `unicode-script` crate(轻)。

### 4.7 链接

`wp:anchor` 内 `<wp:docPr>` 支持子元素 `<a:hlinkClick r:id="rIdHl{n}"/>`;
外部 URL 进 document.xml.rels(TargetMode="External")。页内锚点(Position dest)
需要 bookmark:在目标页 carrier 段落写 `<w:bookmarkStart/End w:name="p{n}"/>`,
链接用 `<a:hlinkClick r:id="" a:action=""/>`? —— Word 的 shape 内跳转支持差,
P1 只做外部 URL,页内跳转记为 P4。

---

## 5. P2 —— 字体工程、兼容矩阵、性能(3 周)

### 5.1 TTC → TTF 提取(`font.rs`,解锁 macOS 系统 CJK 字体)

Songti.ttc / PingFang.ttc 目前跳过。实现 `extract_ttf(data, index) -> Vec<u8>`:
读 ttcf header 定位 face 的 table directory → 拷贝全部 table(共享数据自然去重
失效,可接受)→ 重建 sfnt offset table(searchRange 等三元组)→ 重算 head
checkSumAdjustment(0xB1B0AFBA 规则)。~150 行 + 两个测试(用系统 Songti 做
golden:提取后 ttf-parser 可解析且 glyph 数一致)。fsType 门禁照旧生效。

### 5.2 字体子集化(默认开,`--no-subset-fonts` 逃生门)

嵌入 7 个全量字体 ≈ 11MB/文档,不可接受。方案:**fontations/klippa**
(纯 Rust,Google 维护)按 **Unicode 码点集**子集:

- 收集:`FontCollection` 记录每字体实际用到的 char 集(text.rs 在 font_ref 时上报)。
- 简单脚本(拉丁/CJK/西里尔…):子集时**剥除 GSUB/GPOS/GDEF**——既缩体积,又从
  根本上禁止 Word 重新 shaping(与 kern=0/ligatures=none 策略互为双保险)。
- 连写脚本命中的字体(4.6 判定):保留 layout tables,只裁字形。
- 子集失败(表损坏等)→ 警告 + 回退全量嵌入。
- GUID/ODTTF 基于子集后字节派生,保持确定性输出(同输入 → 逐字节相同 docx,
  已是现状,写进保证)。
验收:calibration.docx 从 ~11MB 降到 < 1.5MB;visreg 不回归。

### 5.3 兼容矩阵(人工 + 半自动,产出 `docs/COMPAT.md`)

| 渲染端 | 测试方式 | 通过标准 |
|---|---|---|
| Word 365 macOS | 人工 checklist(发版前) | 零修复对话框 + 叠加抽查 |
| Word 365 Windows | 虚拟机人工,每 minor 一次 | 同上 |
| WPS Office | 人工 | 无崩溃,主要内容正确 |
| LibreOffice 7.6+ | visreg 自动 | < 0.5% 像素差 |
| Google Docs 导入 | 人工,记录已知问题 | 尽力(anchored 支持差,只记录) |

每项功能在矩阵中标注支持度,README 链接。

### 5.4 性能与规模

- 基准:`benches/export.rs`(criterion):100 页文本、50 页数学、20 页图形。
- 并行:页级并行(rayon)。改造:`Exporter` 拆成每页局部收集器
  (fonts/media 的注册返回占位 key,合并阶段统一分配 rId/docPr/relativeHeight,
  保证输出顺序确定)。目标:M2 Max 上 100 页 < 0.8s。
- XML 体积:`Xml` 缓冲区按页预估 capacity;锚点数 > 30000 时警告
  (Word 打开性能悬崖,实测数据写入 COMPAT.md)。

---

## 6. P3 —— 发布与产品化(2 周)

### 6.1 分发

- crates.io:`typst-docx`(lib)+ `typst-docx-cli`(bin `typst-docx`)。
  版本策略:`0.<typst minor>.<patch>`(0.15.x 对应 typst 0.15),typst 升版后
  两周内跟进。
- cargo-dist:GitHub Releases 预编译(macos-universal2 / windows-x64 /
  linux-x64-musl / linux-arm64-musl);Homebrew tap `mioyisama/tap/typst-docx`。
- MSRV 与 typst-kit 对齐(当前 1.92);`cargo deny` 审计许可证。

### 6.2 API 与选项定型(0.1.0 冻结面)

```rust
pub struct DocxOptions {
    pub embed_fonts: EmbedFonts,        // Subset(默认) | Full | None
    pub raster_ppi: f64,                // 192.0
    pub clip: ClipMode,                 // Raster(默认) | Ignore
    pub pdf_images: bool,               // true
}
pub fn docx(doc: &PagedDocument, options: &DocxOptions) -> DocxOutput;
// DocxOutput.warnings: Vec<Warning> 结构化(code + message + count),
// CLI 聚合输出,--warnings=json 供集成方消费。
```

CLI 同名 flag 一一映射;现有 `docx(&doc)` 保留为 default options 的便捷入口。

### 6.3 文档

- README 双语化(中/英),动图对比(Typst PDF vs Word 截屏叠加)。
- `docs/HOW-IT-WORKS.md`:R1 基线规则、行段模型、ODTTF——把计划书里的
  技术决策沉淀成维护文档。
- FAQ:为什么不可编辑 / 为什么要 Word 2013+ / 字体版权与 fsType。

### 6.4 集成面

- `--watch`(typst-kit `watcher` feature,增量重导出)。
- stdin 输入(`typst-docx - < main.typ`,typst-cli 同款 STDIN_ID 模式)。

---

## 7. P4 —— 远期(不承诺排期)

- **字形轮廓模式** `--text-mode=outlines`:用 ttf-parser 取 glyph outline →
  custGeom,文本变矢量形状。零字体依赖、零 shaping 风险(法务上也绕开 fsType),
  代价是不可选中/复制、XML 膨胀 ~5×。作为"绝对保真"档位。
- **typst-pptx**:drawing 层(write/shape/image/text 的 DrawingML 部分)与
  WordprocessingML 解耦后复用,页 → 幻灯片。评估市场后立项。
- **WASM**:`typst-docx` 编译到 wasm32(zip/miniz 纯 Rust,无阻碍),配合
  typst.ts 做浏览器内导出 demo 页(营销入口)。
- 页内链接跳转、脚注锚点等交互性补全。

## 8. 测试与质量体系(贯穿)

- 单元/不变量测试:每个新 OOXML 元素必须带 roxmltree 子元素顺序断言
  (修复对话框的头号来源就是顺序错误,R2)。
- visreg fixture 目录随功能同 PR 增长;每个 P1 条目至少 1 个专属 fixture。
- 发版前人工 checklist(`docs/RELEASE-CHECKLIST.md`):Word mac 打开全部
  fixture、抽查 3 页 PDF 叠加、WPS 冒烟。
- 确定性:CI 中同一输入构建两次比对 sha256(已具备:FNV GUID、固定 zip 时间戳)。

## 9. 风险登记簿(增量)

| # | 风险 | 影响 | 对策 |
|---|---|---|---|
| R8 | Word 重 shaping(连字/字距/东亚间距)造成漂移 | 中 | P0 三开关 + P2 剥 GSUB/GPOS;残留场景警告 |
| R9 | LibreOffice 渲染 ≠ Word 渲染,visreg 假绿 | 中 | 发版前 Word 人工 checklist 兜底 |
| R10 | 万级锚点下 Word 打开卡顿 | 中 | 3.3 框合并 + 5.4 阈值警告 + 实测数据 |
| R11 | 子集化破坏 Word 字体加载(cmap/name 缺失) | 高 | klippa 保守配置 + 全量回退开关 + visreg |
| R12 | 系统字体许可(fsType 之外的条款) | 低 | 默认尊重 fsType;文档声明用户责任 |
| R13 | typst 内部 API(FontInstance 等)升版破坏 | 中 | 版本策略绑定 typst minor;升版 PR 模板含 API diff 清单 |

## 10. 里程碑节奏

```
第 1 周     P0 完成 → tag v0.0.2(内部)
第 2–5 周   P1 完成 → tag v0.0.3(公开预览,发 typst forum / 少数派试用)
第 6–8 周   P2 完成 → 兼容矩阵发布
第 9–10 周  P3 完成 → v0.1.0 crates.io + Homebrew,正式公开
之后        跟随 typst minor 版本节奏,P4 按反馈立项
```
