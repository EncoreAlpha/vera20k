---
layout: default
title: VMCE 玩家与地图作者指南
lang: zh-CN
---

# VERA Multiplayer Campaign Expansion（VMCE）玩家与地图作者指南

[返回文档首页](../) · **简体中文**

> **警告**
> VERA Multiplayer Campaign Expansion（以下简称 VMCE）目前是 VERA20k 的开发中功能。本文描述的是当前开发分支的行为，接口、地图格式和可用功能都可能随版本变化。不要把开发版行为当作稳定 API 写进长期维护的战役。

VMCE 是 VERA20k 内置的 LAN 多人战役扩展。当前版本实现并验收的是合作战役推进，命名和配置边界为后续其他多人战役规则保留扩展空间。它不是 Phobos、Ares 或 NPatch 的插件，也不加载任何外部 DLL；它复用 VERA20k 已有的原版规则、地图、触发器和资源读取路径，把一张战役地图交给多名玩家共同游玩。

VMCE 不支持观察者。如果房间已有观察者，请先让他们加入玩家席位或离开房间，再由房主选择 VMCE 战役。

胜利采集会等待结果页入场动画结束，再检查结果板并点击准备和继续。「随机」角色显示任务开始时实际分配的所属方。

本页写给两类人：

- **玩过原版《尤里的复仇》的玩家**：想知道 VMCE 和原版战役、原版 Cooperative 模式有什么区别，以及怎么开一局。
- **做过原版 YR 地图和触发器的制作者**：想把单机任务改造成多人战役，或者从零写一张多人战役图；当前合作规则见下文。

> **备注**
> 本页只描述 VMCE 新增或覆盖的行为。没有提到的原版规则、INI 键、动作和事件仍然按原版读取；如果本页与代码不一致，以代码为准。

本文描述当前工作区的行为。逐玩家胜负、关卡间房间和关卡解锁的主要流程已有定向测试和本机多进程覆盖；
[审查总文档](vmce-review.md)是权威的待修复清单（目前 55 条：45 条 A + 10 条仅 B），真实联机
场景覆盖见[英文台账](../vmce-backlog.md)。当前重点未修项包括关间客人预览/简报提示（A44）、
满员房间仍显示可加入（A45）和统计页长名字（A38）；合成结算/失败页采集需要 B91 的 workaround。

如果你只想知道怎么玩，读「安装与启动」「玩家指南」「故障排查」即可；如果你要写图，直接从「地图制作者指南」和「参考」开始。

## 与原始版本的关系

原版 YR 的战役按「一个玩家 + 一个当前所属方」推进：胜负由 PlayerPtr 和战役状态决定，关卡之间由战役存档串联。原版 Cooperative 模式（合作战役）由 CoopCampMD 和 BC6 结果页驱动，按战役关卡推进，并在任务结束时写 `coopsave.ini`。

VMCE 走的是另一条路：

- 战役目录、关卡顺序和每关角色由外部的 `campaign.ini` 描述，而不是 CoopCampMD。
- 每个角色对应地图中的一条主 House（所属方），没有遭遇战式的「按玩家位置分配出生点」。
- 胜负只由地图触发器的结局动作决定；单方主 House 全灭不会自动结束任务。
- 地图里的角色 House 和非角色 House 均不会仅因没有单位而触发多人自动判负或刷「被击败」消息。
- 任务内按战役语义关闭随机箱和 ShortGame，并跳过 Special House 自动全体结盟与 Secret Lab 随机分配；大厅里的 ShortGame/Crates 开关对 VERA 显示为禁用。
- 胜利和战役结束使用原版战役计分页 `0x103`；失败使用 BC6 统计页。客人准备下一关或投票重试，房主在相应条件满足后继续。不写 `coopsave.ini`；已解锁关卡保存在本机。

VMCE 不改变原版战役和普通遭遇战的判定。原版单人战役仍然按原版规则运行；普通多人模式也不会因为 VMCE 的地图格式而改变。

## 安装与启动

### 前置条件

0. 安装一份可用的 VERA20k 开发版，并准备好原版《尤里的复仇》的规则、地图和美术资源。VMCE 不附带原版资源，也不分发 EA 的任何游戏文件。
1. VMCE 不需要 Syringe、Phobos.dll 或 Ares.dll。不要把 VMCE 当成 Phobos 扩展去配置加载器。
2. 所有参战玩家必须能读取同一套原版资源。VMCE 只交换战役和地图的**身份**，不传输文件。

### 编译开关

VMCE 是可选的编译期能力，和 Ares、Phobos、VeraX 一样不在默认构建里：

```bash
python -m tools.cargo_run -- build --profile fast --features vmce --bin vera20k
python -m tools.cargo_run -- build --release --no-default-features --features vmce --bin vera20k
```

不带 `--features vmce` 的构建不会注册 `Multiplayer Campaign` 模式行，也不会扫描 `Maps/VMCE`；VMCE 专属的会话状态、LAN 轮次/选项字段与大厅辅助逻辑同样按开关裁剪，非 VMCE 构建不包含它们。发布工作流使用 `--no-default-features`；要把 VMCE 放进发布包，需要显式加 `--features vmce` 重新构建。

本 fork 版本为 `0.3.2-vmce.N`（当前 `0.3.2-vmce.6`，已合并上游 `v0.3.2`，合并提交 `70d569b86`）；LAN 双方应运行同一 VMCE 构建。

### 战役目录

VMCE 的战役放在 VERA20k 的设置目录下。**每个直接子目录是一个系列战役**，`campaign.ini`、地图和（可选的）系列 CSF 都放在该子目录内：

```text
<VERA20k 设置目录>/Maps/VMCE/
├── 林德拉克计划/
│   ├── campaign.ini
│   ├── ra2md.csf          可选；系列自己的 CSF
│   ├── all01umd.map
│   ├── all02umd.map
│   └── ...
└── 另一个系列/
    ├── campaign.ini
    ├── ra2md.csf
    └── ...
```

