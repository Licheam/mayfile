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

### 会话启动时执行

每次新会话开始时，应按顺序执行：

```bash
# 1. 从 git 历史更新短期记忆
mnemos update

# 2. 读取长期记忆作为上下文
mnemos show -t long
```

### 读取记忆

```bash
# 读取全部记忆（长期 + 短期）
mnemos show

# 仅读取长期记忆
mnemos show -t long

# 仅读取短期记忆
mnemos show -t short
```

### 更新长期记忆

当对话中出现值得长期记住的信息时（如新的架构决策、代码约定、发现的坑），应写入长期记忆：

```bash
# 替换某个 section 的内容
mnemos write -s "架构决策" -c "### 使用 SQLite\n选择 SQLite 作为本地数据库，因为部署简单且足够满足需求。"

# 追加到某个 section
mnemos write -s "重要约束与注意事项" -c "### 并发限制\n数据库连接池不要超过 20。" -a
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
