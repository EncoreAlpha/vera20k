# 多人战役（VMCE）

VMCE 是 VERA20K 的可选 LAN 战役扩展：最多 4 名玩家推进一套外部战役目录
（`Maps/VMCE/<系列>/`），支持逐玩家胜负、关间重选角色的战役房间、本机持久化的关卡解锁，
并使用原生战役结算页与确定性的共享任务。它不加载原版 Cooperative 战役模式，也不读写
`Coopsave.ini`。VMCE 不设观察者席位。

## 构建

VMCE 与 Ares、Phobos 一样是编译期能力：

```sh
python -m tools.cargo_run -- build --profile fast --features vmce --bin vera20k
```

不带 `--features vmce` 时，不注册 `Multiplayer Campaign` 模式行，不扫描
`Maps/VMCE/`，VMCE 专属的会话/LAN 字段也会被裁掉。

当前 fork 版本：`0.3.2-vmce.6`，已合并上游 `v0.3.2`（合并提交 `70d569b86`）。LAN 双方应运行同一 VMCE 构建；合并后的完整验收仍待重跑。

## 状态

| 模块 | 状态 |
|---|---|
| 大厅 | 四个角色、所属方/准备/颜色板、非法战役的阻塞/内联错误 |
| 战役流程 | `NextScenario`、`EndOfGame`、外部系列目录；所选地图的 `campaign.ini` 关卡严格校验，LAN 准入校验清单中每张地图的字节 |
| 逐玩家胜负 | 按任务实际生效的结盟判定胜方/败方；主所属方被消灭的玩家保留原有授权的控制，任务继续等待地图胜负触发器 |
| 关间房间 | 全员返回指定下一关的战役房间，重新选择角色并准备；强制加入/离开与角色数不足的拦截 |
| 关卡解锁 | 结果和下一关身份确认后，仍需经过继续流程才写入本机解锁；仅确认后退出可能丢失解锁（B57），不要求加载下一关 |
| LAN 验证 | 在 0.3.2 合并前的 B4 基线（`6322d411b`）上通过：双人、三人、四人胜利与下一关、双人失败重试，300 个 checksum 一致；各端本机解锁文件已核对。合并基线上的重跑待做（B105） |
| 结果页 | `0x103` 胜利板 + 统计页；原生 BC6 失败统计页；结果页无计时器；按 800x600 创作，适配 1024x768 默认窗口（B103） |
| i18n | 10 种旧语言各有 32 条 VMCE 键：en/zh-CN 真值，其余英文占位；上游新增的越南语无 VMCE 键（B101） |

## 已知未修项

权威清单见[审查总文档](vmce-review.md)，当前共 55 条（45 条 A 审查项 + 10 条仅 B 记录），另有 0.3.2 同步后的新基线项 B101–B105。
与本页状态相关的玩家可见问题：

- **A44/B99：** 关间房间的客人可能看不到下一关地图预览，也收不到「该地图有简报」提示；
  房主在地图带可解码预览数据时能看到（已用带预览的系列地图真机确认）。
- **A45/B100：** 满员的 2/2 或 4/4 VERA 关间房间在 LAN 浏览器仍显示可加入，实际加入会收到
  `TXT_GAME_FULL`。
- **A38/B90：** 胜利/战役完成统计页的长玩家名没有像结算板、失败页那样按原生规则截断。
- **A41/B95：** 地图的 `Intro`/`Brief`/`Action`/`Win`/`Lose`/`PostScore`/`PreMapSelect`
  场景电影在 VMCE 开局与关间继续路径上不会播放。

合成结算/失败页采集 fixture 需要英文台账里记录的 B91 workaround；那是工具问题，不是玩家可见缺陷。
跨机器与异常场景验收见[英文台账](../vmce-backlog.md)。

## 从这里开始

- [VMCE 玩家与地图作者指南](vmce-guide.md) — 战役目录、`campaign.ini`、LAN 流程、
  结果页、测试与故障排查。
- [VMCE 审查总文档](vmce-review.md) — 合并后的待修复项，按修复优先级排列。
- [VMCE backlog](../vmce-backlog.md) — 英文工程台账。
- [VMCE 基线](../vmce-baseline.md) — 上游同步、版本与真实采集基线（英文）。
- [English hub](../vmce.md)。

## 与原生 Cooperative 模式的关系

VMCE 拥有自己的模式、大厅板、战役数据、结果页和统计投影。共享的 LAN 轮次协议
（begin/options/ack/go、关卡身份、绑定 nonce 的结果与下一关一致性、重试投票原语）位于
公共会话层，并由一层薄的 `vmce_round` facade 包装给 VMCE 使用；整套系列准入校验由 VMCE 的
LAN service 自己持有，并作为客人报告地图可用的前提。VMCE 不复制原生 Cooperative 结算 UI；
失败页直接使用原生 BC6 对话框作为统计视图。

## 验收

```sh
python -m tools.vmce_lan_acceptance run --exe "<VERA20K程序路径>" --retail-source "<原版游戏目录>" --series "<VMCE战役目录>" --scenario 2p-win --players 2
```

`--players` 为 `4p-*` 场景选择 2、3 或 4 行；`--scenario` 可选 `4p-win`、`2p-win`、
`2p-retry` 或 `2p-final`（末关场景需要 Entry 就是战役最后一关的系列）。`--entry` 指定从系列中的哪张地图启动（默认 `stage01/all01umd.map`）；系列不是从默认第一关开始时（例如 `2p-final` 的末关系列），传入末关路径并让 `campaign.ini` 的 `Entry` 指向同一关。一次运行中的场景共用
同一战役目录，请选择适合该场景的系列。共享显存有限的机器上建议一次只跑一个场景：刚退出的
采集进程可能仍持有显存，紧接着跑下一个场景会在 wgpu 里报 “Out of Memory”（退出码 101），
而每个场景单独跑都能通过。

VMCE 场景只允许 VMCE 检查点；脚本会拒绝经典 `lan-live-coop-*` 检查点。收据、截图
和对比数据留在 `.scratch/`，不提交仓库。
