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

当前 fork 版本：`0.2.9-vmce.6`，已合并上游 `v0.2.9`。LAN 双方应运行同一 VMCE 构建。

## 状态

| 模块 | 状态 |
|---|---|
| 大厅 | 四个角色、所属方/准备/颜色板、非法战役的阻塞/内联错误 |
| 战役流程 | `NextScenario`、`EndOfGame`、外部系列目录；所选地图的 `campaign.ini` 关卡严格校验，LAN 准入校验清单中每张地图的字节 |
| 逐玩家胜负 | 按任务实际生效的结盟判定胜方/败方；主所属方被消灭的玩家保留原有授权的控制，任务继续等待地图胜负触发器 |
| 关间房间 | 全员返回指定下一关的战役房间，重新选择角色并准备；强制加入/离开与角色数不足的拦截 |
| 关卡解锁 | 结果和下一关身份确认后，仍需经过继续流程才写入本机解锁；仅确认后退出可能丢失解锁（B57），不要求加载下一关 |
| LAN 验证 | 当前构建的双人、三人、四人胜利与下一关、双人失败重试采集通过，300 个 checksum 一致；各端本机解锁文件已核对 |
| 结果页 | `0x103` 胜利板 + 统计页；原生 BC6 失败统计页；结果页无计时器 |
| i18n | en/zh-CN 真值；其余语言英文占位 |

未完成的缺陷与真实联机验收（跨机器、关间中途加入、丢包、B10 送达）见
[中文待办](vmce-backlog.md)。

## 从这里开始

- [VMCE 玩家与地图作者指南](vmce-guide.md) — 战役目录、`campaign.ini`、LAN 流程、
  结果页、测试与故障排查。
- [VMCE 待办（中文）](vmce-backlog.md) — 已确认目标、修复顺序和验收范围。
- [VMCE 代码与界面审查](vmce-audit.md) — 已确认问题、解决方案和验证条件。
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
`2p-retry` 或 `2p-final`（末关场景需要 Entry 就是战役最后一关的系列）。一次运行中的场景共用
同一战役目录，请选择适合该场景的系列。共享显存有限的机器上建议一次只跑一个场景：刚退出的
采集进程可能仍持有显存，紧接着跑下一个场景会在 wgpu 里报 “Out of Memory”（退出码 101），
而每个场景单独跑都能通过。

VMCE 场景只允许 VMCE 检查点；脚本会拒绝经典 `lan-live-coop-*` 检查点。收据、截图
和对比数据留在 `.scratch/`，不提交仓库。