- `campaign.ini` 必须位于 `Maps/VMCE/<系列目录>/` 这一层；一个子目录代表一个可被发现的系列战役。
- `campaign.ini` 里的 `Entry`、`Scenario1..ScenarioN`、`NextScenario` 都以**该系列目录**为根，不要再写系列目录名。
- 地图可以是系列目录下的松散 `.map` 文件，也可以放在其子目录；不能是 `.mix` 内的地图或随机地图生成器产生的临时地图。
- 同一个地图只能属于一个有效系列。如果一张地图同时被两个系列列出，VMCE 会拒绝选择它。
- 系列显示名默认取目录名；`campaign.ini` 的 `[Campaign] Name=<CSF 标签>` 可覆盖。解析顺序是：**系列目录的 `ra2md.csf` → 基础游戏 CSF → 目录名**。例如 `Name=Name:AD03` 会显示为「林德拉克计划」。
- 旧的 `Maps/VeraCampaign/` 和过渡版 `Maps/Vmce/` 目录不再读取；把每个系列整体移到 `Maps/VMCE/<系列>/`，目录内 `campaign.ini` 的相对路径不用改。

### 最小 campaign.ini

```ini
[Campaign]
Name=Name:AD03
Entry=all01umd.map
ScenarioCount=2
Scenario1=all01umd.map
Scenario2=all02umd.map

[Scenario:all01umd.map]
PlayerCount=2
Player1=Americans House
Player2=Alliance House
NextScenario=all02umd.map

[Scenario:all01umd.map:French House]
Player1Control=yes
Player2Control=yes

[Scenario:all02umd.map]
PlayerCount=2
Player1=Americans House
Player2=Alliance House
EndOfGame=yes
```

这个例子说明四件事：

- `Entry` 是当前开始的那一关；它必须出现在 `Scenario1..ScenarioCount` 中，路径相对系列目录。
- 每关的 `[Scenario:<地图相对路径>]` 段把 `Player1..PlayerN` 绑定到地图 `[Houses]` 中的 House 名称。
- 可选地，`[Scenario:<地图相对路径>:<House 名称>]` 段把一条额外 House 的控制权授予某个角色。这个例子把 `French House` 同时授予两名玩家。
- `Name` 是可选的大厅系列显示名，值为 CSF 标签；不写时显示目录名。

### 启动一局

1. 房主和所有玩家放好同一份 `Maps/VMCE/` 内容。系列目录名、`campaign.ini` 内容，以及 `ScenarioN` 里列出的**每一张地图**都必须一致：房主在联机选项中发布整套系列的内容摘要（`campaign.ini` 字节、清单顺序、每张地图的相对路径、字节长度与内容），客人在本地后台校验通过后才报告地图可用，未通过或在跑时不能准备或开局。所选地图的字节与 `campaign.ini` 另外单独校验。
2. 进入 LAN 房间，把游戏模式选为 **Multiplayer Campaign**。游戏会从外部系列目录中列出可用的战役地图，显示名来自 `Name`/目录名。
3. 房主选择地图；客人的地图状态必须是可用（Ready），否则无法开始。
4. 每名玩家确认角色、准备；房主开始。
5. 装载完成后，双方进入同一张地图、同一个确定性模拟。双方看到的开局、剧本和结果应当一致。

如果客人缺少地图、`campaign.ini`，或系列中任何一张地图的字节与房主不同，他会在准备阶段被拒绝（整套系列校验在后台进行，不阻塞大厅和心跳）；VMCE 不把系列 CSF 纳入一致性校验，也不会自动把文件发给客人。

### 如何阅读本页的代码片段

本页的代码块都是 INI 片段：

- `[...]` 是段名；`<...>` 是需要按你的地图替换的名称或路径。
- `;` 后面是注释，原版读取器会忽略。
- 未在片段中出现的键表示保持原版默认值；VMCE 没有为这些键发明新默认值。

## 玩家指南

### 角色、主 House 与共享 House

每个参战角色对应一张地图主 House。角色不是「随便选一个国家」：主 House、它的国家、颜色、基地和初始资金都由地图和 `campaign.ini` 决定。

- 你的主 House 由 `Player1..PlayerN` 决定。
- 每关最多允许 `PlayerCount` 名玩家；实际参战人数不能超过它。当前版本没有专门的空位 AI 接管，建议让 `PlayerCount` 与实际参战人数一致。
- 共享 House（例如例子里的 `French House`）由 `campaign.ini` 的 `PlayerNControl=yes` 授权给指定角色。共享 House 的单位由被授权的角色控制（多个角色可以同时控制），但它不会在结果页单独占一行。
- `PlayerControl=yes` 是旧写法：只有在没有写 `Player1Control` 时它才等价于「授予角色 1」；要让多个角色都控制同一条共享 House，请显式写 `Player1Control`、`Player2Control`……
- 地图里直接标了 `PlayerControl=yes` 的原版主 House **不会**自动交给 VERA 的某个角色；若它是原版战役里玩家控制的额外 House，请在 `campaign.ini` 对应的 `[Scenario:...:House]` 段写 `PlayerNControl=yes`，否则它会保持 AI。
- 大厅角色板只显示主角色 House；通过 `PlayerNControl=yes` 授权给某角色的共享 House 不会在开局前显示（B86）。

原版遭遇战里，玩家选国家、游戏分配起点；VMCE 不会按多人起点 waypoint 自动给角色分配基地。地图作者通常需要为每个角色预置基地和单位，或由触发动作生成；VMCE 本身不会自动发基地车或建造厂。

### 任务中的规则

