---
layout: default
title: VERA Coop Expansion 说明书
lang: zh-CN
---

# VERA Coop Expansion 说明书

[返回文档首页](../) · **简体中文**

> **警告**
> VERA Coop Expansion（以下简称 VCE）目前是 VERA20k 的开发中功能。本文描述的是当前开发分支的行为，接口、地图格式和可用功能都可能随版本变化。不要把开发版行为当作稳定 API 写进长期维护的战役。

VCE 是 VERA20k 内置的 LAN 合作战役模式。它不是 Phobos、Ares 或 NPatch 的插件，也不加载任何外部 DLL；它复用 VERA20k 已有的原版规则、地图、触发器和资源读取路径，把一张为合作而写的地图交给多名玩家共同完成。

本页写给两类人：

- **玩过原版《尤里的复仇》的玩家**：想知道 VCE 和原版战役、原版 Cooperative 模式有什么区别，以及怎么开一局。
- **做过原版 YR 地图和触发器的制作者**：想把单机任务改造成合作任务，或者从零写一张合作图。

> **备注**
> 本页只描述 VCE 新增或覆盖的行为。没有提到的原版规则、INI 键、动作和事件仍然按原版读取；如果本页与代码不一致，以代码为准。

如果你只想知道怎么玩，读「安装与启动」「玩家指南」「故障排查」即可；如果你要写图，直接从「地图制作者指南」和「参考」开始。

## 与原始版本的关系

原版 YR 的战役按「一个玩家 + 一个当前所属方」推进：胜负由 PlayerPtr 和战役状态决定，关卡之间由战役存档串联。原版 Cooperative 模式（合作战役）由 CoopCampMD 和 BC6 结果页驱动，按战役关卡推进，并在任务结束时写 `coopsave.ini`。

VCE 走的是另一条路：

- 战役目录、关卡顺序和每关角色由外部的 `campaign.ini` 描述，而不是 CoopCampMD。
- 每个角色对应地图中的一条主 House（所属方），没有遭遇战式的「按玩家位置分配出生点」。
- 胜负只由地图触发器的结局动作决定；单方主 House 全灭不会自动结束任务。
- 当前版本的结果页只提供退出，不写 `coopsave.ini`、不推进下一关。

VCE 不改变原版战役和普通遭遇战的判定。原版单人战役仍然按原版规则运行；普通多人模式也不会因为 VCE 的地图格式而改变。

## 安装与启动

### 前置条件

0. 安装一份可用的 VERA20k 开发版，并准备好原版《尤里的复仇》的规则、地图和美术资源。VCE 不附带原版资源，也不分发 EA 的任何游戏文件。
1. VCE 不需要 Syringe、Phobos.dll 或 Ares.dll。不要把 VCE 当成 Phobos 扩展去配置加载器。
2. 所有参战玩家必须能读取同一套原版资源。VCE 只交换战役和地图的**身份**，不传输文件。

### 战役目录

VCE 的战役放在 VERA20k 的设置目录下：

```text
<VERA20k 设置目录>/Maps/VeraCoop/
├── my-first-campaign/
│   └── campaign.ini
├── mission01/
│   └── co-op-01.map
└── mission02/
    └── co-op-02.map
```

- `campaign.ini` 必须位于 `Maps/VeraCoop/<战役名>/` 这一层子目录中，一个子目录代表一个可被发现的战役。
- 地图文件可以放在 `Maps/VeraCoop/` 下的任意子目录；`campaign.ini` 中的路径以 `Maps/VeraCoop/` 为根。
- 地图必须是松散文件（`.map`），不能是 `.mix` 内的地图，也不能是随机地图生成器产生的临时地图。
- 同一个地图只能属于一个有效战役。如果一个地图同时被两个战役列出，VCE 会拒绝选择它。

### 最小 campaign.ini

