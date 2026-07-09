# typst-docx

把 Typst 排版结果导出为**视觉上忠实**的 `.docx` 文件。

与语义转换不同,本工具消费 Typst 已排版的 `PagedDocument`,把每一个页面元素
(文本行段、形状、图片)作为**页面绝对定位**的 OOXML 绘图对象写入 Word 文档,
目标是在 Word 中 100% 缩放下与 Typst 的 PDF 输出视觉一致。产物**不适合继续编辑**。

## 使用

```sh
cargo build --release
./target/release/typst-docx input.typ            # 输出 input.docx
./target/release/typst-docx input.typ -o out.docx
```

常用参数(与 typst CLI 对齐):`--root DIR`、`--font-path DIR`、
`--ignore-system-fonts`、`--input key=value`。

## 要求与限制

- **需要 Word 2013 或更新版本**(输出使用无 VML 回退的 `wps` 文本框,旧版
  Word / 极老的第三方阅读器无法显示)。
- 每页一个 Word 分节(section),页边距为 0,内容全部为锚定绘图。
- 文本按行段切分:两端对齐的拉伸空格、大字距(kern)、上下标偏移都会
  按精确坐标拆分重新定位;Word 端以 `w:kern=0` 的自然步进渲染。
- 基线规则:文本框内单段落 `lineRule="exact"`,基线位于
  `框顶 + 行高 − descent`(OS/2 Windows 度量),即 M1 标定规则 R1。
- 字体默认嵌入(ODTTF 混淆,ECMA-376 §15.2.13):
  - `fsType` 限制嵌入的字体、TTC 集合会跳过并警告;
  - CFF/可变字体嵌入但给出警告。
- 部分支持形状线性/径向渐变填充;文本渐变、锥形渐变、平铺填充仍会
  降级并警告(首个色标/黑色)。
- 不支持(降级 + 警告):裁剪、链接、倾斜或非等比缩放变换(只保留平移)、
  SVG/PDF/原始像素图片、文本描边、奇偶填充规则、虚线相位。

## 结构

```
crates/typst-docx       导出库:frame 展平 → text/shape/image 绘图对象 → OPC 打包
crates/typst-docx-cli   瘦 CLI(typst-kit 提供文件/字体/包/诊断)
fixtures/               验收用例:blank / calibration(基线标定)/ shapes / images / gradients
```

## 验收

- `cargo test`:单元测试 + roxmltree 结构不变量(sectPr/anchor/rPr 子元素顺序、
  docPr 唯一、ODTTF 混淆头等)。
- `officecli validate` 对全部 fixture 通过。
- 最终视觉验收:在 Word for macOS 中打开 `fixtures/calibration.docx`,
  应无修复对话框,且字形基线压在红色标定线上;与 `calibration-ref.pdf`
  (typst 直接输出)叠加对比。
