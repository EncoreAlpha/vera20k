# 多人战役（VMCE）

VMCE 是 VERA20K 的可选 LAN 战役扩展：最多 4 名玩家推进一套外部战役目录
（`Maps/VMCE/<系列>/`），使用原生战役结算页与确定性的共享任务。它不加载原版
Cooperative 战役模式，也不读写 `Coopsave.ini`。VMCE 不设观察者席位。

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
| 战役流程 | `NextScenario`、`EndOfGame`、外部系列目录、Entry 关严格校验 |
| LAN 验证 | 当前构建的双进程和四进程胜利与下一关采集通过，300 个 checksum 一致 |
| 失败重试 | 当前构建的客人投票 → 房主重开同一冻结关卡采集通过，300 个 checksum 一致 |
| 结果页 | `0x103` 胜利板 + 统计页；原生 BC6 失败统计页；结果页无计时器 |
| i18n | en/zh-CN 真值；其余语言英文占位 |

当前缺陷与已确定但尚未实现的新胜负规则、关卡间重选角色和关卡解锁，见
[中文待办](vmce-backlog.md)。

## 从这里开始

- [VMCE 玩家与地图作者指南](vmce-guide.md) — 战役目录、`campaign.ini`、LAN 流程、
  结果页、测试与故障排查。
- [VMCE 待办（中文）](vmce-backlog.md) — 已确认目标、修复顺序和验收范围。
- [VMCE backlog](../vmce-backlog.md) — 英文工程台账。
- [VMCE 基线](../vmce-baseline.md) — 上游同步、版本与真实采集基线（英文）。
- [English hub](../vmce.md)。

## 与原生 Cooperative 模式的关系

VMCE 拥有自己的模式、大厅板、战役数据、结果页和统计投影。共享的 LAN 轮次协议
（begin/options/ack/go、关卡身份、重试投票原语）位于公共会话层，并由一层薄的
`vmce_round` facade 包装给 VMCE 使用。VMCE 不复制原生 Cooperative 结算 UI；失败页
直接使用原生 BC6 对话框作为统计视图。

## 验收

```sh
python -m tools.vmce_lan_acceptance run --suite smoke --exe "<VERA20K程序路径>" --retail-source "<原版游戏目录>" --series "<VMCE战役目录>"
```

请替换尖括号内的路径。一次运行中的场景共用同一战役目录，所选战役必须适合套件中的
所有场景；分别验证胜利或重试时，可用 `--scenario` 指定场景并选择相应战役。

VMCE 场景只允许 VMCE 检查点；脚本会拒绝经典 `lan-live-coop-*` 检查点。收据、截图
和对比数据留在 `.scratch/`，不提交仓库。
