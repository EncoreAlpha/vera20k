# VERA 合作战役开发记录

这是 VERA 合作战役 fork 的开发记录，不是发布品牌或正式发布说明。独立开发版本号：`0.1.3-dev`。

## `0.1.3-dev`

- 开局前会校验大厅角色与地图主所属 House，并检查玩家人数；未指定角色会按共享开局种子可复现地随机分配且不重复。
- 解析出的所属方尚未传入启动冻结或游戏 House 初始化；VERA Coop 合作战役仍无法开始。

## `0.1.2-dev`

- 联机大厅现按地图 `[Basic] Player1` 至 `Player4` 显示角色及所属 House 名称。玩家可选择自己的角色，房主可调整所有玩家；同一角色不可重复，未选角色保持 Random。仅 VERA Coop 使用该控件，原版队伍选择和原 Cooperative 模式保持不变。
- LAN 使用现有 VERA 扩展选项快照同步房主分配，并在玩家选择消息中把客机的角色选择送回房主；原版选项文本格式未改变。观察者身份随快照保留。
- WOL 客户端会随角色选择发送可选字段，但真实在线中继尚未验证该字段能否往返，不能据此认定 WOL 角色已可靠同步。
- VERA Coop 合作开局仍未实现；即使角色和地图配置有效，房主点击 Start 仍会被拒绝。当前功能只完成大厅角色选择与 LAN 状态同步。

## `0.1.1-dev`

- 已加入可选 LAN/WOL 网络模式入口和启动角色校验；当前即使校验通过仍会阻止开局，并提示合作游戏尚未实现。
- 已验证 `Player1Control` 缺省时回退到 `PlayerControl`，显式 `Player1Control` 优先；`Player2Control` 至 `Player4Control` 不回退。

### 地图配置（当前仅供读取）

地图的 `[Basic]` 用 `Player1` 到 `Player4` 指定合作角色对应的 House 名称；每个值必须与 `[Houses]` 中列出的名称一致。`Player1` 为空或省略时，会回退读取旧式 `Player=`；`Player2` 到 `Player4` 没有旧式回退。例：

```ini
[Basic]
Player1=Americans
Player2=Russians
Player3=YuriCountry
Player4=AmericansAlly
```

这些键只描述角色到主所属 House 的映射，不会自动创建 House。传统 `PlayerControl` 用于额外标记可控制的 House；角色自己的主所属 House 即使 `PlayerControl=no`，角色仍可控制它。`Player1Control` 至 `Player4Control` 是 VERA 为合作战役增加的扩展键，不是传统地图字段。主所属方保护仍是计划中的规则：若某角色的主所属 House 上还设置了其他角色的 `PlayerNControl` 标记，那些额外标记将被忽略；当前尚未实现此规则。当前解析器会读取角色映射；与 `[Houses]` 名单核对、重复/未知所属方校验尚未接入配置加载流程。

每个 `[Houses]` 名称对应的同名段可写四个角色控制标记 `Player1Control` 至 `Player4Control`，以及通用 `PlayerControl`。例如：

```ini
[Houses]
0=Americans
1=Russians
2=SharedSupport

[Americans]
PlayerControl=yes
Player1Control=yes
Player2Control=no

[Russians]
PlayerControl=yes

[SharedSupport]
Player1Control=yes
Player2Control=yes
```

示例中的额外 House `SharedSupport` 同时设置 `Player1Control=yes` 和 `Player2Control=yes`，展示一个 House 标记由两个角色共享。`Player1Control` 缺省时回退到通用 `PlayerControl`，若显式设置则以 `Player1Control` 为准；例如 `Americans` 的显式 `Player1Control=yes` 优先于通用值。`Player2Control`、`Player3Control` 和 `Player4Control` 不回退到 `PlayerControl`；例如 `Russians` 虽有 `PlayerControl=yes`，但没有 `Player2Control`，因此不能据此推定其控制 Player2。角色控制标记目前尚未接入实际游戏控制行为。VERA Coop 的可选 LAN/WOL 模式入口和启动角色校验已实现；但当前即使校验通过也会阻止开局，并提示合作游戏尚未实现。计划中的独立补充配置指外部配置文件，目前尚未实现。不要据此认为地图已支持合作游玩。

后续版本按版本号倒序追加，最新记录放在最前面。

## `0.1.0-dev`

- 仅支持读取合作战役中的角色和控制标记；合作战役尚不能游玩。