- 你和队友运行同一个确定性世界。任何一端的操作都会以共享命令的形式进入同一帧。
- 角色间的初始同盟和视野由地图 `[Houses]` 的 `Allies=` 决定；VMCE 在装载时会安装这张外交图。要让队友共享视野、互不攻击，请把角色 House 互相写进 `Allies=`。
- 单方主 House 被打光**不会**自动结束任务或触发失败副作用；玩家保留原有共享控制和正常战争迷雾，新生成的自有单位仍可控制。地图触发器可改变外交，玩家不能手动改变外交。
- 结局只来自地图触发器的结局动作。原版单机地图默认的 PlayerPtr 判定不适用于 VMCE。
- 过场镜头、EVA、文字提示和输入锁默认对所有玩家生效。VMCE 不做「只给某个角色看」的过滤。
- 黑幕重置（动作 51）和雷达中断（动作 139）是 House 私有状态，默认只作用于触发器所属方。地图作者需要全队效果时，要为每个角色各写一条。

### 难度

LAN 大厅里的难度滑条对 VMCE 可见，并且由房主统一控制：

- 难度行会被冻结进 start，开局后不能改。
- 难度行只控制地图触发器的难度列：Easy 对应触发器的 Easy 列，Normal 对应 Medium，Hard 对应 Hard。
- 当前版本不通过这个难度行修改单位属性、AI 难度或经济；它只是触发器筛选。
- 当前默认行是 Easy（0）。地图只给 Normal/Hard 写触发时，房主需要在大厅里选对应难度，否则那些触发不会运行。

### 结果页与继续

VMCE 的胜利与末关使用原生战役计分页 `0x103`（布局与输入沿用原版，结果状态、Continue 与重试语义归 VMCE）；失败页改用原生 BC6 合作结算对话框。

- 胜利/末关 `0x103`：par time、your time、rank、commendation 保持原生；顶部报告里的「摧毁敌军数」标签和数值隐藏，原生面板高度不变。页面显示最多 4 行的 `玩家 | 所属方 | 国家 | 结果 | 准备` 板：玩家名来自大厅、所属方是地图 House 名、整行文字用该角色的 House 颜色（无行底条）。
- 右侧按钮列：第一行房主和客人均为 `继续`，第二行 `统计数据`，底部 `离开房间`。客人点击 `继续` 表示准备；房主等待所有客人准备后才能继续。`统计数据` 打开全屏统计页：页头显示任务用时与标准时间，表格按实际参战人数显示（2 人 2 行、4 人 4 行），六列 `玩家 | 击毁 | 损失 | 建造 | 分数 | 收入`。收入只统计任务中获得的正向信用点总额（不扣花费、不是净额），是只读展示统计。统计页背景为 BC6 风格，右侧只在底部保留一个普通态 `返回`；无左侧页签、无底部返回/Leave/其它按钮。该页只在胜利/末关存在，失败 BC6 页本身就是统计。
- 胜利且战役给出下一关时，客人在结果页准备，房主继续后返回指定下一关的房间；玩家重新选择角色并准备，房主再开始。
- Ready 图标和按钮随轮次状态更新；房主的继续按钮在所有在线客人准备后启用。
- 战役末关（`EndOfGame=yes`）同样使用全员 Ready 闸门；各端本地播放战役完成流程并回主菜单。**没有 30 秒强制开始、也没有房主强制拉取**：只要有客人未 Ready，页面就会一直停留，唯一出口是 Leave Room。
- 大厅角色面板：未认领的角色行在国家格显示原生「等待」，空位色块为中性灰；所属方格显示完整 House 名；角色冲突、地图或战役缺失/不一致显示在整行宽的橙色警告条中，而不是普通黄字。
- 大厅只列出所选地图实际存在的主角色：玩家行、准备/颜色槽位、角色面板和房主房间上限都跟随该数量（2–4），例如三角色地图只显示三个席位而不是四个。
- VMCE 房间不提供添加 AI 的选项：主角色面向人类玩家。单人无 AI 房主现在可以开局（B74 已在 B4 关闭）；从普通模式切入时遗留的大厅 AI 会在切换进 VMCE 时清理（B73）。
- 失败页是原生 BC6 合作结算对话框，直接作为统计页显示（页头时间 + 六列 `玩家 | 击毁 | 损失 | 建造 | 分数 | 收入`，行数等于实际参战人数、玩家名来自大厅、时间为裸 `HH:MM:SS`，隐藏聊天区，无 Ready 列、无独立统计入口）。客人点 `重试` 只投一票并立即点亮原生按钮；房主 `重试` 初始为原生禁用态，所有在线客人投票后才点亮，再走既有重开路径（同一冻结关卡、同一 seed、同一角色）——没有超时，房主不投票。BC6 的 `Quit` 改为 `离开房间`（房主离开解散、客人本地退出）。先前的真实 VMCE 2p-retry 采集中，客人投票、房主重开同一关卡，300 个 checksum 一致，双方统计相同。
- 失败统计按开局时冻结的玩家所属方匹配；即使命令来源顺序与大厅席位顺序不同，也不会按行号猜测归属。
- 既没有 `NextScenario` 也没有 `EndOfGame=yes` 的关卡视为非法战役。联机准入会核对清单中每张地图的字节，但选图时只完整校验 Entry 关的战役配置；后续关卡的配置有误会在切关时失败（B05）。
- 胜利进入下一关前，各端交换解析出的下一关身份（地图路径 + 源文件/生效/配置摘要）；不一致时会拒绝切关并请求返回大厅。当前拒绝消息可能在连接关闭时丢失（B10），导致一端继续等待，尚不能保证双方都成功返回。
- 下一关加载时，角色按下一关自己的 `[Basic] PlayerN` / `[Houses]` 重新解析，不沿用上一关的 House 名；下一关 seed 由上一关 seed 与下一关身份确定性派生（两端本地计算一致），冻结难度沿用本局大厅。
- 结果页期间有玩家离开时，双方一起回大厅；BC6 失败页的 Quit 已改为 Leave Room。
- 结果页不写 `coopsave.ini`。下一关只有在**所有在线参与者确认本轮结果与下一关身份**后才在本机解锁：结果报文把本轮 nonce 与解析出的下一关身份摘要绑定，所以只是打开结算页不写任何解锁，结果或下一关不一致的一端不会确认。解锁不要求实际加载下一关。新客人在关间加入并通过整套系列校验后只解锁指定下一关，不补领此前关卡。联机准入校验整套系列（见安装说明），所选地图与 `campaign.ini` 仍单独校验。