```ini
[Campaign]
Entry=mission01/co-op-01.map
ScenarioCount=2
Scenario1=mission01/co-op-01.map
Scenario2=mission02/co-op-02.map

[Scenario:mission01/co-op-01.map]
PlayerCount=2
Player1=Americans House
Player2=Alliance House

[Scenario:mission01/co-op-01.map:French House]
Player1Control=yes
Player2Control=yes

[Scenario:mission02/co-op-02.map]
PlayerCount=2
Player1=Americans House
Player2=Alliance House
EndOfGame=yes
```

这个例子说明三件事：

- `Entry` 是当前开始的那一关；它必须出现在 `Scenario1..ScenarioCount` 中。
- 每关的 `[Scenario:<地图路径>]` 段把 `Player1..PlayerN` 绑定到地图 `[Houses]` 中的 House 名称。
- 可选地，`[Scenario:<地图路径>:<House 名称>]` 段把一条额外 House 的控制权授予某个角色。这个例子把 `French House` 同时授予两名玩家。

### 启动一局

1. 房主和所有玩家放好同一份 `Maps/VeraCoop/` 内容。目录名、`campaign.ini` 内容和地图字节必须一致；VCE 会核对文件身份。
2. 进入 LAN 房间，把游戏模式选为 **VERA Coop**。游戏会从外部战役目录中列出可用的合作地图。
3. 房主选择地图；客人的地图状态必须是可用（Ready），否则无法开始。
4. 每名玩家确认角色、准备；房主开始。
5. 装载完成后，双方进入同一张地图、同一个确定性模拟。双方看到的开局、剧本和结果应当一致。

如果客人缺少地图或 `campaign.ini`，他会在准备阶段被拒绝；VCE 不会自动把文件发给客人。

### 如何阅读本页的代码片段

本页的代码块都是 INI 片段：

- `[...]` 是段名；`<...>` 是需要按你的地图替换的名称或路径。
- `;` 后面是注释，原版读取器会忽略。
- 未在片段中出现的键表示保持原版默认值；VCE 没有为这些键发明新默认值。

## 玩家指南

### 角色、主 House 与共享 House

每个参战角色对应一张地图主 House。角色不是「随便选一个国家」：主 House、它的国家、颜色、基地和初始资金都由地图和 `campaign.ini` 决定。

- 你的主 House 由 `Player1..PlayerN` 决定。
- 每关最多允许 `PlayerCount` 名玩家；实际参战人数不能超过它。当前版本没有专门的空位 AI 接管，建议让 `PlayerCount` 与实际参战人数一致；未使用的角色 House 仍有地图 AI 行为，但不要依赖它。
- 共享 House（例如例子里的 `French House`）由 `campaign.ini` 的 `PlayerNControl=yes` 授权给指定角色。共享 House 的单位由多个玩家共同控制，但它不会在结果页单独占一行。
- `PlayerControl=yes` 是旧写法：只有在没有写 `Player1Control` 时它才等价于「授予角色 1」；要让多个角色都控制同一条共享 House，请显式写 `Player1Control`、`Player2Control`……

原版遭遇战里，玩家选国家、游戏分配起点；VCE 没有这套流程。地图作者必须为每个角色预置基地和单位，VCE 不会自动发基地车或建造厂。

### 任务中的规则

- 你和队友运行同一个确定性世界。任何一端的操作都会以共享命令的形式进入同一帧。
- 单方主 House 被打光**不会**自动判负。这个 House 会被标记为失败，但任务继续，直到地图的结局触发器动作（房主强制结束尚未实现）。
- 结局只来自地图触发器的结局动作。原版单机地图默认的 PlayerPtr 判定不适用于 VCE。
- 过场镜头、EVA、文字提示和输入锁默认对所有玩家生效。VCE 不做「只给某个角色看」的过滤。
- 黑幕重置（动作 51）和雷达中断（动作 139）是 House 私有状态，默认只作用于触发器所属方。地图作者需要全队效果时，要为每个角色各写一条。

### 难度

LAN 大厅里的难度滑条对 VCE 可见，并且由房主统一控制：

