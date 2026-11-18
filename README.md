# MORI

一个用 Rust 编写的极简矢量动画工作室。深色工作区、白色宽银幕画纸、黑色压力笔迹，以音乐拍点组织动画。

## 运行

需要 Rust 1.92+（建议 `rustup update stable`）。

```sh
./scripts/run.sh
# 或者：cargo run --release --locked
```

macOS：可直接运行；导出视频需安装 FFmpeg：`brew install ffmpeg`。使用 `./scripts/bundle-macos.sh` 生成 `target/MORI.app`。支持 Apple Silicon 和 Intel，按本机架构构建。应用使用 OpenGL 3.3 / macOS OpenGL 4.1 核心功能。

Linux（Ubuntu）：

```sh
sudo apt install build-essential pkg-config libasound2-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev
cargo run --release --locked
```

Linux 文件选择器使用 XDG Desktop Portal；桌面环境需有可用的 portal 后端。Windows 使用相同 Cargo 命令构建，视频导出需将 FFmpeg 加入 PATH。

可以用 `cargo run --release -- --demo` 打开内置的轨道动画示例，或 `cargo run --release -- project.mori` 打开工程。新建工程始终为空白。

## 画板

- 矢量轨迹记录坐标、压力和笔宽；鼠标使用满压力，数位笔使用系统压力。macOS 额外接入 AppKit 原生 tablet 事件，并在压力变化时请求重绘，用于 Huion 等系统数位笔驱动。
- 轻点立即成点。笔尖保持原位时，加压会实时扩张该点；松压保留已画出的墨迹。
- 压力曲线、最小笔径、采样距离、Hermite / 线性插值、液化力度可设置。
- 每层独立记忆笔宽；空心双边光标可在白色画纸和黑色笔迹上识别。
- 中键拖动画布；触控板捏合或 Command / Ctrl + 滚轮缩放，Fit 恢复适合窗口。
- 普通笔刷在新时刻落下第一笔，会创建空白曝光；液化第一笔会复制该层上一曝光，再改变矢量点的位置，保留原压力。
- 没有曝光的时刻沿用该层此前的内容。删除曝光恢复延续，空白曝光明确中断延续。
- 撤回、重做没有固定步数上限，历史使用 Arc 共享未改动的矢量数据；历史保留在当前会话中。

## 音乐与图层

Rhythm 页面导入 WAV / MP3 / FLAC / OGG / AAC / M4A，显示波形与曝光轨道。音乐播放由声卡回调的播放时间戳驱动画面，并补偿回调与实际播放之间的输出延迟；没有音乐时使用单调时钟。播放与导出都在曝光的准确时间戳生效，取样后保持对应曝光，P1 的选中时间始终位于格点。实际声卡输出延迟与 Huion 硬件手感需在目标 Mac 上实测。

默认 120 BPM、1/8 拍步长；支持多个以秒定位的 BPM 起始点，边界本身也是格点。可以在当前格点增加起始点，编辑其时间与 BPM。当前实现中，修改 BPM 或起始点保留既有曝光的秒数位置，并立即重算格点。

默认画纸比例为 2.39:1；自下而上为 Paper、Ink / XOR、Movement、Atmosphere / Motion blur。底层保持最底，其他图层可创建、改名、隐藏、重排、删除、反色和改变混合模式。

| 模式 | 作用 |
| --- | --- |
| Base | 白底黑色矢量绘制 |
| XOR | 以白色为中性值的黑白异或，重叠黑色变白 |
| Darken / Lighten | 对图层完整画纸逐通道取暗 / 取亮，反色可改变纸色 |
| Move | 只保留一个手势，按首尾差移动所有下层画面 |
| Motion blur | 按手势方向积分动态快门，多个手势依次叠加 |

Move / Motion blur 提供 Essential、Staccato echoes、Chromatic prism、Neon ribbons 四种效果风格和力度设置。风格效果可产生颜色。效果手势的方向在当前选中层显示为细箭头；箭头不导出。

## 保存与导出

`.mori` 是 ZIP 工程，包含 `document.json` 和嵌入的原始音频，移动工程无需另找音乐文件。保存先写临时文件，再替换目标；关闭、打开和新建前对未保存内容提供提示。

设置页导出 MP4 / H.264，默认 1920 宽、60 FPS、CRF 18、AAC 音乐。可设置范围、宽度、FPS、质量和音乐开关。矢量按输出分辨率渲染，按视频帧时间取样原生曝光并保持，无帧混合；GPU 合成与画板共用实现。镜像、洋葱皮、光标和方向提示属于编辑辅助，不写进导出。编码在后台运行，使用开始导出时的工程快照，可取消；取消会删除未完成的临时文件，并保留已有的目标视频。

## 默认快捷键

所有动作均可在设置页重新绑定，重复组合会提示冲突。Windows / Linux 的 Command 对应 Ctrl。

| 操作 | 快捷键 |
| --- | --- |
| P1 画板 / P2 音乐与图层 | Option+1 / Option+2 |
| 设置 | Command+, |
| 保存 / 打开 / 新建 | Command+S / O / N |
| 普通笔刷 ↔ 液化 | L |
| 撤回 / 重做 | Z / Y |
| 上一格 / 下一格 | A / D |
| 上一层 / 下一层（向上 / 向下） | W / S |
| 缩小 / 放大笔刷 | [ / ] |
| 镜像 / 洋葱皮 | M / O |
| 播放暂停 / 系统全屏 | Space / F |

## 验证

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
# 在可用图形桌面中，验证 GPU 合成并输出 PPM：
cargo run -- --smoke artifacts/gpu
# 无显示器的 Linux 可使用：
xvfb-run -a cargo run -- --smoke artifacts/gpu
```

测试覆盖可变 BPM 边界、首笔继承规则、液化压力与前帧不变、原地加压、无限步历史，以及含音频的工程保存读取。GPU 冒烟模式验证笔迹、移动、定向模糊、彩色效果与原地加压的实际像素面积增长；还会调用 FFmpeg 编码 1 秒含音轨的 60 FPS 视频，并验证取消导出保留已有目标文件，因此运行此模式需要 FFmpeg。桌面截图与交互脚本为 `scripts/ui-smoke.py`，需要 Linux X11、xdotool 和 Pillow。

本工作区的自动验证环境为 Linux；macOS / Windows 构建、真实 Huion 压感、声卡端到端同步和复杂工程的实际帧率需要在对应硬件上验证。