在 0.3.2 合并前的 B4 基线（`6322d411b`）上，本工作区已通过 2、3、4 人胜利切关、双人混合胜负切关、双人失败重试及 `2p-final` 采集；末关两端进入片尾并在按 Escape 后回到主菜单，未观看完整片尾。相关场景的 300 个 checksum 一致，并且各端本机解锁文件只包含指定的下一关。这些采集同时覆盖了结算页、Ready/Continue 操作、绑定 nonce 的结果一致性和整套系列准入校验；跨机器联机和客人中途改角色仍未覆盖。结果页没有计时器（失败为投票闸门、末关为全员准备闸门）。版本为 `0.3.2-vmce.6`；合并基线上的完整矩阵与 LAN 验收仍需重跑（B105），VMCE 两个对话框还需在 1024x768 默认窗口下验证（B103）。跨关资金、`AltNextScenario`/`SkipMapSelect` 仍未实现。所有已知缺口见 [审查总文档](vmce-review.md)。

### 当前不支持的玩家行为

- WOL、CnCNet 或其他在线联机；当前只支持 LAN。
- 任务内存档/读档；退出任务不会保存进度。
- 任务中掉线后的 AI 接管；掉线玩家的 House 不会自动交给 AI。
- 任务中途加入；当前只能在重新开局时加入，任务内重连不支持。
- 房主迁移；房主退出后房间不会换主。
- 跨关进度保存和角色资金继承。失败重试与 `EndOfGame` 完成流程已有实现。

## 地图制作者指南

本节假设你熟悉原版 YR 的 `[Basic]`、`[Houses]`、`[Triggers]`、`[Events]`、`[Actions]` 和 `[Tags]` 格式。VMCE 不改变这些格式的读法；它改变的是**谁控制哪条 House**、**结局如何结算**以及**地图可以新增哪些国家**。

### 在开始之前

- 地图作者不需要写脚本或编译代码，只需要按本页约定写地图 INI 和 `campaign.ini`。
- 不要把原版遭遇战的出生点分配逻辑套过来。VMCE 不会按多人起点 waypoint 分配角色基地；角色 House 的基地由你预置，或由触发动作生成。
- 不要依赖地图 `[Basic] Player`、`Player1..Player4` 来决定角色。VMCE 会用 `campaign.ini` 覆盖这些键；以 `campaign.ini` 为准。
- 同一张地图上的角色 House 必须属于不同的 Country；这是 VMCE 能找到每个角色触发器的前提，详见下文。

### 角色 House 的写法

假设你要做两名角色：

```ini
[Houses]
0=Americans House
1=Alliance House
2=French House

[Americans House]
Country=Americans
Allies=Americans House,Alliance House,French House
Credits=500
PercentBuilt=100

[Alliance House]
Country=Alliance
Allies=Americans House,Alliance House,French House
Credits=500
PercentBuilt=100

[French House]
Country=French
Allies=Americans House,Alliance House,French House
Credits=0
```

要点：

- `Player1`/`Player2` 指向的 House 必须出现在 `[Houses]` 注册表里。
- 所有角色 House 互相写进 `Allies=`。VMCE 在装载时按地图 `[Houses]` 的 `Allies=` 安装外交图；不写就不会共享视野和雷达，也容易误伤。
- 共享 House 也要按需要写进盟友列表；VMCE 不会自动把共享 House 与其他角色结盟。
- 所有角色 House 应使用不同的 `Country`。如果两个角色共用同一个 Country，按国家写的触发器只会命中第一条 House。
- `Credits=` 决定每个角色开局资金。VMCE 当前不做跨关资金继承。
- 角色 House 的 `Country` 必须互不相同。原因见下一节。

### 国家、所属方和触发器的关系

VMCE 沿用原版的解析规则：触发器的所属方字段、事件和动作里的国家参数，解析的是**国家（Country）**，不是具体的 House（所属方）。

原版规则是：一个 Country 可以注册多条 House；按国家名查找时，总是取该国家在 House 数组中的**第一条**。因此：

- 如果两个角色 House 共用同一个 `Country=`，所有按国家写的触发器和事件都会落到第一条 House 上，第二个角色永远不会被统计。
- VMCE 不做覆盖检查，也不会替你把同国多 House 拆开。**请让每个角色主 House 使用不同的 Country。**

如果你需要给两个角色分别写条件，标准做法是给每个角色一个独立 Country，然后用该 Country 作为触发器 owner：

```ini
[Triggers]
ROLE1_FAIL=Americans,<none>,Role1 lost yard,0,1,1,1,0
ROLE2_FAIL=Alliance,<none>,Role2 lost yard,0,1,1,1,0
```

两个角色各写一份，而不是指望引擎自动复制或聚合。

### 地图自定义国家

VMCE 支持地图自己注册国家（只在 Multiplayer Campaign 装载路径合并；VERA20k 的普通战役和遭遇战路径尚未应用这一合并，仍按原有读法）。这让你可以在不改 `rulesmd.ini` 的情况下给角色分配独立国家，例如「美国A / 美国B」。

在地图 INI 中新增 `[Countries]` 条目，并追加国家段：