- 难度行会被冻结进 start，开局后不能改。
- 难度行只控制地图触发器的难度列：Easy 对应触发器的 Easy 列，Normal 对应 Medium，Hard 对应 Hard。
- 当前版本不通过这个难度行修改单位属性、AI 难度或经济；它只是触发器筛选。
- 当前默认行是 Easy（0）。地图只给 Normal/Hard 写触发时，房主需要在大厅里选对应难度，否则那些触发不会运行。

### 结果页与继续

任务结束后，双方进入 BC6 合作结果页：

- 每个角色主 House 占一行；共享 House 不另起一行。
- 结果页显示胜负、行列和难度文本。
- 当前开发版只提供「退出」：不提供 Continue，不进入下一关，不写战役进度。退出后回到合作大厅。

结算页的 ready/Continue、下一关和失败重试仍在开发中，见「当前限制」。

### 当前不支持的玩家行为

- WOL、CnCNet 或其他在线联机；当前只支持 LAN。
- 任务内存档/读档；退出任务不会保存进度。
- 任务中掉线后的 AI 接管；掉线玩家的 House 不会自动交给 AI。
- 任务中途加入；新玩家只能在下一关或重新开局时加入（下一关本身也还没有实现）。
- 房主迁移；房主退出后房间不会换主。
- 跨关进度、失败重试和角色资金继承。

## 地图制作者指南

本节假设你熟悉原版 YR 的 `[Basic]`、`[Houses]`、`[Triggers]`、`[Events]`、`[Actions]` 和 `[Tags]` 格式。VCE 不改变这些格式的读法；它改变的是**谁控制哪条 House**、**结局如何结算**以及**地图可以新增哪些国家**。

### 在开始之前

- 地图作者不需要写脚本或编译代码，只需要按本页约定写地图 INI 和 `campaign.ini`。
- 不要把原版遭遇战的出生点分配逻辑套过来。VCE 不使用多人起点 waypoint；角色 House 的基地由你预置。
- 不要依赖地图 `[Basic] Player`、`Player1..Player4` 来决定角色。VCE 会用 `campaign.ini` 覆盖这些键；以 `campaign.ini` 为准。
- 同一张地图上的角色 House 必须属于不同的 Country；这是 VCE 能找到每个角色触发器的前提，详见下文。

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
- 所有角色 House 互相写进 `Allies=`。VCE 不会自动把队友设为盟友；不写 `Allies=` 就没有共享视野和雷达，也容易误伤。
- `Credits=` 决定每个角色开局资金。VCE 当前不做跨关资金继承。
- 角色 House 的 `Country` 必须互不相同。原因见下一节。

### 国家、所属方和触发器的关系

VCE 沿用原版的解析规则：触发器的所属方字段、事件和动作里的国家参数，解析的是**国家（Country）**，不是具体的 House（所属方）。

原版规则是：一个 Country 可以注册多条 House；按国家名查找时，总是取该国家在 House 数组中的**第一条**。因此：

- 如果两个角色 House 共用同一个 `Country=`，所有按国家写的触发器和事件都会落到第一条 House 上，第二个角色永远不会被统计。
- VCE 不做覆盖检查，也不会替你把同国多 House 拆开。**请让每个角色主 House 使用不同的 Country。**

如果你需要给两个角色分别写条件，标准做法是给每个角色一个独立 Country，然后用该 Country 作为触发器 owner：

```ini
[Triggers]
ROLE1_FAIL=Americans,<none>,Role1 lost yard,0,1,1,1,0
ROLE2_FAIL=Alliance,<none>,Role2 lost yard,0,1,1,1,0
```

两个角色各写一份，而不是指望引擎自动复制或聚合。

### 地图自定义国家

VCE 支持地图自己注册国家（仅 VERA Coop 装载时生效；普通战役和遭遇战不会合并地图国家表）。这让你可以在不改 `rulesmd.ini` 的情况下给角色分配独立国家，例如「美国A / 美国B」。

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

VCE 不校验「每个角色都有对应条件」；如果 owner 写错或国家没注册，触发器会静默地不启动。验收时请用真实的双进程测试确认。

### 触发器的写法

#### 触发器头

`[Triggers]` 的格式保持不变：

