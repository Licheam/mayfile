---
name: mnemos
description: AI Agent 记忆系统。管理项目的短期记忆（基于 git 历史自动生成）和长期记忆（架构决策、代码约定等持久信息）。Agent 应在会话开始时更新记忆，在发现重要信息时写入长期记忆。
---

# Mnemos - AI Agent 记忆系统

Mnemos 让 AI Agent 具备跨会话的持久记忆能力。

## 核心概念

- **短期记忆** (`.memory/short_term.md`): 从 git 提交历史自动生成，记录最近的开发活动
- **长期记忆** (`.memory/long_term.md`): 项目级持久信息，包含项目概述、架构决策、代码风格、技术选型、注意事项

## 可用命令

### 初始化与维护
- **首次使用**：`mnemos init`
- **更新指令**：`mnemos init --only-skills` (推荐，不触碰记忆数据)
- **健康诊断**：`mnemos doctor` (检查环境与文件完整性)
- **危险操作**：`mnemos init -f` (强制覆盖，**会删除所有现有记忆**，严禁在成熟项目使用)

### 会话启动时执行

每次新会话开始时，应按顺序执行以下操作。

首先，从 git 历史更新短期记忆（现在会自动按类型分组并分析热点文件）：
```bash
mnemos update
```

然后，读取长期记忆作为上下文：
```bash
mnemos show -t long
```

### 核心原则：Agent-First
- **优先更新 Skill**：功能变更后，优先确保 `.agent/skills/` 下的指引已同步（可使用 `mnemos init --only-skills`）。
- **安全写入**：始终优先使用 `write_file` 配合 `mnemos write -f` 的流程来更新记忆。

### 读取记忆

读取全部记忆（长期 + 短期）：
```bash
mnemos show
```

在记忆中搜索关键词（跨长期和短期）：
```bash
mnemos search "关键词"
```

限定搜索最近 7 天的短期记忆：
```bash
mnemos search "关键词" -t short -d 7
```

仅读取长期记忆：
```bash
mnemos show -t long
```

仅读取短期记忆：
```bash
mnemos show -t short
```

### 更新长期记忆

当对话中出现值得长期记住的信息时（如新的架构决策、代码约定、发现的坑），应写入长期记忆。

**AI Agent 推荐流程（最稳妥）：**
1. 使用 `write_file` 工具创建一个包含更新内容的临时文件（如 `temp_memory.md`）。
2. 执行 `mnemos write -s "Section名称" -f temp_memory.md [-a]`。
3. 执行 `rm temp_memory.md` 删除临时文件。
4. 执行 `mnemos update` 同步最新的记忆状态。

这种方式可以彻底避免 Markdown 中的反引号、多行文本被 Shell 误解析的问题。

**命令行快捷方式：**
```bash
mnemos write -s "架构决策" -f decision.md
```

**可用 section**:
- 项目概述
- 架构决策
- 代码风格与约定
- 技术选型
- 重要约束与注意事项

### 压缩旧记忆

定期（如每周）或当短期记忆过大时，提取旧内容供归档：

```bash
mnemos compress
```

此命令会返回超过 3 天的旧短期记忆内容。Agent 应总结关键信息后调用 `mnemos write` 写入长期记忆。

## 使用场景

| 场景 | 命令 |
|------|------|
| 新会话开始 | `mnemos update` + `mnemos show -t long` |
| 需要了解项目背景 | `mnemos show` |
| 做出架构决策 | `mnemos write -s "架构决策" -c "..."` |
| 发现重要的坑 | `mnemos write -s "重要约束与注意事项" -c "..." -a` |
| 定期维护 | `mnemos compress` |