```ini
[Countries]
0=Americans
1=Alliance
14=CoopAmerica
15=CoopAlliance

[CoopAmerica]
Name=CoopAmerica
UIName=NAME:CoopAmerica
Side=Allies
Color=Gold

[CoopAlliance]
Name=CoopAlliance
UIName=NAME:CoopAlliance
Side=Allies
Color=DarkBlue
```

规则如下：

- `[Countries]` 条目里，**值**才是国家 ID；已存在的国家保持它们在 rules 中的索引，新国家按地图中的出现顺序追加到注册表末尾。
- 在尤里的复仇中，全局国家数为 14，所以新国家通常从索引 14 开始；不要依赖手写的键名数字，引擎按出现顺序追加。
- 新国家的 `Name=`、`UIName=`、`Side=`、`Color=` 会被读取；`Color=` 可以用规则 `[Colors]` 中已有的条目，也可以由地图自己的 `[Colors]` 段新增或覆盖。
- `Side=` 必须使用规则 `[Sides]` 中已有的侧；当前版本不会合并地图自己新增的 `[Sides]` 条目。
- 在 `[Houses]` 中新增对应的 House，并把 `Country=` 指向新国家：

```ini
[Houses]
14=CoopAmerica House
15=CoopAlliance House

[CoopAmerica House]
Country=CoopAmerica
Allies=CoopAmerica House,CoopAlliance House
Credits=500

[CoopAlliance House]
Country=CoopAlliance
Allies=CoopAmerica House,CoopAlliance House
Credits=500
```

然后在 `campaign.ini` 中把 `Player1`/`Player2` 指向这两条 House，并在触发器中用 `CoopAmerica`/`CoopAlliance` 作为 owner 或国家参数。

VMCE 不校验「每个角色都有对应条件」；如果 owner 写错或国家没注册，触发器会静默地不启动。验收时请用真实的双进程测试确认。

### 触发器的写法

#### 触发器头

`[Triggers]` 的格式保持不变：

```ini
[Triggers]
<ID>=<Owner>,<Linked>,<Name>,<Disabled>,<Easy>,<Medium>,<Hard>,<Transfer>
```

- `<Owner>`：国家名。VMCE 用它解析到地图中的 House；角色条件必须用角色主 House 的 Country。
- `<Disabled>`：`0` 表示启用，非零表示初始禁用。
- `<Easy>/<Medium>/<Hard>`：非零表示该难度列启用；VMCE 用大厅难度行选择哪一列。
- `<Transfer>`：非零表示单位被俘获时转移触发器；它与 VMCE 的合作语义无关。

触发器按内部 House 顺序依次检查；不要依赖这一顺序设计逐帧时序。共享 House 的授权不会改变触发器的 owner，也不会把触发器复制给其他角色。需要每个角色各有一份条件时，请显式复制触发器和事件。

#### 结局动作

以下是当前工作区的结局动作；完整多人验收仍在进行：

| 动作 | 名称 | VMCE 行为 |
|---|---|---|
| `1` | 胜利者是某国 | 按国家找到首个所属方，其盟友胜利，其余玩家失败，包括 AI 所属方触发的情况 |
| `2` | 失败者是某国 | 国家匹配的玩家失败，其他玩家胜利 |
| `67` | 宣告胜利 | 全队立即胜利（跳过结果计时） |
| `68` | 宣告失败 | 全队立即失败（跳过结果计时） |
| `69` | 结束任务 | 有先前结果时恢复结果计时；没有先前结果时按胜利处理。一般不需要在合作图里使用 |

`67`/`68` 对所有角色设置同一结果；需要不同结果时应按上表设计触发条件。

#### 每个角色各写一份条件

因为触发器 owner 是国家，标准做法是每个角色写一条：

```ini
[Triggers]
R1_FAIL=Americans,<none>,Role1 lost construction yard,0,1,1,1,0
R2_FAIL=Alliance,<none>,Role2 lost construction yard,0,1,1,1,0

[Events]
R1_FAIL=1,57,0,2
R2_FAIL=1,57,0,2

[Actions]
R1_FAIL=1,68,0,0,0,0,0,0,A
R2_FAIL=1,68,0,0,0,0,0,0,A

[Tags]
R1_FAIL_TAG=0,Role1 lost construction yard,R1_FAIL
R2_FAIL_TAG=0,Role2 lost construction yard,R2_FAIL
```

上面的事件 57 表示「触发器所属方的特定建筑不再存在」；参数 2 是该地图 `[BuildingTypes]` 中建造厂的索引。请用地编查看实际索引，不要照抄数字。

事件 10 表示「特定国家的所有建筑都被摧毁」，参数是国家索引：

```ini
[Events]
R1_ALL_GONE=1,10,0,0
```

#### 双灭和兜底

VMCE 没有「所有角色都失败」的自动兜底。如果两个角色都被打光，而地图没有对应触发，任务会继续，玩家只能重开或等房主结束。

作者可以在一个触发器里同时放两条事件来表达「双方都失败」：

```ini
[Triggers]
TEAM_WIPE=Americans,<none>,Both roles lost all buildings,0,1,1,1,0

[Events]
TEAM_WIPE=2,10,0,0,10,0,1

[Actions]
TEAM_WIPE=1,68,0,0,0,0,0,0,A
```

注意 `[Events]` 的首个数字是事件数量；这里两条事件同时满足才会启动。不要把两个角色的「失去建造厂」事件放进同一个触发器——事件 57 用的是触发器 owner，放两条也只会查同一个 House。

#### 不要使用的东西