```ini
[Triggers]
<ID>=<Owner>,<Linked>,<Name>,<Disabled>,<Easy>,<Medium>,<Hard>,<Transfer>
```

- `<Owner>`：国家名。VCE 用它解析到地图中的 House；角色条件必须用角色主 House 的 Country。
- `<Disabled>`：`0` 表示启用，非零表示初始禁用。
- `<Easy>/<Medium>/<Hard>`：非零表示该难度列启用；VCE 用大厅难度行选择哪一列。
- `<Transfer>`：非零表示单位被俘获时转移触发器；它与 VCE 的合作语义无关。

所属方启动的触发器按地图 `[Houses]` 的注册顺序轮询；共享 House 的授权不会改变触发器的 owner，也不会把触发器复制给其他角色。需要每个角色各有一份条件时，请显式复制触发器和事件。

#### 结局动作

VCE 的团队胜负只认以下动作：

| 动作 | 名称 | VCE 行为 |
|---|---|---|
| `1` | 胜利者是某国 | 全队胜利；国家参数在 VCE 团队结算中被忽略 |
| `2` | 失败者是某国 | 全队失败；国家参数在 VCE 团队结算中被忽略 |
| `67` | 宣告胜利 | 全队立即胜利（跳过结果计时） |
| `68` | 宣告失败 | 全队立即失败（跳过结果计时） |
| `69` | 结束任务 | 保留先前结果/计时状态；一般不需要在合作图里使用 |

合作图建议只写 `67`/`68`。`1`/`2` 的原版国家参数在 VCE 下不做过滤，容易让作者误以为「只判某个角色」。

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

VCE 没有「所有角色都失败」的自动兜底。如果两个角色都被打光，而地图没有对应触发，任务会继续，玩家只能重开或等房主结束。

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

- **`<Player @ X>` 选择器**：VCE 当前不建立遭遇战式 start-slot 表。`[TeamTypes] House=<Player @ A>`、动作里的 `<Player @ X>` 选择器都会解析失败，表现为静默不出兵/不生效。请改用国家 selector（或每个角色独立国家）。
- **`<random>` owner**：VCE 没有「当前玩家」这个可解析的 owner；不要用它表示角色。
- **`<none>` owner**：它按原版落到国家 0 的第一条 House，通常不是你想要的角色。结局条件请显式写角色国家。
- **动作 15 AllowWin**：网络模式下它不能作为合作胜利门；请直接使用 `67`/`68`。
- **事件 33「被玩家选中」**：网络合作下没有逐端选择事件，永远不会满足；不要用它推进剧情。
- **地图自定义 `[Sides]`**：当前不合并；请使用规则已有的侧。

### 增援、小队和基地车

VCE 不自动复制增援、小队或基地车。原版地图给「自己人」刷的东西，只会按原版逻辑给触发器 owner 对应的那条 House。

如果每个角色都需要自己的增援：

1. 为每个角色复制一份 `[TeamTypes]` 条目，把 `House=` 指向该角色 House 的 Country。
2. 复制对应的 `[TaskForces]`/`[ScriptTypes]`（如果脚本本身没有角色差异，可以复用脚本和任务编队）。
3. 为每个角色复制一份触发器，owner 用该角色的国家，动作生成对应 TeamType。
4. 需要每角色一辆基地车/建造厂时，在角色 House 的初始地图对象或专用触发器里各放一份。

VCE 不会因为你写了「Player1 的增援」就自动给 Player2 一份。

### 表现类动作

- 镜头、EVA、文字、输入锁等动作默认全队可见/可听/被锁。VCE 不做按角色过滤。
- 动作 51（重置全图黑幕）只作用于触发器 owner 的 House；动作 139（中断小地图/雷达）也只作用于 owner。需要全队效果时，为每个角色各写一条，或接受只有触发角色可见。
- 不要在过场文字里写「你的基地」这类只对一名玩家有意义的内容；所有玩家都会看到同一段文字。

### AI 动作与人类 House

VCE 会把角色 House 标为人类控制。原版 AI 专用的动作（例如 AI 生产、AI 集结类动作）作用在人类 House 上时，通常和原版一样不产生效果。