- **`<Player @ X>` 选择器**：VMCE 当前不建立遭遇战式 start-slot 表。`[TeamTypes] House=<Player @ A>`、动作里的 `<Player @ X>` 选择器都会解析失败，表现为静默不出兵/不生效。请改用国家 selector（或每个角色独立国家）。
- **`<random>` owner**：VMCE 没有「当前玩家」这个可解析的 owner；不要用它表示角色。
- **`<none>` owner**：它按原版落到国家 0 的第一条 House，通常不是你想要的角色。结局条件请显式写角色国家。
- **动作 15 AllowWin**：VMCE 已采用战役的标签计数胜利门，但带标签地图的端到端验收尚未完成；`67`/`68` 不能直接替代这个延迟语义。扫描的零售战役地图未使用动作 15。验收进度见[审查 B79](vmce-review.md)。
- **事件 33「被玩家选中」**：网络合作下没有逐端选择事件，永远不会满足；不要用它推进剧情。
- **地图自定义 `[Sides]`**：当前不合并；请使用规则已有的侧。

### 增援、小队和基地车

VMCE 不自动复制增援、小队或基地车。原版地图给「自己人」刷的东西，只会按原版逻辑给触发器 owner 对应的那条 House。

如果每个角色都需要自己的增援：

1. 为每个角色复制一份 `[TeamTypes]` 条目，把 `House=` 指向该角色 House 的 Country。
2. 复制对应的 `[TaskForces]`/`[ScriptTypes]`（如果脚本本身没有角色差异，可以复用脚本和任务编队）。
3. 为每个角色复制一份触发器，owner 用该角色的国家，动作生成对应 TeamType。
4. 需要每角色一辆基地车/建造厂时，在角色 House 的初始地图对象或专用触发器里各放一份。

VMCE 不会因为你写了「Player1 的增援」就自动给 Player2 一份。

### 表现类动作

- 镜头、EVA、文字、输入锁等动作默认全队可见/可听/被锁。VMCE 不做按角色过滤。
- 动作 51（重置全图黑幕）只作用于触发器 owner 的 House；动作 139（中断小地图/雷达）也只作用于 owner。需要全队效果时，为每个角色各写一条，或接受只有触发角色可见。
- 不要在过场文字里写「你的基地」这类只对一名玩家有意义的内容；所有玩家都会看到同一段文字。

### AI 动作与人类 House

VMCE 会把角色 House 标为人类控制。原版 AI 专用的动作（例如 AI 生产、AI 集结类动作）作用在人类 House 上时，通常和原版一样不产生效果。

写合作图时：

- 让 AI 动作只作用于地图中真正的 AI House（非角色 House）。
- 不要把「AI 自动造兵」当作角色基地的补给来源；角色基地需要玩家自己发展，或由地图触发直接给单位。

### 从单机地图改造的检查清单

1. 复制单机地图，保留原始文件。
2. 在 `[Houses]` 中确认每个角色 House 都存在，并给每个角色 House 设置不同的 `Country` 和正确的 `Allies=`。
3. 为每个角色预置基地/单位，或安排触发动作在开局生成；不要依赖遭遇战起点分配或 waypoint 自动生成。
4. 写 `campaign.ini`，把 `Player1..PlayerN` 指向这些 House；需要共享 House 时加控制段。
5. 把原版单机的胜负触发改成「每角色一份条件 + 67/68」；删除依赖 PlayerPtr 的假设。
6. 把原版给「玩家」的增援/小队按角色复制；TeamType 的 `House=` 用角色国家。
7. 检查所有 `<Player @ X>`、`<random>`、`<none>` 用法，替换成显式国家或每角色触发。
8. 检查动作 15、事件 33 等不支持项；按「不要使用的东西」替换。
9. 在双进程 LAN 下真实验收：至少跑一次真实失败链和一次真实胜利链，确认两端同帧进结果页；胜利时再确认 guest ready、host Continue 后两端同帧进入下一关。

## 参考

### campaign.ini 键值

`[Campaign]`：

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `Name` | CSF 标签 | 空（用系列目录名） | 大厅系列显示名；优先查系列目录的 `ra2md.csf`，再查基础游戏 CSF |
| `Entry` | 地图路径 | 无（必填） | 当前开始关卡；相对系列目录，必须在 `Scenario1..ScenarioCount` 中 |
| `ScenarioCount` | 整数 `1..512` | 无（必填） | 本战役列出的地图数量 |
| `Scenario1..ScenarioN` | 地图路径 | 无（必填） | 相对系列目录的松散 `.map` 路径；不能重复 |

`[Scenario:<地图路径>]`：

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `PlayerCount` | 整数 `1..4` | `0`（非法） | 本关玩家角色数；超出范围的 `PlayerN` 键会被拒绝 |
| `Player1..PlayerN` | House 名称 | 无（必填） | 必须出现在地图 `[Houses]` 中；角色 House 必须互不相同 |
| `NextScenario` | 地图路径 | 空 | 胜利后结果页的下一关；相对系列目录，目标必须在 `ScenarioCount` 中 |
| `AltNextScenario` | 地图路径 | 空 | 目前只校验；`SkipMapSelect`/全局变量分支尚未执行 |
| `EndOfGame` | 布尔 | 空 | `yes` 使本关成为战役末关：没有下一关，全员准备后进入战役完成流程 |

`[Scenario:<地图路径>:<House 名称>]`：

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `Player1Control..Player4Control` | 布尔 | 空 | `yes` 表示该角色可以控制这条共享 House |
| `PlayerControl` | 布尔 | 空 | 旧字段；当没有写 `Player1Control` 时等价于 `Player1Control` |

说明：

- `[Scenario:...]` 段的未知键、超出 `PlayerCount` 的 `PlayerN`，以及物理名称相同的重复段/重复键会被拒绝；大小写或路径分隔符不同但逻辑等价的段当前可能未被拒绝（B82），请勿重复。必须填值的键不能留空。`campaign.ini` 上限为 64 KiB。
- `PlayerNControl=yes` 只授予控制权，不会把共享 House 变成独立角色；共享 House 不占结果行。
- `PlayerNControl` 的角色序号超过该关 `PlayerCount` 时当前会被接受但永不生效（B87）。
- 所有 `ScenarioN` 列出的地图必须在本地存在，即使本局只玩第一关。