写合作图时：

- 让 AI 动作只作用于地图中真正的 AI House（非角色 House）。
- 不要把「AI 自动造兵」当作角色基地的补给来源；角色基地需要玩家自己发展，或由地图触发直接给单位。

### 从单机地图改造的检查清单

1. 复制单机地图，保留原始文件。
2. 在 `[Houses]` 中确认每个角色 House 都存在，并给每个角色 House 设置不同的 `Country` 和正确的 `Allies=`。
3. 为每个角色预置基地/单位；不要依赖遭遇战起点分配。
4. 写 `campaign.ini`，把 `Player1..PlayerN` 指向这些 House；需要共享 House 时加控制段。
5. 把原版单机的胜负触发改成「每角色一份条件 + 67/68」；删除依赖 PlayerPtr 的假设。
6. 把原版给「玩家」的增援/小队按角色复制；TeamType 的 `House=` 用角色国家。
7. 检查所有 `<Player @ X>`、`<random>`、`<none>` 用法，替换成显式国家或每角色触发。
8. 检查动作 15、事件 33 等不支持项；按「不要使用的东西」替换。
9. 在双进程 LAN 下真实验收：至少跑一次真实失败链和一次真实胜利链，确认两端同帧进结果页。

## 参考

### campaign.ini 键值

`[Campaign]`：

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `Entry` | 地图路径 | 无（必填） | 当前开始关卡；必须在 `Scenario1..ScenarioCount` 中 |
| `ScenarioCount` | 整数 `1..512` | 无（必填） | 本战役列出的地图数量 |
| `Scenario1..ScenarioN` | 地图路径 | 无（必填） | 相对 `Maps/VeraCoop/` 的松散 `.map` 路径；不能重复 |

`[Scenario:<地图路径>]`：

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `PlayerCount` | 整数 `1..4` | `0`（非法） | 本关玩家角色数；超出范围的 `PlayerN` 键会被拒绝 |
| `Player1..PlayerN` | House 名称 | 无（必填） | 必须出现在地图 `[Houses]` 中；角色 House 必须互不相同 |
| `NextScenario` | 地图路径 | 空 | 目前只校验目标是否在 `ScenarioCount` 中；尚未执行关卡推进 |
| `AltNextScenario` | 地图路径 | 空 | 目前只校验；尚未执行 |
| `EndOfGame` | 布尔 | 空 | 目前只校验；尚未执行战役结束行为 |

`[Scenario:<地图路径>:<House 名称>]`：

| 键 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `Player1Control..Player4Control` | 布尔 | 空 | `yes` 表示该角色可以控制这条共享 House |
| `PlayerControl` | 布尔 | 空 | 旧字段；当没有写 `Player1Control` 时等价于 `Player1Control` |

说明：

- 空值、重复段、重复键、未知键都会被拒绝；战役文件上限 64 KiB。
- `PlayerNControl=yes` 只授予控制权，不会把共享 House 变成独立角色；共享 House 不占结果行。
- 所有 `ScenarioN` 列出的地图必须在本地存在，即使本局只玩第一关。

### 地图新增键值

| 位置 | 键 | 说明 |
|---|---|---|
| `[Countries]` | `N=<国家ID>` | 地图新增国家；已有国家保持规则索引，新值按出现顺序追加 |
| `[<国家ID>]` | `Name=`、`UIName=`、`Side=`、`Color=` | 新国家的显示名、侧和颜色；`Side` 必须已在规则 `[Sides]` 中注册 |
| `[Colors]` | `Name=H,S,V` | 地图新增或覆盖颜色方案，供国家 `Color=` 引用 |
| `[Houses]` | `N=<House 名称>` | 地图所属方注册表；角色和共享 House 都必须在这里 |
| `[<House 名称>]` | `Country=` | 所属方的国家；角色 House 必须使用不同国家 |
| `[<House 名称>]` | `Credits=`、`PercentBuilt=`、`TechLevel=`、`IQ=`、`Allies=`、`Color=` | 角色初始资金/建筑进度/科技等级/AI 智商/盟友列表/颜色；沿用原版读法 |

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

### 当前限制

| 项目 | 当前状态 |
|---|---|
| 联机范围 | LAN；WOL/CnCNet 不支持 |
| 玩家数 | 结构支持最多 4 个角色；当前以 2 人 LAN 验收为主，3–4 人未完整验证 |
| 结果页 | 只有退出；Continue、下一关、失败重试未实现 |
| 战役进度 | 不保存；不写 `coopsave.ini` |
| 任务内存档/读档 | 不支持 |
| 任务中掉线 | 不接管 AI；不自动恢复 |
| 任务中中途加入 | 不支持 |
| 房主迁移 | 不支持 |
| 暂停规则 | 未完成合作暂停状态机；不要依赖暂停行为 |
| 角色跨关重选/资金继承 | 未实现（规划中） |
| 空位角色 AI 接管 | 未实现 |
| `<Player @ X>` 选择器 | 不支持；请用国家 selector |
| AllowWin（动作 15） | 网络合作下不可用；请用 67/68 |
| 事件 33「被玩家选中」 | 不支持 |
| 地图新增 `[Sides]` | 不合并；请用规则已有侧 |
| 随机地图 | 不支持；必须是松散 authored `.map` |
| 3–4 人真机 | 未完整验收 |

### 故障排查

- **地图不出现在 VERA Coop 列表里**：检查 `campaign.ini` 是否在 `Maps/VeraCoop/<战役>/` 下，`Entry` 是否在 `Scenario1..N` 中，地图是否真实存在且是松散 `.map`。日志里以 `ignoring invalid VERA Coop campaign` 开头的行会给出原因。
- **客人一直不能 Ready / 地图状态为 0**：客人缺少同一份 `campaign.ini` 或地图，或者地图字节与房主不一致。VCE 不传输文件，请手动复制同一份内容。
- **提示 `campaign is missing [Scenario:...]`**：`[Scenario:<地图路径>]` 段的写法与 `ScenarioN` 中的路径不一致。路径使用 `/`，并且大小写要匹配。
- **角色 House 找不到**：`campaign.ini` 的 `PlayerN` 名字必须与地图 `[Houses]` 注册表中的名称一致。
- **第二个角色的触发器永远不生效**：两个角色 House 的 `Country=` 相同。给每个角色分配独立国家。
- **自定义国家 owner 不触发**：检查 `[Countries]` 条目是否真的在该地图 INI 中、国家段是否存在、`campaign.ini` 是否把角色 House 指向了该国家；VCE 不会替作者校验覆盖情况。
- **任务打光一个角色也不结束**：这是预期行为。VCE 不用「主 House 全灭」自动判负，请写 67/68 触发器。
- **双灭后任务一直继续**：VCE 没有双灭兜底；请按「双灭和兜底」一节自己写条件。
- **地图只在大厅可见，进游戏后没有基地**：VCE 不发放 MCV 或建造厂；请在角色 House 下预置基地。

### 版本与存档兼容性

VCE 仍处于开发阶段。不同开发版本之间的任务内存档、网络协议和地图行为都**不保证兼容**。当前版本不使用任务内存档；只使用外部 `campaign.ini` 和松散地图。

原版战役存档和普通遭遇战存档与 VCE 的战役进度没有关系；VCE 当前不读取也不写入 `coopsave.ini`。

### 法律与致谢

VERA20k 与 VCE 是非官方的社区项目，与 Electronic Arts Inc. 没有隶属关系，也未获其认可。「Command & Conquer」「Command & Conquer: Red Alert 2」「Command & Conquer: Yuri's Revenge」是 Electronic Arts Inc. 的商标。使用本项目需要你自备合法的原版游戏文件；本项目不附带或分发原版资源。

VCE 的行为建立在对原版引擎、FA2SP 地图编辑器和社区文档的研究之上；感谢原版 YR、FA2SP、Ares、Phobos 以及所有公开地图/触发论文档的作者。本文的排版与语气参考了 Phobos 文档的写作方式，但 VCE 与 Phobos 项目没有隶属关系。