### 地图新增键值

| 位置 | 键 | 说明 |
|---|---|---|
| `[Countries]` | `N=<国家ID>` | 地图新增国家；已有国家保持规则索引，新值按出现顺序追加 |
| `[<国家ID>]` | `Name=`、`UIName=`、`Side=`、`Color=` | 新国家的显示名、侧和颜色；`Side` 必须已在规则 `[Sides]` 中注册 |
| `[Colors]` | `Name=三个数值` | 地图新增颜色方案，供国家 `Color=` 引用；若与规则已有方案同名，VERA 目前更新其数值（原版对已有方案不更新，此差异待定） |
| `[Houses]` | `N=<House 名称>` | 地图所属方注册表；角色和共享 House 都必须在这里 |
| `[<House 名称>]` | `Country=` | 所属方的国家；角色 House 必须使用不同国家 |
| `[<House 名称>]` | `Credits=`、`PercentBuilt=`、`TechLevel=`、`IQ=`、`Allies=`、`Color=` | 角色初始资金/建筑进度/科技等级/AI 智商/盟友列表/颜色；沿用原版读法 |

`[Houses]` 里的 `Allies=` 会在 Multiplayer Campaign 装载时安装成外交图；网络遭遇战本身不读它，但 Multiplayer Campaign 没有 launch teams，因此以地图写的盟友关系为准。

### 触发器格式

| 字段 | 说明 |
|---|---|
| ID | `[Triggers]`/`[Events]`/`[Actions]`/`[Tags]` 共用的触发器 ID |
| Owner | 国家名；决定触发器所属 House 和事件 57/10 等的角色 |
| Linked | 链接触发器 ID，或 `<none>` |
| Name | 触发器名称，供地编显示 |
| Disabled | `0` 启用；非零初始禁用 |
| Easy/Medium/Hard | 对应难度列是否启用 |
| Transfer | 单位被俘获时是否转移 |

### 自动化验收

`tools/vmce_lan_acceptance.py` 负责真实多进程采集：

- `--suite dev`：只跑一次短的 `2p-retry` 流程探针；单个采集默认 120 秒上限，任一端非零退出立即停止。
- `--suite fast` 跑 `4p-win`；`--suite smoke` 和 `--suite full` 跑 `4p-win`、`2p-win`、`2p-retry`，做完整的 300 帧 checksum 比对。
- `VMCE_CAPTURE_TIMEOUT=<秒>` 可覆盖单采集超时（非 dev 默认 300 秒）。
- `compare` 离线比对 receipts；`run` 只负责拉起进程，收据保留在 `.scratch/`。
- 共享显存有限的机器上建议一次只跑一个场景：刚退出的采集进程可能仍持有显存，紧接着跑下一个场景会在 wgpu 里报 “Out of Memory”（退出码 101）；分开单独跑则每个场景都能通过。

### 当前限制

| 项目 | 当前状态 |
|---|---|
| 联机范围 | LAN；WOL/CnCNet 不支持 |
| 玩家数 | 最多 4 个角色，不支持观察者。双人、三人、四人胜利与下一关本机多进程采集通过，包括结果板和鼠标按钮操作 |
| 结果页 | 胜利/末关走 `0x103`；失败走 BC6 统计页，客人投票、房主重试。末关使用全员 Ready 闸门 |
| 战役进度 | 下一关解锁保存在本机；不写 `coopsave.ini`；联机准入校验整套系列内容 |
| 任务内多人选项 | VERA 按战役语义关闭随机箱和 ShortGame（大厅对应开关禁用）；Special House/Secret Lab 的多人专属步骤不执行 |
| 跨关角色/资金/seed | 角色按下一关地图重新解析，不沿用上一关 House 名；资金不继承；下一关 seed 由上一关 seed + 下一关身份确定性派生 |
| 任务内存档/读档 | 不支持 |
| 任务中掉线 | 不接管 AI；不自动恢复 |
| 任务中中途加入 | 不支持 |
| 房主迁移 | 不支持 |
| 暂停规则 | 原生多人 ESC 菜单只在本地打开，LAN/WOL 模态泵继续跑 Main_Tick，没有网络暂停消息；VMCE 保持 sim 继续运行（原生优先），不提供全员暂停 |
| 角色跨关重选/资金继承 | 关间可重选角色；资金继承未实现 |
| 空位角色 AI 接管 | 未实现 |
| `<Player @ X>` 选择器 | 不支持；请用国家 selector |
| AllowWin（动作 15） | 已采用战役胜利门；自定义标签地图的端到端验收待完成，见 A27/B79 |
| 事件 33「被玩家选中」 | 不支持 |
| 地图新增 `[Sides]` | 不合并；请用规则已有侧 |
| 随机地图 | 不支持；必须是松散 authored `.map` |
| 3–4 人联机 | 本机三人、四人胜利与下一关采集通过（各端解锁文件已核对）；跨机器和客人中途更改角色尚未覆盖 |
| 末关/失败重试真实采集 | `2p-retry` 与 `2p-final` 通过；末关覆盖进入片尾及跳过后返回主菜单，未观看完整片尾。失败是投票闸门，末关是全员准备闸门 |
| 上游基线 | 已合并 upstream `0.3.2`（合并提交 `70d569b86`）；版本 `0.3.2-vmce.6`；旧 2p/4p 收据作废，新基线见 `docs/vmce-baseline.md`；10 种旧语言各有 32 条 VMCE 键（en/zh-CN 真值，其余英文占位，B08），越南语无 VMCE 键（B101） |

### 已知未修项

- **A44/B99：** 关间房间的客人可能看不到下一关预览和「该地图有简报」提示；房主在地图带可解码预览数据时能看到。
- **A45/B100：** 满员关间房间在 LAN 浏览器仍显示可加入，实际加入会收到 `TXT_GAME_FULL`。
- **A38/B90：** 胜利/战役完成统计页的长玩家名没有做原生截断。
- **A41/B95：** 地图场景电影在 VMCE 开局和关间继续路径上不播放。
- **B91（工具）：** 合成结算/失败页采集需要把角色地图放到 `settings/Maps/<catalog map>` 才能启动，根因与 workaround 见英文台账。

### 已锁定决策与待验收内容

以下是已锁定的流程语义与仍需验收或修复的内容；它同时是玩家可见承诺和审查边界，不是待实现功能清单。

- 国家索引触发胜利时，以触发所属方的盟友为胜方、未结盟玩家为败方；触发者也可以是 AI。
  最早到达可接受结算时点的玩家结束任务，其余结果按该时点的地图/House 状态推导。VMCE 对各所属方
  的胜利采用战役 AllowWin 标签门（0x004F85F7）；其他玩家可先触发结算。带标签的测试地图及多人
  端到端验收尚未完成（A27/B79）。
  任务中只有地图触发器能改变结盟关系。
- 主所属方被消灭的玩家继续控制原本获授权的共享单位，保留正常战争迷雾；地图为其所属方重新生成单位后可以控制新单位。所有主所属方被消灭也不自动结束任务。`PlayerNControl` 是地图授权，外交变化本身不修改这份授权；动态外交仍需真实联机验收。
- 胜负不同的玩家使用同一结算和继续流程，各自显示胜负，不显示消灭标记；曾被消灭但仍是获胜方盟友的玩家也显示胜利。只要有人胜利，所有玩家均可继续；全员失败时由客人投票、房主重试，沿用本关冻结角色和随机种子。
- 结算页客人全部准备、房主继续后返回战役房间，显示指定下一关，所有玩家重新选择角色并准备；保留随机角色。关卡间可加入或离开，人数超出下一关角色数时保留房间并阻止开始；该房间不能更换指定下一关。
- 完成一关后，当前实现需经过结果页继续流程才在本机解锁下一关；仅完成各端结果与下一关身份确认后就退出结果页，仍可能未解锁（B57）。解锁不要求实际加载下一关。关间客人通过整套系列一致性校验后只解锁已确定的下一关，不补领此前关卡；断线后重新加入也按此处理。同一战役目录和地图路径下的内容更新保留解锁。房主可在重新建房时选择本机已解锁关卡，客人不需事先解锁；新人加入仍需真实联机验收。
- 末关仍走共同结算、完成演出和返回主菜单；不引入观察者。跨关资金与分支键属后续工作；将来若实现分支，只解锁实际选择的分支。

完整路线和验收条件见[审查总文档](vmce-review.md)。

### 故障排查

- **地图不出现在 Multiplayer Campaign 列表里**：检查 `campaign.ini` 是否在 `Maps/VMCE/<系列>/` 下，`Entry` 是否在 `Scenario1..N` 中，地图是否真实存在且是松散 `.map`。日志里以 `ignoring invalid VMCE series` 开头的行会给出原因。
- **客人一直不能 Ready / 地图不可用**：客人缺少同一份 `campaign.ini`、所选地图与房主不一致，或系列中其他地图的字节不同。准入会在后台校验整套系列（每张地图的路径、长度、内容加 `campaign.ini`），校验未完成时也不能准备；系列 CSF 不参与校验。VMCE 不传输文件，请手动复制同一份内容。
- **提示 `campaign is missing [Scenario:...]`**：`[Scenario:<地图路径>]` 段的写法与 `ScenarioN` 中的路径不一致。段名比较时 `/` 和 `\` 等价且忽略 ASCII 大小写；Linux/macOS 的实际文件加载目前存在反斜杠问题，建议路径统一使用 `/`（B81）。`campaign.ini` 中的非 ASCII 地图文件名也可能无法匹配实际文件（B85）。
- **角色 House 找不到**：`campaign.ini` 的 `PlayerN` 名字必须与地图 `[Houses]` 注册表中的名称一致。
- **第二个角色的触发器永远不生效**：两个角色 House 的 `Country=` 相同。给每个角色分配独立国家。
- **自定义国家 owner 不触发**：检查 `[Countries]` 条目是否真的在该地图 INI 中、国家段是否存在、`campaign.ini` 是否把角色 House 指向了该国家；VMCE 不会替作者校验覆盖情况。
- **任务打光一个角色也不结束**：这是预期行为。VMCE 不用「主 House 全灭」自动判负，请写 67/68 触发器。
- **双灭后任务一直继续**：VMCE 没有双灭兜底；请按「双灭和兜底」一节自己写条件。
- **进游戏后角色没有基地**：VMCE 不会发放 MCV 或建造厂；请在该角色 House 下预置基地，或由触发动作生成。

### 版本与存档兼容性

VMCE 仍处于开发阶段。不同开发版本之间的任务内存档、网络协议和地图行为都**不保证兼容**。当前版本不使用任务内存档；只使用外部 `campaign.ini` 和松散地图。

原版战役存档和普通遭遇战存档与 VMCE 的战役进度没有关系；VMCE 当前不读取也不写入 `coopsave.ini`。

### 法律与致谢

VERA20k 与 VMCE 是非官方的社区项目，与 Electronic Arts Inc. 没有隶属关系，也未获其认可。「Command & Conquer」「Command & Conquer: Red Alert 2」「Command & Conquer: Yuri's Revenge」是 Electronic Arts Inc. 的商标。使用本项目需要你自备合法的原版游戏文件；本项目不附带或分发原版资源。

VMCE 的行为建立在对原版引擎、FA2SP 地图编辑器和社区文档的研究之上；感谢原版 YR、FA2SP、Ares、Phobos 以及所有公开地图/触发论文档的作者。
